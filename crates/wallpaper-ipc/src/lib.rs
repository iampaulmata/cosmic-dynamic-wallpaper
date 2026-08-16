//! `wallpaper-ipc` — shared `cosmic-config` schema types and D-Bus client for the
//! Cosmic Dynamic Wallpaper project (spec 7 research.md R2, contracts/wallpaper-ipc-crate.md).
//!
//! The single source of truth `crates/renderer`, `crates/wallpaperctl`, and
//! `crates/wallpaper-settings` all depend on, replacing three independently-defined
//! copies of the same shapes — this project has already been bitten once by exactly
//! that class of bug (see [`renderer_config`]'s module doc). Deliberately dependency-
//! light: no `wgpu`/`smithay-client-toolkit`/`wayland-client`/`calloop`, preserving the
//! property spec 4 originally established for `wallpaperctl` (never linking spec 3's
//! heavy Wayland/GPU dependencies) for the new GUI crate too.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod dbus_client;
pub mod location_config;
pub mod renderer_config;

pub use dbus_client::{DbusClient, DbusError, QueryEntry, BUS_NAME, INTERFACE, OBJECT_PATH};
pub use location_config::{effective_location, LocationConfigEntry, LocationMode, ResolutionStatus, LOCATION_CONFIG_ID};
pub use renderer_config::{
    effective_pack, resolve_assignment, OutputAssignment, OutputId, OutputIdError, RendererConfig, MAX_OUTPUT_ID_BYTES, RENDERER_CONFIG_ID,
};

/// STUN-disclosure copy FR-014 (spec 7) requires before a user opts into
/// IP-geolocation — the one external network touchpoint that mode has. **The single
/// source of truth** for this text (spec 008 research.md R4): before this constant
/// existed here, `crates/wallpaperctl` and `crates/wallpaper-settings` each carried
/// their own independent copy of the same literal string, despite a doc comment
/// claiming they were kept in sync — exactly the drift class this crate exists to
/// prevent (spec 7 research.md R2). Sentence case, a complete grammatical sentence
/// (spec 008 FR-009) — not a lowercase-leading fragment.
pub const IP_GEOLOCATION_DISCLOSURE: &str = "IP-geolocation uses a bundled offline database for the location lookup, and briefly asks a STUN server for this machine's public IP address first, since that's not something the bundled database can determine on its own.";

/// Best-effort, Unix-only: ensure the directory a `cosmic-config` entry is about to be
/// written to already exists at `0700` *before* that write happens (spec 011
/// adversarial re-review finding 2 — `Self::tighten_config_dir_permissions` alone left
/// a real window where the entry's data was briefly written at whatever broader
/// default permissions `cosmic-config`'s own directory creation + the process umask
/// produce). Safe to call unconditionally before every save, not just the first one.
///
/// Deliberately does **not** use `DirBuilder::mode` on a single `recursive(true)`
/// call (a first version of this fix did, and it was itself a finding in the very
/// next adversarial re-review pass, spec 011 adversarial re-review finding 1):
/// `DirBuilder::mode` applies the given mode to *every* directory it has to create
/// along the way, not just the final leaf — verified empirically. Since `dir` here is
/// `dirs::config_dir()/cosmic/{app_id}/v{version}`, that would tighten
/// `dirs::config_dir()/cosmic` itself to `0700` if it didn't already exist — the
/// shared umbrella directory every COSMIC application's own config subtree lives
/// under, which this crate has no authority to lock down as a side effect of
/// protecting its own two subdirectories. Instead: create every ancestor (including
/// this app's own `{app_id}` namespace directory, which holds no sensitive data
/// itself — `cosmic-config` only ever writes actual field files inside the
/// `v{version}` leaf) at whatever default permissions `create_dir_all`/the process
/// umask produce, then create *only* the `v{version}` leaf explicitly and tighten
/// that one directory alone.
#[cfg(unix)]
pub(crate) fn ensure_config_dir_permissions_before_write(app_id: &str, version: u64) {
    use std::os::unix::fs::PermissionsExt;

    let Some(base) = dirs::config_dir() else {
        return;
    };
    let parent = base.join("cosmic").join(app_id);
    let dir = parent.join(format!("v{version}"));

    if let Err(error) = std::fs::create_dir_all(&parent) {
        tracing::warn!(?error, path = %parent.display(), "failed to create config directory ancestors before save");
        return;
    }
    match std::fs::create_dir(&dir) {
        Ok(()) => {
            if let Err(error) = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)) {
                tracing::warn!(?error, path = %dir.display(), "failed to tighten freshly-created config directory permissions before save");
            }
        }
        // Already exists — `tighten_config_dir_permissions` (after the write) is what
        // covers this case, whether it was left at broader permissions from before
        // this fix landed or from any other cause.
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => {
            tracing::warn!(?error, path = %dir.display(), "failed to pre-create config directory before save");
        }
    }
}

