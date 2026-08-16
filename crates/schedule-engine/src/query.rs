//! [`ScheduleQueryResult`] and [`TransitionState`] — the answer to "what's active right
//! now" (FR-004, FR-013, User Story 3; data-model.md) — plus the [`crate::ValidatedPack`]
//! resolution methods (`query`, `next_transition_after`) that produce them.
//!
//! ## Resolution model
//!
//! A pack's images cycle chronologically by anchor instant. Each anchor `T_i` opens a
//! crossfade-in window `[T_i, T_i + crossfade_duration)` during which the pack is
//! transitioning from the *previous* chronological image into image `i` (confirmed by
//! spec.md User Story 1 Acceptance Scenario 2: querying at exactly an anchor's instant
//! reports that the transition into that image has just begun, i.e. progress `0.0`).
//! Outside any such window, the most-recently-passed anchor's image is fully active.
//!
//! Both anchor kinds (solar, clock) share this exact engine — the only difference is how
//! a `(date, image index)` pair resolves to a concrete instant, or fails to (FR-007's
//! polar day/night gaps for solar; DST gaps for clock). That per-kind lookup is passed in
//! as a closure by [`crate::ValidatedPack::query`]/`next_transition_after`.

use chrono::{DateTime, Local, NaiveDate, TimeDelta};

use crate::anchor::TimeAnchor;
use crate::pack::{AnchorKind, ImageId, PackImage, ValidatedPack};
use crate::solar;
use crate::Location;

/// The result of querying a [`crate::ValidatedPack`] for a specific instant
/// (data-model.md `ScheduleQueryResult`).
#[derive(Debug, Clone, PartialEq)]
pub struct ScheduleQueryResult {
    /// The image belonging to the most-recently-passed anchor. Always present, even
    /// mid-transition — **do not read this as "the currently active image"**: while
    /// `transition` is `Some`, this is the *outgoing* image (the one fading out), not
    /// what's on screen right now (spec 011 US8 FR-047 — the name itself invites that
    /// misreading, since it doesn't distinguish "before the most recent anchor" from
    /// "before this transition finishes"). Outside a transition (`transition` is
    /// `None`), it genuinely is the currently active image — the ambiguity only bites
    /// during the crossfade window itself.
    pub active_before: ImageId,
    /// `Some` only when the query instant falls inside a crossfade window.
    pub transition: Option<TransitionState>,
    /// When the next transition begins. `None` only for the degenerate single-image/
    /// static-mode pack, which never transitions (Edge Cases).
    pub next_transition_at: Option<DateTime<Local>>,
    /// The image that will be active once `next_transition_at` arrives — `Some` and
    /// `None` in exact lockstep with `next_transition_at` (same degenerate-pack
    /// exception). Mid-transition this is the incoming image itself (the one
    /// `next_transition_at`'s instant marks as fully active); outside a transition
    /// it's the image belonging to the upcoming anchor.
    pub next_image: Option<ImageId>,
}

/// The crossfade in progress at a queried instant (data-model.md `TransitionState`).
#[derive(Debug, Clone, PartialEq)]
pub struct TransitionState {
    /// The image fading out.
    pub outgoing: ImageId,
    /// The image fading in.
    pub incoming: ImageId,
    /// Crossfade progress, `0.0 <= progress < 1.0`, strictly increasing across the
    /// window (FR-004).
    pub progress: f64,
}

/// The doubling threshold `resolve`'s radius-doubling search loop checks *before*
/// doubling again (see that loop's `if radius >= MAX_SEARCH_RADIUS_DAYS` / `radius *=
/// 2` ordering) — generous enough to cross any realistic multi-month polar night while
/// still terminating in bounded time (constitution Principle VIII: no unbounded
/// loops). **Corrected during implementation** (spec 011 US7 FR-039, research.md
/// R33): despite the name, this is *not* the true worst-case radius the search
/// actually explores — because the loop checks the threshold *before* doubling, the
/// last `build_timeline` call before giving up runs at `radius = 512`
/// (`1 -> 2 -> 4 -> ... -> 256 -> 512`, since `256 < 370` still doubles once more
/// before the `512 >= 370` check finally stops it), not `370`. This is a
/// documentation-accuracy fix only — the check-then-double ordering itself is
/// unchanged (deliberately: see research.md R33's Alternatives), and [`ValidatedPack::
/// query`]'s pole-latitude fast path ([`POLE_LATITUDE_THRESHOLD`]) is what actually
/// eliminates the case that used to hit this true worst case on every call.
const MAX_SEARCH_RADIUS_DAYS: i64 = 370;

