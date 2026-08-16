//! Portal integration for automatic location (spec 6 US1–US3) — talks to
//! `org.freedesktop.portal.Location` via [`ashpd`], driven inside `wallpaperd`'s
//! existing single `calloop` event loop (research.md R5), not a dedicated OS thread.
//!
//! **Write-back contract**: this module never touches [`cosmic_config::Config`] or
//! [`crate::surface::WallpaperDaemon`] directly — [`run`] only ever sends a
//! [`PortalEvent`] over a `calloop::channel`. `wallpaperd.rs`'s event loop is the only
//! place that owns the `Config` handle and the daemon's in-memory state, so it's the
//! only place [`apply_reading`]/[`apply_failure`] are actually called from.
//!
//! **Simplification, documented honestly**: [`run`] is spawned once — at daemon
//! startup if automatic mode is already enabled, or the first time a location-config
//! watch observes a Manual→Automatic transition (`wallpaperd.rs`) — and then keeps
//! running for the remainder of the daemon's lifetime rather than being cancelled if
//! the user switches back to manual mode. This is harmless, not a correctness gap:
//! [`crate::config::effective_location`] ignores `automatic_location` entirely while
//! `mode == Manual`, so a background resolution (or its backoff retries) has no
//! observable effect until automatic mode is re-enabled — at which point the most
//! recently resolved value is already there (spec.md FR-010). Full task cancellation on
//! every mode toggle is not implemented — flagged here rather than silently absorbed,
//! matching this project's established practice for documented gaps (spec 3/4's own
//! READMEs). Full live verification of this module needs a machine with GeoClue2
//! installed and location services enabled — not available in this project's own dev
//! environment (research.md R2); see `README.md`.

use std::time::{Duration, Instant};

use ashpd::desktop::location::{Accuracy, CreateSessionOptions, Location as PortalLocation, LocationProxy, StartOptions};
use futures_util::{Stream, StreamExt};

use schedule_engine::Location;

use crate::config::{ResolutionStatus, LocationConfigEntry, REEVALUATION_DEADLINE};

/// The resolution-attempt timeout (research.md R6) — distinct from spec 3 FR-007's
/// 2-second *reaction* bound (how fast a config change is picked up). Generous enough
/// for a real GeoClue lookup without leaving a solar-anchored pack in limbo
/// indefinitely.
pub const RESOLUTION_TIMEOUT: Duration = Duration::from_secs(5);

/// Exponential backoff bounds for retrying after a failed resolution (research.md R6):
/// never a tight loop, and self-recovers without the user needing to manually toggle
/// automatic mode off and on.
pub const INITIAL_BACKOFF: Duration = Duration::from_secs(30);
/// The backoff ceiling — never waited longer than this between retries.
pub const MAX_BACKOFF: Duration = Duration::from_secs(300);

/// The next backoff delay after a failed attempt — doubles, capped at [`MAX_BACKOFF`].
/// The call site resets to [`INITIAL_BACKOFF`] after every successful resolution.
pub fn next_backoff(current: Duration) -> Duration {
    current.saturating_mul(2).min(MAX_BACKOFF)
}

/// The shape [`run`] receives from `ashpd` before it's validated into spec 1's
/// [`Location`] — a plain, `ashpd`-free struct (data-model.md `PortalLocationReading`)
/// so [`apply_reading`] stays a pure, no-D-Bus unit test (tasks.md T007).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PortalReading {
    /// From `ashpd::desktop::location::Location::latitude()`.
    pub latitude: f64,
    /// From `ashpd::desktop::location::Location::longitude()`.
    pub longitude: f64,
    /// Radius in meters — logged for diagnostics only; spec.md's persisted schema has
    /// no accuracy field.
    pub accuracy: f64,
}

impl From<&PortalLocation> for PortalReading {
    fn from(update: &PortalLocation) -> Self {
        PortalReading { latitude: update.latitude(), longitude: update.longitude(), accuracy: update.accuracy() }
    }
}

/// A resolution outcome, sent from [`run`] back to `wallpaperd.rs`'s event loop over a
/// `calloop::channel` (see module doc's write-back contract).
#[derive(Debug, Clone)]
pub enum PortalEvent {
    /// A successful resolution or a subsequent live update from an already-resolved
    /// session (spec.md US1/US3).
    Reading(PortalReading),
    /// Any resolution failure (portal absent, backend absent, permission declined,
    /// timeout, or a mid-session error — spec.md FR-005), with the specific reason.
    Failure(String),
}

