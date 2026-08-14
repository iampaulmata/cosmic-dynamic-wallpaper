//! `cosmic-config` reading for [`crate::output::RendererConfig`] and
//! [`LocationSource`] (FR-007, FR-015, research.md R4/R7), plus [`Coalescer`]
//! coalescing (FR-014).
//!
//! **Scope note**: the real daemon watches these entries for live changes via
//! `cosmic-config`'s `notify`-backed watch mechanism (`cosmic_config::calloop::
//! ConfigWatchSource`, wired into the event loop in `src/bin/wallpaperd.rs`), feeding
//! detected changes into [`Coalescer`]. This module itself stays event-loop-agnostic:
//! reading the current value and the coalescing logic are both pure and fully
//! testable without any event loop at all — everything watch-*dependent* lives in
//! `wallpaperd.rs`/`surface.rs` instead.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use cosmic_config::cosmic_config_derive::CosmicConfigEntry;
use cosmic_config::{Config, CosmicConfigEntry};
use serde::{Deserialize, Serialize};

use schedule_engine::Location;

use crate::error::RendererError;
use crate::output::{OutputId, RendererConfig};

/// `cosmic-config` application id for [`RendererConfig`] — **must match**
/// `wallpaperctl`'s `RENDERER_CONFIG_ID` exactly (`crates/wallpaperctl/src/config.rs`),
/// since both crates read/write the same on-disk entry. Not fixed by
/// contracts/renderer-config-schema.md.
pub const RENDERER_CONFIG_ID: &str = "com.system76.CosmicWallpaper.Renderer";

/// `cosmic-config` application id for [`LocationSource`] — **must match**
/// `wallpaperctl`'s `LOCATION_CONFIG_ID` exactly.
pub const LOCATION_CONFIG_ID: &str = "com.system76.CosmicWallpaper.Location";

impl RendererConfig {
    /// Open the real, user-global renderer config — the same `cosmic-config` entry
    /// `wallpaperctl assign` writes to.
    pub fn open() -> Result<Config, RendererError> {
        Config::new(RENDERER_CONFIG_ID, Self::VERSION).map_err(RendererError::from)
    }

    /// Open a renderer config rooted at a custom path — test-only.
    #[doc(hidden)]
    #[allow(dead_code)]
    pub fn open_at(path: &std::path::Path) -> Result<Config, RendererError> {
        Config::with_custom_path(RENDERER_CONFIG_ID, Self::VERSION, path.to_path_buf()).map_err(RendererError::from)
    }

    /// Read the current entry, falling back to the all-`None`/empty default if nothing
    /// has been written yet.
    pub fn load(config: &Config) -> Self {
        Self::get_entry(config).unwrap_or_else(|(_errors, default)| default)
    }
}

/// Which source [`effective_location`] should resolve from (spec 6 data-model.md).
/// Default `Manual` — automatic is opt-in, never implicit (FR-002). **Field shape must
/// match `wallpaperctl`'s copy of this enum exactly** (`crates/wallpaperctl/src/
/// config.rs`) — this project's own established lesson: a prior mismatch between these
/// two crates' independently-defined "identical" types silently produced an empty map
/// at runtime.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum LocationMode {
    /// Scheduling uses `location` directly (spec 4's original, unchanged behavior).
    #[default]
    Manual,
    /// Scheduling uses [`effective_location`]'s resolution rather than `location`
    /// directly.
    Automatic,
}

/// Surfaces spec 6.md's Location Availability Status Key Entity without requiring a
/// live daemon query (FR-008). Only meaningful when `mode == Automatic`; still
/// persisted (not reset) when `mode == Manual`. Field shape must match
/// `wallpaperctl`'s copy exactly (see [`LocationMode`]'s doc).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub enum AutomaticStatus {
    /// Automatic mode was just enabled; no resolution attempt has completed yet.
    #[default]
    Unresolved,
    /// `automatic_location` holds a value from a successful portal resolution.
    Resolved,
    /// The most recent resolution attempt failed (portal absent, backend absent,
    /// permission declined, timeout, or a mid-session error) — `reason` is a short,
    /// specific string for display (e.g. this project's own live-observed
    /// `"Location services disabled"`, spec 6 research.md R1), not a generic catch-all.
    Unavailable {
        /// A short, specific string for display — never a generic catch-all.
        reason: String,
    },
}

