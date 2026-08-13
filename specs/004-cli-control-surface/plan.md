# Implementation Plan: CLI Control Surface

**Branch**: `004-cli-control-surface` | **Date**: 2026-08-13 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/004-cli-control-surface/spec.md`

**Note**: This template is filled in by the `/speckit-plan` command; its definition describes the execution workflow.

## Summary

A `wallpaperctl` CLI binary that lets a person actually drive specs 1–3 without hand-editing
config files. Commands that only read/write persisted state (register/list-packs/remove a
pack, assign a pack to an output, set/view/clear a manual location) link directly against
spec 2's `pack-loader` and write `cosmic-config` entries — no running daemon required,
consistent with constitution Principle IV. Commands that need live daemon state (**list
outputs**, query current/next transition, force an immediate re-evaluation) talk to a running
`wallpaperd` (spec 3) over a small D-Bus interface this spec defines — `list outputs` was
originally (incorrectly) grouped with the config-only commands; corrected in spec.md before
this plan was written, since there's no persisted record of connected outputs anywhere.
**Two real gaps in spec 3's already-written (but not yet implemented) artifacts are surfaced
here, not silently worked around** — see Cross-Spec Dependencies below — because this spec's
own Success Criteria (SC-001: a solar-anchored pack visibly scheduled using only CLI commands)
is not actually achievable without them.

## Technical Context

**Language/Version**: Rust, stable toolchain, edition 2021 — same workspace as specs 1–3; no
new MSRV constraint introduced by this spec's dependencies.

**Primary Dependencies**: [`clap`](https://crates.io/crates/clap) (4.6.x, derive macros) for
subcommand parsing (research.md R1); [`serde_json`](https://crates.io/crates/serde_json) for
the machine-readable output mode (FR-013, research.md R2); [`zbus`](https://crates.io/crates/zbus)
(5.x, pure-Rust D-Bus, MSRV 1.87 — well within this workspace's toolchain) as the client for
`list outputs`, query, and force-reevaluation (research.md R3); spec 2's `pack-loader`
(workspace path dependency) for register/list-packs/remove, reusing its `Registry` API
directly (FR-001–FR-004);
spec 1's `schedule-engine` (workspace path dependency) for `Location::new`'s validation rule,
reused verbatim for FR-008 rather than reimplemented; `cosmic-config` (git dependency, already
used by specs 2–3) for writing spec 3's existing `RendererConfig` entry (FR-006) and this
spec's own new `LocationConfig` entry (FR-008). **This crate deliberately does not depend on
spec 3's `renderer` crate as a Rust library** — it only ever talks to a running `wallpaperd`
via `cosmic-config` (indirect, no daemon required) or D-Bus (direct, daemon required), never by
linking its code, keeping the CLI buildable/testable independent of Wayland/GPU code entirely.

**Storage**: `cosmic-config` for the new `LocationConfig` schema (FR-008) — versioned
independently of spec 2's registry schema and spec 3's `RendererConfig` schema (constitution
Principle X, research.md R4). Writes to spec 3's existing `RendererConfig` entry for
assignment/toggle (FR-006) using its already-published shape (spec 3
contracts/renderer-config-schema.md) — not redefined here.

**Testing**: `cargo test` for command-argument parsing/validation and output formatting
(fully headless); `tempfile`-backed integration tests for register/list-packs/remove/assign/
location against real `pack-loader`/`cosmic-config` instances, matching spec 2's research.md
R6 precedent. The D-Bus-dependent commands (`list outputs`, query, force re-evaluation) are
validated via manual QA against a running `wallpaperd` — which does not exist as runnable code
yet, since spec 3 isn't implemented — plus a lightweight mock D-Bus service for the CLI's own
request/response handling in isolation (research.md R6).

**Target Platform**: Linux (COSMIC desktop). Config-only commands need no display/compositor
session at all; the D-Bus-dependent commands (`list outputs`, query, force re-evaluation) need
a session bus (present in any real desktop session) and a reachable `wallpaperd`.

**Project Type**: Binary (CLI) — fourth workspace crate (`crates/wallpaperctl`), depending on
`schedule-engine` and `pack-loader` but explicitly *not* on `renderer` (see Primary
Dependencies). Unlike specs 1–3, this crate has no library contract other specs build against
— it's a leaf, the end-user-facing tool.

**Performance Goals**: Config-only commands (register, list packs, remove, assign, location)
complete fast enough to feel instant in a script — no network I/O beyond the one-time
`cosmic-config` git-dependency fetch already established by specs 2–3. D-Bus-dependent
commands (`list outputs`, query, force re-evaluation) are bounded by a single request/response
round-trip, not expected to exceed roughly a second under normal conditions.

**Constraints**: No panics outside `#[cfg(test)]` code (constitution Principle VIII) — every
fallible path returns a typed error mapped to a non-zero exit code (FR-012); every
data-returning command supports a machine-readable output mode (FR-013); commands that don't
need a daemon must work correctly with none running (FR-011).

