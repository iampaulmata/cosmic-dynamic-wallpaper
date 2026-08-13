# Data Model: CLI Control Surface

Types below are owned by this spec's `wallpaperctl` crate. Spec 1's `Location` (schedule-
engine), spec 2's `PackSource`/`Registry`/`PackRegistryEntry` (pack-loader), and spec 3's
`OutputId`/`RendererConfig`/`OutputAssignment` (renderer) are **not** redefined here — this
crate reads/writes them by reference, exactly as spec 4's plan.md Cross-Spec Dependencies
section describes.

## PackRegistrationRequest

The input to FR-001 (`wallpaperctl register`).

| Field | Type | Notes |
|---|---|---|
| `source` | `PathBuf` | A directory (manifest pack) or single image file (static pack) — resolved into spec 2's `PackSource` by `load_pack` |

Not persisted itself — it's the CLI-side input that produces a spec 2 `PackRegistryEntry` via
`Registry::register` (FR-001, FR-002 idempotency).

## OutputAssignmentRequest

The input to FR-006 (`wallpaperctl assign`).

| Field | Type | Notes |
|---|---|---|
| `target` | `AssignmentTarget` | `Output(OutputId)` (spec 3's identifier) or `SamePackEverywhere` |
| `pack` | `PackSource` | Must already be a known/registered pack (FR-007) — spec 2's `PackSource`, reused |

Resolves directly into a write against spec 3's `RendererConfig.overrides` map or
`same_pack_everywhere` field (contracts, spec 3 `renderer-config-schema.md`) — this spec does
not redefine that shape, only writes to it.

## LocationConfig (persisted via `cosmic-config`, FR-008 — new schema this spec owns)

| Field | Type | Notes |
|---|---|---|
| `schema_version` | `u32` | Versioned independently of spec 2's registry and spec 3's `RendererConfig` schemas (constitution Principle X, research.md R4) |
| `location` | `Option<Location>` | `None` = no location set (only clock-anchored packs usable); `Some` = spec 1's `Location`, reused verbatim |

**Validation rule** (FR-013 in spec.md's numbering, i.e. this spec's own FR-013 covering
machine-readable output — not to be confused with this table's field validation): setting
`location` MUST pass spec 1's `Location::new(latitude, longitude)` validation before being
persisted; an invalid value is rejected before any write occurs (spec.md US3 Scenario 3).

**Consumption note (flagged cross-spec dependency)**: spec 3's `scheduler_bridge.rs` must read
this entry to supply `location` to spec 1's `ValidatedPack::query` for solar-anchored packs —
it does not do so as spec 3 is currently tasked. See plan.md Cross-Spec Dependencies and
contracts/location-config-schema.md.

## ScheduleQueryResponse (CLI-facing, from a live `wallpaperd` via D-Bus, FR-009)

| Field | Type | Notes |
|---|---|---|
| `output` | `OutputId` | Which output this answers for |
| `state` | `QueryState` | `Assigned { active_image: String, next_transition_at: Option<DateTime<Local>> }` or `Unassigned` (spec.md US4 Scenario 2) |

This is a read-only projection of spec 1/3's live internal state (spec.md Key Entities) —
`wallpaperctl` never computes it itself, only requests and displays it (research.md R5).

## CliError

| Variant | Notes |
|---|---|
| `DaemonUnreachable` | FR-011 — a D-Bus-dependent command (`list outputs`, `query`, `reevaluate`) found no running `wallpaperd`; corrected to include `list outputs` (spec.md Assumptions) |
| `PackNotFound { source: PathBuf }` | FR-007 — assignment/removal referenced an unregistered pack (checked against spec 2's local registry, no daemon needed) |
| `OutputNotFound { id: String }` | FR-016 (spec 3) — `query`/`reevaluate` referenced an output `wallpaperd` doesn't currently manage. **Not** used by `assign`: assigning to a not-yet-connected output name is valid (FR-007) and only produces a non-fatal warning (`output.rs`, not a `CliError`) when the daemon happens to be reachable to check against. |
| `InvalidLocation { reason: String }` | FR-008/FR-013 — wraps spec 1's `LocationError` verbatim, not re-worded |
| `PackLoadFailed { source: PathBuf, reason: String }` | FR-001 — wraps spec 2's `ManifestError` verbatim during registration |
| `ConfigError` | Wraps `cosmic-config` I/O failures — same posture as specs 2–3's own config error variants |

Every variant implements `std::error::Error` + `Debug` + `Display`, and `main.rs` maps each to
a distinct non-zero exit code and a specific message on stderr (FR-012, constitution
Principle VIII) — no `unwrap()`/`expect()` outside `#[cfg(test)]` code on any command path.

## Output rendering (FR-013)

Not a persisted type — a rendering concern (`output.rs`) applied uniformly to every
data-returning command's result (pack list, output list, `ScheduleQueryResponse`): a
human-readable formatted-text form by default, and a `serde_json`-derived machine-readable
form when the caller requests it (research.md R2). Every type listed above that a command can
print derives `Serialize` for this purpose.
