//! [`Registry`] — the persisted set of known pack locations, backed by
//! `cosmic-config`'s `CosmicConfigEntry` pattern. Each entry carries a [`PackOrigin`],
//! and there's a second, separate [`RemovedStarterPacks`] entry — see [`PackOrigin`]'s
//! own doc for why the two aren't folded into one schema.

use cosmic_config::cosmic_config_derive::CosmicConfigEntry;
use cosmic_config::{Config, CosmicConfigEntry};
use serde::{Deserialize, Serialize};

use crate::error::{ManifestError, RegistryError};
use crate::load::{load_pack, LoadedPack};
use crate::pack_source::PackSource;

/// The `cosmic-config` application id this crate's registry is stored under.
pub const REGISTRY_CONFIG_ID: &str = "com.system76.CosmicDynamicWallpaper.Registry";

/// The `cosmic-config` application id [`RemovedStarterPacks`] is stored under — its own
/// small, separate entry.
pub const REMOVED_STARTER_PACKS_CONFIG_ID: &str = "com.system76.CosmicDynamicWallpaper.RemovedStarterPacks";

/// The pre-rename application ids — [`Registry::open`] migrates from these once so an
/// existing installation's known packs and removed-starter-packs record survive the
/// rename, never written to again afterward.
const OLD_REGISTRY_CONFIG_ID: &str = "com.system76.CosmicWallpaper.Registry";
const OLD_REMOVED_STARTER_PACKS_CONFIG_ID: &str = "com.system76.CosmicWallpaper.RemovedStarterPacks";

/// The application-id migration above is deliberately content-preserving, not
/// content-transforming — exactly right for a user's own registered pack paths, which
/// this rename never touches. But the *bundled starter pack itself* is a
/// system-installed path that this same rename relocated
/// (`crates/renderer/src/starter_pack.rs`'s `STARTER_PACK_SYSTEM_PATH`), so a
/// verbatim copy leaves an existing installation's starter-pack registry entry (and any
/// removed-starter-pack record) pointing at a path that no longer exists once the old
/// `.deb` is gone. [`Registry::open`] repairs this by rewriting an exact match of the
/// old path to the new one, in both stores. Must match `starter_pack.rs`'s own constant
/// exactly — duplicated here (not imported) because `pack-loader` is a dependency of
/// `renderer`, not the other way around, the same reason `wallpaper-ipc`'s D-Bus
/// constants are duplicated rather than shared.
const OLD_STARTER_PACK_SYSTEM_PATH: &str = "/usr/share/dynamic-wallpaper/starter-pack";
const NEW_STARTER_PACK_SYSTEM_PATH: &str = "/usr/share/cosmic-dynamic-wallpaper/starter-pack";

/// Who registered a [`PackRegistryEntry`]. Default `User` — full backward
/// compatibility with every registry entry that existed before this field, read
/// forward with no behavior change: a new field with a safe default, carried
/// automatically by `cosmic-config`'s per-key fallback.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PackOrigin {
    /// Registered by a person via `wallpaperctl register` or the GUI's equivalent —
    /// never overridden by a starter pack's own default assignment.
    #[default]
    User,
    /// Registered by this project itself (the bundled starter pack's own first-run
    /// self-registration — see `crates/renderer/src/bin/wallpaperd.rs`'s module doc
    /// for why this happens there rather than in a root-run `postinst` script, which
    /// has no access to any particular user's per-user `cosmic-config` store).
    Package,
}

/// Whether a known pack's source is currently reachable — see [`PackRegistryEntry`]'s
/// state notes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegistryStatus {
    /// The pack was reachable and loaded successfully last time it was checked.
    Known,
    /// The pack's source was missing, moved, or unreadable last time it was checked.
    /// The entry is *retained*, just flagged — distinct from [`Registry::remove`],
    /// which deletes the entry outright.
    Unavailable,
}

/// A persisted record of a known pack's source location and reachability, independent
/// of whether it's currently loaded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackRegistryEntry {
    /// The pack's identity key.
    pub source: PackSource,
    /// Whether this source was reachable last time it was checked.
    pub status: RegistryStatus,
    /// Who registered this entry. `#[serde(default)]` — not a `cosmic-config`
    /// schema-version bump, since this field lives inside a `Vec` element, not at the
    /// top-level entry, so it's plain serde's own missing-field tolerance, not
    /// `cosmic-config`'s per-key version fallback — an entry written before this field
    /// existed simply has no `origin` key in its RON and defaults to `User`, exactly
    /// matching what it always meant.
    #[serde(default)]
    pub origin: PackOrigin,
    /// A user-set label overriding this pack's default display name, never written
    /// back to the underlying file/folder. `None` = no override; falls back to the
    /// usual manifest-name/file-stem resolution
    /// (`pack_display::resolve_pack_display_name`). Only ever set by this crate's
    /// callers for a `PackSource::StaticFile` entry in practice — a `Directory`
    /// entry's display name lives in its own manifest `name` field instead — but kept
    /// on the shared struct rather than a `StaticFile`-only side table, since
    /// `PackSource` isn't the enum boundary this struct is built around and
    /// duplicating the whole entry type for one optional field isn't warranted.
    /// `#[serde(default)]` — the same no-version-bump shape `origin` above already
    /// establishes: a `Vec`-element field, not a top-level `cosmic-config` key, so
    /// plain serde's missing-field tolerance is what makes an entry written before
    /// this field existed still load correctly, as `None`.
    #[serde(default)]
    pub display_name: Option<String>,
}

