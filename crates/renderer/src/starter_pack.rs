//! Starter-pack first-run self-registration (spec 7 US2, FR-008/FR-010/FR-011).
//!
//! **Design note, a real correction to tasks.md's literal wording**: research.md/
//! tasks.md originally described `postinst` (spec 5) as the place a starter pack gets
//! registered. This doesn't hold up mechanically: `postinst` runs once, as root, at
//! package-install time, with no per-user context at all — but `cosmic-config` (and
//! therefore this project's pack registry) is a per-user store under each user's own
//! XDG config directory. `postinst` has no way to know which user(s) will ever run this
//! software, let alone write into their individual config directories safely. This
//! module implements the same observable requirement (FR-008: zero-config first run)
//! the *correct* way instead: `wallpaperd`'s own startup, which already runs in the
//! right per-user context (a `systemd --user` service, spec 5), checks once whether
//! this looks like a fresh install and self-registers the bundled starter pack if so.
//! `postinst`'s only remaining job for this feature is installing the static asset
//! files to a well-known system path — a packaging concern handled by `cargo-deb`'s own
//! `assets` list (`crates/renderer/Cargo.toml`), not a script change.

use std::path::Path;

use pack_loader::{PackOrigin, PackSource, Registry};

use crate::output::RendererConfig;

/// The well-known system path the bundled starter pack's static assets are installed
/// to. Not present at all in a dev/`cargo run` environment — handled gracefully below
/// (`PackSource::resolve` failing on a missing path), not an error.
pub const STARTER_PACK_SYSTEM_PATH: &str = "/usr/share/dynamic-wallpaper/starter-pack";

