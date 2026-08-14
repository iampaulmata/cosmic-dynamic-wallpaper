//! [`OutputId`], [`OutputAssignment`], [`RendererState`], and the pure-logic subset of
//! [`ManagedOutput`]/[`IdleWaitState`] (data-model.md).
//!
//! **Scope note**: data-model.md's full `ManagedOutput` also carries an opaque SCTK
//! output handle (`wl_output`), and `IdleWaitState` an opaque `calloop` timer handle —
//! both belong to the unimplemented Wayland integration (see crate `README.md`) and are
//! omitted here rather than faked. What remains is exactly the part that's pure data
//! and pure logic: identity, assignment, and idle/active state.

use std::collections::HashMap;
use std::fmt;

use cosmic_config::cosmic_config_derive::CosmicConfigEntry;
use cosmic_config::CosmicConfigEntry;
use serde::{Deserialize, Serialize};

use pack_loader::PackSource;

use crate::crossfade::CrossfadeTransition;

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

/// The "same pack on all outputs" toggle plus per-output overrides — this crate's own
/// `cosmic-config` schema (data-model.md `RendererConfig`, FR-005–FR-007). Structurally
/// identical to `wallpaperctl::config::RendererConfig` (spec 4 writes this same
/// `cosmic-config` entry) — the two crates don't share a type (neither depends on the
/// other), but must stay shape-compatible; see this crate's `README.md`.
#[derive(Debug, Clone, Default, PartialEq, CosmicConfigEntry)]
#[version = 1]
pub struct RendererConfig {
    /// `None` = the toggle is off.
    pub same_pack_everywhere: Option<PackSource>,
    /// Explicit per-output overrides, keyed by output identifier.
    pub overrides: HashMap<OutputId, PackSource>,
}

/// Resolve `output`'s [`OutputAssignment`] from the current [`RendererConfig`]
/// (data-model.md's Resolution rule, FR-005–FR-007): an `overrides` entry always wins;
/// else `FollowsToggle` if the toggle is set; else `Unassigned`.
pub fn resolve_assignment(output: &OutputId, config: &RendererConfig) -> OutputAssignment {
    if let Some(source) = config.overrides.get(output) {
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

/// The sleeping state between transitions (data-model.md `IdleWaitState`, FR-003) —
/// pure-logic subset (no `calloop` timer handle, see module doc).
#[derive(Debug, Clone, PartialEq)]
pub struct IdleWaitState {
    /// From spec 1's `next_transition_after`; `None` only for a single-image/static
    /// assignment, which never transitions.
    pub next_wake: Option<chrono::DateTime<chrono::Local>>,
}

/// One output's current state — constitution Principle VI's two states, tracked
/// independently per output (data-model.md `RendererState`).
#[derive(Debug, Clone, PartialEq)]
pub enum RendererState {
    /// Sleeping between transitions (FR-003).
    IdleWait(IdleWaitState),
    /// A crossfade currently in progress (FR-001, FR-004).
    ActiveTransition(CrossfadeTransition),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(path: &str) -> PackSource {
        PackSource::StaticFile(path.into())
    }

    #[test]
    fn explicit_override_always_wins() {
        let mut config = RendererConfig {
            same_pack_everywhere: Some(source("/toggle.jpg")),
            overrides: HashMap::new(),
        };
        config.overrides.insert(OutputId::new("DP-3"), source("/override.jpg"));

        let assignment = resolve_assignment(&OutputId::new("DP-3"), &config);
        assert_eq!(assignment, OutputAssignment::Explicit(source("/override.jpg")));
        assert_eq!(effective_pack(&assignment, &config), Some(&source("/override.jpg")));
    }

    #[test]
    fn no_override_follows_toggle_when_set() {
        let config = RendererConfig { same_pack_everywhere: Some(source("/toggle.jpg")), overrides: HashMap::new() };
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

    /// US5 Scenario 3: a toggle-pack change doesn't affect an overridden output — since
    /// `effective_pack` re-derives from the assignment kind, not a cached value, an
    /// `Explicit` output's pack simply never looks at `same_pack_everywhere` at all.
    #[test]
    fn overridden_output_is_unaffected_by_toggle_changes() {
        let mut config = RendererConfig { same_pack_everywhere: Some(source("/a.jpg")), overrides: HashMap::new() };
        config.overrides.insert(OutputId::new("DP-3"), source("/override.jpg"));
        let assignment = resolve_assignment(&OutputId::new("DP-3"), &config);

        config.same_pack_everywhere = Some(source("/b.jpg")); // toggle's pack changes
        assert_eq!(effective_pack(&assignment, &config), Some(&source("/override.jpg")));
    }

    /// US3: two outputs' resolved assignments never cross-mutate — each resolution
    /// call only ever reads its own `OutputId`'s entry.
    #[test]
    fn two_outputs_resolve_independently() {
        let mut config = RendererConfig::default();
        config.overrides.insert(OutputId::new("DP-3"), source("/a.jpg"));

        let dp3 = resolve_assignment(&OutputId::new("DP-3"), &config);
        let edp1 = resolve_assignment(&OutputId::new("eDP-1"), &config);
        assert_eq!(dp3, OutputAssignment::Explicit(source("/a.jpg")));
        assert_eq!(edp1, OutputAssignment::Unassigned);
    }
}
