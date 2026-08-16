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
