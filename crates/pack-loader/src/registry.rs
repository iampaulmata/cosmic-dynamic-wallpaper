//! [`Registry`] — the persisted set of known pack locations (FR-010–FR-012, User Story
//! 4), backed by `cosmic-config`'s `CosmicConfigEntry` pattern (research.md R4).

use cosmic_config::cosmic_config_derive::CosmicConfigEntry;
use cosmic_config::{Config, CosmicConfigEntry};
use serde::{Deserialize, Serialize};

use crate::error::{ManifestError, RegistryError};
use crate::load::{load_pack, LoadedPack};
use crate::pack_source::PackSource;

/// The `cosmic-config` application id this crate's registry is stored under.
pub const REGISTRY_CONFIG_ID: &str = "com.system76.CosmicWallpaper.Registry";

/// Whether a known pack's source is currently reachable (FR-011) — see
/// [`PackRegistryEntry`]'s state notes (data-model.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegistryStatus {
    /// The pack was reachable and loaded successfully last time it was checked.
    Known,
    /// The pack's source was missing, moved, or unreadable last time it was checked.
    /// The entry is *retained*, just flagged — distinct from [`Registry::remove`],
    /// which deletes the entry outright (FR-012).
    Unavailable,
}

/// A persisted record of a known pack's source location and reachability
/// (data-model.md `PackRegistryEntry`), independent of whether it's currently loaded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackRegistryEntry {
    pub source: PackSource,
    pub status: RegistryStatus,
}

/// The on-disk shape `cosmic-config` persists (FR-010). Not part of this crate's public
/// API — [`Registry`] is the interface callers use.
#[derive(Debug, Clone, Default, CosmicConfigEntry, PartialEq)]
#[version = 1]
struct RegistryConfig {
    entries: Vec<PackRegistryEntry>,
}

/// The persisted set of known packs (FR-010–FR-012). Wraps a `cosmic-config::Config`
/// handle plus the current in-memory snapshot of [`RegistryConfig`].
pub struct Registry {
    config: Config,
    state: RegistryConfig,
}

impl Registry {
    /// Open (creating if necessary) the real, user-global registry under the standard
    /// `cosmic-config` XDG location.
    pub fn open() -> Result<Self, RegistryError> {
        let config = Config::new(REGISTRY_CONFIG_ID, RegistryConfig::VERSION)
            .map_err(|e| RegistryError::Storage { message: e.to_string() })?;
        Self::from_config(config)
    }

    /// Open a registry rooted at a custom path — used by tests (`tempfile`-backed,
    /// research.md R6) so registry persistence tests never touch the real user config
    /// directory.
    #[doc(hidden)]
    pub fn open_at(custom_path: &std::path::Path) -> Result<Self, RegistryError> {
        let config =
            Config::with_custom_path(REGISTRY_CONFIG_ID, RegistryConfig::VERSION, custom_path.to_path_buf())
                .map_err(|e| RegistryError::Storage { message: e.to_string() })?;
        Self::from_config(config)
    }

    fn from_config(config: Config) -> Result<Self, RegistryError> {
        let state = RegistryConfig::get_entry(&config).unwrap_or_else(|(_errors, default)| default);
        Ok(Self { config, state })
    }

    /// Persist a new known pack location (FR-010). Idempotent (FR-002 via spec 2's own
    /// identity-by-source rule) — registering an already-known source is a no-op, not a
    /// duplicate or an error.
    pub fn register(&mut self, source: PackSource) -> Result<(), RegistryError> {
        if self.state.entries.iter().any(|e| e.source == source) {
            return Ok(());
        }
        self.state.entries.push(PackRegistryEntry { source, status: RegistryStatus::Known });
        self.persist()
    }

    /// Delete a registry entry outright (FR-012) — distinct from [`Registry::reload_all`]'s
    /// automatic `Unavailable` marking. A no-op (not an error) if `source` isn't
    /// registered, matching `register`'s idempotent posture.
    pub fn remove(&mut self, source: &PackSource) -> Result<(), RegistryError> {
        self.state.entries.retain(|e| &e.source != source);
        self.persist()
    }

    /// Every currently known pack (FR-003), in registration order.
    pub fn known_packs(&self) -> Vec<PackRegistryEntry> {
        self.state.entries.clone()
    }

