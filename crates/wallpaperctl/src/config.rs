//! `cosmic-config`-persisted schemas this crate reads and writes.
//!
//! [`RendererConfig`] is *owned* by spec 3 (contracts/renderer-config-schema.md) — this
//! crate is one of its writers (`wallpaperctl assign`), not its owner; the shape here
//! matches that contract exactly so spec 3, whenever implemented, reads the same data.
//! [`LocationConfigEntry`] is owned by *this* spec (contracts/location-config-schema.md)
//! — `wallpaperctl location` is its only writer, spec 3's `scheduler_bridge.rs` its
//! reader.
//!
//! Both `cosmic-config` application ids below are an implementation decision made here
//! (neither contract names one) — documented prominently since spec 3 must match
//! [`RENDERER_CONFIG_ID`] exactly whenever it implements its own read side.

use std::collections::HashMap;

use cosmic_config::cosmic_config_derive::CosmicConfigEntry;
use cosmic_config::{Config, CosmicConfigEntry};

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

/// The manual latitude/longitude a user provides for solar-anchored pack scheduling
/// (FR-008; data-model.md `LocationConfig`).
#[derive(Debug, Clone, Default, CosmicConfigEntry, PartialEq)]
#[version = 1]
pub struct LocationConfigEntry {
    /// `None` = no location set — only clock-anchored packs are usable.
    pub location: Option<Location>,
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

        assert_eq!(LocationConfigEntry::load(&config).location, None);

        let loc = Location::new(45.5019, -73.5674).unwrap();
        LocationConfigEntry { location: Some(loc) }.save(&config).unwrap();

        assert_eq!(LocationConfigEntry::load(&config).location, Some(loc));
    }
}
