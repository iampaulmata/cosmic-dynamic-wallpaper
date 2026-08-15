//! Pure, read-time-only derivations from an already-registered pack's on-disk content
//! (spec 008 data-model.md, research.md R2/R7) — the single source both
//! `pages::packs` and `pages::assignment` call, instead of each independently
//! deciding what "the pack's name"/"the pack's thumbnail" means.
//!
//! Neither function here adds a new failure mode beyond `pack_loader::load_pack`'s own
//! (constitution Principle VIII) — a failed load simply yields `None`, and every caller
//! shows a clearly-labeled placeholder for that case (spec.md FR-011/FR-020).

use std::path::{Path, PathBuf};

use chrono::{DateTime, Local, TimeDelta};
use pack_loader::PackSource;
use schedule_engine::{AnchorKind, Location, SolarEventKind, TimeAnchor};

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

/// The pack's author/license note, if the manifest declares one (a static pack, with
/// no manifest, never has one). `None` both when there's no note and when the pack
/// can't be loaded at all — callers show a placeholder for either case.
pub fn resolve_pack_author(source: &PackSource) -> Option<String> {
    pack_loader::load_pack(source.path()).ok()?.author
}

/// What the Timeline page needs for one output's live schedule: the current and next
/// image's thumbnail paths, and when the next one goes live.
#[derive(Debug, Clone, PartialEq)]
pub struct ScheduleSnapshot {
    pub current_thumbnail: Option<PathBuf>,
    pub next_thumbnail: Option<PathBuf>,
    pub next_transition_at: Option<DateTime<Local>>,
}

