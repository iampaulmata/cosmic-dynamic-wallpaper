//! `cosmic-config`-persisted schemas this crate reads and writes.
//!
//! [`RendererConfig`] is *owned* by spec 3 (contracts/renderer-config-schema.md) — this
//! crate is one of its writers (`wallpaperctl assign`), not its owner; the shape here
//! matches that contract exactly so spec 3, whenever implemented, reads the same data.
//! [`LocationConfigEntry`] was originally owned by spec 4
//! (contracts/location-config-schema.md, v1) and is now a v2 schema owned by spec 6
//! (specs/006-location-portal-integration/contracts/location-config-schema-v2.md) —
//! `wallpaperctl location` is one writer (`set`/`clear`/`auto`/`manual`), `wallpaperd`
//! is the only writer of `automatic_location`/`automatic_status`, and spec 3's
//! `scheduler_bridge.rs` (via `effective_location()`) is the reader.
//!
//! Both `cosmic-config` application ids below are an implementation decision made here
//! (neither contract names one) — documented prominently since spec 3 must match
//! [`RENDERER_CONFIG_ID`] exactly whenever it implements its own read side.

use std::collections::HashMap;

use cosmic_config::cosmic_config_derive::CosmicConfigEntry;
use cosmic_config::{Config, CosmicConfigEntry};
use serde::{Deserialize, Serialize};

use pack_loader::PackSource;
use schedule_engine::Location;

use crate::error::CliError;

/// `cosmic-config` application id for [`RendererConfig`] (spec 3's schema; this crate
/// writes it). Not fixed by contracts/renderer-config-schema.md — chosen here to match
/// the D-Bus bus name's naming convention (contracts/wallpaperd-dbus-interface.md).
pub const RENDERER_CONFIG_ID: &str = "com.system76.CosmicWallpaper.Renderer";

/// `cosmic-config` application id for [`LocationConfigEntry`] — this spec's own schema
/// (FR-008, contracts/location-config-schema.md), versioned independently of the
/// registry (spec 2) and renderer config (spec 3) schemas per constitution Principle X.
pub const LOCATION_CONFIG_ID: &str = "com.system76.CosmicWallpaper.Location";

/// Spec 3's per-output pack assignment (data-model.md `OutputAssignmentRequest`;
/// contracts/renderer-config-schema.md). `wallpaperctl assign` is one writer of this
/// entry; spec 3's `wallpaperd` is the reader.
#[derive(Debug, Clone, Default, CosmicConfigEntry, PartialEq)]
#[version = 1]
pub struct RendererConfig {
    /// The "same pack on all outputs" toggle (FR-006). `None` means it's off.
    pub same_pack_everywhere: Option<PackSource>,
    /// Per-output explicit overrides, keyed by output identifier (spec 3's `OutputId`
    /// string form, e.g. `"DP-3"`). An output with an entry here always follows it,
    /// regardless of `same_pack_everywhere` (FR-006).
    pub overrides: HashMap<String, PackSource>,
}

impl RendererConfig {
    /// Open the real, user-global renderer config.
    pub fn open() -> Result<Config, CliError> {
        Config::new(RENDERER_CONFIG_ID, Self::VERSION).map_err(CliError::from)
    }

    /// Open a renderer config rooted at a custom path — test-only (mirrors
    /// [`pack_loader::Registry::open_at`]'s pattern). This is a `[[bin]]`-only crate
    /// (no `lib.rs`), so a plain `cargo build`/non-test `cargo clippy` target never
    /// sees this called and flags it dead code; `#[allow]`'d rather than worked around,
    /// since it genuinely is only used by `#[cfg(test)]` code within this same crate.
    #[doc(hidden)]
    #[allow(dead_code)]
    pub fn open_at(custom_path: &std::path::Path) -> Result<Config, CliError> {
        Config::with_custom_path(RENDERER_CONFIG_ID, Self::VERSION, custom_path.to_path_buf())
            .map_err(CliError::from)
    }

    /// Read the current entry, falling back to the all-`None`/empty default if nothing
    /// has been written yet.
    pub fn load(config: &Config) -> Self {
        Self::get_entry(config).unwrap_or_else(|(_errors, default)| default)
    }

    pub fn save(&self, config: &Config) -> Result<(), CliError> {
        self.write_entry(config).map_err(CliError::from)
    }
}

/// Which source `effective_location()` (spec 6 data-model.md) should resolve from.
/// Default `Manual` — automatic is opt-in, never implicit (spec 6 FR-002).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum LocationMode {
    #[default]
    Manual,
    Automatic,
}

/// Surfaces spec 6.md's Location Availability Status Key Entity without requiring a
/// live daemon query (FR-008 — `location get` must work "at any time", daemon or not).
/// Only meaningful when `mode == Automatic`; still persisted (not reset) when
/// `mode == Manual`, so re-enabling automatic mode later doesn't lose the last-known
/// status.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub enum AutomaticStatus {
    /// Automatic mode was just enabled; no resolution attempt has completed yet.
    #[default]
    Unresolved,
    /// `automatic_location` holds a value from a successful portal resolution.
    Resolved,
    /// The most recent resolution attempt failed (portal absent, backend absent,
    /// permission declined, timeout, or a mid-session error) — `reason` is a short,
    /// specific string for display, not a generic catch-all (e.g. this project's own
    /// live-observed `"Location services disabled"`, spec 6 research.md R1).
    Unavailable { reason: String },
}

