//! [`ScheduleQueryResult`] and [`TransitionState`] — the answer to "what's active right
//! now" (FR-004, FR-013, User Story 3; data-model.md).
//!
//! Resolution logic (`ValidatedPack::query`, `ValidatedPack::next_transition_after`) is
//! added to [`crate::ValidatedPack`] in this module by later user stories (US1 solar,
//! US2 clock, US3 degenerate/shared paths) — see tasks.md T013–T024.

use chrono::{DateTime, Local};

use crate::pack::ImageId;

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
