//! `wallpaperctl register <path>` (FR-001, FR-002).

use std::path::Path;

use pack_loader::{load_pack, Registry};

use crate::error::CliError;
use crate::output::{self, Ack};

/// `registry` is an already-open handle (`Registry::open()` in `main.rs`,
/// `Registry::open_at(tempdir)` in tests) rather than opened here — keeps this
/// function testable without ever touching the real user config directory.
pub fn run(path: &Path, registry: &mut Registry, json: bool) -> Result<String, CliError> {
    let loaded = load_pack(path)
        .map_err(|e| CliError::PackLoadFailed { source: path.to_path_buf(), reason: e.to_string() })?;

    registry.register(loaded.source.clone())?;

    Ok(output::render(json, &Ack::ok(), || {
        format!(
            "registered {:?} ({} image{})",
            loaded.name,
            loaded.pack.len(),
            if loaded.pack.len() == 1 { "" } else { "s" }
        )
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_registry() -> (Registry, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        (Registry::open_at(dir.path()).unwrap(), dir)
    }

    /// US1 Scenarios 1-2: a valid multi-image pack directory and a single static image
    /// both become known.
    #[test]
    fn registers_a_valid_directory_pack_and_a_static_image() {
        let (mut registry, dir) = temp_registry();

        let pack_dir = dir.path().join("pack");
        std::fs::create_dir_all(&pack_dir).unwrap();
        std::fs::write(
            pack_dir.join("manifest.toml"),
            r##"
schema_version = 1
name = "Test"
default_scaling = "Fill"
fallback_color = "#000000"

[[images]]
file = "a.png"
anchor = "sunrise"
"##,
        )
        .unwrap();
        image::RgbImage::new(2, 2).save(pack_dir.join("a.png")).unwrap();

        let static_file = dir.path().join("static.png");
        image::RgbImage::new(2, 2).save(&static_file).unwrap();

        assert!(run(&pack_dir, &mut registry, false).is_ok());
        assert!(run(&static_file, &mut registry, false).is_ok());
        assert_eq!(registry.known_packs().len(), 2);
    }

    /// US1 Scenario 3: registering an already-known source is idempotent.
    #[test]
    fn registering_twice_is_idempotent() {
        let (mut registry, dir) = temp_registry();
        let file = dir.path().join("static.png");
        image::RgbImage::new(2, 2).save(&file).unwrap();

        run(&file, &mut registry, false).unwrap();
        run(&file, &mut registry, false).unwrap();
        assert_eq!(registry.known_packs().len(), 1);
    }

    /// US1 Scenario 4: an invalid pack fails clearly and adds nothing to the registry.
    #[test]
    fn invalid_pack_fails_and_registers_nothing() {
        let (mut registry, dir) = temp_registry();
        let pack_dir = dir.path().join("broken");
        std::fs::create_dir_all(&pack_dir).unwrap();
        std::fs::write(pack_dir.join("manifest.toml"), "not valid toml {{{").unwrap();

        let result = run(&pack_dir, &mut registry, false);
        assert!(matches!(result, Err(CliError::PackLoadFailed { .. })));
        assert!(registry.known_packs().is_empty());
    }
}
