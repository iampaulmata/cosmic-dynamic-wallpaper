# Implementation Plan: Core Scheduling Engine

**Branch**: `001-core-scheduling-engine` | **Date**: 2026-08-11 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/001-core-scheduling-engine/spec.md`

**Note**: This template is filled in by the `/speckit-plan` command; its definition describes the execution workflow.

## Summary

A pure, dependency-free-of-I/O Rust library that answers "which wallpaper image is active
right now, and how far through a crossfade are we" for either a solar-event-anchored pack
(using a manually-entered latitude/longitude) or a fully location-free clock-time-anchored
pack. It computes solar event times via the vetted `sunrise` crate, resolves the active/
transitioning image deterministically for any query instant, and reports the next transition
instant so a future daemon can sleep instead of polling. No rendering, Wayland, GPU, or
persistence code is part of this spec — it is the foundation specs 2 (pack loading) and 3
(renderer) build on, per the PRD's suggested spec order.

## Technical Context

**Language/Version**: Rust, stable toolchain, edition 2021. Exact MSRV to be pinned in
`crates/schedule-engine/Cargo.toml` when the crate is created (research.md R5) — no language
feature in this design requires anything beyond current-stable.

**Primary Dependencies**: [`sunrise`](https://crates.io/crates/sunrise) (solar event times,
research.md R1), [`chrono`](https://crates.io/crates/chrono) (date/time handling, pulled in
transitively by `sunrise` — research.md R2).

**Storage**: N/A — this engine is a pure function of its arguments; persisting packs/
location/config is spec 2's responsibility (FR-20).

**Testing**: `cargo test` for unit/integration tests against the acceptance scenarios;
`proptest` (dev-dependency) for determinism/monotonicity properties (SC-003); `cargo llvm-cov`
in CI to enforce the 90% coverage target (SC-005) — research.md R3.

**Target Platform**: Linux (COSMIC desktop), but this crate itself has no platform-specific
code — it's portable pure Rust, which is what makes it independently unit-testable per
constitution Principle V.

**Project Type**: Library (single Rust crate within a workspace that will grow sibling crates
in later specs).

**Performance Goals**: Sub-millisecond query time for packs up to 64 anchors (SC-001).

**Constraints**: No I/O, no network access, no rendering/Wayland/GPU dependency, no panics
outside `#[cfg(test)]` code (constitution Principle VIII) — all fallible paths return
`Result`.

**Scale/Scope**: Single-user, single-machine, packs capped at 64 anchors (FR-001). Not a
multi-tenant or networked service.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

This spec is deliberately scoped to a slice of the constitution's 11 principles — the ones
outside that slice are marked N/A with the reason, not silently skipped, per the
Development Workflow requirement to state compliance explicitly.

| Principle | Status | Notes |
|---|---|---|
| I. Independent Renderer / Exclusive Output Ownership | N/A | No layer-shell/renderer code in this spec; explicitly out of scope (spec.md Assumptions), owned by spec 3. |
| II. Wayland-Native, No X11 | N/A | No windowing/protocol code in this spec. |
| III. GPU-Accelerated Crossfade | N/A | This spec only computes the progress *fraction*; the GPU blend that consumes it is spec 3. |
| IV. Settings Live in cosmic-config | N/A | No persistence in this spec; `Location`/`WallpaperPack` are constructed in-memory by the caller. Persistence is spec 2 (FR-20). |
| V. Solar/Time Logic Pure, Deterministic, Unit-Tested | **PASS** | This is the spec's entire purpose — pure functions (data-model.md), a vetted solar crate not hand-rolled math (research.md R1), determinism required by FR-004/SC-003, ≥90% coverage target (SC-005). |
| VI. Two Scheduling Modes (Idle-Wait / Active-Transition) | Supports, doesn't implement | `next_transition_after()` (contracts/schedule-engine-api.md) supplies the sleep duration a future daemon's idle-wait state needs; the calloop timer/state machine itself is out of scope here. |
| VII. Per-Output Correctness | N/A | No output/monitor concept in this spec; per-output assignment is a daemon/renderer concern (spec 3). |
| VIII. Failures Contained, Never Fatal | **PASS** | Every fallible path (bad location, mixed/duplicate/oversized anchors) returns a typed `Result` (data-model.md Error types); no `unwrap()`/`expect()` outside tests, enforced via `clippy::unwrap_used`/`clippy::expect_used` in CI. |
| IX. Native COSMIC UI | N/A | No UI in this spec. |
| X. Config Schema Versioned | N/A | No schema persisted by this spec. |
| XI. Session Integration | N/A | No daemon/session wiring in this spec. |

**Gate result**: PASS. No Complexity Tracking entries required — the N/A principles reflect
intentional scope exclusion (per the PRD's own spec breakdown), not a violation needing
justification.

## Project Structure

### Documentation (this feature)

```text
specs/001-core-scheduling-engine/
├── plan.md              # This file (/speckit-plan command output)
├── research.md          # Phase 0 output (/speckit-plan command)
├── data-model.md        # Phase 1 output (/speckit-plan command)
├── quickstart.md        # Phase 1 output (/speckit-plan command)
├── contracts/           # Phase 1 output (/speckit-plan command)
│   └── schedule-engine-api.md
└── tasks.md             # Phase 2 output (/speckit-tasks command - NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
Cargo.toml                        # workspace root (created by this spec; later specs add sibling members)
crates/
└── schedule-engine/
    ├── Cargo.toml
    ├── src/
    │   ├── lib.rs                # public API re-exports (contracts/schedule-engine-api.md)
    │   ├── location.rs           # Location + validation (FR-002a)
    │   ├── anchor.rs             # TimeAnchor, SolarEventKind (FR-6)
    │   ├── pack.rs                # PackImage, WallpaperPack, validation (FR-001, FR-006, FR-006a)
    │   ├── solar.rs                # solar event time computation, wraps `sunrise` (FR-002)
    │   ├── query.rs                # ScheduleQueryResult / TransitionState resolution (FR-004, FR-005, FR-009)
    │   └── error.rs                 # LocationError, PackError
    └── tests/
        ├── solar_accuracy.rs        # SC-002 golden reference-value tests (research.md R4)
        ├── determinism.rs           # SC-003 proptest properties
        └── schedule_resolution.rs   # spec.md acceptance scenarios as integration tests
```

**Structure Decision**: A Cargo workspace rooted at the repo root, with this spec's
deliverable as the first member crate under `crates/`. This anticipates the PRD's remaining
specs (pack loading, renderer, CLI, daemon) landing as sibling crates in the same workspace
rather than requiring a restructure later — `schedule-engine` has zero dependency on any of
them, so it can be built and tested standalone today.

## Complexity Tracking

*No entries — Constitution Check passed with no unjustified violations (see table above).*
