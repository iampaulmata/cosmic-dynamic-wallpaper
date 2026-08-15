//! Acceptance scenario tests from spec.md's user stories, as integration tests against
//! the crate's public API (no internal access beyond the `testing` accuracy helper used
//! to build exact expected instants for scenario arrangement — not to verify solar
//! accuracy, that's `solar_accuracy.rs`'s job).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use chrono::{NaiveDate, TimeDelta};
use schedule_engine::{
    Location, PackError, PackImage, SolarEventKind, TimeAnchor, WallpaperPack,
};

fn toronto() -> Location {
    Location::new(43.6532, -79.3832).unwrap()
}

fn jan1_2016() -> NaiveDate {
    NaiveDate::from_ymd_opt(2016, 1, 1).unwrap()
}

fn solar_instant(loc: &Location, date: NaiveDate, event: SolarEventKind) -> chrono::DateTime<chrono::Local> {
    schedule_engine::testing::solar_event_instant(loc, date, event).expect("occurs at this location/date")
}

// ---------------------------------------------------------------------------------
// User Story 1 — Solar-Anchored Schedule Resolves Correctly for a Location
// ---------------------------------------------------------------------------------

/// Scenario 1: queried strictly between two anchors and outside any crossfade window,
/// the engine returns the most-recently-passed anchor's image with no transition.
#[test]
fn us1_scenario1_mid_period_resolution() {
    let loc = toronto();
    let date = jan1_2016();
    let images = vec![
        PackImage::new("sunrise_img", TimeAnchor::Solar { event: SolarEventKind::Sunrise, offset: None }),
        PackImage::new("noon_img", TimeAnchor::Solar { event: SolarEventKind::SolarNoon, offset: None }),
        PackImage::new("sunset_img", TimeAnchor::Solar { event: SolarEventKind::Sunset, offset: None }),
    ];
    let pack = WallpaperPack::validate(images).unwrap();
    let crossfade = TimeDelta::minutes(5);

    let noon = solar_instant(&loc, date, SolarEventKind::SolarNoon);
    let sunset = solar_instant(&loc, date, SolarEventKind::Sunset);
    assert!(sunset - noon > TimeDelta::hours(1), "fixture assumption: plenty of gap after noon");
    let at = noon + TimeDelta::hours(1);

    let result = pack.query(Some(&loc), at, crossfade);
    assert_eq!(result.active_before.as_str(), "noon_img");
    assert!(result.transition.is_none());
    // The image that will be active once `next_transition_at` (sunset) arrives.
    assert_eq!(result.next_image.as_ref().map(|i| i.as_str()), Some("sunset_img"));
}

/// Scenario 2: an image anchored to `civil_dawn - 30m`, queried at exactly that offset
/// instant, reports the transition into that image has just begun (progress 0.0).
#[test]
fn us1_scenario2_offset_anchor_transition_start() {
    let loc = toronto();
    let date = jan1_2016();
    let civil_dawn = solar_instant(&loc, date, SolarEventKind::CivilDawn);
    let offset_instant = civil_dawn - TimeDelta::minutes(30);

    let images = vec![
        PackImage::new("before", TimeAnchor::Solar { event: SolarEventKind::AstronomicalDawn, offset: None }),
        PackImage::new(
            "dawn_offset",
            TimeAnchor::Solar { event: SolarEventKind::CivilDawn, offset: Some(-TimeDelta::minutes(30)) },
        ),
    ];
    let pack = WallpaperPack::validate(images).unwrap();
    let crossfade = TimeDelta::minutes(10);

    let result = pack.query(Some(&loc), offset_instant, crossfade);
    let transition = result.transition.expect("transition should have begun exactly at the anchor instant");
    assert_eq!(transition.incoming.as_str(), "dawn_offset");
    assert_eq!(transition.outgoing.as_str(), "before");
    assert!(transition.progress.abs() < 1e-9, "progress should be ~0.0, was {}", transition.progress);
    // Mid-transition, `next_image` is the incoming image — the one becoming fully
    // active once `next_transition_at` (this transition's end) arrives.
    assert_eq!(result.next_image.as_ref().map(|i| i.as_str()), Some("dawn_offset"));
}

