# Data Model: Wallpaper Renderer

All types below are owned by this spec's `renderer` crate. Spec 1's `ScheduleQueryResult`,
`TransitionState`, and `next_transition_after` (schedule-engine) and spec 2's `LoadedPack`/
`PackSource` (pack-loader) are **not** redefined here — this crate consumes them by reference,
per FR context in spec.md's Assumptions.

## OutputId

A stable identifier for a physical Wayland output, derived from `xdg-output`'s reported
connector name (e.g. `"eDP-1"`, `"DP-3"`) — the same style of identifier COSMIC's own output
tooling (`cosmic-randr`) already persists against, so an assignment survives a reconnect or
reboot as long as the same physical port is used. Not guaranteed globally unique across every
possible dock/GPU combination, but stable enough for this spec's isolation and persistence
requirements (FR-005, FR-009, FR-010) — the same tradeoff every desktop's per-monitor settings
already accept.

## ManagedOutput

The runtime state for one output this daemon has taken exclusive layer-shell ownership of
(constitution Principle I; spec.md Key Entities "Managed Output").

| Field | Type | Notes |
|---|---|---|
| `id` | `OutputId` | Identity key (FR-005, FR-009) |
| `wl_output` | opaque SCTK output handle | Not persisted — rebuilt on every connect (FR-008) |
| `scale` | `f64` | Current fractional scale factor (FR-008); drives `wp_viewporter` config |
| `size` | `(u32, u32)` | Current logical/physical size; changes trigger FR-008's resize path |
| `assignment` | `OutputAssignment` | What this output should display (FR-005, FR-006) |
| `state` | `RendererState` | `IdleWait` or `ActiveTransition` (FR-003, FR-004) — see below |

## OutputAssignment

A tagged union — exactly one of, per output:

- `Explicit(PackSource)` — an explicit per-output override (FR-005), takes precedence over
  the toggle (FR-006).
- `FollowsToggle` — no override; this output shows whatever `RendererConfig.same_pack`
  currently holds, if the toggle is enabled (FR-006).
- `Unassigned` — no override, and the toggle is off (or the toggle has no pack chosen yet):
  a well-defined empty state (FR-009, spec.md Edge Cases), not an error.

`PackSource` here is spec 2's type (`Directory(PathBuf)` | `StaticFile(PathBuf)`), reused by
reference — this spec never redefines pack identity, only points at it.

## RendererState

Enum: `IdleWait(IdleWaitState)` | `ActiveTransition(CrossfadeTransition)` — constitution
Principle VI's two states, tracked independently per `ManagedOutput` (FR-005, spec.md Key
Entities "Idle-Wait State" / "Crossfade Transition").

## IdleWaitState

| Field | Type | Notes |
|---|---|---|
| `next_wake` | `Option<DateTime<Local>>` | From spec 1's `next_transition_after`; `None` only for a single-image/static assignment, which never transitions |
| `timer` | opaque calloop timer handle | The *only* scheduled activity for this output while idle (FR-003) |

## CrossfadeTransition

| Field | Type | Notes |
|---|---|---|
| `outgoing_texture` | GPU texture handle | Already-decoded (research.md R5), uploaded once, reused across frames |
| `incoming_texture` | GPU texture handle | Same |
| `started_at` | `Instant` | Local to this transition; not persisted |
| `duration` | `Duration` | Fixed 45s default (FR-002), configurable |
| `progress` | `f64` | Recomputed each frame-callback tick from `started_at`/`duration`, clamped `[0.0, 1.0]` — the crossfade's own progress bookkeeping, distinct from (but seeded by) spec 1's `ScheduleQueryResult` progress fraction at the moment the transition began |

## RendererConfig (persisted via `cosmic-config`, FR-005–FR-007)

The root of this spec's own `cosmic-config` schema (research.md R4; full on-disk shape in
contracts/renderer-config-schema.md).

| Field | Type | Notes |
|---|---|---|
| `schema_version` | `u32` | Versioned independently of spec 2's registry schema (constitution Principle X) |
| `same_pack_everywhere` | `Option<PackSource>` | `None` = toggle off (FR-006) |
| `overrides` | `Map<OutputId, PackSource>` | Explicit per-output assignments (FR-005); an entry here always wins over `same_pack_everywhere` for that output (FR-006) |

