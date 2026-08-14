//! `wallpaperctl list packs` (FR-003, config-only) and `wallpaperctl list outputs`
//! (FR-005, daemon-required — see spec.md's Assumptions for why the two halves of this
//! story have different daemon requirements).

use pack_loader::Registry;
use serde::Serialize;

use crate::dbus_client::DbusClient;
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
                .map(|e| format!("{}\t{}\t{}", e.name, e.source, e.status))
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

    /// Scenario 4: `list outputs` fails fast with `DaemonUnreachable` when no daemon is
    /// running — real (not mocked), since no service is registered in any test
    /// environment (see dbus_client.rs's own test for the rationale). Skips (rather
    /// than asserting) only if this host has no session bus at all to connect to.
    #[test]
    fn list_outputs_fails_fast_without_a_daemon() {
        if zbus::blocking::Connection::session().is_err() {
            return;
        }
        assert!(matches!(outputs(false), Err(CliError::DaemonUnreachable)));
    }
}