/// Register and assign the bundled starter pack if this looks like a genuinely fresh
/// install (FR-008) — never if the user has explicitly removed it before (FR-010,
/// [`Registry::is_removed_starter_pack`]), and never overriding any assignment already
/// made (FR-011: only touches `renderer_config.same_pack_everywhere` when it and
/// `overrides` are both still completely untouched). Safe to call unconditionally on
/// every `wallpaperd` startup — every branch below is a fast, idempotent no-op once a
/// fresh install has already been handled.
///
/// Returns `true` if it registered (and possibly assigned) the starter pack — the
/// caller (`wallpaperd.rs`) is responsible for persisting `renderer_config` in that
/// case (`Registry`'s own mutation is already persisted internally); this function
/// only receives the plain, already-loaded `RendererConfig` value, not a `Config`
/// handle, so it stays simple to unit-test.
pub fn maybe_register(starter_pack_path: &Path, pack_registry: &mut Registry, renderer_config: &mut RendererConfig) -> bool {
    let Ok(source) = PackSource::resolve(starter_pack_path) else {
        return false; // Not installed via the .deb package — nothing to do.
    };
    if pack_registry.is_removed_starter_pack(&source) {
        return false; // FR-010: a past explicit removal is permanent.
    }
    if !pack_registry.known_packs().is_empty() {
        return false; // Not a fresh install — something is already registered.
    }
    if pack_registry.register_with_origin(source.clone(), PackOrigin::Package).is_err() {
        return false;
    }
    if renderer_config.same_pack_everywhere.is_none() && renderer_config.overrides.is_empty() {
        // FR-011: only ever sets the default when nothing else has been configured —
        // never overrides a user's own explicit assignment made before or after
        // install.
        renderer_config.same_pack_everywhere = Some(source);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn starter_dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn temp_registry() -> (Registry, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        (Registry::open_at(dir.path()).unwrap(), dir)
    }

    /// spec.md US2 Scenario 1: on a fresh install (empty registry, untouched
    /// renderer config), the starter pack is registered as `Package`-origin and
    /// assigned via the same-everywhere toggle.
    #[test]
    fn fresh_install_registers_and_assigns_the_starter_pack() {
        let starter = starter_dir();
        let (mut registry, _dir) = temp_registry();
        let mut renderer_config = RendererConfig::default();

        let did_register = maybe_register(starter.path(), &mut registry, &mut renderer_config);

        assert!(did_register);
        let packs = registry.known_packs();
        assert_eq!(packs.len(), 1);
        assert_eq!(packs[0].origin, PackOrigin::Package);
        assert!(renderer_config.same_pack_everywhere.is_some());
    }

    /// Not installed via the package (dev environment, or the asset simply isn't
    /// there) — a graceful no-op, not an error.
    #[test]
    fn missing_starter_pack_path_is_a_harmless_noop() {
        let (mut registry, _dir) = temp_registry();
        let mut renderer_config = RendererConfig::default();

        let did_register = maybe_register(Path::new("/nonexistent/starter-pack-path"), &mut registry, &mut renderer_config);

        assert!(!did_register);
        assert!(registry.known_packs().is_empty());
    }

    /// FR-010: a user's past explicit removal is permanent — a later "first run"
    /// check (e.g. a package upgrade re-invoking this same startup path) must not
    /// silently re-add it.
    #[test]
    fn previously_removed_starter_pack_is_never_re_registered() {
        let starter = starter_dir();
        let (mut registry, _dir) = temp_registry();
        let source = PackSource::resolve(starter.path()).unwrap();
        registry.register_with_origin(source.clone(), PackOrigin::Package).unwrap();
        registry.remove(&source).unwrap();

        let mut renderer_config = RendererConfig::default();
        let did_register = maybe_register(starter.path(), &mut registry, &mut renderer_config);

        assert!(!did_register);
        assert!(registry.known_packs().is_empty());
        assert!(renderer_config.same_pack_everywhere.is_none());
    }

    /// FR-011 (spec.md US2 Scenario 3): a user's existing explicit assignment, made
    /// before this check ever runs, is never overridden.
    #[test]
    fn existing_user_assignment_is_never_overridden() {
        let starter = starter_dir();
        let (mut registry, _dir) = temp_registry();
        let mut renderer_config = RendererConfig::default();
        renderer_config.overrides.insert("eDP-1".to_string(), pack_loader::PackSource::StaticFile("/user/pack.jpg".into()));

        let did_register = maybe_register(starter.path(), &mut registry, &mut renderer_config);

        // The starter pack is still registered (FR-008 — it should be *available*,
        // browsable via the GUI/`list packs`), just never force-assigned anywhere.
        assert!(did_register);
        assert!(registry.known_packs()[0].origin == PackOrigin::Package);
        assert!(renderer_config.same_pack_everywhere.is_none());
        assert_eq!(renderer_config.overrides.len(), 1, "the user's own override is untouched");
    }

    /// Not a fresh install — some other pack is already known (registered, even if
    /// not assigned) — `known_packs().is_empty()` is the "fresh install" signal, per
    /// quickstart.md's own framing ("no prior wallpaperctl commands ever run").
    #[test]
    fn a_registry_with_any_existing_entry_is_not_touched() {
        let starter = starter_dir();
        let (mut registry, dir) = temp_registry();
        let other = dir.path().join("other.jpg");
        std::fs::write(&other, b"x").unwrap();
        registry.register(pack_loader::PackSource::resolve(&other).unwrap()).unwrap();

        let mut renderer_config = RendererConfig::default();
        let did_register = maybe_register(starter.path(), &mut registry, &mut renderer_config);

        assert!(!did_register);
        assert_eq!(registry.known_packs().len(), 1);
    }

    /// Idempotent across repeated startups: calling this twice in a row (as every
    /// `wallpaperd` restart does) only registers once.
    #[test]
    fn calling_twice_only_registers_once() {
        let starter = starter_dir();
        let (mut registry, _dir) = temp_registry();
        let mut renderer_config = RendererConfig::default();

        assert!(maybe_register(starter.path(), &mut registry, &mut renderer_config));
        assert!(!maybe_register(starter.path(), &mut registry, &mut renderer_config));
        assert_eq!(registry.known_packs().len(), 1);
    }
}