/// Scenario 3: a timestamp inside a crossfade window reports outgoing, incoming, and a
/// progress fraction strictly between 0.0 and 1.0.
#[test]
fn us1_scenario3_in_window_progress_fraction() {
    let loc = toronto();
    let date = jan1_2016();
    let civil_dawn = solar_instant(&loc, date, SolarEventKind::CivilDawn);
    let offset_instant = civil_dawn - TimeDelta::minutes(30);

    let images = vec![
        PackImage::new("before", TimeAnchor::Solar { event: SolarEventKind::AstronomicalDawn, offset: None }),
        PackImage::new(
            "dawn_offset",
            TimeAnchor::Solar { event: SolarEventKind::CivilDawn, offset: Some(-TimeDelta::minutes(30)) },
        ),
    ];
    let pack = WallpaperPack::validate(images).unwrap();
    let crossfade = TimeDelta::minutes(10);

    let at = offset_instant + TimeDelta::minutes(5);
    let result = pack.query(Some(&loc), at, crossfade);
    let transition = result.transition.expect("mid-window");
    assert_eq!(transition.outgoing.as_str(), "before");
    assert_eq!(transition.incoming.as_str(), "dawn_offset");
    assert!(transition.progress > 0.0 && transition.progress < 1.0);
    assert!((transition.progress - 0.5).abs() < 0.01, "expected ~0.5, was {}", transition.progress);
}

/// FR-007: when a solar event doesn't occur for the date/location (polar night), the
/// engine skips that day's missing anchor and holds the adjacent image active straight
/// through, rather than erroring or crashing.
#[test]
fn fr007_polar_night_holds_adjacent_image() {
    // Svalbard: sun does not rise at all around the winter solstice.
    let loc = Location::new(78.2232, 15.6267).unwrap();
    let deep_winter_noon_local = {
        use chrono::TimeZone;
        chrono::Local.from_local_datetime(
            &NaiveDate::from_ymd_opt(2016, 12, 21).unwrap().and_hms_opt(12, 0, 0).unwrap(),
        ).single().expect("unambiguous")
    };

    let images = vec![
        PackImage::new("day_img", TimeAnchor::Solar { event: SolarEventKind::Sunrise, offset: None }),
        PackImage::new("night_img", TimeAnchor::Solar { event: SolarEventKind::Sunset, offset: None }),
    ];
    let pack = WallpaperPack::validate(images).unwrap();
    let crossfade = TimeDelta::minutes(30);

    // Must not panic, and must return a well-formed result even though neither anchor
    // occurs on this date.
    let result = pack.query(Some(&loc), deep_winter_noon_local, crossfade);
    assert!(result.active_before.as_str() == "day_img" || result.active_before.as_str() == "night_img");
    // The next real transition is whenever the sun starts rising/setting again —
    // strictly after the query instant.
    if let Some(next) = result.next_transition_at {
        assert!(next > deep_winter_noon_local);
    }
}

/// FR-006a: two anchors resolving to the exact same instant on a given date are
/// rejected — for solar packs this can only be checked per-date (data-model.md rule 4),
/// via the dedicated check rather than at structural `validate` time.
#[test]
fn fr006a_solar_pack_duplicate_instant_is_rejected_for_the_colliding_date() {
    let loc = toronto();
    let date = jan1_2016();
    let images = vec![
        PackImage::new("a", TimeAnchor::Solar { event: SolarEventKind::Sunrise, offset: None }),
        PackImage::new("b", TimeAnchor::Solar { event: SolarEventKind::Sunrise, offset: None }),
    ];
    // Structurally valid: same anchor kind throughout, distinct image ids.
    let pack = WallpaperPack::validate(images).unwrap();
    assert_eq!(pack.check_solar_duplicate_instant(&loc, date), Err(PackError::DuplicateInstant));
}

// ---------------------------------------------------------------------------------
// User Story 2 — Fully Manual, Location-Free Clock Schedule
// ---------------------------------------------------------------------------------