/// Spec 4's `LocationConfig` entry (v2, spec 6 data-model.md `LocationConfigEntry`) —
/// `wallpaperctl` writes `location`/`mode` (via `set`/`clear`/`auto`/`manual`); this
/// daemon is the *only* writer of `automatic_location`/`automatic_status`
/// (`portal_location.rs`). Field shape must match `wallpaperctl`'s
/// `LocationConfigEntry` exactly (see [`LocationMode`]'s doc for why this is called out
/// explicitly).
#[derive(Debug, Clone, Default, CosmicConfigEntry, PartialEq)]
#[version = 2]
pub struct LocationSource {
    /// `Manual` (default) uses `location` directly; `Automatic` uses
    /// [`effective_location`]'s resolution instead.
    pub mode: LocationMode,
    /// `None` = no manual location set — only clock-anchored packs are usable.
    pub location: Option<Location>,
    /// The last successfully-resolved automatic value (FR-010). Written only by this
    /// daemon.
    pub automatic_location: Option<Location>,
    /// The most recent automatic resolution's outcome. Written only by this daemon.
    pub automatic_status: AutomaticStatus,
}

impl LocationSource {
    /// Open the real, user-global location config — the same `cosmic-config` entry
    /// `wallpaperctl location set|clear|auto|manual` writes to.
    pub fn open() -> Result<Config, RendererError> {
        Config::new(LOCATION_CONFIG_ID, Self::VERSION).map_err(RendererError::from)
    }

    /// Open a location config rooted at a custom path — test-only.
    #[doc(hidden)]
    #[allow(dead_code)]
    pub fn open_at(path: &std::path::Path) -> Result<Config, RendererError> {
        Config::with_custom_path(LOCATION_CONFIG_ID, Self::VERSION, path.to_path_buf()).map_err(RendererError::from)
    }

    /// Read the current entry, falling back to the all-default (`Manual`, no location,
    /// `Unresolved`) entry if nothing has been written yet.
    pub fn load(config: &Config) -> Self {
        Self::get_entry(config).unwrap_or_else(|(_errors, default)| default)
    }

    /// Persist this entry — the only writer of `automatic_location`/`automatic_status`
    /// is this daemon itself (`portal_location.rs`); `wallpaperctl` writes `mode`/
    /// `location`.
    pub fn save(&self, config: &Config) -> Result<(), RendererError> {
        self.write_entry(config).map_err(RendererError::from)
    }
}

/// The single place scheduling code (`scheduler_bridge.rs`, per spec 6 plan.md's
/// Cross-Spec Dependency) asks "what location, if any, should solar-anchored packs use
/// right now?" (spec 6 data-model.md). Pure function, no I/O:
///
/// - `Manual` mode: identical to spec 4's original behavior — `location` or nothing.
/// - `Automatic` mode with a resolved value: use it (spec 6 US1).
/// - `Automatic` mode, never resolved or currently unavailable: fall back to `location`
///   if a manual value happens to also be stored (FR-005's first fallback tier), else
///   `None` (FR-005's second fallback tier) — spec 1/3's existing no-location degrade
///   contract already handles that case; no new failure mode is introduced.
pub fn effective_location(entry: &LocationSource) -> Option<Location> {
    match entry.mode {
        LocationMode::Manual => entry.location,
        LocationMode::Automatic => entry.automatic_location.or(entry.location),
    }
}

/// The re-evaluation deadline FR-007/FR-014 commit to.
pub const REEVALUATION_DEADLINE: Duration = Duration::from_secs(2);

