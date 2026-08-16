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

/// The most `Reevaluate`/`ReevaluateAll` requests [`DbusState::pending`] holds before
/// further calls are rejected/dropped (spec 011 US4 FR-014, research.md R10 —
/// clarified value: 8). Comfortably above any realistic multi-monitor burst of
/// legitimate calls, while bounding the redraw backlog an unauthorized local process
/// spamming this method can force onto the daemon.
pub const MAX_PENDING_DBUS_REQUESTS: usize = 8;

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
    /// Captured once at construction ([`Self::new`]) — this daemon's single `calloop`
    /// main thread, per module doc's "never contended" note. Every interface method
    /// below `debug_assert!`s it's still running there (spec 011 US7 FR-037,
    /// research.md R32): `zbus`'s `Interface` trait requires `Send + Sync` in general
    /// (so a served interface *could* be dispatched from another thread), but this
    /// daemon never actually does that — this makes the assumption a checked,
    /// debug-build-only invariant rather than only a comment a future refactor could
    /// silently invalidate.
    main_thread_id: std::thread::ThreadId,
}

impl DaemonInterface {
    /// Construct a new interface object, capturing the calling thread's id as the
    /// "main thread" every subsequent call's [`Self::assert_main_thread`] checks
    /// against (FR-037) — must be called from the daemon's one `calloop` main thread
    /// (see module doc), same as every other daemon setup step.
    pub fn new(state: Arc<Mutex<DbusState>>) -> Self {
        Self { state, main_thread_id: std::thread::current().id() }
    }

    /// FR-037 (research.md R32): checked restatement of this module's "never
    /// contended" assumption — see [`main_thread_id`](Self::main_thread_id)'s doc
    /// comment. Debug builds only (constitution Principle VIII: never a release-build
    /// panic surface); a violation here would mean `zbus` started dispatching served
    /// calls off the main thread, which would also break `WallpaperDaemon`'s own
    /// no-`Send`/`Sync`-needed assumption elsewhere in this daemon.
    fn assert_main_thread(&self) {
        debug_assert_eq!(
            std::thread::current().id(),
            self.main_thread_id,
            "DaemonInterface method called off the daemon's single main thread — see module doc's \"never contended\" note"
        );
    }
}

#[zbus::interface(interface = "com.system76.CosmicDynamicWallpaper1.Daemon")]
impl DaemonInterface {
    /// `QueryOutput(output_id) -> (assigned, active_image, next_transition_at)` per
    /// the contract — an unmanaged `output_id` is a D-Bus `InvalidArgs` error, which
    /// `wallpaperctl`'s client maps to `CliError::OutputNotFound`.
    ///
    /// Spec 011 US4 FR-017 (research.md R13): `output_id` validated the same way
    /// [`Self::reevaluate`] does, before the snapshot lookup.
    fn query_output(&self, output_id: String) -> zbus::fdo::Result<(bool, String, String)> {
        self.assert_main_thread();
        let id = OutputId::validated(output_id).map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
        let state = lock(&self.state);
        let Some(response) = state.snapshot.get(&id) else {
            return Err(zbus::fdo::Error::InvalidArgs(format!("unmanaged output: {id}")));
        };
        Ok((
            response.assigned,
            response.active_image.clone(),
            response.next_transition_at.map(|t| t.to_rfc3339()).unwrap_or_default(),
        ))
    }