/// Resolves `source`'s current/next image thumbnails and next-update time, entirely
/// client-side via `schedule_engine`'s own public `query` (the identical deterministic
/// algorithm `wallpaperd` itself runs) rather than round-tripping through the daemon —
/// the D-Bus interface reports the *current* image and *when* the next transition
/// happens, but never *which* image comes next, so there's nothing to read that from
/// there; this needs only what's already on hand locally (the assigned pack and the
/// effective location), the same inputs the daemon itself would use.
///
/// `None` when the pack can't be loaded, or — mirroring
/// `renderer::scheduler_bridge::evaluate`'s own documented fix for the same hazard —
/// when it's solar-anchored but no location is available yet: `ValidatedPack::query`
/// panics on that exact combination, so this checks the anchor kind first rather than
/// ever calling into it.
pub fn resolve_schedule_snapshot(
    source: &PackSource,
    location: Option<&Location>,
    crossfade_duration: TimeDelta,
    at: DateTime<Local>,
) -> Option<ScheduleSnapshot> {
    let loaded = pack_loader::load_pack(source.path()).ok()?;
    if loaded.pack.anchor_kind() == AnchorKind::Solar && location.is_none() {
        return None;
    }
    let result = loaded.pack.query(location, at, crossfade_duration);
    let current_id = match &result.transition {
        Some(t) => &t.incoming,
        None => &result.active_before,
    };
    Some(ScheduleSnapshot {
        current_thumbnail: loaded.image_paths.get(current_id).cloned(),
        next_thumbnail: result.next_image.as_ref().and_then(|id| loaded.image_paths.get(id).cloned()),
        next_transition_at: result.next_transition_at,
    })
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
    fn resolve_pack_author_returns_the_manifest_author_for_a_directory_pack() {
        let dir = tempfile::tempdir().unwrap();
        let pack_dir = dir.path().join("mountains");
        write_pack_dir(
            &pack_dir,
            r##"
                schema_version = 1
                name = "Mountains"
                author = "Jane Author"
                default_scaling = "Fill"
                fallback_color = "#000000"
                [[images]]
                file = "noon.png"
                anchor = "solar_noon"
            "##,
            &["noon.png"],
        );
        let source = PackSource::resolve(&pack_dir).unwrap();

        assert_eq!(resolve_pack_author(&source), Some("Jane Author".to_string()));
    }

    #[test]
    fn resolve_pack_author_is_none_when_the_manifest_has_no_author() {
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

        assert_eq!(resolve_pack_author(&source), None);
    }

    #[test]
    fn resolve_pack_author_is_none_for_a_static_file_pack() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("sunrise.png");
        image::RgbImage::new(2, 2).save(&file).unwrap();
        let source = PackSource::resolve(&file).unwrap();

        assert_eq!(resolve_pack_author(&source), None);
    }

    #[test]
    fn resolve_pack_author_is_none_when_load_pack_fails() {
        let dir = tempfile::tempdir().unwrap();
        let source = PackSource::Directory(dir.path().to_path_buf());

        assert_eq!(resolve_pack_author(&source), None);
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

    fn local_at(hh: u32, mm: u32) -> DateTime<Local> {
        use chrono::TimeZone;
        let today = Local::now().date_naive();
        Local.from_local_datetime(&today.and_hms_opt(hh, mm, 0).unwrap()).single().unwrap()
    }

    #[test]
    fn resolve_schedule_snapshot_reports_current_and_next_thumbnails_outside_a_transition() {
        let dir = tempfile::tempdir().unwrap();
        let pack_dir = dir.path().join("clock-pack");
        write_pack_dir(
            &pack_dir,
            r##"
                schema_version = 1
                name = "Clock Pack"
                default_scaling = "Fill"
                fallback_color = "#000000"
                [[images]]
                file = "dawn.png"
                anchor = "06:00"
                [[images]]
                file = "dusk.png"
                anchor = "18:00"
            "##,
            &["dawn.png", "dusk.png"],
        );
        let source = PackSource::resolve(&pack_dir).unwrap();

        // Well clear of both anchors' crossfade windows, between dawn and dusk.
        let snapshot = resolve_schedule_snapshot(&source, None, TimeDelta::minutes(1), local_at(12, 0)).unwrap();
        assert_eq!(snapshot.current_thumbnail.as_ref().and_then(|p| p.file_name()), Some(std::ffi::OsStr::new("dawn.png")));
        assert_eq!(snapshot.next_thumbnail.as_ref().and_then(|p| p.file_name()), Some(std::ffi::OsStr::new("dusk.png")));
        assert!(snapshot.next_transition_at.is_some());
    }

    #[test]
    fn resolve_schedule_snapshot_reports_the_incoming_image_mid_transition() {
        let dir = tempfile::tempdir().unwrap();
        let pack_dir = dir.path().join("clock-pack");
        write_pack_dir(
            &pack_dir,
            r##"
                schema_version = 1
                name = "Clock Pack"
                default_scaling = "Fill"
                fallback_color = "#000000"
                [[images]]
                file = "dawn.png"
                anchor = "06:00"
                [[images]]
                file = "dusk.png"
                anchor = "18:00"
            "##,
            &["dawn.png", "dusk.png"],
        );
        let source = PackSource::resolve(&pack_dir).unwrap();

        // 5 minutes into an hour-long crossfade starting at dusk (18:00): the current
        // thumbnail is the incoming image, not the outgoing one.
        let snapshot = resolve_schedule_snapshot(&source, None, TimeDelta::hours(1), local_at(18, 5)).unwrap();
        assert_eq!(snapshot.current_thumbnail.as_ref().and_then(|p| p.file_name()), Some(std::ffi::OsStr::new("dusk.png")));
    }

    #[test]
    fn resolve_schedule_snapshot_is_none_for_a_solar_pack_with_no_location() {
        let dir = tempfile::tempdir().unwrap();
        let pack_dir = dir.path().join("solar-pack");
        write_pack_dir(
            &pack_dir,
            r##"
                schema_version = 1
                name = "Solar Pack"
                default_scaling = "Fill"
                fallback_color = "#000000"
                [[images]]
                file = "noon.png"
                anchor = "solar_noon"
            "##,
            &["noon.png"],
        );
        let source = PackSource::resolve(&pack_dir).unwrap();

        // Must not panic (`ValidatedPack::query` panics on solar + no location) —
        // just degrades to `None`, same as a load failure.
        assert_eq!(resolve_schedule_snapshot(&source, None, TimeDelta::minutes(1), local_at(12, 0)), None);
    }

    #[test]
    fn resolve_schedule_snapshot_resolves_a_solar_pack_when_a_location_is_given() {
        let dir = tempfile::tempdir().unwrap();
        let pack_dir = dir.path().join("solar-pack");
        write_pack_dir(
            &pack_dir,
            r##"
                schema_version = 1
                name = "Solar Pack"
                default_scaling = "Fill"
                fallback_color = "#000000"
                [[images]]
                file = "sunrise.png"
                anchor = "sunrise"
                [[images]]
                file = "noon.png"
                anchor = "solar_noon"
                [[images]]
                file = "sunset.png"
                anchor = "sunset"
            "##,
            &["sunrise.png", "noon.png", "sunset.png"],
        );
        let source = PackSource::resolve(&pack_dir).unwrap();
        let toronto = Location::new(43.6532, -79.3832).unwrap();

        let snapshot = resolve_schedule_snapshot(&source, Some(&toronto), TimeDelta::minutes(1), local_at(12, 0));
        assert!(snapshot.is_some());
    }

    #[test]
    fn resolve_schedule_snapshot_is_none_when_load_pack_fails() {
        let dir = tempfile::tempdir().unwrap();
        let source = PackSource::Directory(dir.path().to_path_buf());

        assert_eq!(resolve_schedule_snapshot(&source, None, TimeDelta::minutes(1), local_at(12, 0)), None);
    }
}
