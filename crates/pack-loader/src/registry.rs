//! [`Registry`] — the persisted set of known pack locations (FR-010–FR-012, User Story
//! 4), backed by `cosmic-config`'s `CosmicConfigEntry` pattern (research.md R4). Spec 7
//! extends each entry with [`PackOrigin`] (FR-008/FR-010/FR-011) and adds a second,
//! separate [`RemovedStarterPacks`] entry — see [`PackOrigin`]'s own doc for why the
//! two aren't folded into one schema.

use cosmic_config::cosmic_config_derive::CosmicConfigEntry;
use cosmic_config::{Config, CosmicConfigEntry};
use serde::{Deserialize, Serialize};

use crate::error::{ManifestError, RegistryError};
use crate::load::{load_pack, LoadedPack};
use crate::pack_source::PackSource;

/// The `cosmic-config` application id this crate's registry is stored under.
pub const REGISTRY_CONFIG_ID: &str = "com.system76.CosmicWallpaper.Registry";

/// The `cosmic-config` application id [`RemovedStarterPacks`] is stored under — its own
/// small, separate entry (spec 7 data-model.md, contracts/pack-registry-origin.md).
pub const REMOVED_STARTER_PACKS_CONFIG_ID: &str = "com.system76.CosmicWallpaper.RemovedStarterPacks";

/// Who registered a [`PackRegistryEntry`] (spec 7 data-model.md, contracts/
/// pack-registry-origin.md). Default `User` — full backward compatibility with every
/// registry entry that existed before this field, read forward with no behavior change
/// (same no-hand-written-migration pattern spec 6 research.md R7 already verified: a
/// new field with a safe default, carried automatically by `cosmic-config`'s per-key
/// fallback).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PackOrigin {
    /// Registered by a person via `wallpaperctl register` or the GUI's equivalent —
    /// never overridden by a starter pack's own default assignment (FR-011).
    #[default]
    User,
    /// Registered by this project itself (the bundled starter pack's own first-run
    /// self-registration — see `crates/renderer/src/bin/wallpaperd.rs`'s module doc
    /// for why this happens there rather than in a root-run `postinst` script, which
    /// has no access to any particular user's per-user `cosmic-config` store).
    Package,
}

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
    /// The pack's identity key (FR-009).
    pub source: PackSource,
    /// Whether this source was reachable last time it was checked.
    pub status: RegistryStatus,
    /// Who registered this entry (spec 7 FR-008/FR-010/FR-011). `#[serde(default)]`
    /// (not a `cosmic-config` schema-version bump — this field lives inside a `Vec`
    /// element, not at the top-level entry, so it's plain serde's own missing-field
    /// tolerance, not `cosmic-config`'s per-key version fallback) — an entry written
    /// before this field existed simply has no `origin` key in its RON and defaults to
    /// `User`, exactly matching what it always meant.
    #[serde(default)]
    pub origin: PackOrigin,
}

/// The on-disk shape `cosmic-config` persists (FR-010). Not part of this crate's public
/// API — [`Registry`] is the interface callers use.
#[derive(Debug, Clone, Default, CosmicConfigEntry, PartialEq)]
#[version = 1]
struct RegistryConfig {
    entries: Vec<PackRegistryEntry>,
}

/// A separate, minimal `cosmic-config` entry recording which starter-pack sources a
/// user has explicitly removed (spec 7 data-model.md, contracts/
/// pack-registry-origin.md) — deliberately not folded into [`RegistryConfig`] itself,
/// since a removed pack's registry entry no longer exists at all once it's gone;
/// there's nothing left to attach an `origin` to.
#[derive(Debug, Clone, Default, CosmicConfigEntry, PartialEq)]
#[version = 1]
struct RemovedStarterPacksConfig {
    removed: Vec<PackSource>,
}

/// The persisted set of known packs (FR-010–FR-012), plus the separate removed-
/// starter-packs record (spec 7). Wraps two `cosmic-config::Config` handles plus their
/// current in-memory snapshots.
pub struct Registry {
    config: Config,
    state: RegistryConfig,
    removed_config: Config,
    removed_state: RemovedStarterPacksConfig,
}

impl Registry {
    /// Open (creating if necessary) the real, user-global registry under the standard
    /// `cosmic-config` XDG location.
    pub fn open() -> Result<Self, RegistryError> {
        let config = Config::new(REGISTRY_CONFIG_ID, RegistryConfig::VERSION)
            .map_err(|e| RegistryError::Storage { message: e.to_string() })?;
        let removed_config = Config::new(REMOVED_STARTER_PACKS_CONFIG_ID, RemovedStarterPacksConfig::VERSION)
            .map_err(|e| RegistryError::Storage { message: e.to_string() })?;
        Self::from_configs(config, removed_config)
    }

