//! User Story 3 property tests (SC-003): identical inputs produce identical outputs,
//! and crossfade progress is monotonic and stays within `[0.0, 1.0)`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use chrono::{NaiveDate, NaiveTime, TimeDelta, TimeZone};
use proptest::prelude::*;
use schedule_engine::{PackImage, TimeAnchor, ValidatedPack, WallpaperPack};

fn day() -> NaiveDate {
    NaiveDate::from_ymd_opt(2016, 1, 1).unwrap()
}

fn local_at(date: NaiveDate, minutes_from_midnight: i64) -> chrono::DateTime<chrono::Local> {
    let hh = (minutes_from_midnight / 60) as u32;
    let mm = (minutes_from_midnight % 60) as u32;
    chrono::Local
        .from_local_datetime(&date.and_time(NaiveTime::from_hms_opt(hh, mm, 0).unwrap()))
        .single()
        .expect("unambiguous")
}

/// Build an evenly-spaced clock-anchored pack of `count` images across a 1440-minute
/// day, plus a crossfade duration comfortably smaller than the gap between anchors
/// (non-overlapping windows, so progress is monotonic within each one by construction).
fn evenly_spaced_pack(count: usize) -> (ValidatedPack, TimeDelta, i64) {
    let gap_minutes = 1440 / count as i64;
    let images: Vec<_> = (0..count)
        .map(|i| {
            let total = (i as i64) * gap_minutes;
            let hh = (total / 60) as u32;
            let mm = (total % 60) as u32;
            PackImage::new(
                format!("img{i}"),
                TimeAnchor::Clock(NaiveTime::from_hms_opt(hh, mm, 0).unwrap()),
            )
        })
        .collect();
    let pack = WallpaperPack::validate(images).expect("evenly spaced, no duplicates for count <= 1440");
    let crossfade = TimeDelta::minutes(gap_minutes / 3);
    (pack, crossfade, gap_minutes)
}

proptest! {
    /// SC-003: identical `(pack, location, at, crossfade_duration)` always returns an
    /// identical result, across arbitrary packs and query instants.
    #[test]
    fn query_is_deterministic(count in 2usize..=8, minutes in 0i64..1440, extra_ms in 0i64..600_000) {
        let (pack, crossfade, _) = evenly_spaced_pack(count);
        let at = local_at(day(), minutes) + TimeDelta::milliseconds(extra_ms);

        let first = pack.query(None, at, crossfade);
        let second = pack.query(None, at, crossfade);
        prop_assert_eq!(first, second);
    }

    /// SC-003: `next_transition_after` agrees with itself across repeated calls too.
    #[test]
    fn next_transition_after_is_deterministic(count in 2usize..=8, minutes in 0i64..1440) {
        let (pack, _, _) = evenly_spaced_pack(count);
        let at = local_at(day(), minutes);

        let first = pack.next_transition_after(None, at);
        let second = pack.next_transition_after(None, at);
        prop_assert_eq!(first, second);
    }

    /// Progress is monotonic non-decreasing as the query instant advances through a
    /// single (non-overlapping, by construction) crossfade window, and always stays in
    /// `[0.0, 1.0)`.
    #[test]
    fn progress_is_monotonic_within_a_window(
        count in 2usize..=6,
        anchor_idx in 1usize..6,
        t1_frac in 0.0f64..1.0,
        t2_frac in 0.0f64..1.0,
    ) {
        let (pack, crossfade, gap_minutes) = evenly_spaced_pack(count);
        let anchor_idx = anchor_idx % count;
        prop_assume!(anchor_idx >= 1); // need a predecessor in the same day for a clean window

        let anchor_instant = local_at(day(), anchor_idx as i64 * gap_minutes);
        let crossfade_ms = crossfade.num_milliseconds();

        let (lo, hi) = if t1_frac <= t2_frac { (t1_frac, t2_frac) } else { (t2_frac, t1_frac) };
        let at1 = anchor_instant + TimeDelta::milliseconds((lo * crossfade_ms as f64) as i64);
        let at2 = anchor_instant + TimeDelta::milliseconds((hi * crossfade_ms as f64) as i64);

        let r1 = pack.query(None, at1, crossfade);
        let r2 = pack.query(None, at2, crossfade);

        let p1 = r1.transition.as_ref().map(|t| t.progress).unwrap_or(0.0);
        let p2 = r2.transition.as_ref().map(|t| t.progress).unwrap_or(0.0);

        prop_assert!((0.0..1.0).contains(&p1));
        prop_assert!((0.0..1.0).contains(&p2));
        prop_assert!(p1 <= p2 + 1e-9, "progress should be monotonic: p1={p1} at {at1}, p2={p2} at {at2}");
    }
}