/// A latitude this close to a pole (either) that no solar event (sunrise/sunset/etc.)
/// ever resolves for any date (spec 011 US7 FR-038, research.md R33) — the audit's own
/// reproduction: a query at a pole-latitude location previously ran `resolve`'s full
/// radius-doubling search out to its true worst case (see [`MAX_SEARCH_RADIUS_DAYS`]'s
/// doc comment) on *every single call*, for a location that can never succeed.
/// `89.9999` rather than exactly `90.0`/`-90.0` accounts for floating-point
/// representation — no real solar event resolves at exactly the pole either, but a
/// location a hair short of it is still practically the pole and deserves the same
/// fast path.
pub const POLE_LATITUDE_THRESHOLD: f64 = 89.9999;

/// Resolve `(date, image_index) -> instant`, or `None` if that image's anchor doesn't
/// occur on that date (FR-007 polar gaps; DST gaps for clock anchors).
type InstantFn<'a> = dyn Fn(NaiveDate, usize) -> Option<DateTime<Local>> + 'a;

fn build_timeline(
    center: NaiveDate,
    radius: i64,
    image_count: usize,
    instant_fn: &InstantFn,
) -> Vec<(DateTime<Local>, usize)> {
    let mut entries = Vec::new();
    let mut date = center - TimeDelta::days(radius);
    let end = center + TimeDelta::days(radius);
    while date <= end {
        for idx in 0..image_count {
            if let Some(instant) = instant_fn(date, idx) {
                entries.push((instant, idx));
            }
        }
        date += TimeDelta::days(1);
    }
    entries.sort_by_key(|(instant, _)| *instant);
    entries
}

/// Core cyclic resolution engine shared by both anchor kinds. See the module doc for the
/// window model.
fn resolve(
    images: &[PackImage],
    at: DateTime<Local>,
    crossfade_duration: TimeDelta,
    instant_fn: &InstantFn,
) -> ScheduleQueryResult {
    if images.len() == 1 {
        return ScheduleQueryResult {
            active_before: images[0].id.clone(),
            transition: None,
            next_transition_at: None,
            next_image: None,
        };
    }

    let center = at.date_naive();
    let mut radius = 1i64;
    loop {
        let entries = build_timeline(center, radius, images.len(), instant_fn);
        let pos = entries.partition_point(|(instant, _)| *instant <= at);
        let has_prev_prev = pos > 1;
        let has_prev = pos > 0;
        let has_next = pos < entries.len();

        if has_prev_prev && has_prev && has_next {
            let prev_prev = entries[pos - 2];
            let prev = entries[pos - 1];
            let next = entries[pos];
            return build_result(images, at, crossfade_duration, prev_prev, prev, next);
        }

        if radius >= MAX_SEARCH_RADIUS_DAYS {
            // Couldn't find enough surrounding anchors even after a wide search — every
            // realistic Earth latitude short of the poles resolves well within this
            // bound, so treat this as an extreme fallback rather than looping forever
            // or panicking (constitution Principle VIII).
            return fallback_result(images, &entries, at);
        }
        radius *= 2;
    }
}