    /// `QueryAll() -> Array<(output_id, assigned, active_image, next_transition_at)>`
    /// — also backs `wallpaperctl list outputs` (which displays only `output_id`).
    ///
    /// Spec 011 US4 FR-016 (research.md R12): logged so this daemon's log stream
    /// (`journalctl` under the shipped systemd unit) makes the access observable —
    /// this method hands location-derived data (active images, upcoming solar-
    /// transition timestamps) to any co-located same-uid process with no allow-list;
    /// see `contracts/wallpaperd-dbus-hardening.md` for why a full consent/allow-list
    /// mechanism is out of scope for this fix.
    fn query_all(&self) -> Vec<(String, bool, String, String)> {
        self.assert_main_thread();
        tracing::debug!("QueryAll invoked");
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
    ///
    /// Spec 011 US4 FR-017 (research.md R13): `output_id` is validated (non-empty,
    /// bounded length) via the same [`OutputId::validated`] the CLI's `--output` flag
    /// uses, before the known-outputs lookup.
    fn reevaluate(&self, output_id: String) -> zbus::fdo::Result<()> {
        self.assert_main_thread();
        let id = OutputId::validated(output_id).map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
        let mut state = lock(&self.state);
        if !state.known_outputs.contains(&id) {
            return Err(zbus::fdo::Error::InvalidArgs(format!("unmanaged output: {id}")));
        }
        // Spec 011 US4 FR-014 (research.md R10): bounded, same as `reevaluate_all`
        // below — see that method's doc comment for the coalescing half of this fix,
        // which doesn't apply to a specific-output request the way it does to `All`.
        if state.pending.len() >= MAX_PENDING_DBUS_REQUESTS {
            return Err(zbus::fdo::Error::LimitsExceeded(
                "too many pending re-evaluation requests — the daemon hasn't caught up yet".to_string(),
            ));
        }
        state.pending.push_back(ReevaluateRequest::One(id));
        Ok(())
    }

    /// `ReevaluateAll() -> ()` — always succeeds (there's no output to be invalid
    /// about); enqueued the same way as [`Self::reevaluate`].
    ///
    /// Spec 011 US4 FR-014 (research.md R10): this is the method the audit reproduced
    /// an unauthenticated-local-process DoS through (a tight call loop growing the
    /// pending queue without bound, each entry forcing a full re-evaluation/redraw of
    /// every output). Two defenses, in order: (1) coalescing — a repeated call while an
    /// `All` is already pending is a silent no-op, since a second full re-evaluation
    /// adds nothing a first one didn't already cover; this alone turns an unbounded
    /// spam loop into O(1) additional work after the first call. (2) a hard bound on
    /// top, for the remaining case of many distinct `Reevaluate(id)` calls mixed in —
    /// dropped and logged rather than queued once full, since this method's `()`
    /// return gives the caller no way to observe a rejection anyway.
    fn reevaluate_all(&self) {
        self.assert_main_thread();
        let mut state = lock(&self.state);
        if state.pending.iter().any(|r| matches!(r, ReevaluateRequest::All)) {
            return;
        }
        if state.pending.len() >= MAX_PENDING_DBUS_REQUESTS {
            tracing::warn!("dropping ReevaluateAll — pending D-Bus request queue is full ({MAX_PENDING_DBUS_REQUESTS} entries)");
            return;
        }
        state.pending.push_back(ReevaluateRequest::All);
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

    fn interface() -> DaemonInterface {
        DaemonInterface::new(Arc::new(Mutex::new(DbusState::default())))
    }

    /// Spec 011 US7 FR-037 (research.md R32): every interface method's
    /// `assert_main_thread` call must not fire when called from the same thread that
    /// constructed the interface — the ordinary, expected case every other test in
    /// this module already exercises implicitly. This test just makes that assumption
    /// explicit: if it ever panicked, every other test above would too.
    #[test]
    fn interface_methods_do_not_panic_when_called_from_the_constructing_thread() {
        let iface = interface();
        iface.reevaluate_all();
        assert!(iface.query_output("eDP-1".to_string()).is_err()); // unmanaged, but doesn't panic
        let _ = iface.query_all();
    }

    /// Spec 011 US4 FR-014 (research.md R10) — the audit's exact reproduction shape: a
    /// tight `ReevaluateAll` call loop. A repeated call while one `All` is already
    /// pending must be a no-op, not additional queue growth.
    #[test]
    fn reevaluate_all_coalesces() {
        let iface = interface();
        for _ in 0..100 {
            iface.reevaluate_all();
        }
        let state = lock(&iface.state);
        assert_eq!(state.pending.len(), 1, "100 calls while one All is pending must collapse to exactly one queued entry");
        assert!(matches!(state.pending.front(), Some(ReevaluateRequest::All)));
    }

    /// Spec 011 US4 FR-014 (research.md R10) — the queue never grows past
    /// `MAX_PENDING_DBUS_REQUESTS`, even when every call names a *different* output
    /// (so coalescing alone can't bound it).
    #[test]
    fn pending_queue_bounded() {
        let iface = interface();
        {
            let mut state = lock(&iface.state);
            state.known_outputs = (0..64).map(|i| OutputId::new(format!("OUT-{i}"))).collect();
        }
        let mut accepted = 0;
        let mut rejected = 0;
        for i in 0..64 {
            match iface.reevaluate(format!("OUT-{i}")) {
                Ok(()) => accepted += 1,
                Err(_) => rejected += 1,
            }
        }
        assert_eq!(accepted, MAX_PENDING_DBUS_REQUESTS, "exactly the bound's worth of distinct-output requests should be accepted");
        assert_eq!(rejected, 64 - MAX_PENDING_DBUS_REQUESTS);
        assert_eq!(lock(&iface.state).pending.len(), MAX_PENDING_DBUS_REQUESTS);
    }

    /// Spec 011 US4 FR-017 (research.md R13) — an empty or oversized `output_id` is
    /// rejected before the known-outputs lookup, for both `reevaluate` and
    /// `query_output`.
    #[test]
    fn output_id_validated() {
        let iface = interface();
        assert!(iface.reevaluate(String::new()).is_err());
        assert!(iface.reevaluate("x".repeat(wallpaper_ipc::MAX_OUTPUT_ID_BYTES + 1)).is_err());
        assert!(iface.query_output(String::new()).is_err());
        assert!(iface.query_output("x".repeat(wallpaper_ipc::MAX_OUTPUT_ID_BYTES + 1)).is_err());
    }
}