**Resolution rule** (FR-005–FR-007): for a given `OutputId`, if `overrides` has an entry, that
output's `OutputAssignment` is `Explicit`; else if `same_pack_everywhere` is `Some`, it's
`FollowsToggle`; else it's `Unassigned`.

## PendingChange (in-memory only, not persisted)

The coalescing unit for FR-014 — not a `cosmic-config` type, purely an in-process debounce
structure.

| Field | Type | Notes |
|---|---|---|
| `output` | `OutputId` | Which output this pending re-evaluation targets |
| `latest_config_snapshot` | `RendererConfig` (or the relevant slice) | Replaced wholesale by each new change arriving for the same output before re-evaluation runs (FR-014) — never queued/appended |
| `deadline` | `Instant` | Re-evaluation must run by `arrival + 2s` (FR-007, spec.md Clarifications) |

## LocationSource (consumed, added Amendment 2026-08-13)

Not owned by this crate — spec 4's `LocationConfig` `cosmic-config` entry, read by `config.rs`
(research.md R7) the same way `RendererConfig` is read. Shape (for reference; authoritative
definition lives in spec 4's data-model.md/contracts/location-config-schema.md):

| Field | Type | Notes |
|---|---|---|
| `location` | `Option<Location>` | spec 1's `Location`, reused verbatim; `None` = no manual location set |

`scheduler_bridge.rs` passes this value as-is into spec 1's `ValidatedPack::query(location, at,
duration)` for solar-anchored packs (FR-015). A `None` value when a solar-anchored pack is
assigned degrades that output per `RendererError`'s existing containment posture (FR-013's
pattern), not a new error variant.

## QueryResponse / ReevaluateRequest (D-Bus-facing, added Amendment 2026-08-13, FR-016)

The shapes `dbus_service.rs` constructs and returns — mirrors spec 4's `ScheduleQueryResponse`
(spec 4 data-model.md) exactly, since spec 4's CLI is this interface's only intended caller;
authoritative wire contract is specs/004-cli-control-surface/contracts/wallpaperd-dbus-interface.md.

| Field | Type | Notes |
|---|---|---|
| `output` | `OutputId` | Which output this answers for |
| `assigned` | `bool` | `false` if `OutputAssignment::Unassigned` (data-model.md above) |
| `active_image` | `String` | Empty when `assigned` is `false` |
| `next_transition_at` | `Option<DateTime<Local>>` | From the output's current `IdleWaitState.next_wake` or in-progress `CrossfadeTransition`; `None` for a static/degenerate pack (no transition ever) |

Read-only — never a way to *change* `ManagedOutput` state (research.md R5/R8); `Reevaluate`/
`ReevaluateAll` trigger the existing re-evaluation path (FR-007) rather than accepting any
new state to apply.

## RendererError

- `SurfaceCreationFailed { output: OutputId, reason: String }` — layer-shell or GPU surface
  setup failed for one output (FR-008, FR-013 posture: contained to that output).
- `GpuDeviceUnavailable { reason: String }` — no working `wgpu` backend found at startup
  (research.md R3's fallback already attempted and exhausted).
- `TextureUploadFailed { path: PathBuf, reason: String }` — full pixel decode/upload failed
  for an image spec 2 already validated as header-readable; contained to the affected output
  (FR-013), does not affect others.
- `ConfigError` — wraps `cosmic-config` I/O failures for `RendererConfig` (constitution
  Principle VIII, same posture as spec 2's `RegistryError`).
- `OutputProtocolError { reason: String }` — the compositor doesn't support a required
  protocol (e.g. no `wlr-layer-shell-unstable-v1`) — a startup-time, whole-daemon condition,
  not a per-output one.
- `OutputNotManaged { id: OutputId }` (added, Amendment 2026-08-13) — a D-Bus `QueryOutput`/
  `Reevaluate` call named an output this daemon doesn't currently manage (FR-016, spec.md
  User Story 7 Scenario 4); mapped to a D-Bus error reply per
  contracts/wallpaperd-dbus-interface.md (spec 4).

All error types implement `std::error::Error`, `Debug`, and `Display`, and name the specific
output/file/reason at fault — per constitution Principle VIII, no `unwrap()`/`expect()`
outside `#[cfg(test)]` code on any of these paths.