fn build_result(
    images: &[PackImage],
    at: DateTime<Local>,
    crossfade_duration: TimeDelta,
    prev_prev: (DateTime<Local>, usize),
    prev: (DateTime<Local>, usize),
    next: (DateTime<Local>, usize),
) -> ScheduleQueryResult {
    let (prev_instant, prev_idx) = prev;
    let (_, prev_prev_idx) = prev_prev;
    let (next_instant, next_idx) = next;

    let transition_end = prev_instant + crossfade_duration;
    if crossfade_duration > TimeDelta::zero() && at < transition_end {
        let elapsed = at - prev_instant;
        let progress = progress_fraction(elapsed, crossfade_duration);
        ScheduleQueryResult {
            active_before: images[prev_prev_idx].id.clone(),
            transition: Some(TransitionState {
                outgoing: images[prev_prev_idx].id.clone(),
                incoming: images[prev_idx].id.clone(),
                progress,
            }),
            next_transition_at: Some(transition_end),
            // The incoming image is exactly what becomes active once this
            // transition ends at `transition_end`.
            next_image: Some(images[prev_idx].id.clone()),
        }
    } else {
        ScheduleQueryResult {
            active_before: images[prev_idx].id.clone(),
            transition: None,
            next_transition_at: Some(next_instant),
            next_image: Some(images[next_idx].id.clone()),
        }
    }
}

/// `elapsed / total`, clamped strictly inside `[0.0, 1.0)` (data-model.md
/// `TransitionState.progress`). Callers only invoke this when `elapsed < total` in
/// `DateTime` terms, but millisecond truncation could round the ratio up to exactly
/// `1.0` at the boundary, so this clamps defensively rather than ever reporting an
/// out-of-range fraction (spec.md Edge Cases).
fn progress_fraction(elapsed: TimeDelta, total: TimeDelta) -> f64 {
    let ratio = elapsed.num_milliseconds() as f64 / total.num_milliseconds() as f64;
    ratio.clamp(0.0, 1.0 - f64::EPSILON)
}

/// Extreme fallback when no surrounding anchors were found within
/// [`MAX_SEARCH_RADIUS_DAYS`] (see `resolve`). Picks the nearest known occurrence as
/// statically active; if literally nothing occurred in the search window, falls back to
/// the pack's first image so the daemon always has *something* to render rather than
/// erroring (constitution Principle VIII).
fn fallback_result(
    images: &[PackImage],
    entries: &[(DateTime<Local>, usize)],
    at: DateTime<Local>,
) -> ScheduleQueryResult {
    let idx = entries
        .iter()
        .min_by_key(|(instant, _)| (*instant - at).num_milliseconds().abs())
        .map(|(_, idx)| *idx)
        .unwrap_or(0);
    ScheduleQueryResult {
        active_before: images[idx].id.clone(),
        transition: None,
        next_transition_at: None,
        next_image: None,
    }
}

/// `at`'s wall-clock date combined with `time`, resolved to a concrete [`DateTime<Local>`].
///
/// Handles the two DST edge cases deterministically (research.md R2's pitfall,
/// spec.md's DST Edge Case): a nonexistent wall-clock time (spring-forward gap) is
/// treated as not occurring that date, same spirit as FR-007's solar gaps; an ambiguous
/// wall-clock time (fall-back overlap) deterministically picks the earlier of the two
/// possible instants (SC-003).
fn resolve_clock_instant(date: NaiveDate, time: chrono::NaiveTime) -> Option<DateTime<Local>> {
    use chrono::offset::LocalResult;
    match date.and_time(time).and_local_timezone(Local) {
        LocalResult::Single(dt) => Some(dt),
        LocalResult::Ambiguous(earliest, _latest) => Some(earliest),
        LocalResult::None => None,
    }
}