/// The on-disk shape `cosmic-config` persists. Not part of this crate's public API —
/// [`Registry`] is the interface callers use.
#[derive(Debug, Clone, Default, CosmicConfigEntry, PartialEq)]
#[version = 1]
struct RegistryConfig {
    entries: Vec<PackRegistryEntry>,
}

/// A separate, minimal `cosmic-config` entry recording which starter-pack sources a
/// user has explicitly removed — deliberately not folded into [`RegistryConfig`]
/// itself, since a removed pack's registry entry no longer exists at all once it's
/// gone; there's nothing left to attach an `origin` to.
#[derive(Debug, Clone, Default, CosmicConfigEntry, PartialEq)]
#[version = 1]
struct RemovedStarterPacksConfig {
    removed: Vec<PackSource>,
}

/// The persisted set of known packs, plus the separate removed-starter-packs record.
/// Wraps two `cosmic-config::Config` handles plus their current in-memory snapshots.
pub struct Registry {
    config: Config,
    state: RegistryConfig,
    removed_config: Config,
    removed_state: RemovedStarterPacksConfig,
    /// Cross-process advisory-lock file path guarding the read-modify-write cycle in
    /// [`Registry::register_with_origin`]/[`Registry::remove`]/[`Registry::reload_all`].
    lock_path: std::path::PathBuf,
}

/// Resolves the directory a registry lock file should live in — alongside the
/// `cosmic-config` store it guards. `custom_path` mirrors `Config::with_custom_path`'s
/// own root (test-only, [`Registry::open_at`]); `None` reconstructs `Config::new`'s
/// real, production directory convention (`dirs::config_dir()/cosmic/{app_id}/
/// v{version}/`) — `cosmic_config::Config` exposes no path accessor of its own, so
/// this is the one place in this crate that has to reconstruct that
/// internal-but-stable convention rather than call a public accessor.
fn registry_lock_path(custom_path: Option<&std::path::Path>) -> Result<std::path::PathBuf, RegistryError> {
    let dir = match custom_path {
        Some(p) => p.to_path_buf(),
        None => dirs::config_dir()
            .ok_or_else(|| RegistryError::LockFailed { message: "no resolvable config directory".to_string() })?
            .join("cosmic")
            .join(REGISTRY_CONFIG_ID)
            .join(format!("v{}", RegistryConfig::VERSION)),
    };
    Ok(dir.join(".registry.lock"))
}

impl Registry {
    /// Open (creating if necessary) the real, user-global registry under the standard
    /// `cosmic-config` XDG location — migrating forward from the pre-rename application
    /// ids if nothing has been written under the new ones yet.
    pub fn open() -> Result<Self, RegistryError> {
        let config = Config::new(REGISTRY_CONFIG_ID, RegistryConfig::VERSION)
            .map_err(|e| RegistryError::Storage { message: e.to_string() })?;
        let removed_config = Config::new(REMOVED_STARTER_PACKS_CONFIG_ID, RemovedStarterPacksConfig::VERSION)
            .map_err(|e| RegistryError::Storage { message: e.to_string() })?;
        let mut state = migrate_registry_config(&config);
        let mut removed_state = migrate_removed_starter_packs_config(&removed_config);
        // Repair any entry left pointing at the pre-rename starter-pack system path —
        // see `OLD_STARTER_PACK_SYSTEM_PATH`'s doc comment for why the migration
        // above can't already handle this on its own.
        if repair_relocated_starter_pack_entries(&mut state.entries) {
            state.write_entry(&config).map_err(|e| RegistryError::Storage { message: e.to_string() })?;
        }
        if repair_relocated_starter_pack_removed(&mut removed_state.removed) {
            removed_state.write_entry(&removed_config).map_err(|e| RegistryError::Storage { message: e.to_string() })?;
        }
        let lock_path = registry_lock_path(None)?;
        Ok(Self { config, state, removed_config, removed_state, lock_path })
    }

