//! [`IdleWaitState`]/[`RendererState`] (data-model.md) — the pure-logic subset that
//! stays in this crate because it depends on [`CrossfadeTransition`] (GPU-adjacent,
//! renderer-only). [`OutputId`], [`OutputAssignment`], [`RendererConfig`],
//! [`resolve_assignment`], and [`effective_pack`] moved to [`wallpaper_ipc`] (spec 7
//! research.md R2) and are re-exported here unchanged so existing call sites in this
//! crate don't need updating beyond their `use` paths.
//!
//! **Scope note**: data-model.md's full `ManagedOutput` also carries an opaque SCTK
//! output handle (`wl_output`) — that belongs to the Wayland integration (`surface.rs`)
//! and is omitted here rather than faked.

pub use wallpaper_ipc::{effective_pack, resolve_assignment, OutputAssignment, OutputId, RendererConfig, RENDERER_CONFIG_ID};

use crate::crossfade::CrossfadeTransition;

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