impl ValidatedPack {
    /// Answer "what's active right now" for this pack (FR-004, FR-013).
    ///
    /// `location` is required (`Some`) for solar-anchored packs and ignored for
    /// clock-anchored packs (FR-003). Passing `None` for a solar-anchored pack is a
    /// caller contract violation — see contracts/schedule-engine-api.md — since
    /// [`crate::WallpaperPack::validate`] already guarantees a pack's anchor kind before
    /// a [`ValidatedPack`] can exist at all.
    ///
    /// Pure and deterministic: identical inputs always produce an identical result
    /// (FR-004, SC-003) — `at` is the only source of "now".
    pub fn query(
        &self,
        location: Option<&Location>,
        at: DateTime<Local>,
        crossfade_duration: TimeDelta,
    ) -> ScheduleQueryResult {
        match self.anchor_kind() {
            AnchorKind::Solar => {
                let location = match location {
                    Some(loc) => loc,
                    None => panic!(
                        "ValidatedPack::query: a Location is required for solar-anchored packs"
                    ),
                };
                // Spec 011 US7 FR-038 (research.md R33): a pole-latitude location can
                // never resolve a solar event on any date — skip straight to the same
                // "give up" fallback `resolve`'s own MAX_SEARCH_RADIUS_DAYS branch
                // would eventually reach anyway, rather than running the full
                // radius-doubling search to its true worst-case extent on every call
                // for a location that can never succeed.
                if location.latitude().abs() >= POLE_LATITUDE_THRESHOLD {
                    return fallback_result(self.images(), &[], at);
                }
                let instant_fn = solar_instant_fn(self.images(), location);
                resolve(self.images(), at, crossfade_duration, &instant_fn)
            }
            AnchorKind::Clock => {
                let instant_fn = clock_instant_fn(self.images());
                resolve(self.images(), at, crossfade_duration, &instant_fn)
            }
        }
    }

    /// Report the next anchor instant strictly after `at` (FR-005) — the wake-up a
    /// caller in the idle-wait scheduling mode (constitution Principle VI) can sleep
    /// until instead of polling. `None` only for the degenerate single-image/
    /// static-mode pack, which never transitions.
    ///
    /// This method has no `crossfade_duration` parameter (contracts/
    /// schedule-engine-api.md), so unlike [`ValidatedPack::query`]'s
    /// `next_transition_at` field it cannot special-case "already mid-transition,
    /// return when *this* transition ends" — it always answers with the next anchor's
    /// own instant. That's exactly right for its purpose: a caller only needs a single
    /// idle-wait wake-up target while nothing is animating; once a transition starts,
    /// the caller moves to active-transition mode and drives rendering per-frame
    /// instead of calling this again until it returns to idle.
    pub fn next_transition_after(
        &self,
        location: Option<&Location>,
        at: DateTime<Local>,
    ) -> Option<DateTime<Local>> {
        // Reuse `query`'s search at a zero crossfade duration: with no window ever
        // open, its `next_transition_at` is always exactly "the next anchor instant
        // strictly after `at`" — the answer this method needs — without duplicating
        // the timeline search.
        self.query(location, at, TimeDelta::zero()).next_transition_at
    }
}

fn solar_instant_fn<'a>(
    images: &'a [PackImage],
    location: &'a Location,
) -> impl Fn(NaiveDate, usize) -> Option<DateTime<Local>> + 'a {
    move |date, idx| match images[idx].anchor {
        TimeAnchor::Solar { event, offset } => solar::resolve_solar_anchor(location, date, event, offset),
        TimeAnchor::Clock(_) => None,
    }
}

