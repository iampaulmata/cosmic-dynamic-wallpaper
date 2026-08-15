//! The live D-Bus service (T049/T053/T054, FR-016, User Story 7) — a `zbus` server
//! implementing `specs/004-cli-control-surface/contracts/wallpaperd-dbus-interface.md`
//! exactly, so `crates/wallpaperctl/src/dbus_client.rs` (already implemented and
//! tested, zero changes needed there) gets real answers instead of "daemon
//! unreachable".
//!
//! **Integration shape**: this daemon is a single-threaded Wayland/GPU client driven
//! by `calloop` (`src/bin/wallpaperd.rs`); `zbus`'s connection is built with
//! `internal_executor(false)` so it spawns no driver thread of its own, and its
//! executor is ticked forward as a foreign future via `calloop`'s `block_on` (calloop's
//! own documented mechanism for this, and the exact pattern `zbus`'s own
//! [`crate::Connection::executor`] doc example uses — just driven by `calloop` instead
//! of `tokio::spawn`). `zbus`'s `Interface` trait requires `Send + Sync` (so a served
//! interface *could* be called from another thread in general, even though this
//! program never actually does that) — [`DbusState`] is therefore behind `Arc<Mutex<_>>`
//! rather than the cheaper `Rc<RefCell<_>>` a truly single-threaded design would use.
//! The lock is never contended (everything still runs on the one main thread); poison
//! is handled by recovering the guard rather than panicking, per this crate's
//! `deny(unwrap_used, expect_used)` lint.
//!
//! [`DaemonInterface`]'s methods only ever read/write this small mirror — never
//! `WallpaperDaemon` itself, which stays exactly as it was (no interior mutability, no
//! `Send`/`Sync` requirement, never touched off the main thread). Read methods
//! (`QueryOutput`/`QueryAll`) answer directly from the mirror; write-shaped calls
//! (`Reevaluate`/`ReevaluateAll`) only enqueue a request, drained by
//! [`crate::surface::WallpaperDaemon::drain_dbus_requests`] on the next event-loop
//! tick — fire-and-forget, matching `wallpaperctl`'s own usage (it never chains a query
//! immediately after a reevaluate call).

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use crate::dbus_types::QueryResponse;
use crate::output::OutputId;

/// D-Bus bus name this daemon registers — must match
/// `wallpaperctl::dbus_client::BUS_NAME` exactly.
pub const BUS_NAME: &str = "com.system76.CosmicDynamicWallpaper1";
/// Object path the interface below is served at — must match
/// `wallpaperctl::dbus_client::OBJECT_PATH` exactly.
pub const OBJECT_PATH: &str = "/com/system76/CosmicDynamicWallpaper1";
/// D-Bus interface name — must match `wallpaperctl::dbus_client::INTERFACE` exactly.
pub const INTERFACE: &str = "com.system76.CosmicDynamicWallpaper1.Daemon";

/// A pending `Reevaluate`/`ReevaluateAll` call — drained by the main loop, since only
/// `&mut WallpaperDaemon` can actually re-evaluate and redraw.
#[derive(Debug, Clone)]
pub enum ReevaluateRequest {
    One(OutputId),
    All,
}

/// The read-only snapshot [`DaemonInterface`] answers `QueryOutput`/`QueryAll` from,
/// plus the outbound `Reevaluate`/`ReevaluateAll` request queue. Refreshed by
/// [`crate::surface::WallpaperDaemon::refresh_dbus_snapshot`] after every evaluation —
/// never more than one event-loop tick stale.
#[derive(Debug, Default)]
pub struct DbusState {
    snapshot: HashMap<OutputId, QueryResponse>,
    known_outputs: Vec<OutputId>,
    pending: VecDeque<ReevaluateRequest>,
}

impl DbusState {
    /// Replace the snapshot wholesale — cheap, since `responses`/`known` are already
    /// computed by the caller from already-loaded, in-memory pack state (no I/O).
    pub fn refresh(&mut self, responses: Vec<QueryResponse>, known: Vec<OutputId>) {
        self.snapshot = responses.into_iter().map(|r| (r.output.clone(), r)).collect();
        self.known_outputs = known;
    }

    /// Drain every pending `Reevaluate`/`ReevaluateAll` request, in the order received
    /// — called by [`crate::surface::WallpaperDaemon::drain_dbus_requests`] once per
    /// event-loop tick.
    pub fn drain(&mut self) -> Vec<ReevaluateRequest> {
        self.pending.drain(..).collect()
    }
}

/// Lock `state`, recovering from mutex poisoning by taking the guard anyway (never
/// panics — this crate denies `unwrap`/`expect` outside tests) rather than propagating
/// the poison. Sound here: no code holding this lock ever panics mid-borrow in normal
/// operation (all interface methods below are a few lines of plain data manipulation),
/// and even in the pathological case, a poisoned-but-recovered `DbusState` degrades to
/// "possibly stale answers", never a crash or a hang.
fn lock(state: &Mutex<DbusState>) -> std::sync::MutexGuard<'_, DbusState> {
    state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The actual D-Bus-facing object, served at [`OBJECT_PATH`]. Holds only the small
