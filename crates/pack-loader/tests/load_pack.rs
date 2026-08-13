//! Acceptance scenario tests from spec.md's User Stories 1–3, run against committed
//! fixture directories under `tests/fixtures/` (research.md R6).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use pack_loader::{load_pack, ManifestError, ScalingMode};

fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(rel)
}

// ---------------------------------------------------------------------------------
// User Story 1 — Load a Multi-Image Time-Anchored Pack From a Directory
// ---------------------------------------------------------------------------------

/// Scenario 1: a valid manifest + all referenced images present loads a pack whose
/// images, anchors, and metadata match the manifest.
#[test]
fn us1_scenario1_valid_pack_loads_with_correct_images_and_metadata() {
    let loaded = load_pack(&fixture("valid_pack")).expect("valid fixture should load");
    assert_eq!(loaded.name, "Valid Test Pack");
    assert_eq!(loaded.author.as_deref(), Some("Test Fixture <fixture@example.com> - CC0"));
    assert_eq!(loaded.default_scaling, ScalingMode::Fill);
    assert_eq!(loaded.pack.len(), 3);
    assert_eq!(loaded.image_paths.len(), 3);
    assert!(loaded.image_paths.values().all(|p| p.is_file()));
}

/// Scenario 2: a manifest referencing a missing image file fails loading with a clear
/// error naming the missing file, and no partial pack is returned.
#[test]
fn us1_scenario2_missing_image_file_is_a_clear_error() {
    let result = load_pack(&fixture("invalid/missing_image"));
    match result {
        Err(ManifestError::MissingImageFile { file }) => assert_eq!(file, "ghost.png"),
        other => panic!("expected MissingImageFile, got {other:?}"),
    }
}

/// Scenario 3: anchors spec 1's own validation would reject (here: mixed anchor types)
/// surface that same validation error rather than being silently accepted/truncated.
#[test]
fn us1_scenario3_spec1_validation_errors_are_surfaced() {
    let result = load_pack(&fixture("invalid/mixed_anchors"));
    match result {
        Err(ManifestError::InvalidPack(schedule_engine::PackError::MixedAnchorTypes)) => {}
        other => panic!("expected InvalidPack(MixedAnchorTypes), got {other:?}"),
    }
}

/// Scenario 4: image files present in the directory but not referenced by the manifest
/// are ignored (`unused_extra.png` in the fixture), not an error.
#[test]
fn us1_scenario4_untracked_extra_files_are_ignored() {
    let loaded = load_pack(&fixture("valid_pack")).expect("valid fixture should load");
    // Only the 3 manifest-declared images are in the pack, even though a 4th
    // (unused_extra.png) sits in the same directory.
    assert_eq!(loaded.pack.len(), 3);
    assert!(!loaded.image_paths.keys().any(|id| id.as_str() == "unused_extra.png"));
}

/// Edge Case: a malformed manifest fails with a specific parse error, not a crash.
#[test]
fn malformed_manifest_is_a_clear_parse_error() {
    let result = load_pack(&fixture("invalid/malformed_manifest"));
    assert!(matches!(result, Err(ManifestError::ParseFailure { .. })));
}

/// Edge Case: a manifest declaring a newer schema version than supported fails with a
/// specific "unsupported schema version" error rather than guessing.
#[test]
fn unsupported_schema_version_is_a_clear_error() {
    let result = load_pack(&fixture("invalid/unsupported_schema"));
    assert!(matches!(result, Err(ManifestError::UnsupportedSchemaVersion { found: 99, .. })));
}

/// FR-006a: a manifest entry that resolves outside the pack directory (`..` traversal)
/// is rejected rather than read.
#[test]
fn path_traversal_is_rejected() {
    let result = load_pack(&fixture("invalid/path_traversal"));
    match result {
        Err(ManifestError::PathEscapesPackDirectory { file }) => {
            assert_eq!(file, "../secret_outside.png")
        }
        other => panic!("expected PathEscapesPackDirectory, got {other:?}"),
    }
}

/// spec.md Edge Cases: an image file that exists but is corrupt/unreadable fails with a
/// clear error naming the file, same contained-failure posture as a missing file.
#[test]
fn unreadable_image_is_a_clear_error() {
    let result = load_pack(&fixture("invalid/unreadable_image"));
    match result {
        Err(ManifestError::UnreadableImage { file, .. }) => assert_eq!(file, "broken.png"),
        other => panic!("expected UnreadableImage, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------------
// User Story 2 — Zero-Config Static Wallpaper
// ---------------------------------------------------------------------------------

/// Scenario 1: pointing the loader at a single valid image file with no manifest
/// produces a static one-image pack with no time anchors.
#[test]
fn us2_scenario1_single_file_loads_as_static_pack() {
    let loaded = load_pack(&fixture("static_image/photo.png")).expect("valid image should load");
    assert_eq!(loaded.pack.len(), 1);
    assert!(loaded.pack.is_static());
}

/// Scenario 2: a file that isn't a readable image fails with a clear error rather than
/// silently producing a broken pack.
#[test]
fn us2_scenario2_unreadable_static_file_is_a_clear_error() {
    let result = load_pack(&fixture("invalid/not_an_image.dat"));
    assert!(matches!(result, Err(ManifestError::UnreadableImage { .. })));
}

// ---------------------------------------------------------------------------------
// User Story 3 — Configure Scaling & Fit Behavior
// ---------------------------------------------------------------------------------

/// Scenario 1 & 2: a pack-level default scaling mode applies to unoverridden images,
/// while an image with its own `scaling` entry reports that override instead.
#[test]
fn us3_scenarios1_2_pack_default_and_per_image_override() {
    let loaded = load_pack(&fixture("scaling_overrides")).expect("valid fixture should load");
    assert_eq!(loaded.default_scaling, ScalingMode::Fill);

    let wide_id = loaded.image_paths.keys().find(|id| id.as_str() == "wide.png").unwrap();
    let tall_id = loaded.image_paths.keys().find(|id| id.as_str() == "tall.png").unwrap();

    // wide.png has no per-image override, so it reports the pack default (Fill).
    assert_eq!(loaded.image_scaling[wide_id], ScalingMode::Fill);
    // tall.png declares its own "Fit" override.
    assert_eq!(loaded.image_scaling[tall_id], ScalingMode::Fit);
}

/// Scenario 3: an invalid scaling mode name fails loading with a clear error.
#[test]
fn us3_scenario3_invalid_scaling_mode_is_a_clear_error() {
    let result = load_pack(&fixture("invalid/invalid_scaling_mode"));
    match result {
        Err(ManifestError::InvalidScalingMode { value }) => assert_eq!(value, "Zoom"),
        other => panic!("expected InvalidScalingMode, got {other:?}"),
    }
}

/// Scenario 3: a malformed fallback color value fails loading with a clear error.
#[test]
fn us3_scenario3_malformed_color_is_a_clear_error() {
    let result = load_pack(&fixture("invalid/malformed_color"));
    assert!(matches!(result, Err(ManifestError::InvalidColor { .. })));
}
