//! Pure, read-time-only derivations from an already-registered pack's on-disk content
//! (spec 008 data-model.md, research.md R2/R7) — the single source both
//! `pages::packs` and `pages::assignment` call, instead of each independently
//! deciding what "the pack's name"/"the pack's thumbnail" means.
//!
//! Neither function here adds a new failure mode beyond `pack_loader::load_pack`'s own
//! (constitution Principle VIII) — a failed load simply yields `None`, and every caller
//! shows a clearly-labeled placeholder for that case (spec.md FR-011/FR-020).

use std::path::{Path, PathBuf};

use pack_loader::PackSource;
use schedule_engine::{SolarEventKind, TimeAnchor};

/// The human-readable label for a registered pack (spec.md FR-010, Key Entities "Pack
/// name"): a directory pack's manifest `name`; a static-file pack's filename with the
/// extension stripped (the `/speckit-clarify` session's specific wording); `None` if
/// the pack can't be loaded at all (its registry entry is already `Unavailable`).
pub fn resolve_pack_name(source: &PackSource) -> Option<String> {
    let loaded = pack_loader::load_pack(source.path()).ok()?;
    match source {
        PackSource::Directory(_) => Some(loaded.name),
        PackSource::StaticFile(_) => {
            Path::new(&loaded.name).file_stem().map(|s| s.to_string_lossy().into_owned())
        }
    }
}

/// A representative preview image for a registered pack (spec.md FR-018/FR-019, Key
/// Entities "Pack thumbnail"): the image anchored to solar noon if one exists,
/// otherwise the pack's first image in manifest/declaration order (which is also its
/// only image, for a single-image static pack — those are always `Clock`-anchored, so
/// the `find` below never matches and falls straight through to `.first()`). `None` if
/// the pack can't be loaded at all.
pub fn resolve_thumbnail_path(source: &PackSource) -> Option<PathBuf> {
    let loaded = pack_loader::load_pack(source.path()).ok()?;
    let chosen = loaded
        .pack
        .images()
        .iter()
        .find(|img| matches!(img.anchor, TimeAnchor::Solar { event: SolarEventKind::SolarNoon, .. }))
        .or_else(|| loaded.pack.images().first())?;
    loaded.image_paths.get(&chosen.id).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_pack_dir(dir: &std::path::Path, manifest_body: &str, images: &[&str]) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("manifest.toml"), manifest_body).unwrap();
        for name in images {
            image::RgbImage::new(2, 2).save(dir.join(name)).unwrap();
        }
    }

    #[test]
    fn resolve_pack_name_returns_the_manifest_name_for_a_directory_pack() {
        let dir = tempfile::tempdir().unwrap();
        let pack_dir = dir.path().join("mountains");
        write_pack_dir(
            &pack_dir,
            r##"
                schema_version = 1
                name = "Mountains"
                default_scaling = "Fill"
                fallback_color = "#000000"
                [[images]]
                file = "noon.png"
                anchor = "solar_noon"
            "##,
            &["noon.png"],
        );
        let source = PackSource::resolve(&pack_dir).unwrap();

        assert_eq!(resolve_pack_name(&source), Some("Mountains".to_string()));
    }

    #[test]
    fn resolve_pack_name_strips_the_extension_for_a_static_file_pack() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("sunrise.png");
        image::RgbImage::new(2, 2).save(&file).unwrap();
        let source = PackSource::resolve(&file).unwrap();

        assert_eq!(resolve_pack_name(&source), Some("sunrise".to_string()));
    }

    #[test]
    fn resolve_pack_name_is_none_when_load_pack_fails() {
        let dir = tempfile::tempdir().unwrap();
        // A directory with no manifest.toml at all — load_pack::ManifestNotFound.
        let source = PackSource::Directory(dir.path().to_path_buf());

        assert_eq!(resolve_pack_name(&source), None);
    }

    #[test]
    fn resolve_thumbnail_path_picks_the_solar_noon_anchored_image() {
        let dir = tempfile::tempdir().unwrap();
        let pack_dir = dir.path().join("mountains");
        write_pack_dir(
            &pack_dir,
            r##"
                schema_version = 1
                name = "Mountains"
                default_scaling = "Fill"
                fallback_color = "#000000"
                [[images]]
                file = "dawn.png"
                anchor = "sunrise"
                [[images]]
                file = "noon.png"
                anchor = "solar_noon"
                [[images]]
                file = "dusk.png"
                anchor = "sunset"
            "##,
            &["dawn.png", "noon.png", "dusk.png"],
        );
        let source = PackSource::resolve(&pack_dir).unwrap();

        let thumbnail = resolve_thumbnail_path(&source).unwrap();
        assert_eq!(thumbnail.file_name().unwrap(), "noon.png");
    }

    #[test]
    fn resolve_thumbnail_path_falls_back_to_the_first_image_with_no_solar_noon_anchor() {
        let dir = tempfile::tempdir().unwrap();
        let pack_dir = dir.path().join("no-noon");
        write_pack_dir(
            &pack_dir,
            r##"
                schema_version = 1
                name = "No Noon"
                default_scaling = "Fill"
                fallback_color = "#000000"
                [[images]]
                file = "dawn.png"
                anchor = "sunrise"
                [[images]]
                file = "dusk.png"
                anchor = "sunset"
            "##,
            &["dawn.png", "dusk.png"],
        );
        let source = PackSource::resolve(&pack_dir).unwrap();

        let thumbnail = resolve_thumbnail_path(&source).unwrap();
        assert_eq!(thumbnail.file_name().unwrap(), "dawn.png");
    }

    #[test]
    fn resolve_thumbnail_path_uses_the_only_image_for_a_static_file_pack() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("sunrise.png");
        image::RgbImage::new(2, 2).save(&file).unwrap();
        let source = PackSource::resolve(&file).unwrap();

        let thumbnail = resolve_thumbnail_path(&source).unwrap();
        assert_eq!(thumbnail, source.path());
    }

    #[test]
    fn resolve_thumbnail_path_is_none_when_load_pack_fails() {
        let dir = tempfile::tempdir().unwrap();
        let source = PackSource::Directory(dir.path().to_path_buf());

        assert_eq!(resolve_thumbnail_path(&source), None);
    }
}
