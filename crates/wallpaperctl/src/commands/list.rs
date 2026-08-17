//! `wallpaperctl list packs` (FR-003, config-only) and `wallpaperctl list outputs`
//! (FR-005, daemon-required — see spec.md's Assumptions for why the two halves of this
//! story have different daemon requirements).

use pack_loader::Registry;
use serde::Serialize;

use wallpaper_ipc::DbusClient;
use crate::error::CliError;
use crate::output;

/// One entry in `list packs`' output — enough to choose a pack for `assign` (FR-003).
/// Includes `name`, which spec 2's `PackRegistryEntry` doesn't itself carry (only the
/// source location is persisted) — obtained here by reloading each known pack, which
/// also has the side effect of refreshing each entry's `Known`/`Unavailable` status
/// (same as `Registry::reload_all`'s own contract).
#[derive(Debug, Serialize)]
pub struct PackListEntry {
    pub name: String,
    pub source: String,
    pub status: &'static str,
}

pub fn packs(registry: &mut Registry, json: bool) -> String {
    let results = registry.reload_all();
    let entries: Vec<PackListEntry> = results
        .into_iter()
        .map(|(source, result)| {
            let source_str = source.path().display().to_string();
            match result {
                Ok(loaded) => PackListEntry { name: loaded.name, source: source_str, status: "known" },
                Err(_) => PackListEntry { name: source_str.clone(), source: source_str, status: "unavailable" },
            }
        })
        .collect();

    output::render(json, &entries, || {
        if entries.is_empty() {
            "no packs registered".to_string()
        } else {
            entries
                .iter()
                // FR-018 (research.md R14): `e.name` is untrusted, pack-author-
                // controlled data — sanitized here, in the human-readable rendering
                // closure only, so it can never masquerade as extra tab-delimited
                // rows. `e.source`/`e.status` are this program's own values, not
                // untrusted manifest content.
                .map(|e| format!("{}\t{}\t{}", output::sanitize_for_tsv(&e.name), e.source, e.status))
                .collect::<Vec<_>>()
                .join("\n")
        }
    })
}

/// `wallpaperd`-managed outputs, by the same identifier spec 3's assignment
/// configuration uses (FR-005). Reuses `QueryAll` (research.md R5) rather than a
/// separate `ListOutputs` D-Bus method, displaying only each entry's `output_id`.
pub fn outputs(json: bool) -> Result<String, CliError> {
    let client = DbusClient::connect()?;
    let entries = client.query_all()?;
    let ids: Vec<String> = entries.into_iter().map(|e| e.output).collect();

    Ok(output::render(json, &ids, || {
        if ids.is_empty() {
            "no outputs managed".to_string()
        } else {
            ids.join("\n")
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Scenarios 1-2: registered packs are shown with identifying info; an empty
    /// registry reports clearly, not as an error.
    #[test]
    fn lists_registered_packs_and_handles_empty_registry() {
        let dir = tempfile::tempdir().unwrap();
        let mut registry = Registry::open_at(dir.path()).unwrap();

        assert_eq!(packs(&mut registry, false), "no packs registered");

        let file = dir.path().join("pack.png");
        image::RgbImage::new(2, 2).save(&file).unwrap();
        let source = pack_loader::PackSource::resolve(&file).unwrap();
        registry.register(source).unwrap();

        let out = packs(&mut registry, false);
        assert!(out.contains("known"));
        assert!(out.contains(&file.file_name().unwrap().to_string_lossy().to_string()));
    }

    /// Spec 011 US5 FR-018 (research.md R14) — the audit's own reproduction: a pack
    /// whose manifest `name` contains a tab and a newline must not render as extra
    /// fake rows in the human-readable output; `--json` must still carry the raw
    /// value verbatim (its own string escaping already makes structural injection
    /// impossible, and downstream JSON consumers expect the untouched value).
    #[test]
    fn tab_newline_escaped_in_human_output_but_not_json() {
        let dir = tempfile::tempdir().unwrap();
        let pack_dir = dir.path().join("evil-pack");
        std::fs::create_dir(&pack_dir).unwrap();
        image::RgbImage::new(2, 2).save(pack_dir.join("a.png")).unwrap();
        std::fs::write(
            pack_dir.join(pack_loader::MANIFEST_FILE_NAME),
            "schema_version = 1\nname = \"evil\\tDP-3\\tknown\\nfake-row\"\ndefault_scaling = \"Fill\"\nfallback_color = \"#000000\"\n\n[[images]]\nfile = \"a.png\"\nanchor = \"sunrise\"\n",
        )
        .unwrap();

        let registry_dir = tempfile::tempdir().unwrap();
        let mut registry = Registry::open_at(registry_dir.path()).unwrap();
        registry.register(pack_loader::PackSource::resolve(&pack_dir).unwrap()).unwrap();

        let human = packs(&mut registry, false);
        assert_eq!(human.lines().count(), 1, "a crafted name must not fabricate extra rows:\n{human}");
        // Exactly the two real `name\tsource\tstatus` field-separator tabs remain —
        // none from the (now-sanitized) name field itself.
        assert_eq!(human.matches('\t').count(), 2, "unexpected tab count in: {human:?}");

        let json = packs(&mut registry, true);
        assert!(json.contains("evil\\tDP-3\\tknown\\nfake-row"), "--json must still carry the raw value, JSON-escaped: {json}");
    }

    /// Scenario 4: `list outputs` fails fast with `DaemonUnreachable` when no daemon is
    /// running — real (not mocked), since no service is registered in any test
    /// environment (see wallpaper_ipc::dbus_client's own test for the rationale). Skips (rather
    /// than asserting) only if this host has no session bus at all to connect to.
    #[test]
    fn list_outputs_fails_fast_without_a_daemon() {
        if zbus::blocking::Connection::session().is_err() {
            return;
        }
        assert!(matches!(outputs(false), Err(CliError::DaemonUnreachable)));
    }
}
