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
