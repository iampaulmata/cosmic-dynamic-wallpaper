//! [`QueryResponse`] — the shape a live D-Bus query answers with (data-model.md
//! `QueryResponse`, FR-016, Amendment 2026-08-13). Mirrors spec 4's
//! `ScheduleQueryResponse` exactly (spec 4 is this interface's only intended caller).
//!
//! **Scope note**: this is only the pure data-mapping half of User Story 7 — building a
//! `QueryResponse` from an output's resolved state. The actual `zbus` server exposing
//! `QueryOutput`/`QueryAll`/`Reevaluate`/`ReevaluateAll` over the session bus
//! (contracts/wallpaperd-dbus-interface.md) needs the daemon's live `calloop` event
//! loop to serve requests against, which this pass doesn't implement (see `README.md`).

use chrono::{DateTime, Local};

use schedule_engine::{ImageId, ScheduleQueryResult};

use crate::output::OutputId;

/// The answer to "what's active on this output right now, and what's next" — the
/// D-Bus-facing read-only projection (data-model.md `QueryResponse`).
#[derive(Debug, Clone, PartialEq)]
pub struct QueryResponse {
    /// Which output this answers for.
    pub output: OutputId,
    /// `false` if the output's `OutputAssignment` is `Unassigned`.
    pub assigned: bool,
    /// Empty when `assigned` is `false`.
    pub active_image: String,
    /// `None` for a static/degenerate pack (no transition ever) or when unassigned.
    pub next_transition_at: Option<DateTime<Local>>,
}

impl QueryResponse {
    /// The `Unassigned` case (data-model.md; spec.md US7 Scenario 2) — a well-defined
    /// empty response, not an error.
    pub fn unassigned(output: OutputId) -> Self {
        Self { output, assigned: false, active_image: String::new(), next_transition_at: None }
    }

    /// Build a response from spec 1's own query result for an assigned, resolved
    /// output (spec.md US7 Scenario 1).
    pub fn from_schedule_result(output: OutputId, result: &ScheduleQueryResult) -> Self {
        let active_image = active_image_id(result).to_string();
        Self { output, assigned: true, active_image, next_transition_at: result.next_transition_at }
    }
}

/// The image id to report as "currently active" — the incoming image while a
/// transition is in progress (it's the one becoming visible), otherwise
/// `active_before` (data-model.md's own field doc: "always present, even mid-transition
/// — it's the outgoing image in that case" describes `active_before` specifically; this
/// helper picks whichever id best answers "what's on screen right now" for display
/// purposes, favoring the incoming image once a crossfade has genuinely started).
fn active_image_id(result: &ScheduleQueryResult) -> &ImageId {
    match &result.transition {
        Some(t) => &t.incoming,
        None => &result.active_before,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use schedule_engine::TransitionState;

    #[test]
    fn unassigned_response_has_no_image_or_next_transition() {
        let r = QueryResponse::unassigned(OutputId::new("eDP-1"));
        assert!(!r.assigned);
        assert!(r.active_image.is_empty());
        assert_eq!(r.next_transition_at, None);
    }

    #[test]
    fn assigned_response_reports_active_before_outside_a_transition() {
        let result = ScheduleQueryResult {
            active_before: ImageId::new("dawn.jpg"),
            transition: None,
            next_transition_at: Some(chrono::Local::now()),
        };
        let r = QueryResponse::from_schedule_result(OutputId::new("DP-3"), &result);
        assert!(r.assigned);
        assert_eq!(r.active_image, "dawn.jpg");
        assert!(r.next_transition_at.is_some());
    }

    #[test]
    fn assigned_response_reports_incoming_image_mid_transition() {
        let result = ScheduleQueryResult {
            active_before: ImageId::new("dawn.jpg"),
            transition: Some(TransitionState {
                outgoing: ImageId::new("dawn.jpg"),
                incoming: ImageId::new("noon.jpg"),
                progress: 0.5,
            }),
            next_transition_at: None,
        };
        let r = QueryResponse::from_schedule_result(OutputId::new("DP-3"), &result);
        assert_eq!(r.active_image, "noon.jpg");
    }
}
