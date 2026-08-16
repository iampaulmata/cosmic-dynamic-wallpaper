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
pub const RENDERER_CONFIG_ID: &str = "com.system76.CosmicDynamicWallpaper.Renderer";

/// The pre-rename application id (spec 009-project-rename, FR-004a) —
/// [`RendererConfig::migrate_from_old_app_id`] reads this once so an existing
/// installation's settings survive the rename, never written to again afterward.
const OLD_RENDERER_CONFIG_ID: &str = "com.system76.CosmicWallpaper.Renderer";

/// A stable identifier for a physical Wayland output, derived from `xdg-output`'s
/// reported connector name (e.g. `"eDP-1"`, `"DP-3"`) — data-model.md `OutputId`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct OutputId(String);

/// The longest identifier [`OutputId::validated`] accepts (spec 011 US4/US5, FR-017/
/// FR-019) — generous headroom over any real connector name (`"eDP-1"`, `"DP-3"`, ...
/// are a handful of bytes), while still bounding the two untrusted boundaries that
/// construct an `OutputId` from external input: the D-Bus `output_id` argument and
/// `wallpaperctl assign --output`.
pub const MAX_OUTPUT_ID_BYTES: usize = 256;

/// Why [`OutputId::validated`] rejected a candidate identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputIdError {
    /// The identifier was empty.
    Empty,
    /// The identifier exceeded [`MAX_OUTPUT_ID_BYTES`].
    TooLong {
        /// The rejected identifier's actual length, in bytes.
        len: usize,
    },
    /// The identifier contained a byte outside the charset every real Wayland
    /// connector name uses (spec 011 US5 FR-019 — the audit's own reproduction,
    /// `--output "DP-3;rm -rf /"`, is only caught by this check; it's well within the
    /// length limit and non-empty).
    InvalidCharacters,
}

impl fmt::Display for OutputIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OutputIdError::Empty => write!(f, "output id must not be empty"),
            OutputIdError::TooLong { len } => {
                write!(f, "output id is {len} bytes, longer than the {MAX_OUTPUT_ID_BYTES}-byte limit")
            }
            OutputIdError::InvalidCharacters => {
                write!(f, "output id must contain only ASCII letters, digits, '-', and '_' (real connector names, e.g. \"eDP-1\", \"DP-3\", never do otherwise)")
            }
        }
    }
}

impl std::error::Error for OutputIdError {}

impl OutputId {
    /// Wrap a connector-name string as an opaque [`OutputId`] with no validation —
    /// for trusted internal construction from a real Wayland connector name (spec 3's
    /// own compositor-reported strings), never from CLI/D-Bus input directly.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Construct an [`OutputId`] from untrusted input (a D-Bus method argument, or
    /// `wallpaperctl assign --output`'s value — spec 011 US4/US5, FR-017/FR-019,
    /// research.md R13), rejecting an empty, overlong, or oddly-shaped identifier
    /// rather than silently accepting a value that can never match a real output.
    /// Every real Wayland connector name (`"eDP-1"`, `"DP-3"`, `"HDMI-A-1"`,
    /// `"Virtual-1"`) is ASCII letters/digits/`-`/`_` only — the character check below
    /// is generous within that shape, not a hand-picked allow-list of known prefixes.
    pub fn validated(id: impl Into<String>) -> Result<Self, OutputIdError> {
        let id = id.into();
        if id.is_empty() {
            return Err(OutputIdError::Empty);
        }
        if id.len() > MAX_OUTPUT_ID_BYTES {
            return Err(OutputIdError::TooLong { len: id.len() });
        }
        if !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
            return Err(OutputIdError::InvalidCharacters);
        }
        Ok(Self(id))
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

    /// Load `new_config`, migrating forward from the pre-rename application id
    /// (spec 009-project-rename, FR-004a/contracts/config-migration.md) if nothing has
    /// been written under the new id yet. Use this in place of a bare [`Self::load`] at
    /// every startup call site — see `contracts/config-migration.md` for the full
    /// behavior contract (idempotent, never mutates the old store, a fresh install with
    /// no old store at all is a silent no-op, not an error).
    pub fn migrate_from_old_app_id(new_config: &Config) -> Self {
        Self::migrate_core(new_config, Config::new(OLD_RENDERER_CONFIG_ID, Self::VERSION))
    }