    /// Attempt to reload every known pack independently (FR-011) — one failing pack is
    /// marked [`RegistryStatus::Unavailable`] without preventing the others from
    /// loading. Returns each source's fresh load result alongside the updated status.
    pub fn reload_all(&mut self) -> Vec<(PackSource, Result<LoadedPack, ManifestError>)> {
        let mut results = Vec::with_capacity(self.state.entries.len());
        for entry in &mut self.state.entries {
            let outcome = load_pack(entry.source.path());
            entry.status = if outcome.is_ok() { RegistryStatus::Known } else { RegistryStatus::Unavailable };
            results.push((entry.source.clone(), outcome));
        }
        // Best-effort persist of the refreshed statuses; a failure here doesn't change
        // what's returned to the caller (constitution Principle VIII: don't let a
        // secondary write failure mask the primary reload result).
        let _ = self.persist();
        results
    }

    fn persist(&self) -> Result<(), RegistryError> {
        self.state
            .write_entry(&self.config)
            .map_err(|e| RegistryError::Storage { message: e.to_string() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_registry() -> (Registry, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let registry = Registry::open_at(dir.path()).unwrap();
        (registry, dir)
    }

    fn source(path: &std::path::Path) -> PackSource {
        PackSource::StaticFile(path.to_path_buf())
    }

    #[test]
    fn register_then_reopen_still_reports_the_pack() {
        let (mut registry, dir) = temp_registry();
        let file = dir.path().join("pack.jpg");
        std::fs::write(&file, b"x").unwrap();
        let src = source(&file);

        registry.register(src.clone()).unwrap();
        assert_eq!(registry.known_packs().len(), 1);

        // Reopen fresh, same custom path — simulates a daemon restart (US4 scenario 1).
        let reopened = Registry::open_at(dir.path()).unwrap();
        assert!(reopened.known_packs().iter().any(|e| e.source == src));
    }

    #[test]
    fn register_is_idempotent() {
        let (mut registry, dir) = temp_registry();
        let file = dir.path().join("pack.jpg");
        std::fs::write(&file, b"x").unwrap();
        let src = source(&file);

        registry.register(src.clone()).unwrap();
        registry.register(src.clone()).unwrap();
        assert_eq!(registry.known_packs().len(), 1);
    }

    #[test]
    fn remove_deletes_the_entry_outright() {
        let (mut registry, dir) = temp_registry();
        let file = dir.path().join("pack.jpg");
        std::fs::write(&file, b"x").unwrap();
        let src = source(&file);

        registry.register(src.clone()).unwrap();
        registry.remove(&src).unwrap();
        assert!(registry.known_packs().is_empty());
    }

    #[test]
    fn remove_of_unknown_source_is_a_harmless_noop() {
        let (mut registry, dir) = temp_registry();
        let src = source(&dir.path().join("never-registered.jpg"));
        assert!(registry.remove(&src).is_ok());
    }

    #[test]
    fn reload_all_marks_a_vanished_pack_unavailable_without_affecting_others() {
        let (mut registry, dir) = temp_registry();
        let present = dir.path().join("present.png");
        let vanished = dir.path().join("vanished.png");
        // Both need to be *real* images — load_pack's static-file path header-validates
        // readability, so garbage bytes would fail for a reason unrelated to this test.
        image::RgbImage::new(2, 2).save(&present).unwrap();
        image::RgbImage::new(2, 2).save(&vanished).unwrap();

        registry.register(source(&present)).unwrap();
        registry.register(source(&vanished)).unwrap();

        // The vanished pack's file disappears out from under it before the next reload.
        std::fs::remove_file(&vanished).unwrap();

        let results = registry.reload_all();
        assert_eq!(results.len(), 2);

        let present_ok = results.iter().any(|(s, r)| s.path() == present && r.is_ok());
        assert!(present_ok, "the still-present pack should still load fine");

        let statuses = registry.known_packs();
        let vanished_entry = statuses.iter().find(|e| e.source.path() == vanished).unwrap();
        assert_eq!(vanished_entry.status, RegistryStatus::Unavailable);
        let present_entry = statuses.iter().find(|e| e.source.path() == present).unwrap();
        assert_eq!(present_entry.status, RegistryStatus::Known);
    }
}
