//! `wallpaperctl assign --output <id> <pack>` / `assign --same-everywhere <pack>`
//! (FR-006, FR-007).

use std::path::Path;

use cosmic_config::Config;
use pack_loader::Registry;

use crate::config::RendererConfig;
use crate::dbus_client::DbusClient;
use crate::error::CliError;
use crate::output::{self, Ack};
use crate::pack_ref::find_registered;

/// What an assignment targets — exactly one of a specific output or the "same pack
/// everywhere" toggle (data-model.md `OutputAssignmentRequest.target`).
pub enum AssignTarget {
    Output(String),
    SameEverywhere,
}

/// `registry`/`renderer_config` are already-open handles (see `register.rs`'s doc
/// comment for why) — `main.rs` passes the real ones, tests pass tempdir-backed ones.
pub fn run(
    target: AssignTarget,
    pack_path: &Path,
    registry: &Registry,
    renderer_config: &Config,
    json: bool,
) -> Result<String, CliError> {
    // FR-007: checked against the local registry — never requires a daemon.
    let source = find_registered(registry, pack_path)
        .ok_or_else(|| CliError::PackNotFound { source: pack_path.to_path_buf() })?;

    let mut state = RendererConfig::load(renderer_config);
    let target_desc = match &target {
        AssignTarget::Output(id) => {
            state.overrides.insert(id.clone(), source.clone());
            format!("output {id:?}")
        }
        AssignTarget::SameEverywhere => {
            state.same_pack_everywhere = Some(source.clone());
            "all outputs (same-everywhere)".to_string()
        }
    };
    state.save(renderer_config)?;

    // FR-007: a non-fatal warning only, and only if the daemon happens to be
    // reachable — assigning to a not-yet-connected output name is a legitimate
    // "configure ahead of time" case, never a failure.
    if let AssignTarget::Output(id) = &target {
        if let Ok(client) = DbusClient::connect() {
            if let Ok(entries) = client.query_all() {
                if !entries.iter().any(|e| &e.output == id) {
                    eprintln!(
                        "warning: output {id:?} is not currently connected — the assignment is saved and will apply once it connects"
                    );
                }
            }
        }
    }

    Ok(output::render(json, &Ack::ok(), || format!("assigned to {target_desc}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Registry, Config, tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let registry_dir = dir.path().join("registry");
        std::fs::create_dir_all(&registry_dir).unwrap();
        let renderer_dir = dir.path().join("renderer");
        std::fs::create_dir_all(&renderer_dir).unwrap();

        let mut registry = Registry::open_at(&registry_dir).unwrap();
        let pack_file = dir.path().join("pack.png");
        image::RgbImage::new(2, 2).save(&pack_file).unwrap();
        let source = pack_loader::PackSource::resolve(&pack_file).unwrap();
        registry.register(source).unwrap();

        let renderer_config = RendererConfig::open_at(&renderer_dir).unwrap();
        (registry, renderer_config, dir, pack_file)
    }

    /// Scenario 1 & 4: assigning a registered pack to an output writes
    /// `RendererConfig.overrides`, including a not-currently-connected name (no daemon
    /// running in this test, so the "configure ahead of time" case applies).
    #[test]
    fn assigns_a_registered_pack_to_an_output() {
        let (registry, renderer_config, _dir, pack_file) = setup();

        let result = run(
            AssignTarget::Output("DP-3".to_string()),
            &pack_file,
            &registry,
            &renderer_config,
            false,
        );
        assert!(result.is_ok());

        let state = RendererConfig::load(&renderer_config);
        assert!(state.overrides.contains_key("DP-3"));
    }

    /// Scenario 2: "same pack everywhere" sets `same_pack_everywhere`.
    #[test]
    fn same_everywhere_sets_the_toggle() {
        let (registry, renderer_config, _dir, pack_file) = setup();

        run(AssignTarget::SameEverywhere, &pack_file, &registry, &renderer_config, false).unwrap();

        let state = RendererConfig::load(&renderer_config);
        assert!(state.same_pack_everywhere.is_some());
    }

    /// Scenario 3: assigning a different pack to an already-assigned output replaces
    /// the old value.
    #[test]
    fn reassigning_an_output_replaces_the_old_pack() {
        let (registry, renderer_config, dir, pack_file) = setup();
        run(
            AssignTarget::Output("DP-3".to_string()),
            &pack_file,
            &registry,
            &renderer_config,
            false,
        )
        .unwrap();

        let mut registry = registry;
        let second_file = dir.path().join("second.png");
        image::RgbImage::new(2, 2).save(&second_file).unwrap();
        let second_source = pack_loader::PackSource::resolve(&second_file).unwrap();
        registry.register(second_source.clone()).unwrap();

        run(
            AssignTarget::Output("DP-3".to_string()),
            &second_file,
            &registry,
            &renderer_config,
            false,
        )
        .unwrap();

        let state = RendererConfig::load(&renderer_config);
        assert_eq!(state.overrides.get("DP-3"), Some(&second_source));
    }

    /// Scenario 5: assigning an unregistered pack fails clearly with no write.
    #[test]
    fn assigning_an_unregistered_pack_fails() {
        let (registry, renderer_config, dir, _pack_file) = setup();
        let unregistered = dir.path().join("never-registered.png");
        image::RgbImage::new(2, 2).save(&unregistered).unwrap();

        let result = run(
            AssignTarget::Output("DP-3".to_string()),
            &unregistered,
            &registry,
            &renderer_config,
            false,
        );
        assert!(matches!(result, Err(CliError::PackNotFound { .. })));
        assert!(RendererConfig::load(&renderer_config).overrides.is_empty());
    }
}