/// [`DbusState`] mirror — never `WallpaperDaemon` itself (see module doc).
pub struct DaemonInterface {
    pub state: Arc<Mutex<DbusState>>,
}

#[zbus::interface(interface = "com.system76.CosmicDynamicWallpaper1.Daemon")]
impl DaemonInterface {
    /// `QueryOutput(output_id) -> (assigned, active_image, next_transition_at)` per
    /// the contract — an unmanaged `output_id` is a D-Bus `InvalidArgs` error, which
    /// `wallpaperctl`'s client maps to `CliError::OutputNotFound`.
    fn query_output(&self, output_id: String) -> zbus::fdo::Result<(bool, String, String)> {
        let state = lock(&self.state);
        let id = OutputId::new(output_id.clone());
        let Some(response) = state.snapshot.get(&id) else {
            return Err(zbus::fdo::Error::InvalidArgs(format!("unmanaged output: {output_id}")));
        };
        Ok((
            response.assigned,
            response.active_image.clone(),
            response.next_transition_at.map(|t| t.to_rfc3339()).unwrap_or_default(),
        ))
    }

    /// `QueryAll() -> Array<(output_id, assigned, active_image, next_transition_at)>`
    /// — also backs `wallpaperctl list outputs` (which displays only `output_id`).
    fn query_all(&self) -> Vec<(String, bool, String, String)> {
        let state = lock(&self.state);
        state
            .known_outputs
            .iter()
            .filter_map(|id| state.snapshot.get(id))
            .map(|r| {
                (
                    r.output.as_str().to_string(),
                    r.assigned,
                    r.active_image.clone(),
                    r.next_transition_at.map(|t| t.to_rfc3339()).unwrap_or_default(),
                )
            })
            .collect()
    }

    /// `Reevaluate(output_id) -> ()` — validated synchronously against the current
    /// snapshot's known outputs, then enqueued; the actual re-evaluation happens on
    /// the next event-loop tick (fire-and-forget, matching `wallpaperctl`'s usage).
    fn reevaluate(&self, output_id: String) -> zbus::fdo::Result<()> {
        let mut state = lock(&self.state);
        let id = OutputId::new(output_id.clone());
        if !state.known_outputs.contains(&id) {
            return Err(zbus::fdo::Error::InvalidArgs(format!("unmanaged output: {output_id}")));
        }
        state.pending.push_back(ReevaluateRequest::One(id));
        Ok(())
    }

    /// `ReevaluateAll() -> ()` — always succeeds (there's no output to be invalid
    /// about); enqueued the same way as [`Self::reevaluate`].
    fn reevaluate_all(&self) {
        lock(&self.state).pending.push_back(ReevaluateRequest::All);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Local};

    fn response(output: &str, assigned: bool) -> QueryResponse {
        QueryResponse { output: OutputId::new(output), assigned, active_image: String::new(), next_transition_at: None }
    }

    #[test]
    fn refresh_replaces_the_snapshot_wholesale() {
        let mut state = DbusState::default();
        state.refresh(vec![response("eDP-1", true)], vec![OutputId::new("eDP-1")]);
        assert_eq!(state.snapshot.len(), 1);
        assert!(state.snapshot.contains_key(&OutputId::new("eDP-1")));

        state.refresh(vec![response("DP-3", false)], vec![OutputId::new("DP-3")]);
        assert_eq!(state.snapshot.len(), 1);
        assert!(!state.snapshot.contains_key(&OutputId::new("eDP-1")));
        assert!(state.snapshot.contains_key(&OutputId::new("DP-3")));
    }

    #[test]
    fn drain_returns_requests_in_order_and_empties_the_queue() {
        let mut state = DbusState::default();
        state.pending.push_back(ReevaluateRequest::One(OutputId::new("eDP-1")));
        state.pending.push_back(ReevaluateRequest::All);

        let drained = state.drain();
        assert_eq!(drained.len(), 2);
        assert!(matches!(drained[0], ReevaluateRequest::One(ref id) if id == &OutputId::new("eDP-1")));
        assert!(matches!(drained[1], ReevaluateRequest::All));
        assert!(state.drain().is_empty());
    }

    #[test]
    fn lock_recovers_from_a_poisoned_mutex_instead_of_panicking() {
        let mutex = Mutex::new(DbusState::default());
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            panic!("poison the mutex");
        }));
        assert!(mutex.is_poisoned());

        // `lock()` still returns a usable guard rather than panicking again.
        let guard = lock(&mutex);
        drop(guard);
    }

    /// A no-daemon-history sanity check: [`QueryResponse`] with `next_transition_at:
    /// None` (unassigned/no upcoming transition) round-trips to an empty string, not a
    /// placeholder that would confuse `wallpaperctl`'s parser.
    #[test]
    fn none_next_transition_maps_to_empty_string_not_a_placeholder() {
        let r = response("eDP-1", false);
        assert_eq!(r.next_transition_at.map(|t: DateTime<Local>| t.to_rfc3339()).unwrap_or_default(), "");
    }
}
