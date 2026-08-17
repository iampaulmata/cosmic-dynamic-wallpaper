//! [`LoadedPack`] and [`load_pack`] — the top-level entry point (contracts/
//! pack-loader-api.md) that turns a directory (manifest pack, FR-001–FR-003, FR-006,
//! FR-006a, FR-008) or a single image file (static pack, FR-004) into a fully validated,
//! spec-1-compatible in-memory pack.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use schedule_engine::{ImageId, PackImage, ValidatedPack, WallpaperPack};

use crate::error::ManifestError;
use crate::image_check;
use crate::manifest::{self, Color, ScalingMode};
use crate::pack_source::PackSource;
use crate::path_safety;

/// The manifest filename this loader looks for inside a pack directory. Not itself
/// spec'd by name anywhere in spec.md/data-model.md — an implementation choice, kept in
/// one named constant so it's easy to find/change.
pub const MANIFEST_FILE_NAME: &str = "manifest.toml";

/// The largest a `manifest.toml` may be before it's rejected outright, checked before
/// the file is read fully into memory (spec 011 US3 FR-011, research.md R8 —
/// clarified value: 512 KB, roughly 40x a realistic 64-anchor manifest's actual size).
pub const MAX_MANIFEST_BYTES: u64 = 512 * 1024;

/// The output of a successful load (data-model.md `LoadedPack`) — what spec 1
/// (scheduling) and spec 3 (renderer) actually consume.
#[derive(Debug, Clone)]
pub struct LoadedPack {
    /// Identity key (FR-009): directory path for a manifest pack, file path for a
    /// static pack.
    pub source: PackSource,
    /// Display name — from the manifest, or derived from the filename for a static
    /// pack.
    pub name: String,
    /// Optional author/license note, from the manifest (`None` for a static pack).
    pub author: Option<String>,
    /// Pack-level default scaling mode (FR-005).
    pub default_scaling: ScalingMode,
    /// Fallback fill color for letterboxed edges under `Fit`/`Center` scaling (FR-005).
    pub fallback_color: Color,
    /// Spec 1's validated pack — built by handing every resolved `(image id,
    /// TimeAnchor)` pair to [`schedule_engine::WallpaperPack::validate`] (FR-003).
    pub pack: ValidatedPack,
    /// Resolved, containment-checked absolute paths per image id (FR-006a) — this
    /// loader's own bookkeeping, not part of spec 1's contract.
    pub image_paths: HashMap<ImageId, PathBuf>,
    /// Resolved per-image scaling (override or pack default applied, FR-005).
    pub image_scaling: HashMap<ImageId, ScalingMode>,
}

/// Load a pack from `path` (contracts/pack-loader-api.md).
///
/// - If `path` is a directory: look for [`MANIFEST_FILE_NAME`], parse it, resolve and
///   containment-check every image path, header-validate each image is readable, and
///   hand the resolved anchor list to spec 1's `WallpaperPack::validate`.
/// - If `path` is a single image file: produce the static, manifest-free pack (FR-004).
///
/// Never panics; every failure mode is returned as a [`ManifestError`], not thrown
/// (constitution Principle VIII).
pub fn load_pack(path: &Path) -> Result<LoadedPack, ManifestError> {
    if path.is_dir() {
        load_directory_pack(path)
    } else if path.is_file() {
        load_static_pack(path)
    } else {
        Err(ManifestError::Io { path: path.to_path_buf(), message: "path does not exist".to_string() })
    }
}

fn load_directory_pack(dir: &Path) -> Result<LoadedPack, ManifestError> {
    let manifest_path = dir.join(MANIFEST_FILE_NAME);
    if !manifest_path.is_file() {
        return Err(ManifestError::ManifestNotFound { path: manifest_path });
    }
    // Spec 011 US3 FR-011 (research.md R8): reject an oversized manifest before it's
    // read fully into memory — a single `stat` (`metadata`), cheap even against a
    // multi-gigabyte attack file.
    let size = std::fs::metadata(&manifest_path)
        .map_err(|e| ManifestError::Io { path: manifest_path.clone(), message: e.to_string() })?
        .len();
    if size > MAX_MANIFEST_BYTES {
        return Err(ManifestError::ManifestTooLarge { path: manifest_path.clone(), size });
    }
    let text = std::fs::read_to_string(&manifest_path)
        .map_err(|e| ManifestError::Io { path: manifest_path.clone(), message: e.to_string() })?;
    let parsed = manifest::parse(&text, &manifest_path)?;

    // Spec 011 US3 FR-010 (research.md R7): reject an over-cap image count *before*
    // any per-image filesystem work (resolve/containment-check/header-read) runs
    // below — `WallpaperPack::validate` (called after that loop) already enforces
    // `MAX_ANCHORS`, but only after every declared entry has already cost a handful of
    // syscalls each. A manifest declaring 500,000 entries previously forced 500,000
    // syscalls before being rejected; this is a single, cheap length check first,
    // returning the exact same error shape `validate` would have produced anyway.
    if parsed.images.len() > schedule_engine::MAX_ANCHORS {
        return Err(ManifestError::InvalidPack(schedule_engine::PackError::TooManyAnchors { count: parsed.images.len() }));
    }

    let mut pack_images = Vec::with_capacity(parsed.images.len());
    let mut image_paths = HashMap::with_capacity(parsed.images.len());
    let mut image_scaling = HashMap::with_capacity(parsed.images.len());

    for img in &parsed.images {
        // FR-006a: resolve + containment-check before anything else touches the path.
        let resolved = path_safety::resolve_and_check(dir, &img.file)?;
        // FR-006 / User Story 1 Scenario: reject an unreadable/non-image file.
        image_check::check_readable(&resolved, &img.file)?;

        let id = ImageId::new(img.file.clone());
        pack_images.push(PackImage::new(id.clone(), img.anchor));
        image_paths.insert(id.clone(), resolved);
        image_scaling.insert(id, img.scaling.unwrap_or(parsed.default_scaling));
    }

    // FR-003: hand the resolved anchor list to spec 1's own validation rather than
    // re-implementing anchor-correctness rules — mixed types, the 64-anchor cap, and
    // duplicate-instant ties all apply here by inheritance.
    let pack = WallpaperPack::validate(pack_images)?;

    let source = PackSource::resolve(dir)?;

    Ok(LoadedPack {
        source,
        name: parsed.name,
        author: parsed.author,
        default_scaling: parsed.default_scaling,
        fallback_color: parsed.fallback_color,
        pack,
        image_paths,
        image_scaling,
    })
}