/// Best-effort, Unix-only: tighten the on-disk directory a `cosmic-config` entry was
/// just written to, to `0700` (spec 011 US7 FR-030, research.md R25) — location and
/// renderer config can hold locally-sensitive detail (GPS coordinates, filesystem
/// paths), and `cosmic-config`'s own directory creation does not restrict group/other
/// read access by default. `cosmic_config::Config` exposes no path accessor of its
/// own, so this reconstructs its internal-but-stable on-disk convention
/// (`dirs::config_dir()/cosmic/{app_id}/v{version}/`) the same way
/// `pack_loader::registry`'s lock-file path does. Never fails the caller (constitution
/// Principle VIII) — the config write itself already succeeded by the time this runs,
/// so a permission-tightening failure is logged, not propagated.
///
/// Kept as a second pass *after* the write, alongside
/// [`ensure_config_dir_permissions_before_write`] which now runs *before* it (spec 011
/// adversarial re-review finding 2) — this one still matters for a directory that
/// already existed at broader permissions from before that pre-write fix landed, which
/// the pre-write pass leaves untouched (it only tightens a directory it just created
/// itself, never one that already existed).
#[cfg(unix)]
pub(crate) fn tighten_config_dir_permissions(app_id: &str, version: u64) {
    use std::os::unix::fs::PermissionsExt;

    let Some(base) = dirs::config_dir() else {
        return;
    };
    let dir = base.join("cosmic").join(app_id).join(format!("v{version}"));
    if let Err(error) = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)) {
        tracing::warn!(?error, path = %dir.display(), "failed to tighten config directory permissions after save");
    }
}

#[cfg(test)]
mod disclosure_tests {
    use super::IP_GEOLOCATION_DISCLOSURE;

    /// T025 (spec 008 FR-009): a properly capitalized, complete sentence — not a
    /// lowercase-leading fragment.
    #[test]
    fn ip_geolocation_disclosure_is_sentence_case_and_terminated() {
        let first_char = IP_GEOLOCATION_DISCLOSURE.chars().next().expect("non-empty");
        assert!(first_char.is_uppercase(), "must start with an uppercase letter: {IP_GEOLOCATION_DISCLOSURE:?}");
        assert!(IP_GEOLOCATION_DISCLOSURE.ends_with('.'), "must end with terminal punctuation: {IP_GEOLOCATION_DISCLOSURE:?}");
    }
}

#[cfg(unix)]
#[cfg(test)]
mod config_dir_permission_tests {
    use std::os::unix::fs::PermissionsExt;

    use super::{ensure_config_dir_permissions_before_write, test_support};

    /// Spec 011 adversarial re-review finding 1 — the audit's own reproduction: an
    /// earlier version of this fix used `DirBuilder::mode`, which applies the given
    /// mode to *every* directory it creates along a `recursive(true)` path, not just
    /// the final leaf — so if `dirs::config_dir()/cosmic` (the shared umbrella
    /// directory every COSMIC app's own config subtree lives under) didn't already
    /// exist, it got swept into `0700` too, an authority this crate doesn't have over
    /// a directory it doesn't own. Confirms the fix: only the `v{version}` leaf ends
    /// up tightened; its `{app_id}` parent (and the shared `cosmic` directory one
    /// level above that) are left at whatever ordinary default permissions
    /// `create_dir_all`/the process umask produce — the same permissions a plain,
    /// unrelated directory created the ordinary way gets in this same test
    /// environment, not hardcoded against a specific assumed umask.
    #[test]
    fn only_the_leaf_version_directory_is_tightened_not_its_ancestors() {
        test_support::with_scratch_xdg_config_home(|| {
            let base = dirs::config_dir().expect("XDG_CONFIG_HOME was just set");

            // A reference directory created the ordinary way, to observe this test
            // environment's actual default (umask-derived) directory permissions,
            // rather than assuming a specific value.
            let reference = base.join("reference-dir");
            std::fs::create_dir_all(&reference).expect("create reference dir");
            let default_mode = std::fs::metadata(&reference).expect("stat reference dir").permissions().mode() & 0o777;

            ensure_config_dir_permissions_before_write("com.example.TestApp", 1);

            let cosmic_dir = base.join("cosmic");
            let app_dir = cosmic_dir.join("com.example.TestApp");
            let leaf_dir = app_dir.join("v1");

            let cosmic_mode = std::fs::metadata(&cosmic_dir).expect("stat cosmic dir").permissions().mode() & 0o777;
            let app_mode = std::fs::metadata(&app_dir).expect("stat app dir").permissions().mode() & 0o777;
            let leaf_mode = std::fs::metadata(&leaf_dir).expect("stat leaf dir").permissions().mode() & 0o777;

            assert_eq!(leaf_mode, 0o700, "the v{{version}} leaf directory must be tightened to 0700");
            assert_eq!(cosmic_mode, default_mode, "the shared `cosmic` umbrella directory must not be swept into the leaf's 0700");
            assert_eq!(app_mode, default_mode, "this app's own namespace directory holds no sensitive data itself and must not be tightened either");
        });
    }
}

/// Test-only helpers shared by `location_config`'s and `renderer_config`'s own test
/// modules (spec 011 US7 FR-030's `save_tightens_permissions` tests in both need this).
#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::Mutex;

    /// Serializes any test that must mutate the process-wide `XDG_CONFIG_HOME` env
    /// var — the only way to redirect `cosmic-config`'s own `dirs::config_dir()`
    /// resolution (and therefore this crate's `tighten_config_dir_permissions`, which
    /// deliberately mirrors that same resolution) away from the real user config
    /// directory in a test. Rust's default test harness runs a crate's tests in
    /// parallel, so without this, two such tests would race each other via this same
    /// process-wide env var (same precedent as `wallpaperctl::main`'s own
    /// `with_scratch_xdg_config_home` test helper).
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    pub(crate) fn with_scratch_xdg_config_home<T>(f: impl FnOnce() -> T) -> T {
        let _guard = ENV_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = tempfile::tempdir().expect("tempdir");
        // SAFETY: serialized by ENV_LOCK above — no concurrent access to this
        // process-wide env var from anywhere else in this crate's test binary.
        unsafe { std::env::set_var("XDG_CONFIG_HOME", dir.path()) };
        let result = f();
        unsafe { std::env::remove_var("XDG_CONFIG_HOME") };
        result
    }
}
