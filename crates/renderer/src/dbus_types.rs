//! [`QueryResponse`] — the shape a live D-Bus query answers with. Mirrors
//! `wallpaper_ipc::dbus_client`'s `ScheduleQueryResponse` shape closely, since that's
//! this type's real consumer on the client side.
//!
//! **Scope note**: this is only the pure data-mapping half — building a `QueryResponse`
//! from an output's resolved state. The actual `zbus` server exposing
//! `QueryOutput`/`QueryAll`/`Reevaluate`/`ReevaluateAll` over the session bus lives in
//! [`crate::dbus_service`].

use chrono::{DateTime, Local};

use schedule_engine::{ImageId, ScheduleQueryResult};

use crate::output::OutputId;

/// The answer to "what's active on this output right now, and what's next" — the
/// D-Bus-facing read-only projection.
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
    /// The `Unassigned` case — a well-defined empty response, not an error.
    pub fn unassigned(output: OutputId) -> Self {
        Self { output, assigned: false, active_image: String::new(), next_transition_at: None }
    }

    /// Build a response from the scheduling engine's own query result for an
    /// assigned, resolved output.
    pub fn from_schedule_result(output: OutputId, result: &ScheduleQueryResult) -> Self {
        let active_image = active_image_id(result).to_string();
        Self { output, assigned: true, active_image, next_transition_at: result.next_transition_at }
    }
}

/// The image id to report as "currently active" — the incoming image while a
/// transition is in progress (it's the one becoming visible), otherwise
/// `active_before` (`active_before` is always present, even mid-transition, but
/// during a transition it's the *outgoing* image, not what's on screen right now —
/// this helper picks whichever id best answers "what's on screen right now" for
/// display purposes, favoring the incoming image once a crossfade has genuinely
/// started).
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
            next_image: Some(ImageId::new("noon.jpg")),
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
            next_image: Some(ImageId::new("noon.jpg")),
        };
        let r = QueryResponse::from_schedule_result(OutputId::new("DP-3"), &result);
        assert_eq!(r.active_image, "noon.jpg");
    }
}