/// In-process debounce for FR-014: a repeated change to the same output before its
/// pending re-evaluation runs replaces the earlier one wholesale — never queued,
/// never processed twice.
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

    /// Regression test for a real bug found manually testing this crate against a
    /// live `wallpaperctl`-written config (see `RendererConfig`'s own doc comment):
    /// `overrides` must parse a plain-string-keyed RON map, matching exactly what
    /// `wallpaperctl`'s `HashMap<String, PackSource>` writes — not silently fall back
    /// to an empty map because `OutputId`'s newtype form doesn't match. Written by
    /// hand-constructing the RON text `wallpaperctl` actually produces, rather than
    /// depending on the `wallpaperctl` crate itself just for this one shape check.
    #[test]
    fn overrides_parses_the_exact_shape_wallpaperctl_writes() {
        let dir = tempfile::tempdir().unwrap();
        let config = RendererConfig::open_at(dir.path()).unwrap();

        let overrides_path = dir.path().join("cosmic").join(RENDERER_CONFIG_ID).join("v1").join("overrides");
        std::fs::create_dir_all(overrides_path.parent().unwrap()).unwrap();
        std::fs::write(&overrides_path, r#"{"eDP-1": Directory("/home/user/pack")}"#).unwrap();

        let loaded = RendererConfig::load(&config);
        assert_eq!(loaded.overrides.get("eDP-1"), Some(&pack_loader::PackSource::Directory("/home/user/pack".into())));
    }

    #[test]
    fn renderer_config_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let config = RendererConfig::open_at(dir.path()).unwrap();

        let mut state = RendererConfig::load(&config);
        assert_eq!(state, RendererConfig::default());

        state.overrides.insert("DP-3".to_string(), pack_loader::PackSource::StaticFile("/x.jpg".into()));
        state.write_entry(&config).unwrap();

        let reloaded = RendererConfig::load(&config);
        assert_eq!(reloaded.overrides.len(), 1);
    }

    #[test]
    fn location_source_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let config = LocationSource::open_at(dir.path()).unwrap();
        let default = LocationSource::load(&config);
        assert_eq!(default.location, None);
        assert_eq!(default.mode, LocationMode::Manual);
        assert_eq!(default.automatic_location, None);
        assert_eq!(default.automatic_status, AutomaticStatus::Unresolved);

        let loc = Location::new(45.5019, -73.5674).unwrap();
        LocationSource { location: Some(loc), ..LocationSource::default() }.write_entry(&config).unwrap();
        assert_eq!(LocationSource::load(&config).location, Some(loc));
    }

    /// Regression test (spec 6 research.md R7, tasks.md T004): `cosmic-config`'s
    /// built-in previous-version fallback needs no hand-written migration code — see
    /// `wallpaperctl`'s identical regression test for the full rationale.
    #[test]
    fn v1_location_entry_migrates_to_v2_with_no_hand_written_migration() {
        #[derive(Debug, Clone, Default, CosmicConfigEntry, PartialEq)]
        #[version = 1]
        struct LocationSourceV1 {
            location: Option<Location>,
        }

        let dir = tempfile::tempdir().unwrap();
        let loc = Location::new(45.5019, -73.5674).unwrap();
        let v1_config = Config::with_custom_path(LOCATION_CONFIG_ID, LocationSourceV1::VERSION, dir.path().to_path_buf()).unwrap();
        LocationSourceV1 { location: Some(loc) }.write_entry(&v1_config).unwrap();

        let v2_config = LocationSource::open_at(dir.path()).unwrap();
        let loaded = LocationSource::load(&v2_config);

        assert_eq!(loaded.mode, LocationMode::Manual);
        assert_eq!(loaded.location, Some(loc));
        assert_eq!(loaded.automatic_location, None);
        assert_eq!(loaded.automatic_status, AutomaticStatus::Unresolved);
    }

    /// T005: `effective_location()`'s three branches (spec 6 data-model.md).
    #[test]
    fn effective_location_manual_mode_returns_location() {
        let loc = Location::new(45.5019, -73.5674).unwrap();
        let entry = LocationSource { mode: LocationMode::Manual, location: Some(loc), ..LocationSource::default() };
        assert_eq!(effective_location(&entry), Some(loc));
    }

    #[test]
    fn effective_location_automatic_mode_resolved_returns_automatic_location() {
        let manual = Location::new(45.5019, -73.5674).unwrap();
        let automatic = Location::new(51.5072, -0.1276).unwrap();
        let entry = LocationSource {
            mode: LocationMode::Automatic,
            location: Some(manual),
            automatic_location: Some(automatic),
            automatic_status: AutomaticStatus::Resolved,
        };
        assert_eq!(effective_location(&entry), Some(automatic));
    }

    #[test]
    fn effective_location_automatic_mode_unresolved_falls_back_to_manual_then_none() {
        let manual = Location::new(45.5019, -73.5674).unwrap();

        let with_manual_fallback = LocationSource { mode: LocationMode::Automatic, location: Some(manual), ..LocationSource::default() };
        assert_eq!(effective_location(&with_manual_fallback), Some(manual));

        let with_no_fallback = LocationSource { mode: LocationMode::Automatic, ..LocationSource::default() };
        assert_eq!(effective_location(&with_no_fallback), None);

        let unavailable = LocationSource {
            mode: LocationMode::Automatic,
            location: Some(manual),
            automatic_status: AutomaticStatus::Unavailable { reason: "Location services disabled".into() },
            ..LocationSource::default()
        };
        assert_eq!(effective_location(&unavailable), Some(manual));
    }

    /// FR-014: rapid repeated changes to the same output collapse to a single pending
    /// entry, never queued or individually processed.
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
    /// untouched (spec.md US4 Scenario 3 / FR-007).
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