**Scale/Scope**: Single-user desktop CLI. Not a scripting *library* — its only public surface
is its own binary's argument/output contract (contracts/wallpaperctl-cli.md), unlike specs
1–2's Rust-API contracts.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|---|---|---|
| I. Independent Renderer / Exclusive Output Ownership | N/A | No rendering or output ownership in this spec — it only writes config spec 3 later acts on. |
| II. Wayland-Native, No X11 | N/A | No windowing/protocol code. |
| III. GPU-Accelerated Crossfade | N/A | No rendering in this spec. |
| IV. Settings Live in cosmic-config | **PASS** | Every persisted write (pack registry via spec 2's `Registry`, output assignment via spec 3's `RendererConfig`, and this spec's new `LocationConfig`) goes through `cosmic-config` (FR-006, FR-008, research.md R4) — nothing is written to a bespoke format. |
| V. Solar/Time Logic Pure, Deterministic, Unit-Tested | N/A (consumed, not re-implemented) | FR-008 reuses spec 1's `Location::new` validation rule verbatim (spec.md Assumptions) — this spec never reimplements latitude/longitude validity checks. |
| VI. Two Scheduling Modes | N/A | No scheduling loop in this spec. |
| VII. Per-Output Correctness | Supports, doesn't implement | FR-005–FR-007 read/write the per-output identifiers spec 3 already defines and isolates (`OutputId`) — this spec doesn't itself guarantee output isolation, spec 3 already does. |
| VIII. Failures Contained, Never Fatal | **PASS** | FR-012 (every failure exits non-zero with a specific message), FR-007 (no partial/invalid writes on a failed assignment), no `unwrap()`/`expect()` outside tests — same CI lint gate as specs 1–3. |
| IX. Native COSMIC UI | **PASS** | This spec *is* the constitution's own named allowance: "a CLI-only control path... is an acceptable substitute for a full GUI in early milestones." |
| X. Config Schema Versioned | **PASS** | The new `LocationConfig` schema (FR-008) carries its own `schema_version`, independent of spec 2's registry and spec 3's `RendererConfig` schemas (research.md R4). |
| XI. Session Integration | N/A | No autostart/packaging in this spec — spec 5. |

**Gate result**: PASS. No Complexity Tracking entries required — every principle in scope is a
PASS or an intentional, justified N/A.

### ⚠️ Cross-Spec Dependencies (flagged, not yet resolved in spec 3)

This spec's own Success Criteria (spec.md SC-001: a user can get a solar-anchored pack
visibly scheduled using *only* CLI commands) surfaced two real gaps in spec 3's
already-written plan/tasks — neither is a constitution violation, but both are necessary for
this spec's contract to actually work end-to-end once implemented, and neither exists in spec
3's artifacts today:

1. **Spec 3's `RendererConfig` has no location field, and nothing in spec 3's tasks reads
   one.** FR-008 here persists a `LocationConfig` entry, but spec 3's `scheduler_bridge.rs`
   (spec 3 tasks.md T021) calls spec 1's `ValidatedPack::query(location, at, duration)` — for
   a solar-anchored pack, `location` must come from *somewhere*, and nothing in spec 3 reads
   this spec's new config entry. See contracts/location-config-schema.md for the shape spec
   3 needs to consume.
2. **Spec 3's `wallpaperd` binary has no D-Bus service, and nothing in spec 3's tasks adds
   one.** FR-009/FR-010 here require a live D-Bus interface on a running `wallpaperd` (research
   R3/R5) — spec 3, as tasked, never starts a D-Bus service at all. See
   contracts/wallpaperd-dbus-interface.md for the interface this spec depends on spec 3
   implementing.

