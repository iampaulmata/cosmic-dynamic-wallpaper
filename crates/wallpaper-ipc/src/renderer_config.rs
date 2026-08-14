//! [`OutputId`], [`OutputAssignment`], and [`RendererConfig`] — spec 3's per-output pack
//! assignment `cosmic-config` schema (data-model.md `OutputAssignmentRequest`,
//! contracts/renderer-config-schema.md), extracted here (spec 7 research.md R2) as the
//! single source of truth `crates/renderer`, `crates/wallpaperctl`, and
//! `crates/wallpaper-settings` all depend on, instead of each independently defining
//! (and risking drifting) their own copy — the exact bug class this project already hit
//! once (see this module's own regression test).

use std::collections::HashMap;
use std::fmt;

use cosmic_config::cosmic_config_derive::CosmicConfigEntry;
use cosmic_config::{Config, CosmicConfigEntry};
use serde::{Deserialize, Serialize};

use pack_loader::PackSource;

/// `cosmic-config` application id for [`RendererConfig`] — shared by every reader/writer.
pub const RENDERER_CONFIG_ID: &str = "com.system76.CosmicWallpaper.Renderer";

/// A stable identifier for a physical Wayland output, derived from `xdg-output`'s
/// reported connector name (e.g. `"eDP-1"`, `"DP-3"`) — data-model.md `OutputId`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct OutputId(String);

impl OutputId {
    /// Wrap a connector-name string as an opaque [`OutputId`].
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Borrow the identifier as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OutputId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// What one output should display — exactly one of, per output (data-model.md
/// `OutputAssignment`, FR-005, FR-006).
#[derive(Debug, Clone, PartialEq)]
pub enum OutputAssignment {
    /// An explicit per-output override — always takes precedence over the toggle.
    Explicit(PackSource),
    /// No override; follows `RendererConfig.same_pack_everywhere` if it's `Some`.
    FollowsToggle,
    /// No override, and the toggle is off (or has no pack chosen) — a well-defined
    /// empty state, not an error (FR-009).
    Unassigned,
}

/// The "same pack on all outputs" toggle, per-output overrides, and the crossfade
/// duration — this project's own `cosmic-config` schema (data-model.md `RendererConfig`,
/// FR-005–FR-007; spec 7 adds `crossfade_duration_secs`, contracts/gui-application.md).
///
/// **`overrides` is keyed by plain `String`, not [`OutputId`]** — found to matter for
/// real, not just in principle, while manually testing this crate against a live
/// `wallpaperctl`-written config (2026-08-13): RON deserializes a `HashMap<OutputId,
/// _>` key expecting `OutputId`'s newtype-struct textual form, not a bare string — the
/// two are *not* wire-compatible despite `OutputId` being "just a string wrapper", so a
/// mismatch here silently produces an empty `overrides` map (RON parse error swallowed
/// by `load`'s `unwrap_or_else` fallback to `Default`) rather than the assignment that
/// was actually on disk. `String` guarantees byte-for-byte compatibility.
#[derive(Debug, Clone, PartialEq, CosmicConfigEntry)]
#[version = 1]
pub struct RendererConfig {
    /// `None` = the toggle is off.
    pub same_pack_everywhere: Option<PackSource>,
    /// Explicit per-output overrides, keyed by output identifier (as a plain string —
    /// see this struct's doc comment for why not `OutputId`).
    pub overrides: HashMap<String, PackSource>,
    /// Crossfade transition duration in seconds (spec 7 FR-006). Defaults to `45` —
    /// spec 3's pre-spec-7 `CROSSFADE_DURATION` constant value, so upgrading changes
    /// nothing until a user visits the GUI's Crossfade page or runs a future CLI
    /// equivalent.
    pub crossfade_duration_secs: u32,
}

impl Default for RendererConfig {
    fn default() -> Self {
        Self { same_pack_everywhere: None, overrides: HashMap::new(), crossfade_duration_secs: 45 }
    }
}

impl RendererConfig {
    /// Open the real, user-global renderer config.
    pub fn open() -> Result<Config, cosmic_config::Error> {
        Config::new(RENDERER_CONFIG_ID, Self::VERSION)
    }

