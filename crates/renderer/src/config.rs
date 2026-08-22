//! Re-exports of [`wallpaper_ipc`]'s shared `cosmic-config` schema types
//! ([`RendererConfig`], [`LocationConfigEntry`], [`LocationMode`], [`ResolutionStatus`],
//! [`effective_location`]) — this crate no longer independently defines them: a prior
//! mismatch between two independently-defined "identical" types across this crate and
//! `wallpaperctl` silently produced an empty map at runtime, exactly the bug class
//! extracting a single shared crate structurally prevents. This module now holds only
//! what's genuinely renderer-specific: [`Coalescer`] (a debounce), which depends on no
//! schema type beyond `OutputId`.
//!
//! **Scope note**: the real daemon watches these entries for live changes via
//! `cosmic-config`'s `notify`-backed watch mechanism (`cosmic_config::calloop::
//! ConfigWatchSource`, wired into the event loop in `src/bin/wallpaperd.rs`), feeding
//! detected changes into [`Coalescer`]. This module itself stays event-loop-agnostic —
//! everything watch-*dependent* lives in `wallpaperd.rs`/`surface.rs` instead.

use std::collections::HashMap;
use std::time::{Duration, Instant};

pub use wallpaper_ipc::{effective_location, LocationConfigEntry, LocationMode, ResolutionStatus, LOCATION_CONFIG_ID};

use crate::output::OutputId;

/// The re-evaluation deadline this crate commits to.
pub const REEVALUATION_DEADLINE: Duration = Duration::from_secs(2);

/// In-process debounce: a repeated change to the same output before its pending
/// re-evaluation runs replaces the earlier one wholesale — never queued, never
/// processed twice.
#[derive(Debug, Default)]
pub struct Coalescer {
    pending: HashMap<OutputId, Instant>,
}

impl Coalescer {
    /// A fresh coalescer with nothing pending.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that `output` changed at `now`. If a change for the same output is
    /// already pending, its deadline is *replaced*, not extended or queued — the
    /// re-evaluation that eventually runs only ever sees "the output changed", not how
    /// many times or when first.
    pub fn record_change(&mut self, output: OutputId, now: Instant) {
        self.pending.insert(output, now + REEVALUATION_DEADLINE);
    }

    /// Every output whose deadline has arrived as of `now` — draining them from the
    /// pending set (each is returned at most once per `record_change` call before it's
    /// drained).
    pub fn due(&mut self, now: Instant) -> Vec<OutputId> {
        let due: Vec<OutputId> =
            self.pending.iter().filter(|(_, deadline)| **deadline <= now).map(|(id, _)| id.clone()).collect();
        for id in &due {
            self.pending.remove(id);
        }
        due
    }

    /// `true` if `output` has a re-evaluation currently pending.
    pub fn is_pending(&self, output: &OutputId) -> bool {
        self.pending.contains_key(output)
    }

    /// The earliest pending deadline, if any — peeked without draining, so the
    /// idle-wait timer's wake computation can ensure it fires no later than any
    /// pending coalesced change, without consuming it (unlike [`Coalescer::due`]).
    pub fn earliest_pending(&self) -> Option<Instant> {
        self.pending.values().min().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rapid repeated changes to the same output collapse to a single pending entry,
    /// never queued or individually processed.
    #[test]
    fn repeated_changes_to_the_same_output_coalesce() {
        let mut coalescer = Coalescer::new();
        let now = Instant::now();
        let output = OutputId::new("DP-3");

        coalescer.record_change(output.clone(), now);
        coalescer.record_change(output.clone(), now + Duration::from_millis(500));
        coalescer.record_change(output.clone(), now + Duration::from_millis(900));

        // Not due yet at the *first* change's original deadline (now + 2s) — the
        // deadline was pushed out by the later changes.
        assert!(coalescer.due(now + Duration::from_millis(1900)).is_empty());
        assert!(coalescer.is_pending(&output));

        // Due once the *latest* change's own 2s deadline arrives, and reported exactly
        // once (not three times for the three record_change calls).
        let due = coalescer.due(now + Duration::from_millis(900) + REEVALUATION_DEADLINE);
        assert_eq!(due, vec![output.clone()]);
        assert!(!coalescer.is_pending(&output));
    }

    /// A change affecting only one output leaves an unrelated output's pending state
    /// untouched.
    #[test]
    fn changes_to_different_outputs_are_independent() {
        let mut coalescer = Coalescer::new();
        let now = Instant::now();
        coalescer.record_change(OutputId::new("DP-3"), now);

        let due = coalescer.due(now + REEVALUATION_DEADLINE);
        assert_eq!(due, vec![OutputId::new("DP-3")]);
        assert!(!coalescer.is_pending(&OutputId::new("eDP-1")));
    }

    /// `earliest_pending` peeks the minimum deadline without draining it — used by the
    /// idle-wait timer's wake computation.
    #[test]
    fn earliest_pending_reports_the_soonest_deadline_without_draining() {
        let mut coalescer = Coalescer::new();
        assert_eq!(coalescer.earliest_pending(), None);

        let now = Instant::now();
        coalescer.record_change(OutputId::new("DP-3"), now + Duration::from_secs(5));
        coalescer.record_change(OutputId::new("eDP-1"), now);

        assert_eq!(coalescer.earliest_pending(), Some(now + REEVALUATION_DEADLINE));
        // Peeking doesn't consume — both entries are still pending afterward.
        assert!(coalescer.is_pending(&OutputId::new("DP-3")));
        assert!(coalescer.is_pending(&OutputId::new("eDP-1")));
    }
}
