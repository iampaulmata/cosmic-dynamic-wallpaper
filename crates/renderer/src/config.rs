//! `cosmic-config` reading for [`crate::output::RendererConfig`] and
//! [`LocationSource`] (FR-007, FR-015, research.md R4/R7), plus [`PendingChange`]
//! coalescing (FR-014).
//!
//! **Scope note**: the real daemon watches these entries for live changes via
//! `cosmic-config`'s `notify`-backed watch mechanism, feeding detected changes into
//! `Coalescer`'s coalescing. That watch wiring (needs the `calloop` event loop — see
//! `README.md`) isn't implemented here; what's implemented is everything
//! watch-independent: reading the current value, and the coalescing logic itself,
//! which is pure and fully testable without any event loop at all.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use cosmic_config::cosmic_config_derive::CosmicConfigEntry;
use cosmic_config::{Config, CosmicConfigEntry};

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

/// Spec 4's `LocationConfig` entry, consumed (never written) here (data-model.md
/// `LocationSource`, FR-015). Field shape must match `wallpaperctl`'s
/// `LocationConfigEntry` exactly.
#[derive(Debug, Clone, Default, CosmicConfigEntry, PartialEq)]
#[version = 1]
pub struct LocationSource {
    /// `None` = no manual location set — only clock-anchored packs are usable.
    pub location: Option<Location>,
}

impl LocationSource {
    /// Open the real, user-global location config — the same `cosmic-config` entry
    /// `wallpaperctl location set|clear` writes to.
    pub fn open() -> Result<Config, RendererError> {
        Config::new(LOCATION_CONFIG_ID, Self::VERSION).map_err(RendererError::from)
    }

    /// Open a location config rooted at a custom path — test-only.
    #[doc(hidden)]
    #[allow(dead_code)]
    pub fn open_at(path: &std::path::Path) -> Result<Config, RendererError> {
        Config::with_custom_path(LOCATION_CONFIG_ID, Self::VERSION, path.to_path_buf()).map_err(RendererError::from)
    }

    /// Read the current entry, falling back to `location: None` if nothing has been
    /// written yet.
    pub fn load(config: &Config) -> Self {
        Self::get_entry(config).unwrap_or_else(|(_errors, default)| default)
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
        assert_eq!(LocationSource::load(&config).location, None);

        let loc = Location::new(45.5019, -73.5674).unwrap();
        LocationSource { location: Some(loc) }.write_entry(&config).unwrap();
        assert_eq!(LocationSource::load(&config).location, Some(loc));
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
}