    /// Open a renderer config rooted at a custom path — test-only.
    #[doc(hidden)]
    pub fn open_at(custom_path: &std::path::Path) -> Result<Config, cosmic_config::Error> {
        Config::with_custom_path(RENDERER_CONFIG_ID, Self::VERSION, custom_path.to_path_buf())
    }

    /// Read the current entry, falling back to the default (empty overrides, toggle
    /// off, 45s crossfade) if nothing has been written yet.
    pub fn load(config: &Config) -> Self {
        Self::get_entry(config).unwrap_or_else(|(_errors, default)| default)
    }

    /// Persist this entry.
    pub fn save(&self, config: &Config) -> Result<(), cosmic_config::Error> {
        self.write_entry(config)
    }
}

/// Resolve `output`'s [`OutputAssignment`] from the current [`RendererConfig`]
/// (data-model.md's Resolution rule, FR-005–FR-007): an `overrides` entry always wins;
/// else `FollowsToggle` if the toggle is set; else `Unassigned`.
pub fn resolve_assignment(output: &OutputId, config: &RendererConfig) -> OutputAssignment {
    if let Some(source) = config.overrides.get(output.as_str()) {
        OutputAssignment::Explicit(source.clone())
    } else if config.same_pack_everywhere.is_some() {
        OutputAssignment::FollowsToggle
    } else {
        OutputAssignment::Unassigned
    }
}

/// The actual [`PackSource`] an [`OutputAssignment`] currently points at, re-derived
/// from the *current* config each time (not cached in the assignment itself) — so a
/// change to the toggle's chosen pack is picked up by every `FollowsToggle` output
/// without each one needing its own copy.
pub fn effective_pack<'a>(assignment: &'a OutputAssignment, config: &'a RendererConfig) -> Option<&'a PackSource> {
    match assignment {
        OutputAssignment::Explicit(source) => Some(source),
        OutputAssignment::FollowsToggle => config.same_pack_everywhere.as_ref(),
        OutputAssignment::Unassigned => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(path: &str) -> PackSource {
        PackSource::StaticFile(path.into())
    }

    #[test]
    fn explicit_override_always_wins() {
        let mut config = RendererConfig { same_pack_everywhere: Some(source("/toggle.jpg")), ..RendererConfig::default() };
        config.overrides.insert("DP-3".to_string(), source("/override.jpg"));

        let assignment = resolve_assignment(&OutputId::new("DP-3"), &config);
        assert_eq!(assignment, OutputAssignment::Explicit(source("/override.jpg")));
        assert_eq!(effective_pack(&assignment, &config), Some(&source("/override.jpg")));
    }

    #[test]
    fn no_override_follows_toggle_when_set() {
        let config = RendererConfig { same_pack_everywhere: Some(source("/toggle.jpg")), ..RendererConfig::default() };
        let assignment = resolve_assignment(&OutputId::new("eDP-1"), &config);
        assert_eq!(assignment, OutputAssignment::FollowsToggle);
        assert_eq!(effective_pack(&assignment, &config), Some(&source("/toggle.jpg")));
    }

    #[test]
    fn no_override_and_toggle_off_is_unassigned() {
        let config = RendererConfig::default();
        let assignment = resolve_assignment(&OutputId::new("eDP-1"), &config);
        assert_eq!(assignment, OutputAssignment::Unassigned);
        assert_eq!(effective_pack(&assignment, &config), None);
    }

    #[test]
    fn overridden_output_is_unaffected_by_toggle_changes() {
        let mut config = RendererConfig { same_pack_everywhere: Some(source("/a.jpg")), ..RendererConfig::default() };
        config.overrides.insert("DP-3".to_string(), source("/override.jpg"));
        let assignment = resolve_assignment(&OutputId::new("DP-3"), &config);

        config.same_pack_everywhere = Some(source("/b.jpg")); // toggle's pack changes
        assert_eq!(effective_pack(&assignment, &config), Some(&source("/override.jpg")));
    }

    #[test]
    fn two_outputs_resolve_independently() {
        let mut config = RendererConfig::default();
        config.overrides.insert("DP-3".to_string(), source("/a.jpg"));

        let dp3 = resolve_assignment(&OutputId::new("DP-3"), &config);
        let edp1 = resolve_assignment(&OutputId::new("eDP-1"), &config);
        assert_eq!(dp3, OutputAssignment::Explicit(source("/a.jpg")));
        assert_eq!(edp1, OutputAssignment::Unassigned);
    }

    /// Regression test for a real bug found manually testing this crate against a live
    /// `wallpaperctl`-written config (see this module's doc comment): `overrides` must
    /// parse a plain-string-keyed RON map.
    #[test]
    fn overrides_parses_the_exact_shape_wallpaperctl_writes() {
        let dir = tempfile::tempdir().unwrap();
        let config = RendererConfig::open_at(dir.path()).unwrap();

        let overrides_path = dir.path().join("cosmic").join(RENDERER_CONFIG_ID).join("v1").join("overrides");
        std::fs::create_dir_all(overrides_path.parent().unwrap()).unwrap();
        std::fs::write(&overrides_path, r#"{"eDP-1": Directory("/home/user/pack")}"#).unwrap();

        let loaded = RendererConfig::load(&config);
        assert_eq!(loaded.overrides.get("eDP-1"), Some(&PackSource::Directory("/home/user/pack".into())));
    }

    #[test]
    fn renderer_config_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let config = RendererConfig::open_at(dir.path()).unwrap();

        let mut state = RendererConfig::load(&config);
        assert_eq!(state, RendererConfig::default());
        assert_eq!(state.crossfade_duration_secs, 45);

        state.overrides.insert("DP-3".to_string(), PackSource::StaticFile("/x.jpg".into()));
        state.crossfade_duration_secs = 30;
        state.save(&config).unwrap();

        let reloaded = RendererConfig::load(&config);
        assert_eq!(reloaded.overrides.len(), 1);
        assert_eq!(reloaded.crossfade_duration_secs, 30);
    }

    /// T018: closes research.md R2's own real-bug precedent (see this module's doc
    /// comment) — a `RendererConfig` value written through a handle simulating
    /// `wallpaperctl`'s write path round-trips byte-for-byte when read back through a
    /// second, independently-opened handle simulating `wallpaperd`'s load path. Now
    /// structurally guaranteed (both "simulated" call sites are the exact same type
    /// from the exact same crate), not just guarded by a per-field regression test.
    #[test]
    fn a_value_written_by_a_simulated_wallpaperctl_round_trips_through_a_simulated_wallpaperd() {
        let dir = tempfile::tempdir().unwrap();

        let wallpaperctl_handle = RendererConfig::open_at(dir.path()).unwrap();
        let mut written = RendererConfig::load(&wallpaperctl_handle);
        written.overrides.insert("DP-3".to_string(), source("/a.jpg"));
        written.same_pack_everywhere = Some(source("/b.jpg"));
        written.crossfade_duration_secs = 20;
        written.save(&wallpaperctl_handle).unwrap();

        let wallpaperd_handle = RendererConfig::open_at(dir.path()).unwrap();
        let read = RendererConfig::load(&wallpaperd_handle);
        assert_eq!(read, written);
    }

    /// New field, safe default, carried automatically by `cosmic-config`'s per-key
    /// fallback (spec 6 research.md R7's verified mechanism) — a pre-spec-7 entry with
    /// no `crossfade_duration_secs` key on disk still loads with the correct default.
    #[test]
    fn missing_crossfade_duration_key_defaults_to_45() {
        let dir = tempfile::tempdir().unwrap();
        let config = RendererConfig::open_at(dir.path()).unwrap();

        let overrides_path = dir.path().join("cosmic").join(RENDERER_CONFIG_ID).join("v1").join("overrides");
        std::fs::create_dir_all(overrides_path.parent().unwrap()).unwrap();
        std::fs::write(&overrides_path, r#"{}"#).unwrap();

        assert_eq!(RendererConfig::load(&config).crossfade_duration_secs, 45);
    }
}
