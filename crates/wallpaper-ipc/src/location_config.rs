//! [`LocationConfigEntry`] v3 — this project's location `cosmic-config` schema
//! (spec 6 data-model.md, spec 7 data-model.md/contracts/location-config-schema-v3.md),
//! extracted here (spec 7 research.md R2) as the single source of truth
//! `crates/renderer`, `crates/wallpaperctl`, and `crates/wallpaper-settings` all depend
//! on. Field names/types are the wire-compatible contract every writer/reader shares —
//! see [`crate::renderer_config`]'s doc comment for why this project treats that
//! precisely, not casually.

use cosmic_config::cosmic_config_derive::CosmicConfigEntry;
use cosmic_config::{Config, CosmicConfigEntry};
use serde::{Deserialize, Serialize};

use schedule_engine::Location;

/// `cosmic-config` application id for [`LocationConfigEntry`] — shared by every
/// reader/writer.
pub const LOCATION_CONFIG_ID: &str = "com.system76.CosmicWallpaper.Location";

/// Which source [`effective_location`] should resolve from. Default `Manual` —
/// automatic/IP-geolocation modes are opt-in, never implicit (spec 6 FR-002, spec 7
/// FR-012).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum LocationMode {
    /// Scheduling uses `location` directly (spec 4's original, unchanged behavior).
    #[default]
    Manual,
    /// Scheduling uses `automatic_location`/`automatic_status` (spec 6's portal-based
    /// resolution) via [`effective_location`].
    Automatic,
    /// Scheduling uses `ip_location`/`ip_status` (spec 7's offline-database
    /// IP-geolocation) via [`effective_location`].
    IpGeolocation,
}

/// Surfaces this project's Location Availability Status Key Entity without requiring a
/// live daemon query (spec 4 FR-008). Renamed from spec 6's `AutomaticStatus` (spec 7
/// research.md R9) — the exact same shape, now shared by both the portal
/// (`automatic_status`) and IP-geolocation (`ip_status`) fields rather than defining a
/// second, structurally-identical enum. Only meaningful when `mode` matches the
/// corresponding source; still persisted (not reset) when it doesn't, so switching back
/// later doesn't lose the last-known status.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub enum ResolutionStatus {
    /// No resolution attempt has completed yet for this mode.
    #[default]
    Unresolved,
    /// The corresponding `*_location` field holds a value from a successful
    /// resolution.
    Resolved,
    /// The most recent resolution attempt failed — `reason` is a short, specific
    /// string for display (e.g. `"Location services disabled"` for the portal, or
    /// `"public IP discovery failed: STUN request timed out"` for IP-geolocation),
    /// never a generic catch-all.
    Unavailable {
        /// A short, specific string for display — never a generic catch-all.
        reason: String,
    },
}

/// This project's location config entry (v3 — supersedes spec 6's v2). `wallpaperctl`/
/// the GUI write `mode`/`location` (via `set`/`clear`/`auto`/`manual`/`ip`);
/// `wallpaperd` is the *only* writer of `automatic_location`/`automatic_status` (spec 6
/// portal resolution) and `ip_location`/`ip_status` (spec 7 IP-geolocation).
#[derive(Debug, Clone, Default, CosmicConfigEntry, PartialEq)]
#[version = 3]
pub struct LocationConfigEntry {
    /// `Manual` (default) uses `location` directly; `Automatic`/`IpGeolocation` use
    /// [`effective_location`]'s resolution instead.
    pub mode: LocationMode,
    /// The manual value — spec 4's original field, unchanged meaning. Never cleared by
    /// switching modes or by a successful automatic/IP-geolocation resolution.
    pub location: Option<Location>,
    /// The last successfully-resolved portal (spec 6) value. Written only by
    /// `wallpaperd`.
    pub automatic_location: Option<Location>,
    /// The most recent portal resolution's outcome. Written only by `wallpaperd`.
    pub automatic_status: ResolutionStatus,
    /// The last successfully-resolved IP-geolocation (spec 7) value. Written only by
    /// `wallpaperd`.
    pub ip_location: Option<Location>,
    /// The most recent IP-geolocation resolution's outcome. Written only by
    /// `wallpaperd`.
    pub ip_status: ResolutionStatus,
}

