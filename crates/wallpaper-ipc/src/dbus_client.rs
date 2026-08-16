//! `zbus` client wrapper for `wallpaperd`'s session-bus interface
//! (contracts/wallpaperd-dbus-interface.md) — backs `wallpaperctl`'s `query`,
//! `reevaluate`, `list outputs` (FR-005, FR-009, FR-010) and, new in spec 7, the GUI's
//! Timeline page (FR-005). Moved here unchanged from `crates/wallpaperctl/src/
//! dbus_client.rs` (spec 7 research.md R2) — the protocol itself
//! (contracts/wallpaperd-dbus-interface.md) is untouched, only the implementation's
//! location.

use std::fmt;

use serde::Serialize;

/// Session-bus well-known name (contracts/wallpaperd-dbus-interface.md).
pub const BUS_NAME: &str = "com.system76.CosmicDynamicWallpaper1";
/// Object path the interface is served at.
pub const OBJECT_PATH: &str = "/com/system76/CosmicDynamicWallpaper1";
/// D-Bus interface name.
pub const INTERFACE: &str = "com.system76.CosmicDynamicWallpaper1.Daemon";

/// One output's schedule state, as reported by `QueryOutput`/`QueryAll`
/// (data-model.md `ScheduleQueryResponse`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QueryEntry {
    /// The output's identifier (spec 3's `OutputId` string form, e.g. `"DP-3"`).
    pub output: String,
    /// Whether this output currently has a pack assigned.
    pub assigned: bool,
    /// The currently-active image's id, or empty if unassigned.
    pub active_image: String,
    /// RFC 3339 timestamp of the next scheduled transition, or empty if none.
    pub next_transition_at: String,
}

/// Every way a [`DbusClient`] call can fail — deliberately minimal (this crate stays
/// free of any consuming crate's own error type; each of `wallpaperctl`/
/// `wallpaper-settings` maps this into their own error enum via `From`).
#[derive(Debug)]
pub enum DbusError {
    /// No running `wallpaperd` reachable on the session bus (FR-011) — the coarser,
    /// safer default for any failure that isn't specifically an unmanaged-output error.
    DaemonUnreachable,
    /// The daemon is reachable but rejected the request as `InvalidArgs` — either the
    /// output isn't currently managed, or `output_id` itself failed validation
    /// (`OutputId::validated`, e.g. empty or containing disallowed characters). Spec
    /// 011 US8 FR-044 (previously this variant always rendered a fixed "does not
    /// currently manage" message, silently discarding the daemon's actual reason —
    /// misleading for the validation-failure case, which isn't about output
    /// management at all).
    OutputNotFound {
        /// The output id that was rejected.
        id: String,
        /// The daemon's own `InvalidArgs` message, when the D-Bus reply carried one
        /// (it always does today, but `zbus::Error::MethodError`'s description is
        /// `Option<String>` at the protocol level, so this stays optional too).
        detail: Option<String>,
    },
}

impl fmt::Display for DbusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DbusError::DaemonUnreachable => write!(f, "wallpaperd is not running or not reachable on the session bus"),
            DbusError::OutputNotFound { id, detail: Some(detail) } => write!(f, "wallpaperd rejected output {id:?}: {detail}"),
            DbusError::OutputNotFound { id, detail: None } => write!(f, "wallpaperd does not currently manage an output named {id:?}"),
        }
    }
}

impl std::error::Error for DbusError {}

/// A live connection to `wallpaperd`'s D-Bus interface.
pub struct DbusClient {
    proxy: zbus::blocking::Proxy<'static>,
}

impl DbusClient {
    /// Connect to the session bus and resolve `wallpaperd`'s object. Fails immediately
    /// with [`DbusError::DaemonUnreachable`] if no session bus is reachable at all —
    /// the actual "no `wallpaperd` running" case surfaces on the first real method call
    /// instead (D-Bus doesn't know a well-known name has no owner until you ask it to
    /// do something), which every method below maps the same way.
    pub fn connect() -> Result<Self, DbusError> {
        let connection = zbus::blocking::Connection::session().map_err(|_| DbusError::DaemonUnreachable)?;
        let proxy = zbus::blocking::Proxy::new(&connection, BUS_NAME, OBJECT_PATH, INTERFACE).map_err(|_| DbusError::DaemonUnreachable)?;
        Ok(Self { proxy })
    }

