//! Bridges [`crate::output::OutputAssignment`] resolution, a loaded pack, and a
//! manually-provided location into spec 1's `ScheduleQueryResult` for one output right
//! now (FR-015). Pure logic — no Wayland/GPU/D-Bus involved; the real daemon's job is
//! to feed this function's result into the crossfade/idle-wait machinery (`crossfade.rs`,
//! `output.rs`), which this pass doesn't wire up (see `README.md`).
//!
//! **Real bug found and fixed while implementing this bridge**: spec 1's
//! `ValidatedPack::query`/`next_transition_after` *panic* if called with
//! `location: None` on a solar-anchored pack — documented in
//! contracts/schedule-engine-api.md as a caller contract violation, since spec 1's own
//! validation already guarantees a pack's anchor kind is known before query() can be
//! called at all. But this daemon *can* legitimately reach that state at runtime (a
//! solar-anchored pack assigned to an output before any location is ever configured,
//! FR-015's own Edge Case) — calling query() naively there would crash the whole
//! daemon, not just degrade the one output, directly violating FR-013. This module
//! checks the pack's anchor kind *before* ever calling into spec 1, so that condition
//! becomes [`RendererError::LocationRequired`] (a per-output degrade) instead of a
//! panic.

use chrono::{DateTime, Local, TimeDelta};

use pack_loader::LoadedPack;
use schedule_engine::{AnchorKind, Location, ScheduleQueryResult};

use crate::error::RendererError;
use crate::output::OutputId;

/// Evaluate what one output should currently show, given the pack it's assigned (if
/// any), the current manual location (if any), and a query instant.
///
/// - `pack: None` (the output's `OutputAssignment` resolved to `Unassigned`) →
///   `Ok(None)` — a well-defined empty state, not an error (FR-009).
/// - A solar-anchored pack with no location available → `Err(LocationRequired)` —
///   degrades this one output rather than panicking (see module doc).
/// - Otherwise → `Ok(Some(result))`, spec 1's own deterministic answer.
pub fn evaluate(
    output: &OutputId,
    pack: Option<&LoadedPack>,
    location: Option<&Location>,
    at: DateTime<Local>,
    crossfade_duration: TimeDelta,
) -> Result<Option<ScheduleQueryResult>, RendererError> {
    let Some(pack) = pack else {
        return Ok(None);
    };

    if pack.pack.anchor_kind() == AnchorKind::Solar && location.is_none() {
        return Err(RendererError::LocationRequired { output: output.clone() });
    }

    Ok(Some(pack.pack.query(location, at, crossfade_duration)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pack_loader::{PackSource, ScalingMode};
    use schedule_engine::{PackImage, SolarEventKind, TimeAnchor, ValidatedPack, WallpaperPack};
    use std::collections::HashMap;

    fn loaded_pack(pack: ValidatedPack) -> LoadedPack {
        LoadedPack {
            source: PackSource::StaticFile("/x.jpg".into()),
            name: "test".to_string(),
            author: None,
            default_scaling: ScalingMode::Fill,
            fallback_color: pack_loader::Color { r: 0, g: 0, b: 0, a: 255 },
            pack,
            image_paths: HashMap::new(),
            image_scaling: HashMap::new(),
        }
    }

    fn solar_pack() -> ValidatedPack {
        WallpaperPack::validate(vec![
            PackImage::new("a", TimeAnchor::solar(SolarEventKind::Sunrise, None)),
            PackImage::new("b", TimeAnchor::solar(SolarEventKind::Sunset, None)),
        ])
        .unwrap()
    }

    fn clock_pack() -> ValidatedPack {
        use chrono::NaiveTime;
        WallpaperPack::validate(vec![
            PackImage::new("a", TimeAnchor::clock(NaiveTime::from_hms_opt(6, 0, 0).unwrap())),
            PackImage::new("b", TimeAnchor::clock(NaiveTime::from_hms_opt(18, 0, 0).unwrap())),
        ])
        .unwrap()
    }

    #[test]
    fn unassigned_output_is_ok_none() {
        let result = evaluate(&OutputId::new("DP-3"), None, None, Local::now(), TimeDelta::seconds(45));
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn solar_pack_without_location_degrades_this_output_only() {
        let loaded = loaded_pack(solar_pack());
        let result = evaluate(&OutputId::new("DP-3"), Some(&loaded), None, Local::now(), TimeDelta::seconds(45));
        assert!(matches!(result, Err(RendererError::LocationRequired { .. })));
    }

    #[test]
    fn solar_pack_with_location_resolves_normally() {
        let loaded = loaded_pack(solar_pack());
        let loc = Location::new(45.5019, -73.5674).unwrap();
        let result = evaluate(&OutputId::new("DP-3"), Some(&loaded), Some(&loc), Local::now(), TimeDelta::seconds(45));
        assert!(matches!(result, Ok(Some(_))));
    }

    #[test]
    fn clock_pack_never_needs_a_location() {
        let loaded = loaded_pack(clock_pack());
        let result = evaluate(&OutputId::new("DP-3"), Some(&loaded), None, Local::now(), TimeDelta::seconds(45));
        assert!(matches!(result, Ok(Some(_))));
    }
}
