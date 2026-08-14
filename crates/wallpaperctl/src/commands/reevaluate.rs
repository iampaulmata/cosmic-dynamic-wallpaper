//! `wallpaperctl reevaluate [--output <id>]` (FR-010).

use crate::dbus_client::DbusClient;
use crate::error::CliError;
use crate::output::{self, Ack};

pub fn run(output_id: Option<&str>, json: bool) -> Result<String, CliError> {
    let client = DbusClient::connect()?;
    let human = match output_id {
        Some(id) => {
            client.reevaluate(id)?;
            format!("re-evaluated {id:?}")
        }
        None => {
            client.reevaluate_all()?;
            "re-evaluated all outputs".to_string()
        }
    };
    Ok(output::render(json, &Ack::ok(), || human))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Scenario 3: forcing re-evaluation with no daemon running fails immediately.
    #[test]
    fn fails_fast_without_a_daemon() {
        if zbus::blocking::Connection::session().is_err() {
            return;
        }
        assert!(matches!(run(None, false), Err(CliError::DaemonUnreachable)));
        assert!(matches!(run(Some("DP-3"), false), Err(CliError::DaemonUnreachable)));
    }
}