    /// Open a registry rooted at a custom path — used by tests (`tempfile`-backed) so
    /// registry persistence tests never touch the real user config directory.
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
        Self::from_configs(config, removed_config, registry_lock_path(Some(custom_path))?)
    }

    fn from_configs(config: Config, removed_config: Config, lock_path: std::path::PathBuf) -> Result<Self, RegistryError> {
        let state = RegistryConfig::get_entry(&config).unwrap_or_else(|(_errors, default)| default);
        let removed_state = RemovedStarterPacksConfig::get_entry(&removed_config).unwrap_or_else(|(_errors, default)| default);
        Ok(Self { config, state, removed_config, removed_state, lock_path })
    }

    /// Persist a new known pack location, `User`-origin — the only way a person or the
    /// GUI can register a pack. Idempotent (registering an already-known source is a
    /// no-op, not a duplicate or an error).
    pub fn register(&mut self, source: PackSource) -> Result<(), RegistryError> {
        self.register_with_origin(source, PackOrigin::User)
    }

    /// [`Registry::register`], but with an explicit [`PackOrigin`] — used only by
    /// `wallpaperd`'s own starter-pack self-registration; every other caller
    /// (`wallpaperctl register`, the GUI) goes through [`Registry::register`] and is
    /// always `User`-origin. `origin` defaults to `User`, and nothing in this crate's
    /// own public CLI-facing API lets a caller claim `Package` origin for themselves.
    pub fn register_with_origin(&mut self, source: PackSource, origin: PackOrigin) -> Result<(), RegistryError> {
        self.with_locked_state(|entries| {
            if entries.iter().any(|e| e.source == source) {
                return;
            }
            entries.push(PackRegistryEntry { source, status: RegistryStatus::Known, origin, display_name: None });
        })
    }

    /// Sets (or clears, with `None`) `source`'s display-name override. A no-op `Ok(())`
    /// if `source` isn't currently registered, matching [`Registry::register`]/
    /// [`Registry::remove`]'s own idempotent posture — there's no entry to attach a
    /// name to, and silently creating one here would be a surprising side effect for a
    /// "set the name" call to have. Routed through the same [`Self::with_locked_state`]
    /// read-modify-write pattern every other mutation of `entries` uses — a smaller
    /// mutation than `register`/`remove` doesn't make it exempt from the same
    /// lost-update race those exist to close. Callers are expected to have already
    /// normalized a blank/whitespace-only name to `None` — this method itself doesn't
    /// inspect the string, the same division of responsibility [`PackSource::resolve`]
    /// already has for path validation ahead of `register`.
    pub fn set_display_name(&mut self, source: &PackSource, name: Option<String>) -> Result<(), RegistryError> {
        self.with_locked_state(|entries| {
            if let Some(entry) = entries.iter_mut().find(|e| &e.source == source) {
                entry.display_name = name;
            }
        })
    }

    /// Delete a registry entry outright — distinct from [`Registry::reload_all`]'s
    /// automatic `Unavailable` marking. A no-op (not an error) if `source` isn't
    /// registered, matching `register`'s idempotent posture. If the removed entry was
    /// `Package`-origin, also records the removal in the separate removed-starter-packs
    /// store (see [`Registry::is_removed_starter_pack`]) so a later re-registration
    /// attempt (`wallpaperd`'s own first-run check) doesn't silently undo it.
    pub fn remove(&mut self, source: &PackSource) -> Result<(), RegistryError> {
        let removed_origin = self.with_locked_state(|entries| {
            let removed_origin = entries.iter().find(|e| &e.source == source).map(|e| e.origin);
            entries.retain(|e| &e.source != source);
            removed_origin
        })?;

        if removed_origin == Some(PackOrigin::Package) {
            // Routed through `with_locked_removed_state` rather than writing straight
            // to `self.removed_state` (an in-memory snapshot, potentially stale
            // relative to another process's concurrent write) — the same lost-update
            // race `with_locked_state` above exists to close for the main `entries`
            // list applies here too.
            self.with_locked_removed_state(|removed| {
                if !removed.contains(source) {
                    removed.push(source.clone());
                }
            })?;
        }
        Ok(())
    }

    /// [`Self::with_locked_state`], applied to the separate removed-starter-packs
    /// store instead of the main entries list — see that method's doc comment for the
    /// full rationale.
    fn with_locked_removed_state<T>(&mut self, mutate: impl FnOnce(&mut Vec<PackSource>) -> T) -> Result<T, RegistryError> {
        let lock_file = self.open_lock_file()?;
        let mut file_lock = fd_lock::RwLock::new(lock_file);
        let _guard = file_lock.write().map_err(|e| RegistryError::LockFailed { message: e.to_string() })?;

        let mut fresh = RemovedStarterPacksConfig::get_entry(&self.removed_config).unwrap_or_else(|(_errors, default)| default);
        let result = mutate(&mut fresh.removed);
        fresh.write_entry(&self.removed_config).map_err(|e| RegistryError::Storage { message: e.to_string() })?;
        self.removed_state = fresh;
        Ok(result)
    }

    /// Open (creating if necessary) [`Self::lock_path`] — the shared cross-process
    /// advisory-lock file guarding a read-modify-write cycle against either of this
    /// crate's two `cosmic-config` stores. Split out of
    /// [`Self::with_locked_state`]/[`Self::with_locked_removed_state`] purely to avoid
    /// duplicating this open-the-file boilerplate between them. Returns the plain
    /// `File`, not an already-locked guard — `fd_lock`'s `RwLock`/`RwLockWriteGuard`
    /// are tied together by a borrow, so the `RwLock` itself has to be constructed in
    /// each caller's own stack frame for the guard to have somewhere to live; each
    /// caller still does that (two lines: `let mut file_lock =
    /// fd_lock::RwLock::new(...); let _guard = file_lock.write()?;`) immediately
    /// after calling this.
    fn open_lock_file(&self) -> Result<std::fs::File, RegistryError> {
        if let Some(parent) = self.lock_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| RegistryError::LockFailed { message: e.to_string() })?;
        }
        std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&self.lock_path)
            .map_err(|e| RegistryError::LockFailed { message: e.to_string() })
    }

    /// Acquire the cross-process lock, then run `mutate` against a *freshly re-read*
    /// snapshot of `entries` — not `self.state.entries`, which may already be stale
    /// relative to another process's concurrent write. This is what actually closes
    /// the lost-update race: `wallpaperctl register` (a fresh process) and
    /// `wallpaperd`'s own long-lived in-process `Registry` would otherwise each load a
    /// snapshot once and unconditionally overwrite the whole entries list on
    /// `persist()` — whichever wrote last would silently discard the other's change.
    /// Re-reading fresh *under the lock*, immediately before mutating and writing
    /// back, closes that lost-update window: by the time `mutate` runs, no other
    /// process can be mid-write, and this call sees whatever the last writer (this
    /// process or another) actually committed. `self.state` is updated to match what
    /// was written, so this instance's own subsequent reads (e.g. `known_packs`) stay
    /// consistent too.
    ///
    /// Guards the main `entries` list specifically — `register_with_origin`'s
    /// mutation, and the `entries`-retaining half of `remove`'s. `remove`'s *other*
    /// mutation, of the separate removed-starter-packs store, is guarded the same way
    /// but through [`Self::with_locked_removed_state`] instead.
    /// [`Registry::reload_all`]'s own `persist()` call remains unlocked/best-effort
    /// (its own doc comment already frames it that way): the `status` field it
    /// refreshes is cheaply re-derivable on the very next reload, so a lost update
    /// there is much lower stakes than losing a `register`/`remove` outright.
    fn with_locked_state<T>(&mut self, mutate: impl FnOnce(&mut Vec<PackRegistryEntry>) -> T) -> Result<T, RegistryError> {
        let lock_file = self.open_lock_file()?;
        let mut file_lock = fd_lock::RwLock::new(lock_file);
        let _guard = file_lock.write().map_err(|e| RegistryError::LockFailed { message: e.to_string() })?;

        let mut fresh = RegistryConfig::get_entry(&self.config).unwrap_or_else(|(_errors, default)| default);
        let result = mutate(&mut fresh.entries);
        fresh.write_entry(&self.config).map_err(|e| RegistryError::Storage { message: e.to_string() })?;
        self.state = fresh;
        Ok(result)
    }

    /// Whether `source` was previously registered as a starter pack and explicitly
    /// removed — the check `wallpaperd`'s own first-run self-registration makes
    /// before re-adding the bundled starter pack, so an upgrade never silently undoes
    /// a user's deliberate removal.
    pub fn is_removed_starter_pack(&self, source: &PackSource) -> bool {
        self.removed_state.removed.contains(source)
    }

    /// Every currently known pack, in registration order.
    pub fn known_packs(&self) -> Vec<PackRegistryEntry> {
        self.state.entries.clone()
    }

    /// Attempt to reload every known pack independently — one failing pack is marked
    /// [`RegistryStatus::Unavailable`] without preventing the others from loading.
    /// Returns each source's fresh load result alongside the updated status.
    pub fn reload_all(&mut self) -> Vec<(PackSource, Result<LoadedPack, ManifestError>)> {
        let mut results = Vec::with_capacity(self.state.entries.len());
        for entry in &mut self.state.entries {
            let outcome = load_pack(entry.source.path());
            entry.status = if outcome.is_ok() { RegistryStatus::Known } else { RegistryStatus::Unavailable };
            results.push((entry.source.clone(), outcome));
        }
        // Best-effort persist of the refreshed statuses; a failure here doesn't change
        // what's returned to the caller — a secondary write failure shouldn't mask
        // the primary reload result.
        let _ = self.persist();
        results
    }

    fn persist(&self) -> Result<(), RegistryError> {
        self.state
            .write_entry(&self.config)
            .map_err(|e| RegistryError::Storage { message: e.to_string() })
    }
}