    /// The pure migration core, taking both handles already open — split out from
    /// [`Self::migrate_from_old_app_id`] purely so tests can construct the old handle
    /// via [`Self::open_at`]'s custom-path equivalent instead of the real,
    /// well-known system path `Config::new` always resolves to.
    fn migrate_core(new_config: &Config, old_config: Result<Config, cosmic_config::Error>) -> Self {
        let current = Self::load(new_config);
        if current != Self::default() {
            return current; // already migrated, or configured fresh under the new id
        }
        let Ok(old_config) = old_config else {
            return current; // no old store at all — fresh install, not an error
        };
        let old = Self::load(&old_config);
        if old == Self::default() {
            return current; // old install existed but was never actually configured
        }
        // Best-effort: a write failure here just means the next call retries the same
        // migration (the old store is never mutated, so nothing is lost either way).
        let _ = old.save(new_config);
        old
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
    fn output_id_validated_rejects_empty() {
        assert_eq!(OutputId::validated(""), Err(OutputIdError::Empty));
    }

    #[test]
    fn output_id_validated_rejects_oversized() {
        let too_long = "x".repeat(MAX_OUTPUT_ID_BYTES + 1);
        assert_eq!(OutputId::validated(too_long.clone()), Err(OutputIdError::TooLong { len: too_long.len() }));
    }

    #[test]
    fn output_id_validated_accepts_real_connector_names() {
        assert_eq!(OutputId::validated("DP-3").unwrap(), OutputId::new("DP-3"));
        assert_eq!(OutputId::validated("eDP-1").unwrap(), OutputId::new("eDP-1"));
        assert_eq!(OutputId::validated("HDMI-A-1").unwrap(), OutputId::new("HDMI-A-1"));
        assert_eq!(OutputId::validated("Virtual-1").unwrap(), OutputId::new("Virtual-1"));
        // Exactly at the limit is still accepted (only *over* the limit is rejected).
        let at_limit = "x".repeat(MAX_OUTPUT_ID_BYTES);
        assert!(OutputId::validated(at_limit).is_ok());
    }

    /// Spec 011 US5 FR-019 (research.md R13/R15) — the audit's own reproduction:
    /// `--output "DP-3;rm -rf /"` is non-empty and well within the length limit, so
    /// only a character-class check catches it.
    #[test]
    fn output_id_validated_rejects_shell_metacharacters() {
        assert_eq!(OutputId::validated("DP-3;rm -rf /"), Err(OutputIdError::InvalidCharacters));
        assert_eq!(OutputId::validated("DP-3\n"), Err(OutputIdError::InvalidCharacters));
        assert_eq!(OutputId::validated("../etc/passwd"), Err(OutputIdError::InvalidCharacters));
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

    /// contracts/config-migration.md's 4 required test cases, applied to
    /// `RendererConfig`.
    mod migration {
        use super::*;

        fn old_config(root: &std::path::Path) -> Config {
            Config::with_custom_path(OLD_RENDERER_CONFIG_ID, RendererConfig::VERSION, root.to_path_buf()).unwrap()
        }

        fn new_config(root: &std::path::Path) -> Config {
            Config::with_custom_path(RENDERER_CONFIG_ID, RendererConfig::VERSION, root.to_path_buf()).unwrap()
        }

        #[test]
        fn migrates_real_content_and_is_idempotent_on_a_second_call() {
            let dir = tempfile::tempdir().unwrap();
            let old = old_config(dir.path());
            let mut written = RendererConfig::load(&old);
            written.same_pack_everywhere = Some(source("/old.jpg"));
            written.save(&old).unwrap();

            let new = new_config(dir.path());
            let migrated = RendererConfig::migrate_core(&new, Ok(old_config(dir.path())));
            assert_eq!(migrated.same_pack_everywhere, Some(source("/old.jpg")));
            assert_eq!(RendererConfig::load(&new).same_pack_everywhere, Some(source("/old.jpg")));

            // Idempotent: a second call sees the new store already populated.
            let migrated_again = RendererConfig::migrate_core(&new, Ok(old_config(dir.path())));
            assert_eq!(migrated_again, migrated);
        }

        /// `cosmic_config::Config::new`/`with_custom_path` create their directory on
        /// open rather than failing when it doesn't exist yet, so opening the old
        /// store essentially always succeeds even on a genuinely fresh install — the
        /// real "nothing to migrate" signal is `load()` returning `Default`, covered
        /// by the sibling test below. This test instead covers the `Err` branch itself
        /// (a real open failure, e.g. an invalid application id) to prove it degrades
        /// to the same no-op rather than panicking.
        #[test]
        fn a_failed_old_config_open_is_a_silent_no_op() {
            let dir = tempfile::tempdir().unwrap();
            let new = new_config(dir.path());
            let invalid_open = Config::with_custom_path("../invalid", RendererConfig::VERSION, dir.path().to_path_buf());
            assert!(invalid_open.is_err(), "fixture assumption: this name is rejected by cosmic-config");

            let result = RendererConfig::migrate_core(&new, invalid_open);
            assert_eq!(result, RendererConfig::default());
        }

        #[test]
        fn old_store_exists_but_was_never_configured_stays_default() {
            let dir = tempfile::tempdir().unwrap();
            let _ = old_config(dir.path()); // opened, but nothing ever written to it
            let new = new_config(dir.path());

            let result = RendererConfig::migrate_core(&new, Ok(old_config(dir.path())));
            assert_eq!(result, RendererConfig::default());
            assert_eq!(RendererConfig::load(&new), RendererConfig::default());
        }

        #[test]
        fn new_store_already_populated_never_consults_the_old_one() {
            let dir = tempfile::tempdir().unwrap();
            let old = old_config(dir.path());
            let mut old_entry = RendererConfig::load(&old);
            old_entry.same_pack_everywhere = Some(source("/old.jpg"));
            old_entry.save(&old).unwrap();

            let new = new_config(dir.path());
            let mut new_entry = RendererConfig::load(&new);
            new_entry.same_pack_everywhere = Some(source("/already-configured.jpg"));
            new_entry.save(&new).unwrap();

            let result = RendererConfig::migrate_core(&new, Ok(old_config(dir.path())));
            assert_eq!(result.same_pack_everywhere, Some(source("/already-configured.jpg")));
        }
    }
}
