//! Resolve a user-supplied path argument to an already-registered
//! [`pack_loader::PackSource`] — shared by `assign` and `remove`, both of which
//! reference a pack by the same path a user originally passed to `register`.

use std::path::Path;

use pack_loader::{PackSource, Registry};

/// Find the registered [`PackSource`] matching `path`, or `None` if it isn't
/// registered.
///
/// Prefers an exact canonical-path match (the common case: the pack still exists on
/// disk). Falls back to a lexical match against the raw argument so a since-vanished
/// pack (spec 2 FR-011's `Unavailable` case) can still be found by the same path a user
/// originally registered it with — `std::fs::canonicalize` requires the target to
/// exist, which an unavailable pack's source, by definition, might not.
pub fn find_registered(registry: &Registry, path: &Path) -> Option<PackSource> {
    if let Ok(canonical) = std::fs::canonicalize(path) {
        if let Some(entry) = registry.known_packs().into_iter().find(|e| e.source.path() == canonical) {
            return Some(entry.source);
        }
    }
    registry.known_packs().into_iter().find(|e| e.source.path() == path).map(|e| e.source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_still_present_pack_by_canonical_path() {
        let dir = tempfile::tempdir().unwrap();
        let mut registry = Registry::open_at(dir.path()).unwrap();
        let file = dir.path().join("pack.png");
        image::RgbImage::new(2, 2).save(&file).unwrap();
        let source = PackSource::resolve(&file).unwrap();
        registry.register(source.clone()).unwrap();

        assert_eq!(find_registered(&registry, &file), Some(source));
    }

    #[test]
    fn finds_a_vanished_pack_by_lexical_path() {
        let dir = tempfile::tempdir().unwrap();
        let mut registry = Registry::open_at(dir.path()).unwrap();
        let file = dir.path().join("pack.png");
        image::RgbImage::new(2, 2).save(&file).unwrap();
        let source = PackSource::resolve(&file).unwrap();
        registry.register(source.clone()).unwrap();

        std::fs::remove_file(&file).unwrap();

        assert_eq!(find_registered(&registry, &file), Some(source));
    }

    #[test]
    fn returns_none_for_an_unregistered_path() {
        let dir = tempfile::tempdir().unwrap();
        let registry = Registry::open_at(dir.path()).unwrap();
        assert_eq!(find_registered(&registry, Path::new("/never/registered.jpg")), None);
    }
}
