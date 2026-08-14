//! Timeline page (spec.md FR-005, contracts/gui-application.md) — today's schedule
//! visualization via `wallpaper_ipc::DbusClient`'s `QueryOutput`/`QueryAll` (spec 4's
//! existing D-Bus interface, unchanged). Read-only — same "daemon unreachable"
//! fallback UX `wallpaperctl query` uses, not a new failure mode.

use cosmic::widget;
use cosmic::Element;
use wallpaper_ipc::{DbusClient, DbusError, QueryEntry};

/// T022: maps a `DbusClient` query outcome 1:1, including the "daemon unreachable"
/// state — pure, independent of `libcosmic` rendering.
#[derive(Debug, Clone, PartialEq)]
pub enum TimelineState {
    Unreachable,
    Data(Vec<QueryEntry>),
}

pub fn from_query_result(result: Result<Vec<QueryEntry>, DbusError>) -> TimelineState {
    match result {
        Ok(entries) => TimelineState::Data(entries),
        Err(_) => TimelineState::Unreachable,
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Refresh,
}

pub struct State {
    pub timeline: TimelineState,
}

impl State {
    pub fn load() -> Self {
        let result = DbusClient::connect().and_then(|client| client.query_all());
        Self { timeline: from_query_result(result) }
    }
}

pub fn view(state: &State) -> Element<'_, Message> {
    let mut section = widget::settings::section().title("Today's schedule");
    match &state.timeline {
        TimelineState::Unreachable => {
            section = section.add(widget::text::body("wallpaperd is not running or not reachable — start it to see live schedule data."));
        }
        TimelineState::Data(entries) if entries.is_empty() => {
            section = section.add(widget::text::body("No outputs managed yet."));
        }
        TimelineState::Data(entries) => {
            for entry in entries {
                let detail = if entry.assigned {
                    format!("{} (next: {})", entry.active_image, entry.next_transition_at)
                } else {
                    "unassigned".to_string()
                };
                section = section.add(widget::settings::item(entry.output.clone(), widget::text::body(detail)));
            }
        }
    }
    let refresh = widget::button::standard("Refresh").on_press(Message::Refresh);
    widget::scrollable(widget::column::with_capacity(2).push(refresh).push(section)).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T022: matches `DbusClient`'s query response shape 1:1, including the "daemon
    /// unreachable" state.
    #[test]
    fn daemon_unreachable_maps_to_the_unreachable_state() {
        assert_eq!(from_query_result(Err(DbusError::DaemonUnreachable)), TimelineState::Unreachable);
    }

    #[test]
    fn output_not_found_also_maps_to_the_unreachable_state() {
        // Timeline queries all outputs (QueryAll), which never returns
        // OutputNotFound — but this page's mapping is deliberately total over every
        // DbusError variant, not just the one QueryAll can actually produce.
        assert_eq!(from_query_result(Err(DbusError::OutputNotFound { id: "DP-3".to_string() })), TimelineState::Unreachable);
    }

    #[test]
    fn successful_query_maps_to_the_data_state() {
        let entries = vec![QueryEntry { output: "DP-3".to_string(), assigned: true, active_image: "dawn.jpg".to_string(), next_transition_at: "t".to_string() }];
        assert_eq!(from_query_result(Ok(entries.clone())), TimelineState::Data(entries));
    }
}
