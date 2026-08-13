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
    /// mid-transition (it's the outgoing image in that case).
    pub active_before: ImageId,
    /// `Some` only when the query instant falls inside a crossfade window.
    pub transition: Option<TransitionState>,
    /// When the next transition begins. `None` only for the degenerate single-image/
    /// static-mode pack, which never transitions (Edge Cases).
    pub next_transition_at: Option<DateTime<Local>>,
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

/// How many days to search outward, at most, before giving up and falling back to a
/// static result (see `resolve`'s doc). Generous enough to cross any realistic
/// multi-month polar night while still terminating in bounded time (constitution
/// Principle VIII: no unbounded loops).
const MAX_SEARCH_RADIUS_DAYS: i64 = 370;

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
    let (next_instant, _) = next;

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
        }
    } else {
        ScheduleQueryResult {
            active_before: images[prev_idx].id.clone(),
            transition: None,
            next_transition_at: Some(next_instant),
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