fn clock_instant_fn(images: &[PackImage]) -> impl Fn(NaiveDate, usize) -> Option<DateTime<Local>> + '_ {
    move |date, idx| match images[idx].anchor {
        TimeAnchor::Clock(time) => resolve_clock_instant(date, time),
        TimeAnchor::Solar { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::WallpaperPack;
    use crate::anchor::SolarEventKind;

    fn img(id: &str) -> PackImage {
        PackImage::new(id, TimeAnchor::Clock(chrono::NaiveTime::from_hms_opt(6, 0, 0).unwrap()))
    }

    #[test]
    fn fallback_result_picks_nearest_entry() {
        let images = vec![img("a"), img("b")];
        let at = chrono::Local::now();
        let entries = vec![(at - TimeDelta::days(10), 0), (at + TimeDelta::hours(1), 1)];
        let result = fallback_result(&images, &entries, at);
        // Closer to `at` is the +1h entry (image "b") than the -10d one (image "a").
        assert_eq!(result.active_before.as_str(), "b");
        assert!(result.transition.is_none());
        assert!(result.next_transition_at.is_none());
    }

    #[test]
    fn fallback_result_defaults_to_first_image_when_no_entries_at_all() {
        let images = vec![img("only")];
        let at = chrono::Local::now();
        let result = fallback_result(&images, &[], at);
        assert_eq!(result.active_before.as_str(), "only");
    }

    #[test]
    #[should_panic(expected = "Location is required")]
    fn query_panics_when_location_missing_for_solar_pack() {
        let images = vec![
            PackImage::new("a", TimeAnchor::Solar { event: SolarEventKind::Sunrise, offset: None }),
            PackImage::new("b", TimeAnchor::Solar { event: SolarEventKind::Sunset, offset: None }),
        ];
        let pack = WallpaperPack::validate(images).unwrap();
        let _ = pack.query(None, chrono::Local::now(), TimeDelta::minutes(5));
    }

    /// Spec 011 US7 FR-038 (research.md R33) — the audit's own reproduction: a
    /// pole-latitude query previously ran the full radius-doubling search (real
    /// per-date solar-position calculations that never resolve) on every call, for a
    /// location that can never succeed. Verified two ways: first, that the fast path
    /// produces the same degenerate-fallback shape the full search would eventually
    /// reach anyway; second, via a *relative* timing comparison against the real full
    /// search measured back-to-back on the same run — immune to absolute-timing
    /// flakiness on a slow/loaded CI machine (unlike a fixed millisecond threshold),
    /// while still failing against the pre-fix code (which *is* the full search, so
    /// the ratio would be ~1x instead of orders of magnitude).
    #[test]
    fn pole_latitude_returns_none_fast() {
        let images = vec![
            PackImage::new("a", TimeAnchor::Solar { event: SolarEventKind::Sunrise, offset: None }),
            PackImage::new("b", TimeAnchor::Solar { event: SolarEventKind::Sunset, offset: None }),
        ];
        let pack = WallpaperPack::validate(images).unwrap();
        let pole = crate::Location::new(89.99995, 0.0).unwrap();
        let at = chrono::Local::now();

        let start = std::time::Instant::now();
        let result = pack.query(Some(&pole), at, TimeDelta::minutes(5));
        let fast_path_elapsed = start.elapsed();
        assert_eq!(result.active_before.as_str(), "a", "falls back to the first image, per fallback_result's own contract");
        assert!(result.transition.is_none());

        // The real full search, reproduced directly (bypassing `query`'s fast path) —
        // a pole location never resolves a solar event on any date, so this measures
        // exactly the cost the fast path above avoids paying.
        let instant_fn = solar_instant_fn(pack.images(), &pole);
        let start = std::time::Instant::now();
        let _ = resolve(pack.images(), at, TimeDelta::minutes(5), &instant_fn);
        let full_search_elapsed = start.elapsed();

        assert!(
            fast_path_elapsed.saturating_mul(5) < full_search_elapsed,
            "fast path ({fast_path_elapsed:?}) should be far cheaper than the full radius-doubling search ({full_search_elapsed:?})"
        );
    }

    #[test]
    fn resolve_clock_instant_handles_dst_ambiguous_and_nonexistent_times() {
        // Host-dependent: only meaningfully exercises the Ambiguous/None branches on a
        // system whose `Local` timezone observes DST on the US schedule (this dev
        // environment does — America/New_York). On a non-DST-observing host these are
        // harmless `Single` resolutions instead; either way must not panic.
        let fall_back = NaiveDate::from_ymd_opt(2016, 11, 6).unwrap();
        let ambiguous = resolve_clock_instant(fall_back, chrono::NaiveTime::from_hms_opt(1, 30, 0).unwrap());
        assert!(ambiguous.is_some());

        let spring_forward = NaiveDate::from_ymd_opt(2016, 3, 13).unwrap();
        // 02:30 doesn't exist on a spring-forward day in a DST-observing zone — must not
        // panic either way.
        let _ = resolve_clock_instant(spring_forward, chrono::NaiveTime::from_hms_opt(2, 30, 0).unwrap());
    }
}