Both are small, mechanical additions to spec 3's not-yet-implemented crate (a new config field
plus read, and a new D-Bus service module) — not a redesign. They're called out explicitly
here, per the same "don't silently absorb a gap" posture this project has used for every prior
cross-spec scope decision (spec 3's FR-16, this spec's own pack-registration/location scope
decisions), rather than left as a bug someone discovers during spec 3's implementation. This
plan does not modify spec 3's artifacts itself — that's a decision for the user, reported at
completion.

**Post-Phase-1 re-check**: Design artifacts (research.md, data-model.md, contracts/,
quickstart.md) confirm the same shape the table above already accounts for — `LocationConfig`'s
own schema version (Principle X), the D-Bus interface's read/trigger-only scope avoiding a
second competing config pathway (Principle IV), and both flagged Cross-Spec Dependencies
remain exactly as described, not expanded by Phase 1 design. Gate result unchanged: PASS.

## Project Structure

### Documentation (this feature)

```text
specs/004-cli-control-surface/
├── plan.md              # This file (/speckit-plan command output)
├── research.md          # Phase 0 output (/speckit-plan command)
├── data-model.md        # Phase 1 output (/speckit-plan command)
├── quickstart.md        # Phase 1 output (/speckit-plan command)
├── contracts/            # Phase 1 output (/speckit-plan command)
│   ├── wallpaperctl-cli.md
│   ├── location-config-schema.md
│   └── wallpaperd-dbus-interface.md
└── tasks.md              # Phase 2 output (/speckit-tasks command - NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
Cargo.toml                        # workspace root (spec 1); this spec adds a fourth member
crates/
├── schedule-engine/               # spec 1 (dependency, not modified here)
├── pack-loader/                   # spec 2 (dependency, not modified here)
├── renderer/                      # spec 3 (NOT a dependency — see Technical Context; needs amendment, see Cross-Spec Dependencies)
└── wallpaperctl/
    ├── Cargo.toml
    ├── src/
    │   ├── main.rs                 # clap CLI entry point, subcommand dispatch
    │   ├── commands/
    │   │   ├── register.rs         # FR-001, FR-002 — spec 2's Registry::register
    │   │   ├── list.rs             # FR-003 (list packs, config-only) + FR-005 (list outputs, D-Bus-dependent — corrected, see spec.md Assumptions)
    │   │   ├── remove.rs           # FR-004 — spec 2's Registry::remove
    │   │   ├── assign.rs           # FR-006, FR-007 — writes spec 3's RendererConfig; config-only, no live-output validation required
    │   │   ├── location.rs         # FR-008 — reads/writes/clears LocationConfig, reuses spec 1's Location::new
    │   │   ├── query.rs            # FR-009 — D-Bus call to wallpaperd
    │   │   └── reevaluate.rs       # FR-010 — D-Bus call to wallpaperd
    │   ├── output.rs                # human vs. machine-readable rendering (FR-013), shared across commands
    │   ├── dbus_client.rs           # zbus client wrapper, "daemon unreachable" handling (FR-011, research.md R3) — shared by list-outputs, query, reevaluate
    │   └── error.rs                 # CliError — Result-typed, maps to exit codes (FR-012, constitution Principle VIII)
    └── tests/
        ├── command_parsing.rs           # clap arg parsing/validation — headless
        ├── register_list_remove.rs      # tempfile-backed, against real pack-loader Registry (list *packs* only)
        ├── assign_location.rs           # tempfile-backed, against real cosmic-config writes
        └── dbus_mock.rs                  # list-outputs/query/reevaluate request-construction + response-parsing against a mock zbus service (research.md R6) — no real wallpaperd needed
```

**Structure Decision**: `wallpaperctl` joins the workspace as a fourth member with path
dependencies on `schedule-engine` and `pack-loader` only — not `renderer`, by design (see
Technical Context). This keeps the CLI's own test suite fully independent of Wayland/GPU code,
mirroring specs 1–2's headless-testability posture rather than spec 3's split one, for
everything except the three D-Bus-dependent commands (`list outputs`, `query`, `reevaluate`).

## Complexity Tracking

*No entries — Constitution Check passed with no unjustified violations. The Cross-Spec
Dependencies noted above are a real gap in spec 3's artifacts, not a violation of this spec's
own constitution compliance — they're called out for visibility, not filed as complexity this
spec must justify.*