    /// Open a registry rooted at a custom path — used by tests (`tempfile`-backed,
    /// research.md R6) so registry persistence tests never touch the real user config
    /// directory.
    #[doc(hidden)]
    pub fn open_at(custom_path: &std::path::Path) -> Result<Self, RegistryError> {
        let config =
            Config::with_custom_path(REGISTRY_CONFIG_ID, RegistryConfig::VERSION, custom_path.to_path_buf())
                .map_err(|e| RegistryError::Storage { message: e.to_string() })?;
        let removed_config = Config::with_custom_path(
            REMOVED_STARTER_PACKS_CONFIG_ID,
            RemovedStarterPacksConfig::VERSION,
            custom_path.to_path_buf(),
        )
        .map_err(|e| RegistryError::Storage { message: e.to_string() })?;
        Self::from_configs(config, removed_config)
    }

    fn from_configs(config: Config, removed_config: Config) -> Result<Self, RegistryError> {
        let state = RegistryConfig::get_entry(&config).unwrap_or_else(|(_errors, default)| default);
        let removed_state = RemovedStarterPacksConfig::get_entry(&removed_config).unwrap_or_else(|(_errors, default)| default);
        Ok(Self { config, state, removed_config, removed_state })
    }

    /// Persist a new known pack location (FR-010), `User`-origin (spec 7 FR-011 — the
    /// only way a person or the GUI can register a pack). Idempotent (FR-002 via spec
    /// 2's own identity-by-source rule) — registering an already-known source is a
    /// no-op, not a duplicate or an error.
    pub fn register(&mut self, source: PackSource) -> Result<(), RegistryError> {
        self.register_with_origin(source, PackOrigin::User)
    }

    /// [`Registry::register`], but with an explicit [`PackOrigin`] — used only by
    /// `wallpaperd`'s own starter-pack self-registration (spec 7 FR-008); every other
    /// caller (`wallpaperctl register`, the GUI) goes through [`Registry::register`]
    /// and is always `User`-origin (contracts/pack-registry-origin.md — `origin`
    /// defaults to `User`, and nothing in this crate's own public CLI-facing API lets a
    /// caller claim `Package` origin for themselves).
    pub fn register_with_origin(&mut self, source: PackSource, origin: PackOrigin) -> Result<(), RegistryError> {
        if self.state.entries.iter().any(|e| e.source == source) {
            return Ok(());
        }
        self.state.entries.push(PackRegistryEntry { source, status: RegistryStatus::Known, origin });
        self.persist()
    }

    /// Delete a registry entry outright (FR-012) — distinct from [`Registry::reload_all`]'s
    /// automatic `Unavailable` marking. A no-op (not an error) if `source` isn't
    /// registered, matching `register`'s idempotent posture. If the removed entry was
    /// `Package`-origin, also records the removal in the separate removed-starter-packs
    /// store (spec 7 FR-010, [`Registry::is_removed_starter_pack`]) so a later
    /// re-registration attempt (`wallpaperd`'s own first-run check) doesn't silently
    /// undo it.
    pub fn remove(&mut self, source: &PackSource) -> Result<(), RegistryError> {
        let removed_origin = self.state.entries.iter().find(|e| &e.source == source).map(|e| e.origin);
        self.state.entries.retain(|e| &e.source != source);
        self.persist()?;

        if removed_origin == Some(PackOrigin::Package) && !self.removed_state.removed.contains(source) {
            self.removed_state.removed.push(source.clone());
            self.removed_state
                .write_entry(&self.removed_config)
                .map_err(|e| RegistryError::Storage { message: e.to_string() })?;
        }
        Ok(())
    }