    /// `QueryOutput(output_id) -> (assigned, active_image, next_transition_at)` (FR-009).
    pub fn query_output(&self, output_id: &str) -> Result<QueryEntry, DbusError> {
        let (assigned, active_image, next_transition_at): (bool, String, String) =
            self.proxy.call("QueryOutput", &(output_id,)).map_err(|e| map_error(&e, output_id))?;
        Ok(QueryEntry { output: output_id.to_string(), assigned, active_image, next_transition_at })
    }

    /// `QueryAll() -> [(output_id, assigned, active_image, next_transition_at)]`
    /// (FR-009 with no `--output`; also backs `list outputs`, FR-005, research.md R5).
    pub fn query_all(&self) -> Result<Vec<QueryEntry>, DbusError> {
        let entries: Vec<(String, bool, String, String)> = self.proxy.call("QueryAll", &()).map_err(|e| map_error(&e, ""))?;
        Ok(entries
            .into_iter()
            .map(|(output, assigned, active_image, next_transition_at)| QueryEntry { output, assigned, active_image, next_transition_at })
            .collect())
    }

    /// `Reevaluate(output_id) -> ()` (FR-010).
    pub fn reevaluate(&self, output_id: &str) -> Result<(), DbusError> {
        self.proxy.call("Reevaluate", &(output_id,)).map_err(|e| map_error(&e, output_id))
    }

    /// `ReevaluateAll() -> ()` (FR-010 with no `--output`).
    pub fn reevaluate_all(&self) -> Result<(), DbusError> {
        self.proxy.call("ReevaluateAll", &()).map_err(|e| map_error(&e, ""))
    }
}

/// Map a `zbus` call failure to a [`DbusError`]. A D-Bus method-error reply naming
/// `InvalidArgs` (or similar) is treated as "the daemon is reachable but rejected this
/// request" ([`DbusError::OutputNotFound`], only meaningful when `output_id` is
/// non-empty), carrying forward the reply's own description text (FR-044) rather than
/// discarding it; every other failure (no session bus, no such well-known name/service,
/// timeout, etc.) is [`DbusError::DaemonUnreachable`] — the coarser, safer default per
/// FR-011.
fn map_error(e: &zbus::Error, output_id: &str) -> DbusError {
    if !output_id.is_empty() {
        if let zbus::Error::MethodError(name, detail, ..) = e {
            if name.to_string().contains("InvalidArgs") {
                return DbusError::OutputNotFound { id: output_id.to_string(), detail: detail.clone() };
            }
        }
    }
    DbusError::DaemonUnreachable
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
            return;
        };
        assert!(matches!(client.query_all(), Err(DbusError::DaemonUnreachable)));
        assert!(matches!(client.reevaluate_all(), Err(DbusError::DaemonUnreachable)));
        assert!(matches!(client.query_output("DP-3"), Err(DbusError::DaemonUnreachable)));
    }

    /// Spec 011 US8 FR-044: the daemon's real `InvalidArgs` description must reach
    /// `Display` output instead of being collapsed to a single fixed "does not
    /// currently manage" message regardless of the actual rejection reason (e.g. an
    /// `output_id` that failed validation, not merely one that isn't managed).
    #[test]
    fn output_not_found_display_includes_the_daemons_detail_when_present() {
        let with_detail = DbusError::OutputNotFound {
            id: "DP-3;rm -rf /".to_string(),
            detail: Some("output id contains disallowed characters".to_string()),
        };
        let message = with_detail.to_string();
        assert!(message.contains("disallowed characters"), "expected the daemon's real reason in: {message}");

        let without_detail = DbusError::OutputNotFound { id: "DP-9".to_string(), detail: None };
        assert!(without_detail.to_string().contains("does not currently manage"));
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