impl LocationConfigEntry {
    pub fn open() -> Result<Config, cosmic_config::Error> {
        Config::new(LOCATION_CONFIG_ID, Self::VERSION)
    }

    #[doc(hidden)]
    pub fn open_at(custom_path: &std::path::Path) -> Result<Config, cosmic_config::Error> {
        Config::with_custom_path(LOCATION_CONFIG_ID, Self::VERSION, custom_path.to_path_buf())
    }

    pub fn load(config: &Config) -> Self {
        Self::get_entry(config).unwrap_or_else(|(_errors, default)| default)
    }

    pub fn save(&self, config: &Config) -> Result<(), cosmic_config::Error> {
        self.write_entry(config)
    }
}

/// The single place scheduling code asks "what location, if any, should solar-anchored
/// packs use right now?" (spec 6/spec 7 data-model.md). Pure function, no I/O:
///
/// - `Manual` mode: `location` or nothing (spec 4's original behavior).
/// - `Automatic`/`IpGeolocation` mode with a resolved value: use it.
/// - `Automatic`/`IpGeolocation` mode, never resolved or currently unavailable: fall
///   back to `location` if a manual value happens to also be stored, else `None` —
///   spec 1/3's existing no-location degrade contract already handles that case; no new
///   failure mode is introduced (spec 6 FR-005, spec 7 FR-015).
pub fn effective_location(entry: &LocationConfigEntry) -> Option<Location> {
    match entry.mode {
        LocationMode::Manual => entry.location,
        LocationMode::Automatic => entry.automatic_location.or(entry.location),
        LocationMode::IpGeolocation => entry.ip_location.or(entry.location),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn location_config_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let config = LocationConfigEntry::open_at(dir.path()).unwrap();

        let default = LocationConfigEntry::load(&config);
        assert_eq!(default.location, None);
        assert_eq!(default.mode, LocationMode::Manual);
        assert_eq!(default.automatic_location, None);
        assert_eq!(default.automatic_status, ResolutionStatus::Unresolved);
        assert_eq!(default.ip_location, None);
        assert_eq!(default.ip_status, ResolutionStatus::Unresolved);

        let loc = Location::new(45.5019, -73.5674).unwrap();
        LocationConfigEntry { location: Some(loc), ..LocationConfigEntry::default() }.save(&config).unwrap();

        assert_eq!(LocationConfigEntry::load(&config).location, Some(loc));
    }

    /// ⚠️ Real finding, discovered while writing this test (spec 6 research.md R7's
    /// "per-key fallback" claim, re-verified here, is **only true one version hop at a
    /// time**, not transitively): `cosmic_config::Config::{new,with_custom_path}_inner`
    /// builds its `previous: Option<Box<Config>>` link by recursing with
    /// `look_for_previous: false` — so `previous.previous` is always `None`, no matter
    /// how many versions back the chain nominally spans. A direct v1 (spec 4) -> v3
    /// (this spec) jump — skipping v2 entirely, i.e. a machine that never ran a
    /// spec-6-era build even once — does **not** carry `location` forward; it's
    /// silently lost, not misinterpreted, but still a real gap in constitution
    /// Principle X's "MUST NOT silently misinterpret an old-format value" spirit. This
    /// project has never shipped a public release, so no real user is known to be in
    /// exactly this state today (every machine that ever ran a v2-era build already has
    /// a `v2/` directory on disk, which the *next* test below confirms bridges
    /// correctly) — flagged here as a documented, verified limitation rather than
    /// silently left for a future contributor to rediscover.
    #[test]
    fn v1_location_entry_does_not_migrate_directly_to_v3_skipping_v2() {
        #[derive(Debug, Clone, Default, CosmicConfigEntry, PartialEq)]
        #[version = 1]
        struct LocationV1 {
            location: Option<Location>,
        }

        let dir = tempfile::tempdir().unwrap();
        let loc = Location::new(45.5019, -73.5674).unwrap();
        let v1_config = Config::with_custom_path(LOCATION_CONFIG_ID, LocationV1::VERSION, dir.path().to_path_buf()).unwrap();
        LocationV1 { location: Some(loc) }.write_entry(&v1_config).unwrap();

        let v3_config = LocationConfigEntry::open_at(dir.path()).unwrap();
        let loaded = LocationConfigEntry::load(&v3_config);

        // Documents the actual (verified) behavior — not the originally-assumed one.
        assert_eq!(loaded.mode, LocationMode::Manual);
        assert_eq!(loaded.location, None, "cosmic-config's previous chain is one hop only — see this test's doc comment");
    }

    /// T013: a hand-written v2-shaped (spec 6) RON entry loads via the new v3 struct as
    /// `mode: Manual` (unchanged), `ip_location: None`, `ip_status: Unresolved` —
    /// confirms `cosmic-config`'s per-key fallback still works across the
    /// `AutomaticStatus` -> `ResolutionStatus` rename (research.md R9): the on-disk key
    /// name/shape for `automatic_status` is unchanged, only the Rust type's name
    /// changed, so a v2 value written by the *old* type name still deserializes
    /// correctly into the renamed type.
    #[test]
    fn v2_location_entry_migrates_to_v3_across_the_automaticstatus_rename() {
        #[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
        enum AutomaticStatusV2 {
            #[default]
            Unresolved,
            Resolved,
            Unavailable {
                reason: String,
            },
        }

        #[derive(Debug, Clone, Default, CosmicConfigEntry, PartialEq)]
        #[version = 2]
        struct LocationV2 {
            mode: LocationMode,
            location: Option<Location>,
            automatic_location: Option<Location>,
            automatic_status: AutomaticStatusV2,
        }

        let dir = tempfile::tempdir().unwrap();
        let loc = Location::new(45.5019, -73.5674).unwrap();
        let automatic = Location::new(51.5072, -0.1276).unwrap();
        let v2_config = Config::with_custom_path(LOCATION_CONFIG_ID, LocationV2::VERSION, dir.path().to_path_buf()).unwrap();
        LocationV2 {
            mode: LocationMode::Automatic,
            location: Some(loc),
            automatic_location: Some(automatic),
            automatic_status: AutomaticStatusV2::Resolved,
        }
        .write_entry(&v2_config)
        .unwrap();

        let v3_config = LocationConfigEntry::open_at(dir.path()).unwrap();
        let loaded = LocationConfigEntry::load(&v3_config);

        assert_eq!(loaded.mode, LocationMode::Automatic);
        assert_eq!(loaded.location, Some(loc));
        assert_eq!(loaded.automatic_location, Some(automatic));
        assert_eq!(loaded.automatic_status, ResolutionStatus::Resolved);
        assert_eq!(loaded.ip_location, None);
        assert_eq!(loaded.ip_status, ResolutionStatus::Unresolved);
    }

    /// T011: `effective_location()`'s three-way match, all nine `(mode,
    /// resolution-state)` combinations.
    #[test]
    fn effective_location_manual_mode_returns_location() {
        let loc = Location::new(45.5019, -73.5674).unwrap();
        let entry = LocationConfigEntry { mode: LocationMode::Manual, location: Some(loc), ..LocationConfigEntry::default() };
        assert_eq!(effective_location(&entry), Some(loc));
    }

    #[test]
    fn effective_location_manual_mode_with_no_location_is_none() {
        let entry = LocationConfigEntry { mode: LocationMode::Manual, ..LocationConfigEntry::default() };
        assert_eq!(effective_location(&entry), None);
    }

    #[test]
    fn effective_location_automatic_mode_resolved_returns_automatic_location() {
        let manual = Location::new(45.5019, -73.5674).unwrap();
        let automatic = Location::new(51.5072, -0.1276).unwrap();
        let entry = LocationConfigEntry {
            mode: LocationMode::Automatic,
            location: Some(manual),
            automatic_location: Some(automatic),
            automatic_status: ResolutionStatus::Resolved,
            ..LocationConfigEntry::default()
        };
        assert_eq!(effective_location(&entry), Some(automatic));
    }

    #[test]
    fn effective_location_automatic_mode_unresolved_falls_back_to_manual_then_none() {
        let manual = Location::new(45.5019, -73.5674).unwrap();

        let with_fallback = LocationConfigEntry { mode: LocationMode::Automatic, location: Some(manual), ..LocationConfigEntry::default() };
        assert_eq!(effective_location(&with_fallback), Some(manual));

        let with_no_fallback = LocationConfigEntry { mode: LocationMode::Automatic, ..LocationConfigEntry::default() };
        assert_eq!(effective_location(&with_no_fallback), None);
    }

    #[test]
    fn effective_location_automatic_mode_unavailable_falls_back_to_manual() {
        let manual = Location::new(45.5019, -73.5674).unwrap();
        let entry = LocationConfigEntry {
            mode: LocationMode::Automatic,
            location: Some(manual),
            automatic_status: ResolutionStatus::Unavailable { reason: "Location services disabled".into() },
            ..LocationConfigEntry::default()
        };
        assert_eq!(effective_location(&entry), Some(manual));
    }

    /// T049 (US3): `mode: IpGeolocation` falls back to `location` then `None` when
    /// unresolved/unavailable (spec.md FR-015) — mirrors `Automatic` mode's posture
    /// exactly, extending T011's coverage to the new third variant.
    #[test]
    fn effective_location_ip_geolocation_mode_resolved_returns_ip_location() {
        let manual = Location::new(45.5019, -73.5674).unwrap();
        let ip_resolved = Location::new(40.7128, -74.006).unwrap();
        let entry = LocationConfigEntry {
            mode: LocationMode::IpGeolocation,
            location: Some(manual),
            ip_location: Some(ip_resolved),
            ip_status: ResolutionStatus::Resolved,
            ..LocationConfigEntry::default()
        };
        assert_eq!(effective_location(&entry), Some(ip_resolved));
    }

    #[test]
    fn effective_location_ip_geolocation_mode_unresolved_falls_back_to_manual_then_none() {
        let manual = Location::new(45.5019, -73.5674).unwrap();

        let with_fallback = LocationConfigEntry { mode: LocationMode::IpGeolocation, location: Some(manual), ..LocationConfigEntry::default() };
        assert_eq!(effective_location(&with_fallback), Some(manual));

        let with_no_fallback = LocationConfigEntry { mode: LocationMode::IpGeolocation, ..LocationConfigEntry::default() };
        assert_eq!(effective_location(&with_no_fallback), None);
    }

    #[test]
    fn effective_location_ip_geolocation_mode_unavailable_falls_back_to_manual() {
        let manual = Location::new(45.5019, -73.5674).unwrap();
        let entry = LocationConfigEntry {
            mode: LocationMode::IpGeolocation,
            location: Some(manual),
            ip_status: ResolutionStatus::Unavailable { reason: "public IP discovery failed: STUN request timed out".into() },
            ..LocationConfigEntry::default()
        };
        assert_eq!(effective_location(&entry), Some(manual));
    }

    /// The three modes never cross-contaminate: an `Automatic`-mode entry with an
    /// unrelated resolved `ip_location` still ignores it.
    #[test]
    fn modes_do_not_cross_contaminate() {
        let automatic = Location::new(51.5072, -0.1276).unwrap();
        let ip = Location::new(40.7128, -74.006).unwrap();
        let entry = LocationConfigEntry {
            mode: LocationMode::Automatic,
            automatic_location: Some(automatic),
            automatic_status: ResolutionStatus::Resolved,
            ip_location: Some(ip),
            ip_status: ResolutionStatus::Resolved,
            ..LocationConfigEntry::default()
        };
        assert_eq!(effective_location(&entry), Some(automatic));
    }
}