/// [`Registry::open`]'s production entry point for [`RegistryConfig`]'s migration —
/// see [`migrate_registry_config_core`] for the actual logic.
fn migrate_registry_config(new_config: &Config) -> RegistryConfig {
    migrate_registry_config_core(new_config, Config::new(OLD_REGISTRY_CONFIG_ID, RegistryConfig::VERSION))
}

/// The pure migration core, taking both handles already open — split out purely so
/// tests can construct the old handle via a custom path instead of the real,
/// well-known system path `Config::new` always resolves to.
fn migrate_registry_config_core(new_config: &Config, old_config: Result<Config, cosmic_config::Error>) -> RegistryConfig {
    let current = RegistryConfig::get_entry(new_config).unwrap_or_else(|(_errors, default)| default);
    if current != RegistryConfig::default() {
        return current; // already migrated, or configured fresh under the new id
    }
    let Ok(old_config) = old_config else {
        return current; // no old store at all — fresh install, not an error
    };
    let old = RegistryConfig::get_entry(&old_config).unwrap_or_else(|(_errors, default)| default);
    if old == RegistryConfig::default() {
        return current; // old install existed but was never actually configured
    }
    // Best-effort: a write failure here just means the next call retries the same
    // migration (the old store is never mutated, so nothing is lost either way).
    let _ = old.write_entry(new_config);
    old
}