    /// Whether `source` was previously registered as a starter pack and explicitly
    /// removed (spec 7 FR-010) — the check `wallpaperd`'s own first-run
    /// self-registration makes before re-adding the bundled starter pack, so an
    /// upgrade never silently undoes a user's deliberate removal.
    pub fn is_removed_starter_pack(&self, source: &PackSource) -> bool {
        self.removed_state.removed.contains(source)
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

    /// T037: `PackRegistryEntry.origin` defaults to `User` for a freshly-registered
    /// pack (the only path `wallpaperctl register`/the GUI can take).
    #[test]
    fn register_defaults_to_user_origin() {
        let (mut registry, dir) = temp_registry();
        let file = dir.path().join("pack.jpg");
        std::fs::write(&file, b"x").unwrap();
        registry.register(source(&file)).unwrap();

        assert_eq!(registry.known_packs()[0].origin, PackOrigin::User);
    }

    /// T037: a pre-existing (pre-spec-7) registry entry — hand-written in the exact
    /// 2-field RON shape this crate wrote before `origin` existed — loads unchanged,
    /// defaulting `origin` to `User` via serde's own field-level default (not
    /// `cosmic-config`'s version-chain mechanism, since this field lives inside a `Vec`
    /// element — see `PackRegistryEntry::origin`'s doc comment).
    #[test]
    fn pre_existing_entry_with_no_origin_field_loads_as_user_origin() {
        let dir = tempfile::tempdir().unwrap();
        let entries_path = dir.path().join("cosmic").join(REGISTRY_CONFIG_ID).join("v1").join("entries");
        std::fs::create_dir_all(entries_path.parent().unwrap()).unwrap();
        std::fs::write(&entries_path, r#"[(source: StaticFile("/home/user/old-pack.jpg"), status: Known)]"#).unwrap();

        let registry = Registry::open_at(dir.path()).unwrap();
        let packs = registry.known_packs();
        assert_eq!(packs.len(), 1);
        assert_eq!(packs[0].origin, PackOrigin::User);
        assert_eq!(packs[0].status, RegistryStatus::Known);
    }

    /// T038: removing a `Package`-origin entry appends its source to the removed-
    /// starter-packs store; removing a `User`-origin entry does not (spec.md FR-010).
    #[test]
    fn removing_a_package_origin_entry_records_it_removed() {
        let (mut registry, dir) = temp_registry();
        let starter = dir.path().join("starter.jpg");
        std::fs::write(&starter, b"x").unwrap();
        let starter_src = source(&starter);
        registry.register_with_origin(starter_src.clone(), PackOrigin::Package).unwrap();

        registry.remove(&starter_src).unwrap();

        assert!(registry.is_removed_starter_pack(&starter_src));
    }

    #[test]
    fn removing_a_user_origin_entry_does_not_record_it_removed() {
        let (mut registry, dir) = temp_registry();
        let file = dir.path().join("pack.jpg");
        std::fs::write(&file, b"x").unwrap();
        let src = source(&file);
        registry.register(src.clone()).unwrap(); // User-origin, the only path register() takes.

        registry.remove(&src).unwrap();

        assert!(!registry.is_removed_starter_pack(&src));
    }

    /// T039: a simulated `postinst`/`wallpaperd` first-run re-registration attempt
    /// skips a starter pack already listed as removed (spec.md US2 Scenario 2) — this
    /// crate's `register_with_origin` doesn't itself consult the removed list (that
    /// check is the caller's job, `is_removed_starter_pack`, exercised here the same
    /// way `wallpaperd`'s startup logic actually uses it).
    #[test]
    fn simulated_first_run_skips_a_previously_removed_starter_pack() {
        let (mut registry, dir) = temp_registry();
        let starter = dir.path().join("starter.jpg");
        std::fs::write(&starter, b"x").unwrap();
        let starter_src = source(&starter);

        registry.register_with_origin(starter_src.clone(), PackOrigin::Package).unwrap();
        registry.remove(&starter_src).unwrap();

        // Simulates a later "first run" check re-opening the registry fresh (e.g. a
        // package upgrade re-invoking wallpaperd) and consulting the removed list
        // before deciding whether to re-register.
        let reopened = Registry::open_at(dir.path()).unwrap();
        assert!(reopened.is_removed_starter_pack(&starter_src));
        assert!(reopened.known_packs().is_empty(), "the removal itself is still respected too");
    }

    /// Regression: removing an already-removed `Package`-origin source a second time
    /// doesn't duplicate the removed-list entry.
    #[test]
    fn removing_a_package_origin_entry_twice_does_not_duplicate_the_removed_record() {
        let (mut registry, dir) = temp_registry();
        let starter = dir.path().join("starter.jpg");
        std::fs::write(&starter, b"x").unwrap();
        let starter_src = source(&starter);
        registry.register_with_origin(starter_src.clone(), PackOrigin::Package).unwrap();
        registry.remove(&starter_src).unwrap();

        // Re-register (e.g. a naive re-run that doesn't check is_removed_starter_pack)
        // then remove again.
        registry.register_with_origin(starter_src.clone(), PackOrigin::Package).unwrap();
        registry.remove(&starter_src).unwrap();

        assert!(registry.is_removed_starter_pack(&starter_src));
    }
}