fn clock_pack(anchors: &[(&str, u32, u32)]) -> schedule_engine::ValidatedPack {
    let images = anchors
        .iter()
        .map(|(id, hh, mm)| {
            PackImage::new(*id, TimeAnchor::Clock(chrono::NaiveTime::from_hms_opt(*hh, *mm, 0).unwrap()))
        })
        .collect();
    WallpaperPack::validate(images).unwrap()
}

fn local_at(date: NaiveDate, hh: u32, mm: u32) -> chrono::DateTime<chrono::Local> {
    use chrono::TimeZone;
    chrono::Local
        .from_local_datetime(&date.and_hms_opt(hh, mm, 0).unwrap())
        .single()
        .expect("unambiguous wall-clock time")
}

/// Scenario 1: clock-time anchors `06:00` (A), `12:00` (B), `20:00` (C), queried at
/// `15:00`, reports image B active.
#[test]
fn us2_scenario1_clock_resolution() {
    let pack = clock_pack(&[("a", 6, 0), ("b", 12, 0), ("c", 20, 0)]);
    let at = local_at(jan1_2016(), 15, 0);
    let result = pack.query(None, at, TimeDelta::minutes(5));
    assert_eq!(result.active_before.as_str(), "b");
    assert!(result.transition.is_none());
    assert_eq!(result.next_image.as_ref().map(|i| i.as_str()), Some("c"));
}

/// Scenario 2: a clock-only pack returns a valid result with `location: None` — no
/// location is required, requested, or read anywhere on this path (FR-003).
#[test]
fn us2_scenario2_zero_location_required() {
    let pack = clock_pack(&[("a", 6, 0), ("b", 12, 0), ("c", 20, 0)]);
    let at = local_at(jan1_2016(), 3, 0); // before the first anchor — exercises wraparound too
    let result = pack.query(None, at, TimeDelta::minutes(5));
    // Just needs to resolve to *some* valid image without panicking on the `None`
    // location — FR-009's midnight wraparound means this is yesterday's last anchor.
    assert_eq!(result.active_before.as_str(), "c");
}

/// Scenario 3: a pack manifest mixing solar and clock anchors is rejected at
/// validation time with a clear error, not a partial/ambiguous schedule.
#[test]
fn us2_scenario3_mixed_anchor_types_rejected() {
    let images = vec![
        PackImage::new("solar", TimeAnchor::Solar { event: SolarEventKind::Sunrise, offset: None }),
        PackImage::new("clock", TimeAnchor::Clock(chrono::NaiveTime::from_hms_opt(8, 0, 0).unwrap())),
    ];
    assert_eq!(WallpaperPack::validate(images), Err(PackError::MixedAnchorTypes));
}

/// DST edge case (spec.md Edge Cases): querying across the dates a DST-observing
/// system clock would transition on must not crash or produce a non-deterministic
/// result. Written host-timezone-agnostically (asserts determinism/no-panic rather than
/// a specific UTC-offset jump) since whether *this* test host's `Local` timezone
/// actually observes DST on these dates isn't something a portable test can assume.
#[test]
fn dst_transition_dates_do_not_crash_and_stay_deterministic() {
    let pack = clock_pack(&[("a", 2, 30), ("b", 14, 0)]);
    // 2016's US spring-forward (Mar 13) and fall-back (Nov 6) dates — arbitrary but
    // real DST-transition dates in a commonly-configured zone; harmless no-ops on a
    // host whose `Local` doesn't observe DST at all.
    for date in [
        NaiveDate::from_ymd_opt(2016, 3, 13).unwrap(),
        NaiveDate::from_ymd_opt(2016, 11, 6).unwrap(),
    ] {
        let at = local_at(date, 12, 0);
        let first = pack.query(None, at, TimeDelta::minutes(5));
        let second = pack.query(None, at, TimeDelta::minutes(5));
        assert_eq!(first, second, "SC-003: identical inputs must produce identical results");
    }
}