/// Validate `reading` through spec 1's [`Location::new`] and record a successful
/// resolution (spec.md US1 Scenarios 1–2, data-model.md's validate-before-write rule).
/// An out-of-range/non-finite reading from a misbehaving backend is treated as a
/// resolution failure, never partially written — delegates to [`apply_failure`].
pub fn apply_reading(entry: &mut LocationConfigEntry, reading: PortalReading) {
    match Location::new(reading.latitude, reading.longitude) {
        Ok(location) => {
            entry.automatic_location = Some(location);
            entry.automatic_status = ResolutionStatus::Resolved;
        }
        Err(e) => apply_failure(entry, e.to_string()),
    }
}

/// Record a resolution failure (portal absent, backend absent, permission declined,
/// timeout, or a mid-session error — spec.md FR-005), written back immediately with no
/// grace period. `automatic_location` is cleared, not left stale:
/// [`crate::config::effective_location`]'s fallback to the manual `location` only
/// triggers when `automatic_location` is `None` (contracts/location-config-schema-v2.md
/// "freshly-degraded example"), so a prior successful resolution must not linger once
/// it's known to be stale.
pub fn apply_failure(entry: &mut LocationConfigEntry, reason: String) {
    entry.automatic_location = None;
    entry.automatic_status = ResolutionStatus::Unavailable { reason };
}

/// In-process debounce for FR-032 (spec 011 US7, research.md R27) — the audit's own
/// framing: unlike every other config write in this daemon, a raw `PortalEvent` was
/// applied and persisted synchronously as it arrived, one write per event, instead of
/// coalescing a rapid burst (the portal settling through several intermediate readings)
/// the way `crate::config::Coalescer` already does for FR-014's per-output
/// re-evaluations. Mirrors that struct's exact "record replaces the pending deadline,
/// drained exactly once when due" semantics and the same [`REEVALUATION_DEADLINE`]
/// window, specialized to a single buffered [`PortalEvent`] slot instead of a
/// per-`OutputId` map (there's only ever one location stream to debounce here).
#[derive(Debug, Default)]
pub struct PortalDebouncer {
    pending: Option<PortalEvent>,
    deadline: Option<Instant>,
}

impl PortalDebouncer {
    /// A fresh debouncer with nothing pending.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record `event` at `now`, replacing any not-yet-applied pending one and pushing
    /// the deadline out to `now + REEVALUATION_DEADLINE` — a later event arriving
    /// before the deadline supersedes the earlier one wholesale, never queued or
    /// applied twice.
    pub fn record(&mut self, event: PortalEvent, now: Instant) {
        self.pending = Some(event);
        self.deadline = Some(now + REEVALUATION_DEADLINE);
    }

    /// The pending event, drained, if its deadline has arrived as of `now` — `None`
    /// otherwise (nothing pending, or not yet due). Mirrors [`crate::config::Coalescer::
    /// due`]'s "returned at most once" contract.
    pub fn due(&mut self, now: Instant) -> Option<PortalEvent> {
        if self.deadline.is_some_and(|deadline| deadline <= now) {
            self.deadline = None;
            self.pending.take()
        } else {
            None
        }
    }

    /// Whether an event is currently pending, not yet due.
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
}

/// One resolution attempt: create a portal session requesting [`Accuracy::City`]
/// (research.md R4), call `Start`, and await the session's first `LocationUpdated`
/// value — wrapped in [`RESOLUTION_TIMEOUT`]. Returns the still-open stream alongside
/// the first reading on success, so the caller can keep receiving further updates from
/// the *same* session without re-creating it (spec.md US3, tasks.md T021) — recreating
/// a session on every subsequent update would both waste portal/GeoClue resources and
/// risk missing updates during the recreation window.
async fn start_session() -> Result<(PortalReading, impl Stream<Item = PortalLocation> + Unpin), String> {
    let attempt = async {
        let proxy = LocationProxy::new().await.map_err(|e| e.to_string())?;
        let session =
            proxy.create_session(CreateSessionOptions::default().set_accuracy(Accuracy::City)).await.map_err(|e| e.to_string())?;
        // Subscribed *before* `start()` so the first `LocationUpdated` signal (which
        // can arrive as soon as `start()`'s own request completes) is never missed.
        let mut stream: std::pin::Pin<Box<dyn Stream<Item = PortalLocation>>> =
            Box::pin(proxy.receive_location_updated().await.map_err(|e| e.to_string())?);
        let request = proxy.start(&session, None, StartOptions::default()).await.map_err(|e| e.to_string())?;
        request.response().map_err(|e| e.to_string())?;
        let update = stream.next().await.ok_or_else(|| "portal session ended without a location update".to_string())?;
        let reading = PortalReading::from(&update);
        Ok((reading, stream))
    };
    let timeout = async {
        async_io::Timer::after(RESOLUTION_TIMEOUT).await;
        Err("resolution attempt timed out".to_string())
    };
    futures_lite::future::or(attempt, timeout).await
}