/// [`Registry::open`]'s production entry point for [`RemovedStarterPacksConfig`]'s
/// migration — see [`migrate_removed_starter_packs_config_core`] for the actual
/// logic.
fn migrate_removed_starter_packs_config(new_config: &Config) -> RemovedStarterPacksConfig {
    migrate_removed_starter_packs_config_core(
        new_config,
        Config::new(OLD_REMOVED_STARTER_PACKS_CONFIG_ID, RemovedStarterPacksConfig::VERSION),
    )
}

/// The pure migration core — see [`migrate_registry_config_core`]'s doc, identical
/// shape applied to the other of this crate's two `cosmic-config` entries.
fn migrate_removed_starter_packs_config_core(
    new_config: &Config,
    old_config: Result<Config, cosmic_config::Error>,
) -> RemovedStarterPacksConfig {
    let current = RemovedStarterPacksConfig::get_entry(new_config).unwrap_or_else(|(_errors, default)| default);
    if current != RemovedStarterPacksConfig::default() {
        return current;
    }
    let Ok(old_config) = old_config else {
        return current;
    };
    let old = RemovedStarterPacksConfig::get_entry(&old_config).unwrap_or_else(|(_errors, default)| default);
    if old == RemovedStarterPacksConfig::default() {
        return current;
    }
    let _ = old.write_entry(new_config);
    old
}

/// Rewrites `source` in place if it's an exact match for the pre-rename starter-pack
/// system path, returning whether it changed anything. Only the `Directory` variant is
/// checked — the bundled starter pack is always a manifest-based directory pack, never
/// a `StaticFile` (see `starter_pack.rs`'s own doc comment).
fn repair_source_if_relocated(source: &mut PackSource) -> bool {
    let PackSource::Directory(path) = source else { return false };
    if path.as_path() != std::path::Path::new(OLD_STARTER_PACK_SYSTEM_PATH) {
        return false;
    }
    *path = std::path::PathBuf::from(NEW_STARTER_PACK_SYSTEM_PATH);
    true
}

/// [`Registry::open`]'s repair pass over every registered pack's source. Deliberately
/// `fold`, not `any` — `any` short-circuits on the first match, which would silently
/// leave every subsequent relocated entry unrepaired; every element must be visited for
/// its mutating side effect regardless of what earlier ones returned.
#[allow(clippy::unnecessary_fold)]
fn repair_relocated_starter_pack_entries(entries: &mut [PackRegistryEntry]) -> bool {
    entries.iter_mut().fold(false, |changed, entry| repair_source_if_relocated(&mut entry.source) || changed)
}

