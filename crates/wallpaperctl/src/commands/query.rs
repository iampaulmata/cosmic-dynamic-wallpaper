//! `wallpaperctl query [--output <id>]` (FR-009).

use crate::dbus_client::{DbusClient, QueryEntry};
use crate::error::CliError;
use crate::output;

pub fn run(output_id: Option<&str>, json: bool) -> Result<String, CliError> {
    let client = DbusClient::connect()?;
    match output_id {
        Some(id) => {
            let entry = client.query_output(id)?;
            Ok(output::render(json, &entry, || human_one(&entry)))
        }
        None => {
            let entries = client.query_all()?;
            Ok(output::render(json, &entries, || human_many(&entries)))
        }
    }
}

fn human_one(entry: &QueryEntry) -> String {
    if entry.assigned {
        format!("{}: {} (next: {})", entry.output, entry.active_image, entry.next_transition_at)
    } else {
        format!("{}: unassigned", entry.output)
    }
}

fn human_many(entries: &[QueryEntry]) -> String {
    if entries.is_empty() {
        return "no outputs managed".to_string();
    }
    entries.iter().map(human_one).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Scenario 3: querying with no daemon running fails immediately with a clear
    /// error rather than hanging — real, not mocked (see dbus_client.rs's rationale).
    #[test]
    fn fails_fast_without_a_daemon() {
        if zbus::blocking::Connection::session().is_err() {
            return;
        }
        assert!(matches!(run(None, false), Err(CliError::DaemonUnreachable)));
        assert!(matches!(run(Some("DP-3"), false), Err(CliError::DaemonUnreachable)));
    }

    #[test]
    fn human_rendering_distinguishes_assigned_and_unassigned() {
        let assigned = QueryEntry {
            output: "DP-3".to_string(),
            assigned: true,
            active_image: "dawn.jpg".to_string(),
            next_transition_at: "2026-08-14T06:12:00-04:00".to_string(),
        };
        assert!(human_one(&assigned).contains("dawn.jpg"));

        let unassigned = QueryEntry {
            output: "eDP-1".to_string(),
            assigned: false,
            active_image: String::new(),
            next_transition_at: String::new(),
        };
        assert!(human_one(&unassigned).contains("unassigned"));
    }
}