/// FR-006a for clock packs: this is a static, one-time structural check (unlike solar
/// packs' per-date check) since a `NaiveTime` doesn't depend on any resolved date.
#[test]
fn fr006a_clock_pack_duplicate_instant_is_rejected_at_validate_time() {
    let images = vec![
        PackImage::new("a", TimeAnchor::Clock(chrono::NaiveTime::from_hms_opt(8, 0, 0).unwrap())),
        PackImage::new("b", TimeAnchor::Clock(chrono::NaiveTime::from_hms_opt(8, 0, 0).unwrap())),
    ];
    assert_eq!(WallpaperPack::validate(images), Err(PackError::DuplicateInstant));
}

// ---------------------------------------------------------------------------------
// User Story 3 — Deterministic State Query for Downstream Consumers: edge cases
// (property-based determinism/monotonicity coverage lives in tests/determinism.rs)
// ---------------------------------------------------------------------------------

/// Edge Cases: a single-image (degenerate static-mode) pack is always active, with no
/// transition ever reported and no meaningful next-transition instant.
#[test]
fn single_image_pack_is_always_active_with_no_transition() {
    let images = vec![PackImage::new(
        "only",
        TimeAnchor::Clock(chrono::NaiveTime::from_hms_opt(6, 0, 0).unwrap()),
    )];
    let pack = WallpaperPack::validate(images).unwrap();

    for hh in [0, 6, 12, 18, 23] {
        let at = local_at(jan1_2016(), hh, 0);
        let result = pack.query(None, at, TimeDelta::minutes(5));
        assert_eq!(result.active_before.as_str(), "only");
        assert!(result.transition.is_none());
        assert!(result.next_transition_at.is_none());
        assert!(result.next_image.is_none(), "no next image for a pack that never transitions");
    }
    assert!(pack.next_transition_after(None, local_at(jan1_2016(), 6, 0)).is_none());
}

/// Edge Cases: when two consecutive anchors are closer together than the configured
/// crossfade duration, progress must still be well-defined — always in `[0.0, 1.0)`,
/// monotonic *within* whichever window is currently active — rather than a
/// discontinuity or out-of-range fraction. (Each anchor always owns its own single
/// current window in this engine's model — see query.rs's module doc — so an
/// overlapping predecessor window never produces two simultaneously "active"
/// transitions to reconcile.)
#[test]
fn overlapping_crossfade_windows_produce_well_defined_progress() {
    let images = vec![
        PackImage::new("a", TimeAnchor::Clock(chrono::NaiveTime::from_hms_opt(10, 0, 0).unwrap())),
        PackImage::new("b", TimeAnchor::Clock(chrono::NaiveTime::from_hms_opt(10, 10, 0).unwrap())),
    ];
    let pack = WallpaperPack::validate(images).unwrap();
    let crossfade = TimeDelta::minutes(20); // wider than the 10-minute gap: windows overlap

    let date = jan1_2016();
    let anchor_a = local_at(date, 10, 0);
    let anchor_b = local_at(date, 10, 10);

    // Inside A's window, before B starts: outgoing is yesterday's "b", incoming "a".
    let mid_a = pack.query(None, anchor_a + TimeDelta::minutes(5), crossfade);
    let t = mid_a.transition.expect("inside a's window");
    assert!((0.0..1.0).contains(&t.progress));

    // Exactly at B: a fresh window starts (progress resets to 0), even though A's own
    // window hadn't finished yet — well-defined, not a panic or out-of-range value.
    let at_b = pack.query(None, anchor_b, crossfade);
    let t = at_b.transition.expect("b's window begins exactly at its own anchor");
    assert_eq!(t.outgoing.as_str(), "a");
    assert_eq!(t.incoming.as_str(), "b");
    assert!(t.progress.abs() < 1e-9);

    // Partway through B's (overlapping) window: still well-defined and in range.
    let mid_b = pack.query(None, anchor_b + TimeDelta::minutes(10), crossfade);
    let t = mid_b.transition.expect("inside b's window");
    assert!((0.0..1.0).contains(&t.progress));
    assert!((t.progress - 0.5).abs() < 0.01);
}