/// [`Registry::open`]'s repair pass over the removed-starter-packs record — see
/// [`repair_relocated_starter_pack_entries`]'s doc for why this is `fold`, not `any`.
#[allow(clippy::unnecessary_fold)]
fn repair_relocated_starter_pack_removed(removed: &mut [PackSource]) -> bool {
    removed.iter_mut().fold(false, |changed, source| repair_source_if_relocated(source) || changed)
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

        // Reopen fresh, same custom path — simulates a daemon restart.
        let reopened = Registry::open_at(dir.path()).unwrap();
        assert!(reopened.known_packs().iter().any(|e| e.source == src));
    }

    /// `wallpaperctl register` (a fresh process) and `wallpaperd`'s own long-lived
    /// registry, both opened from the same on-disk store *before* either writes, then
    /// each registering a different pack. Without the locked read-modify-write in
    /// `with_locked_state`, whichever `persist()` landed last would silently discard
    /// the other's entry (both loaded their snapshot once, at `open_at` time, and
    /// wrote it back unconditionally). `register`'s locked read-modify-write instead
    /// re-reads fresh from disk under the lock, so the second write can't clobber the
    /// first.
    #[test]
    fn concurrent_persist_serializes() {
        let dir = tempfile::tempdir().unwrap();
        // Two independent `Registry` handles on the same on-disk store, opened before
        // either has written anything — exactly the "two processes, same store"
        // shape a daemon-plus-CLI-invocation race has.
        let mut registry_a = Registry::open_at(dir.path()).unwrap();
        let mut registry_b = Registry::open_at(dir.path()).unwrap();

        let file_a = dir.path().join("a.jpg");
        let file_b = dir.path().join("b.jpg");
        std::fs::write(&file_a, b"a").unwrap();
        std::fs::write(&file_b, b"b").unwrap();

        registry_a.register(source(&file_a)).unwrap();
        registry_b.register(source(&file_b)).unwrap();

        // Neither write was lost — re-opening a third handle sees both.
        let reopened = Registry::open_at(dir.path()).unwrap();
        let known = reopened.known_packs();
        assert_eq!(known.len(), 2, "expected both concurrent registrations to survive, got {known:?}");
        assert!(known.iter().any(|e| e.source == source(&file_a)));
        assert!(known.iter().any(|e| e.source == source(&file_b)));
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

    /// `PackRegistryEntry.origin` defaults to `User` for a freshly-registered pack
    /// (the only path `wallpaperctl register`/the GUI can take).
    #[test]
    fn register_defaults_to_user_origin() {
        let (mut registry, dir) = temp_registry();
        let file = dir.path().join("pack.jpg");
        std::fs::write(&file, b"x").unwrap();
        registry.register(source(&file)).unwrap();

        assert_eq!(registry.known_packs()[0].origin, PackOrigin::User);
    }

    /// A pre-existing (pre-spec-7) registry entry — hand-written in the exact
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

    /// Removing a `Package`-origin entry appends its source to the removed-
    /// starter-packs store; removing a `User`-origin entry does not.
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

    /// A simulated `postinst`/`wallpaperd` first-run re-registration attempt
    /// skips a starter pack already listed as removed — this crate's
    /// `register_with_origin` doesn't itself consult the removed list (that
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

    /// Migration test cases (idempotent real-content migration, failed-open no-op,
    /// never-configured-old-store no-op, already-populated-new-store no-op), applied
    /// to both of this crate's `cosmic-config` entries.
    mod migration {
        use super::*;

        fn old_registry(root: &std::path::Path) -> Config {
            Config::with_custom_path(OLD_REGISTRY_CONFIG_ID, RegistryConfig::VERSION, root.to_path_buf()).unwrap()
        }

        fn new_registry(root: &std::path::Path) -> Config {
            Config::with_custom_path(REGISTRY_CONFIG_ID, RegistryConfig::VERSION, root.to_path_buf()).unwrap()
        }

        fn entry(path: &str) -> PackRegistryEntry {
            PackRegistryEntry { source: PackSource::StaticFile(path.into()), status: RegistryStatus::Known, origin: PackOrigin::User, display_name: None }
        }

        #[test]
        fn registry_migrates_real_content_and_is_idempotent_on_a_second_call() {
            let dir = tempfile::tempdir().unwrap();
            let old = old_registry(dir.path());
            RegistryConfig { entries: vec![entry("/old.jpg")] }.write_entry(&old).unwrap();

            let new = new_registry(dir.path());
            let migrated = migrate_registry_config_core(&new, Ok(old_registry(dir.path())));
            assert_eq!(migrated.entries, vec![entry("/old.jpg")]);
            assert_eq!(RegistryConfig::get_entry(&new).unwrap().entries, vec![entry("/old.jpg")]);

            let migrated_again = migrate_registry_config_core(&new, Ok(old_registry(dir.path())));
            assert_eq!(migrated_again, migrated);
        }

        #[test]
        fn registry_failed_old_config_open_is_a_silent_no_op() {
            let dir = tempfile::tempdir().unwrap();
            let new = new_registry(dir.path());
            let invalid_open = Config::with_custom_path("../invalid", RegistryConfig::VERSION, dir.path().to_path_buf());
            assert!(invalid_open.is_err(), "fixture assumption: this name is rejected by cosmic-config");

            let result = migrate_registry_config_core(&new, invalid_open);
            assert_eq!(result, RegistryConfig::default());
        }

        #[test]
        fn registry_old_store_exists_but_was_never_configured_stays_default() {
            let dir = tempfile::tempdir().unwrap();
            let _ = old_registry(dir.path());
            let new = new_registry(dir.path());

            let result = migrate_registry_config_core(&new, Ok(old_registry(dir.path())));
            assert_eq!(result, RegistryConfig::default());
        }

        #[test]
        fn registry_new_store_already_populated_never_consults_the_old_one() {
            let dir = tempfile::tempdir().unwrap();
            let old = old_registry(dir.path());
            RegistryConfig { entries: vec![entry("/old.jpg")] }.write_entry(&old).unwrap();

            let new = new_registry(dir.path());
            RegistryConfig { entries: vec![entry("/already-configured.jpg")] }.write_entry(&new).unwrap();

            let result = migrate_registry_config_core(&new, Ok(old_registry(dir.path())));
            assert_eq!(result.entries, vec![entry("/already-configured.jpg")]);
        }

        fn old_removed(root: &std::path::Path) -> Config {
            Config::with_custom_path(OLD_REMOVED_STARTER_PACKS_CONFIG_ID, RemovedStarterPacksConfig::VERSION, root.to_path_buf()).unwrap()
        }

        fn new_removed(root: &std::path::Path) -> Config {
            Config::with_custom_path(REMOVED_STARTER_PACKS_CONFIG_ID, RemovedStarterPacksConfig::VERSION, root.to_path_buf()).unwrap()
        }

        #[test]
        fn removed_starter_packs_migrates_real_content_and_is_idempotent() {
            let dir = tempfile::tempdir().unwrap();
            let old = old_removed(dir.path());
            let removed_src = PackSource::StaticFile("/starter.jpg".into());
            RemovedStarterPacksConfig { removed: vec![removed_src.clone()] }.write_entry(&old).unwrap();

            let new = new_removed(dir.path());
            let migrated = migrate_removed_starter_packs_config_core(&new, Ok(old_removed(dir.path())));
            assert_eq!(migrated.removed, vec![removed_src.clone()]);

            let migrated_again = migrate_removed_starter_packs_config_core(&new, Ok(old_removed(dir.path())));
            assert_eq!(migrated_again, migrated);
        }

        #[test]
        fn removed_starter_packs_failed_old_config_open_is_a_silent_no_op() {
            let dir = tempfile::tempdir().unwrap();
            let new = new_removed(dir.path());
            let invalid_open = Config::with_custom_path("../invalid", RemovedStarterPacksConfig::VERSION, dir.path().to_path_buf());
            assert!(invalid_open.is_err(), "fixture assumption: this name is rejected by cosmic-config");

            let result = migrate_removed_starter_packs_config_core(&new, invalid_open);
            assert_eq!(result, RemovedStarterPacksConfig::default());
        }

        #[test]
        fn removed_starter_packs_old_store_exists_but_was_never_configured_stays_default() {
            let dir = tempfile::tempdir().unwrap();
            let _ = old_removed(dir.path());
            let new = new_removed(dir.path());

            let result = migrate_removed_starter_packs_config_core(&new, Ok(old_removed(dir.path())));
            assert_eq!(result, RemovedStarterPacksConfig::default());
        }

        #[test]
        fn removed_starter_packs_new_store_already_populated_never_consults_the_old_one() {
            let dir = tempfile::tempdir().unwrap();
            let old = old_removed(dir.path());
            RemovedStarterPacksConfig { removed: vec![PackSource::StaticFile("/old.jpg".into())] }.write_entry(&old).unwrap();

            let already_configured = PackSource::StaticFile("/already-configured.jpg".into());
            let new = new_removed(dir.path());
            RemovedStarterPacksConfig { removed: vec![already_configured.clone()] }.write_entry(&new).unwrap();

            let result = migrate_removed_starter_packs_config_core(&new, Ok(old_removed(dir.path())));
            assert_eq!(result.removed, vec![already_configured]);
        }
    }

    /// Regression: a registry entry migrated verbatim from the old application id
    /// still points at the pre-rename starter-pack system path, which no longer
    /// exists once the old package is removed — `Registry::open`'s repair pass must
    /// fix this up.
    mod starter_pack_relocation {
        use super::*;

        fn registry_entry(source: PackSource, origin: PackOrigin) -> PackRegistryEntry {
            PackRegistryEntry { source, status: RegistryStatus::Unavailable, origin, display_name: None }
        }

        #[test]
        fn repairs_a_registered_entry_pointing_at_the_old_starter_pack_path() {
            let mut entries =
                vec![registry_entry(PackSource::Directory(OLD_STARTER_PACK_SYSTEM_PATH.into()), PackOrigin::Package)];

            assert!(repair_relocated_starter_pack_entries(&mut entries));
            assert_eq!(entries[0].source, PackSource::Directory(NEW_STARTER_PACK_SYSTEM_PATH.into()));
        }

        #[test]
        fn leaves_an_unrelated_entry_untouched() {
            let mut entries = vec![registry_entry(PackSource::Directory("/home/user/my-pack".into()), PackOrigin::User)];
            let original = entries.clone();

            assert!(!repair_relocated_starter_pack_entries(&mut entries));
            assert_eq!(entries, original);
        }

        #[test]
        fn is_a_harmless_noop_once_already_repaired() {
            let mut entries =
                vec![registry_entry(PackSource::Directory(NEW_STARTER_PACK_SYSTEM_PATH.into()), PackOrigin::Package)];

            assert!(!repair_relocated_starter_pack_entries(&mut entries));
        }

        #[test]
        fn repairs_the_removed_starter_packs_record_too() {
            let mut removed = vec![PackSource::Directory(OLD_STARTER_PACK_SYSTEM_PATH.into())];

            assert!(repair_relocated_starter_pack_removed(&mut removed));
            assert_eq!(removed, vec![PackSource::Directory(NEW_STARTER_PACK_SYSTEM_PATH.into())]);
        }

        #[test]
        fn does_not_touch_a_static_file_source_even_with_a_matching_path() {
            // The starter pack is always a Directory pack — a StaticFile source is
            // never relocated even if (implausibly) its path matched.
            let mut source = PackSource::StaticFile(OLD_STARTER_PACK_SYSTEM_PATH.into());
            assert!(!repair_source_if_relocated(&mut source));
            assert_eq!(source, PackSource::StaticFile(OLD_STARTER_PACK_SYSTEM_PATH.into()));
        }
    }

    // --- Spec 012: PackRegistryEntry.display_name / Registry::set_display_name ---

    #[test]
    fn set_display_name_round_trips_through_save_and_reload() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("sunrise.png");
        std::fs::write(&file, b"x").unwrap();
        let src = source(&file);

        let (mut registry, custom_dir) = temp_registry();
        registry.register(src.clone()).unwrap();
        registry.set_display_name(&src, Some("Sunrise Glow".to_string())).unwrap();

        let reopened = Registry::open_at(custom_dir.path()).unwrap();
        let entry = reopened.known_packs().into_iter().find(|e| e.source == src).unwrap();
        assert_eq!(entry.display_name.as_deref(), Some("Sunrise Glow"));
    }

    #[test]
    fn set_display_name_to_none_clears_a_previously_set_name() {
        let (mut registry, dir) = temp_registry();
        let file = dir.path().join("a.png");
        std::fs::write(&file, b"x").unwrap();
        let src = source(&file);
        registry.register(src.clone()).unwrap();
        registry.set_display_name(&src, Some("Custom".to_string())).unwrap();

        registry.set_display_name(&src, None).unwrap();

        let entry = registry.known_packs().into_iter().find(|e| e.source == src).unwrap();
        assert_eq!(entry.display_name, None);
    }

    #[test]
    fn set_display_name_on_an_unregistered_source_is_a_harmless_noop() {
        let (mut registry, dir) = temp_registry();
        let file = dir.path().join("never-registered.png");
        let src = source(&file);

        assert!(registry.set_display_name(&src, Some("X".to_string())).is_ok());
        assert!(registry.known_packs().is_empty(), "must not create an entry just to name it");
    }

    /// A display name doesn't survive its pack being deleted — `remove` discards the
    /// whole entry, and `register`ing the same source again afterward starts fresh
    /// (`display_name: None`), never recovering the previous value.
    #[test]
    fn a_removed_and_re_registered_source_starts_with_no_display_name() {
        let (mut registry, dir) = temp_registry();
        let file = dir.path().join("a.png");
        std::fs::write(&file, b"x").unwrap();
        let src = source(&file);
        registry.register(src.clone()).unwrap();
        registry.set_display_name(&src, Some("Custom Name".to_string())).unwrap();

        registry.remove(&src).unwrap();
        registry.register(src.clone()).unwrap();

        let entry = registry.known_packs().into_iter().find(|e| e.source == src).unwrap();
        assert_eq!(entry.display_name, None, "a custom name must not silently reappear after delete + re-add");
    }

    /// An entry written before this field existed has no `display_name` key in its
    /// on-disk RON at all — `#[serde(default)]` must still load it as `None` rather
    /// than failing to parse. Mirrors `pre_existing_entry_with_no_origin_field_loads_
    /// as_user_origin` above exactly: hand-writes the pre-existing 3-field shape
    /// (`source`, `status`, `origin` — no `display_name`) directly, rather than
    /// round-tripping through this crate's own `Serialize` impl, which would always
    /// include the field and so could never actually exercise serde's missing-field
    /// fallback.
    #[test]
    fn pre_existing_entry_with_no_display_name_field_loads_as_none() {
        let dir = tempfile::tempdir().unwrap();
        let entries_path = dir.path().join("cosmic").join(REGISTRY_CONFIG_ID).join("v1").join("entries");
        std::fs::create_dir_all(entries_path.parent().unwrap()).unwrap();
        std::fs::write(&entries_path, r#"[(source: StaticFile("/home/user/old-pack.jpg"), status: Known, origin: User)]"#).unwrap();

        let registry = Registry::open_at(dir.path()).unwrap();
        let packs = registry.known_packs();
        assert_eq!(packs.len(), 1);
        assert_eq!(packs[0].display_name, None);
    }
}
