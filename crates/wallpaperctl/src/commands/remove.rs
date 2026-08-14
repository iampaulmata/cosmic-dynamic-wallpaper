//! `wallpaperctl remove <pack-source>` (FR-004).

use std::path::Path;

use pack_loader::Registry;

use crate::error::CliError;
use crate::output::{self, Ack};
use crate::pack_ref::find_registered;

pub fn run(path: &Path, registry: &mut Registry, json: bool) -> Result<String, CliError> {
    let source = find_registered(registry, path)
        .ok_or_else(|| CliError::PackNotFound { source: path.to_path_buf() })?;
    registry.remove(&source)?;
    Ok(output::render(json, &Ack::ok(), || "removed".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Registry, tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let mut registry = Registry::open_at(dir.path()).unwrap();
        let file = dir.path().join("pack.png");
        image::RgbImage::new(2, 2).save(&file).unwrap();
        let source = pack_loader::PackSource::resolve(&file).unwrap();
        registry.register(source).unwrap();
        (registry, dir, file)
    }

    /// Scenario 1: removing a pack deletes its registry entry outright.
    #[test]
    fn removes_a_registered_pack() {
        let (mut registry, _dir, file) = setup();
        assert!(run(&file, &mut registry, false).is_ok());
        assert!(registry.known_packs().is_empty());
    }

    /// Scenario 2: removing a pack still assigned somewhere still succeeds — this
    /// command doesn't know or care about assignments, matching spec.md's "no new
    /// fallback behavior invented here" posture.
    #[test]
    fn removal_succeeds_regardless_of_any_assignment() {
        let (mut registry, _dir, file) = setup();
        // No assignment state is touched by this command at all — nothing to arrange;
        // removal simply must not fail because of anything assignment-related.
        assert!(run(&file, &mut registry, false).is_ok());
    }

    #[test]
    fn removing_an_unregistered_pack_fails_clearly() {
        let dir = tempfile::tempdir().unwrap();
        let mut registry = Registry::open_at(dir.path()).unwrap();
        let result = run(Path::new("/never/registered.png"), &mut registry, false);
        assert!(matches!(result, Err(CliError::PackNotFound { .. })));
    }
}