fn load_static_pack(file: &Path) -> Result<LoadedPack, ManifestError> {
    let file_name = file
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| ManifestError::NonUtf8Path { path: file.to_path_buf() })?
        .to_string();

    image_check::check_readable(file, &file_name)?;

    let id = ImageId::new(file_name.clone());
    let pack = WallpaperPack::validate(vec![PackImage::new(
        id.clone(),
        // A static pack has no time anchor at all (FR-004). Spec 1's degenerate
        // single-image case (data-model.md Assumptions) is "one always-active image
        // with no transitions" — modeled here as a single Clock anchor at midnight,
        // which is never observably different from a true anchor-less image since a
        // one-image pack is always active regardless of what its lone anchor says
        // (`ValidatedPack::is_static`/`query` never consult it).
        schedule_engine::TimeAnchor::clock(chrono::NaiveTime::MIN),
    )])?;

    let source = PackSource::resolve(file)?;

    let mut image_paths = HashMap::with_capacity(1);
    image_paths.insert(id.clone(), source.path().to_path_buf());
    let mut image_scaling = HashMap::with_capacity(1);
    image_scaling.insert(id, ScalingMode::Fill);

    Ok(LoadedPack {
        source,
        name: file_name,
        author: None,
        default_scaling: ScalingMode::Fill,
        fallback_color: Color { r: 0, g: 0, b: 0, a: 255 },
        pack,
        image_paths,
        image_scaling,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_pack_on_a_nonexistent_path_is_an_io_error() {
        let missing = std::env::temp_dir().join("pack-loader-load-test-does-not-exist-12345");
        assert!(matches!(load_pack(&missing), Err(ManifestError::Io { .. })));
    }

    #[test]
    fn load_pack_on_a_directory_with_no_manifest_is_manifest_not_found() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(load_pack(dir.path()), Err(ManifestError::ManifestNotFound { .. })));
    }

    /// Spec 011 US3 FR-010 (research.md R7): a manifest declaring more entries than
    /// `MAX_ANCHORS` must be rejected *before* any per-image filesystem work runs —
    /// proven here by having every declared entry reference a file that doesn't exist.
    /// If the per-image loop ran even once before the cap check, the first entry
    /// would fail with `MissingImageFile`, not `TooManyAnchors` — asserting the
    /// specific error variant is what actually proves the ordering, not just that
    /// *some* error occurred.
    #[test]
    fn anchor_cap_rejected_before_per_image_io() {
        let dir = tempfile::tempdir().unwrap();
        let mut manifest = String::from("schema_version = 1\nname = \"x\"\ndefault_scaling = \"Fill\"\nfallback_color = \"#000000\"\n\n");
        for i in 0..(schedule_engine::MAX_ANCHORS + 1) {
            manifest.push_str(&format!("[[images]]\nfile = \"does-not-exist-{i}.png\"\nanchor = \"sunrise\"\n\n"));
        }
        std::fs::write(dir.path().join(MANIFEST_FILE_NAME), manifest).unwrap();

        let result = load_pack(dir.path());
        assert!(
            matches!(result, Err(ManifestError::InvalidPack(schedule_engine::PackError::TooManyAnchors { .. }))),
            "expected TooManyAnchors (rejected before any per-image I/O), got {result:?}"
        );
    }

    /// Spec 011 US3 FR-011 (research.md R8) — a `manifest.toml` over
    /// `MAX_MANIFEST_BYTES` is rejected before being read fully into memory. Content is
    /// deliberately not otherwise-valid TOML: if the size check didn't run first, this
    /// would instead surface as a `ParseFailure`, not `ManifestTooLarge` — asserting the
    /// specific variant is what proves the ordering.
    #[test]
    fn oversized_manifest_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let oversized = "# padding\n".repeat((MAX_MANIFEST_BYTES as usize / 10) + 1000);
        assert!(oversized.len() as u64 > MAX_MANIFEST_BYTES);
        std::fs::write(dir.path().join(MANIFEST_FILE_NAME), &oversized).unwrap();

        let result = load_pack(dir.path());
        assert!(matches!(result, Err(ManifestError::ManifestTooLarge { .. })), "expected ManifestTooLarge, got {result:?}");
    }
}