/// The location config entry (spec 6 data-model.md `LocationConfigEntry`, v2 —
/// supersedes spec 4's v1 `{ location: Option<Location> }` shape). Field names/types
/// **must match** `renderer`'s mirror of this type exactly
/// (`crates/renderer/src/config.rs`'s `LocationSource`) — this project's own
/// established lesson: a prior mismatch between two independently-defined "identical"
/// types silently produced an empty map at runtime.
#[derive(Debug, Clone, Default, CosmicConfigEntry, PartialEq)]
#[version = 2]
pub struct LocationConfigEntry {
    /// `Manual` (default) uses `location` directly; `Automatic` uses
    /// `effective_location()`'s resolution instead (spec 6 data-model.md).
    pub mode: LocationMode,
    /// The manual value — spec 4's original field, unchanged meaning. Never cleared by
    /// switching to `Automatic` mode or by a successful automatic resolution (FR-007).
    pub location: Option<Location>,
    /// The last successfully-resolved automatic value, persisted so a restarted daemon
    /// has an immediate value (FR-010). Written only by `wallpaperd`.
    pub automatic_location: Option<Location>,
    /// The most recent automatic resolution's outcome. Written only by `wallpaperd`.
    pub automatic_status: AutomaticStatus,
}

impl LocationConfigEntry {
    pub fn open() -> Result<Config, CliError> {
        Config::new(LOCATION_CONFIG_ID, Self::VERSION).map_err(CliError::from)
    }

    #[doc(hidden)]
    #[allow(dead_code)]
    pub fn open_at(custom_path: &std::path::Path) -> Result<Config, CliError> {
        Config::with_custom_path(LOCATION_CONFIG_ID, Self::VERSION, custom_path.to_path_buf())
            .map_err(CliError::from)
    }

    pub fn load(config: &Config) -> Self {
        Self::get_entry(config).unwrap_or_else(|(_errors, default)| default)
    }

    pub fn save(&self, config: &Config) -> Result<(), CliError> {
        self.write_entry(config).map_err(CliError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renderer_config_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let config = RendererConfig::open_at(dir.path()).unwrap();

        let mut state = RendererConfig::load(&config);
        assert_eq!(state, RendererConfig::default());

        state.overrides.insert("DP-3".to_string(), PackSource::StaticFile("/x.jpg".into()));
        state.save(&config).unwrap();

        let reloaded = RendererConfig::load(&config);
        assert_eq!(
            reloaded.overrides.get("DP-3"),
            Some(&PackSource::StaticFile("/x.jpg".into()))
        );
    }

    #[test]
    fn location_config_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let config = LocationConfigEntry::open_at(dir.path()).unwrap();

        let default = LocationConfigEntry::load(&config);
        assert_eq!(default.location, None);
        assert_eq!(default.mode, LocationMode::Manual);
        assert_eq!(default.automatic_location, None);
        assert_eq!(default.automatic_status, AutomaticStatus::Unresolved);

        let loc = Location::new(45.5019, -73.5674).unwrap();
        LocationConfigEntry { location: Some(loc), ..LocationConfigEntry::default() }.save(&config).unwrap();

        assert_eq!(LocationConfigEntry::load(&config).location, Some(loc));
    }

    /// Regression test (spec 6 research.md R7, tasks.md T004): `cosmic-config`'s
    /// built-in previous-version fallback needs no hand-written migration code — a
    /// v1-shaped entry's `location` value carries forward automatically once the v2
    /// struct's `#[version = 2]` chains back to the v1 directory, and every new v2-only
    /// field simply takes its `Default`.
    #[test]
    fn v1_location_entry_migrates_to_v2_with_no_hand_written_migration() {
        #[derive(Debug, Clone, Default, CosmicConfigEntry, PartialEq)]
        #[version = 1]
        struct LocationV1 {
            location: Option<Location>,
        }

        let dir = tempfile::tempdir().unwrap();
        let loc = Location::new(45.5019, -73.5674).unwrap();
        let v1_config = Config::with_custom_path(LOCATION_CONFIG_ID, LocationV1::VERSION, dir.path().to_path_buf()).unwrap();
        LocationV1 { location: Some(loc) }.write_entry(&v1_config).unwrap();

        let v2_config = LocationConfigEntry::open_at(dir.path()).unwrap();
        let loaded = LocationConfigEntry::load(&v2_config);

        assert_eq!(loaded.mode, LocationMode::Manual);
        assert_eq!(loaded.location, Some(loc));
        assert_eq!(loaded.automatic_location, None);
        assert_eq!(loaded.automatic_status, AutomaticStatus::Unresolved);
    }
}