/// Drive automatic location resolution for the remainder of this daemon's lifetime
/// (module doc's Simplification note): repeatedly attempt a resolution, stay subscribed
/// to its session's ongoing `LocationUpdated` stream for as long as it keeps producing
/// updates (spec.md US3), and retry with exponential backoff on any failure or session
/// end (research.md R6) — every outcome is sent to `events` for `wallpaperd.rs`'s event
/// loop to apply. Returns only when `events` is disconnected (the daemon is shutting
/// down).
///
/// **Implementation note on the retry timer**: rather than a separate
/// `calloop::timer::Timer` event source coordinating back into this task (tasks.md
/// T017's literal wording), the backoff delay is an `async_io::Timer::after` awaited
/// directly inside this same task — both this task and the resolution timeout
/// ([`start_session`]) already run cooperatively inside `wallpaperd.rs`'s
/// `calloop::futures::Executor`, and `async_io::Timer` is the identical primitive
/// `zbus`'s own `async-io` backend already relies on elsewhere in this process (no
/// second concurrency model introduced, research.md R5's actual requirement) — this
/// achieves the same "never a tight loop, self-recovering" contract with one state
/// machine instead of two coordinating ones.
pub async fn run(events: calloop::channel::Sender<PortalEvent>) {
    let mut backoff = INITIAL_BACKOFF;
    loop {
        match start_session().await {
            Ok((reading, mut stream)) => {
                backoff = INITIAL_BACKOFF;
                if events.send(PortalEvent::Reading(reading)).is_err() {
                    return;
                }
                // T021: stay subscribed to this same session for as long as it keeps
                // producing updates — no new session, no backoff wait, until it ends.
                while let Some(update) = stream.next().await {
                    if events.send(PortalEvent::Reading(PortalReading::from(&update))).is_err() {
                        return;
                    }
                }
                // The session ended without an explicit error — treated the same as a
                // failure so the daemon degrades and retries rather than silently going
                // stale forever.
                if events.send(PortalEvent::Failure("portal session ended".to_string())).is_err() {
                    return;
                }
            }
            Err(reason) => {
                if events.send(PortalEvent::Failure(reason)).is_err() {
                    return;
                }
            }
        }
        async_io::Timer::after(backoff).await;
        backoff = next_backoff(backoff);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(latitude: f64, longitude: f64) -> PortalReading {
        PortalReading { latitude, longitude, accuracy: 1000.0 }
    }

    /// T007: a successful resolved reading validates through `Location::new` and
    /// produces `automatic_location: Some(..)`, `automatic_status: Resolved`.
    #[test]
    fn apply_reading_with_a_valid_value_resolves() {
        let mut entry = LocationConfigEntry::default();
        apply_reading(&mut entry, reading(45.5019, -73.5674));
        assert_eq!(entry.automatic_status, ResolutionStatus::Resolved);
        let loc = entry.automatic_location.unwrap();
        assert_eq!((loc.latitude(), loc.longitude()), (45.5019, -73.5674));
    }

    /// data-model.md: an out-of-range reading from a misbehaving backend is a
    /// resolution failure, never partially written.
    #[test]
    fn apply_reading_with_an_out_of_range_value_is_treated_as_a_failure() {
        let mut entry = LocationConfigEntry::default();
        apply_reading(&mut entry, reading(200.0, 0.0));
        assert_eq!(entry.automatic_location, None);
        assert!(matches!(entry.automatic_status, ResolutionStatus::Unavailable { .. }));
    }

    /// T013: a portal error/timeout/absence maps to `Unavailable { reason }` with the
    /// specific error string preserved verbatim — including this project's own
    /// live-observed `"Location services disabled"` string (research.md R1) as a
    /// literal test case, not a generic placeholder.
    #[test]
    fn apply_failure_preserves_the_reason_verbatim() {
        let mut entry = LocationConfigEntry::default();
        apply_failure(&mut entry, "Location services disabled".to_string());
        assert_eq!(entry.automatic_status, ResolutionStatus::Unavailable { reason: "Location services disabled".to_string() });
    }

    /// T013/data-model.md: a failure clears any previously-resolved `automatic_location`
    /// rather than leaving it stale — `effective_location()`'s fallback to `location`
    /// only triggers when `automatic_location` is `None`.
    #[test]
    fn apply_failure_clears_a_previously_resolved_automatic_location() {
        let mut entry = LocationConfigEntry::default();
        apply_reading(&mut entry, reading(45.5019, -73.5674));
        assert!(entry.automatic_location.is_some());

        apply_failure(&mut entry, "Location services disabled".to_string());
        assert_eq!(entry.automatic_location, None);
    }

    /// T015: repeated resolution failures back off exponentially (30s start, 5-minute
    /// cap), never a tight loop.
    #[test]
    fn next_backoff_doubles_and_caps_at_five_minutes() {
        let mut backoff = INITIAL_BACKOFF;
        assert_eq!(backoff, Duration::from_secs(30));

        backoff = next_backoff(backoff);
        assert_eq!(backoff, Duration::from_secs(60));

        backoff = next_backoff(backoff);
        assert_eq!(backoff, Duration::from_secs(120));

        backoff = next_backoff(backoff);
        assert_eq!(backoff, Duration::from_secs(240));

        backoff = next_backoff(backoff);
        assert_eq!(backoff, MAX_BACKOFF); // 480s would exceed the cap.

        // Stays capped, never grows unbounded or wraps.
        backoff = next_backoff(backoff);
        assert_eq!(backoff, MAX_BACKOFF);
    }

    /// Spec 011 US7 FR-032 (research.md R27) — the audit's own reproduction: a rapid
    /// burst of readings collapses to a single pending entry, never queued or
    /// individually processed (mirrors `config::tests::repeated_changes_to_the_same_
    /// output_coalesce` for `Coalescer`).
    #[test]
    fn portal_debouncer_coalesces_a_rapid_burst() {
        let mut debouncer = PortalDebouncer::new();
        let now = Instant::now();

        debouncer.record(PortalEvent::Reading(reading(45.5019, -73.5674)), now);
        debouncer.record(PortalEvent::Reading(reading(45.5, -73.5)), now + Duration::from_millis(500));
        debouncer.record(PortalEvent::Failure("transient glitch".to_string()), now + Duration::from_millis(900));

        // Not due yet at the *first* event's original deadline — pushed out by the
        // later events.
        assert!(debouncer.due(now + Duration::from_millis(1900)).is_none());
        assert!(debouncer.is_pending());

        // Due once the *latest* event's own deadline arrives, and only the latest
        // event (the failure) is what gets applied.
        let due = debouncer.due(now + Duration::from_millis(900) + REEVALUATION_DEADLINE);
        assert!(matches!(due, Some(PortalEvent::Failure(reason)) if reason == "transient glitch"));
        assert!(!debouncer.is_pending());
    }

    /// Draining via `due` is a one-shot: a second call at the same or a later instant
    /// sees nothing pending.
    #[test]
    fn portal_debouncer_due_drains_exactly_once() {
        let mut debouncer = PortalDebouncer::new();
        let now = Instant::now();
        debouncer.record(PortalEvent::Reading(reading(45.5019, -73.5674)), now);

        let deadline = now + REEVALUATION_DEADLINE;
        assert!(debouncer.due(deadline).is_some());
        assert!(debouncer.due(deadline).is_none());
    }

    /// T019: a `LocationUpdated` value distinct from the currently-stored
    /// `automatic_location` is applied and produces a genuinely different resolved
    /// value — the caller (`wallpaperd.rs`) is the one that diffs this against the
    /// prior state to decide whether to coalesce a re-evaluation (spec.md US3
    /// Scenario 1, FR-006); this asserts the write side of that path is correct.
    #[test]
    fn apply_reading_with_a_new_value_replaces_the_old_one() {
        let mut entry = LocationConfigEntry::default();
        apply_reading(&mut entry, reading(45.5019, -73.5674));
        let first = entry.automatic_location;

        apply_reading(&mut entry, reading(51.5072, -0.1276));
        let second = entry.automatic_location;

        assert_ne!(first, second);
        assert_eq!(second.unwrap().latitude(), 51.5072);
    }
}
