//! `zbus` client wrapper for `wallpaperd`'s session-bus interface
//! (contracts/wallpaperd-dbus-interface.md) — backs `query`, `reevaluate`, and
//! `list outputs` (FR-005, FR-009, FR-010).
//!
//! ⚠️ Cross-spec dependency: this interface must be *implemented* by spec 3's
//! `wallpaperd` binary, which does not exist yet as of this crate's implementation (see
//! that contract's own header). Every method here fails with
//! [`CliError::DaemonUnreachable`] until it does — which is also exactly the correct,
//! testable behavior for FR-011's "fail fast, don't hang" requirement, so these paths
//! are fully exercised today even without a running daemon.

use serde::Serialize;

use crate::error::CliError;

/// Session-bus well-known name (tentative — contracts/wallpaperd-dbus-interface.md).
pub const BUS_NAME: &str = "com.system76.CosmicWallpaper1";
pub const OBJECT_PATH: &str = "/com/system76/CosmicWallpaper1";
pub const INTERFACE: &str = "com.system76.CosmicWallpaper1.Daemon";

/// One output's schedule state, as reported by `QueryOutput`/`QueryAll`
/// (data-model.md `ScheduleQueryResponse`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QueryEntry {
    pub output: String,
    pub assigned: bool,
    pub active_image: String,
    pub next_transition_at: String,
}

/// A live connection to `wallpaperd`'s D-Bus interface.
pub struct DbusClient {
    proxy: zbus::blocking::Proxy<'static>,
}

impl DbusClient {
    /// Connect to the session bus and resolve `wallpaperd`'s object. Fails immediately
    /// with [`CliError::DaemonUnreachable`] if no session bus is reachable at all —
    /// the actual "no `wallpaperd` running" case surfaces on the first real method call
    /// instead (D-Bus doesn't know a well-known name has no owner until you ask it to
    /// do something), which every method below maps the same way.
    pub fn connect() -> Result<Self, CliError> {
        let connection =
            zbus::blocking::Connection::session().map_err(|_| CliError::DaemonUnreachable)?;
        let proxy = zbus::blocking::Proxy::new(&connection, BUS_NAME, OBJECT_PATH, INTERFACE)
            .map_err(|_| CliError::DaemonUnreachable)?;
        Ok(Self { proxy })
    }

    /// `QueryOutput(output_id) -> (assigned, active_image, next_transition_at)` (FR-009).
    pub fn query_output(&self, output_id: &str) -> Result<QueryEntry, CliError> {
        let (assigned, active_image, next_transition_at): (bool, String, String) =
            self.proxy.call("QueryOutput", &(output_id,)).map_err(|e| map_error(&e, output_id))?;
        Ok(QueryEntry { output: output_id.to_string(), assigned, active_image, next_transition_at })
    }

    /// `QueryAll() -> [(output_id, assigned, active_image, next_transition_at)]`
    /// (FR-009 with no `--output`; also backs `list outputs`, FR-005, research.md R5).
    pub fn query_all(&self) -> Result<Vec<QueryEntry>, CliError> {
        let entries: Vec<(String, bool, String, String)> =
            self.proxy.call("QueryAll", &()).map_err(|e| map_error(&e, ""))?;
        Ok(entries
            .into_iter()
            .map(|(output, assigned, active_image, next_transition_at)| QueryEntry {
                output,
                assigned,
                active_image,
                next_transition_at,
            })
            .collect())
    }

    /// `Reevaluate(output_id) -> ()` (FR-010).
    pub fn reevaluate(&self, output_id: &str) -> Result<(), CliError> {
        self.proxy.call("Reevaluate", &(output_id,)).map_err(|e| map_error(&e, output_id))
    }

    /// `ReevaluateAll() -> ()` (FR-010 with no `--output`).
    pub fn reevaluate_all(&self) -> Result<(), CliError> {
        self.proxy.call("ReevaluateAll", &()).map_err(|e| map_error(&e, ""))
    }
}

/// Map a `zbus` call failure to a [`CliError`]. A D-Bus method-error reply naming
/// `InvalidArgs` (or similar) is treated as "the daemon is reachable but doesn't
/// manage that output" ([`CliError::OutputNotFound`], only meaningful when
/// `output_id` is non-empty); every other failure (no session bus, no such
/// well-known name/service, timeout, etc.) is [`CliError::DaemonUnreachable`] —
/// the coarser, safer default per FR-011.
fn map_error(e: &zbus::Error, output_id: &str) -> CliError {
    if !output_id.is_empty() {
        if let zbus::Error::MethodError(name, ..) = e {
            if name.to_string().contains("InvalidArgs") {
                return CliError::OutputNotFound { id: output_id.to_string() };
            }
        }
    }
    CliError::DaemonUnreachable
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FR-011: with no `wallpaperd` actually running, every method must fail fast with
    /// `DaemonUnreachable` rather than hanging — exercised for real (not mocked)
    /// against whatever session bus this test host has, since no service is registered
    /// under `BUS_NAME` in any test environment.
    #[test]
    fn every_method_fails_fast_when_no_daemon_is_running() {
        let Ok(client) = DbusClient::connect() else {
            // No session bus reachable at all on this host — DaemonUnreachable from
            // `connect()` itself already demonstrates the required fail-fast behavior.
            return;
        };
        assert!(matches!(client.query_all(), Err(CliError::DaemonUnreachable)));
        assert!(matches!(client.reevaluate_all(), Err(CliError::DaemonUnreachable)));
        assert!(matches!(client.query_output("DP-3"), Err(CliError::DaemonUnreachable)));
    }

    #[test]
    fn query_entry_serializes_for_json_output() {
        let entry = QueryEntry {
            output: "DP-3".to_string(),
            assigned: true,
            active_image: "dawn.jpg".to_string(),
            next_transition_at: "2026-08-14T06:12:00-04:00".to_string(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("DP-3"));
        assert!(json.contains("dawn.jpg"));
    }
}
