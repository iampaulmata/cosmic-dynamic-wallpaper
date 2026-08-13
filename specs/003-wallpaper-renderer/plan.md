# Implementation Plan: Wallpaper Renderer

**Branch**: `003-wallpaper-renderer` | **Date**: 2026-08-13 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/003-wallpaper-renderer/spec.md`

**Note**: This template is filled in by the `/speckit-plan` command; its definition describes the execution workflow.

## Summary

The Wayland layer-shell client that turns spec 1's per-output schedule answer and spec 2's
loaded pack images into what a user actually sees: a GPU-composited crossfade at each
scheduled transition, and otherwise nothing running at all. It takes exclusive ownership of
the background layer-shell surface on every output it manages (via `smithay-client-toolkit`),
tracks each output's independent pack assignment (explicit override or the "same pack
everywhere" toggle), reacts to assignment/hotplug changes within 2 seconds by coalescing
rapid-fire changes to a single re-evaluation, and blends between images on the GPU (`wgpu`)
paced strictly by the compositor's frame callback — never a free-running timer. No CLI, GUI,
`cosmic-bg` supersession, or session/systemd packaging is part of this spec; it produces a
runnable daemon binary that specs 4 and 5 build on top of, not a finished installable product.

**Amendment 2026-08-13** (while planning spec 4): this daemon also reads a manually-provided
location (spec 4's new `LocationConfig` entry) for solar-anchored scheduling, and exposes a
minimal, read/trigger-only D-Bus interface so spec 4's CLI can query live state and force a
re-evaluation — neither existed in this spec's original scope; both are now FR-015/FR-016 and
User Story 7 in spec.md. See spec 4's `contracts/location-config-schema.md` and
`.../wallpaperd-dbus-interface.md` for the exact shapes this plan now targets.

## Technical Context

**Language/Version**: Rust, stable toolchain, edition 2021 — same workspace as specs 1–2; no
new MSRV constraint introduced by this spec's dependencies.

**Primary Dependencies**: [`smithay-client-toolkit`](https://github.com/Smithay/client-toolkit)
(0.20.x, wlr-layer-shell-unstable-v1 + output/xdg-output tracking + `wp_viewporter`/
`fractional-scale-v1`, research.md R1) — the same protocol set `cosmic-bg` itself already
uses, confirmed during research; [`calloop`](https://github.com/Smithay/calloop) (event loop,
constitution-mandated) plus
[`calloop-wayland-source`](https://github.com/Smithay/calloop-wayland-source) (0.3.x,
bridges `wayland-client`'s `EventQueue` into calloop, research.md R2) — the exact adapter
`cosmic-bg` uses for the same purpose; [`wgpu`](https://wgpu.rs) (0.2x-series current stable)
for the GPU-composited crossfade blend, chosen over raw GL for backend portability/safety
(research.md R3), bridged to SCTK's Wayland objects via
[`raw-window-handle`](https://github.com/rust-windowing/raw-window-handle) (0.6.x, research.md
R3); [`image`](https://crates.io/crates/image) (0.25.x, already a spec 2 dependency) reused
here for full pixel decode → GPU texture upload (research.md R5); `cosmic-config` (git
dependency, already a spec 2 dependency) for a new, separately-versioned output-assignment/
toggle schema (research.md R4); spec 1's `schedule-engine` and spec 2's `pack-loader`
(workspace path dependencies) — this is the first crate to depend on both.
[`zbus`](https://crates.io/crates/zbus) (5.x, added by Amendment 2026-08-13) as the *server*
side of the D-Bus interface FR-016/User Story 7 expose — the counterpart to spec 4's `zbus`
client usage (spec 4 research.md R3), same crate on both ends of the same interface.

**Storage**: `cosmic-config` for the output-assignment/"same pack everywhere" toggle schema
(FR-005–FR-007) — a new schema, versioned separately from spec 2's pack registry schema
(research.md R4). Also **reads** (never writes) spec 4's `LocationConfig` entry (Amendment
2026-08-13, FR-015) for solar-anchored scheduling — a schema this crate consumes but doesn't
own. No other persistent storage.

**Testing**: `cargo test` for the pure, headless-testable assignment-resolution and
change-coalescing logic (FR-005–FR-007, FR-011, FR-014) with zero Wayland/GPU dependency.
Wayland/GPU-touching code is exercised via a documented manual QA checklist (constitution's
own explicit allowance when CI cannot yet run compositor-backed tests) plus an exploratory CI
smoke test on Weston's headless backend with a software Vulkan/GL driver, asserting only
"surface created, no crash on hotplug" — not visual/pixel correctness (research.md R6).

**Target Platform**: Linux (COSMIC desktop). Real rendering requires a Wayland compositor with
`wlr-layer-shell-unstable-v1` support (`cosmic-comp` in production); the assignment/coalescing
logic module has no such requirement and is portable/unit-testable on its own.

**Project Type**: Library + binary — third workspace crate (`crates/renderer`), producing both
a testable library surface (assignment/coalescing logic) and the actual daemon binary
(`wallpaperd`) that specs 4 (CLI) and 5 (session integration/packaging) build on top of.

**Performance Goals**: Crossfade renders at the compositor's presented frame rate with no
dropped-frame artifacts on integrated graphics (SC-006); idle-state CPU/GPU usage
indistinguishable from a fully idle desktop between transitions (SC-002); a hotplug or
assignment/config change is reflected within 2 seconds (SC-004, SC-005 — resolved in spec.md
Clarifications).

**Constraints**: No X11 code paths anywhere (NFR-6, constitution Principle II); no `unsafe`
outside a documented, justified GPU/FFI boundary shim (constitution Technology Stack
Constraints); no panics outside `#[cfg(test)]` (constitution Principle VIII) — every fallible
path (invalid pack, unreadable image, hotplug/resize failure) returns `Result`; redraws paced
exclusively by `wl_surface.frame` callbacks, never a free-running timer (constitution
Principle II); zero active render loop or frame-callback subscription outside an in-progress
crossfade (constitution Principle III/VI).

**Scale/Scope**: Up to 8 simultaneously managed outputs (spec.md Clarifications). Crossfade
duration is a single fixed, configurable value (default 45s, spec.md FR-002) — no
period-length scaling. Still-image content only (PRD Non-Goal NG1); no CLI/GUI surface is
built by this spec (specs 4/future GUI).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

This is the PRD's own highest-risk spec — the first to touch Wayland, GPU, and multi-output
code at all — so most of the constitution's principles are directly in scope rather than N/A.

| Principle | Status | Notes |
|---|---|---|
| I. Independent Renderer / Exclusive Output Ownership | **PASS** | This spec's core structural decision — `Managed Output` (data-model.md) takes exclusive layer-shell ownership per output (FR-005); no delegation to or coexistence with `cosmic-bg` on a managed output. Actually disabling `cosmic-bg` on install is spec 5's job (Assumptions), but this spec's renderer never shares a surface. |
| II. Wayland-Native, No X11 | **PASS** | `smithay-client-toolkit` / `wlr-layer-shell-unstable-v1` only (research.md R1); redraws paced by `wl_surface.frame`, never a timer (FR-001, FR-003). |
| III. GPU-Accelerated Crossfade | **PASS** | `wgpu`-composited two-texture blend (FR-001, research.md R3), not CPU pixel blending; idle outside an active transition (FR-003, FR-004). |
| IV. Settings Live in cosmic-config | **PASS** | The new output-assignment/toggle schema (FR-005–FR-007) is persisted via `cosmic-config` (research.md R4), versioned distinctly from spec 2's pack-registry schema per Principle X. FR-016's D-Bus interface (Amendment) is deliberately read/trigger-only — it never becomes a second way to *change* persisted state alongside `cosmic-config`, preserving this principle's single-source-of-truth intent. |
| V. Solar/Time Logic Pure, Deterministic, Unit-Tested | N/A (consumed, not re-implemented) | This spec never recomputes solar/clock schedule logic — it calls spec 1's `ScheduleQueryResult`/`next_transition_after` (spec.md Key Entities) and treats the answer as ground truth. |
| VI. Two Scheduling Modes | **PASS** | Idle-Wait / Active-Transition (spec.md Key Entities) is this spec's own state machine — FR-003/FR-004 are exactly Principle VI's two states, per output. |
| VII. Per-Output Correctness Under Hotplug and Scaling | **PASS** | Independent `Managed Output` state (FR-005), hotplug/resize/rescale handling (FR-008–FR-010), tested up to 8 outputs (SC-003, spec.md Clarifications) — fully owned by this spec, not inherited from `cosmic-bg`. |
| VIII. Failures Contained, Never Fatal | **PASS** | Invalid/unreadable pack degrades only that output (FR-013); in-progress crossfade cancellation is clean, not corrupting (FR-011, FR-012); no `unwrap()`/`expect()` outside tests, same CI lint gate as specs 1–2. |
| IX. Native COSMIC UI | N/A | No UI in this spec — CLI (spec 4) and any future GUI are separate specs; this spec is a headless daemon. |
| X. Config Schema Versioned | **PASS** | The output-assignment/toggle schema carries its own `schema_version`, independent of spec 2's pack-registry schema (research.md R4) — the same versioned-migration posture spec 2 established. |
| XI. Session Integration | N/A (supports, doesn't implement) | This spec produces the `wallpaperd` binary spec 5 will wrap in a systemd unit and `cosmic-bg`-supersession install flow — the autostart/uninstall mechanics themselves are spec 5's scope, not this one's. |

*(Amendment 2026-08-13: FR-015's location consumption doesn't change any principle's status —
still N/A/consumed under Principle V, same as the rest of spec 1's contract. FR-016's D-Bus
service doesn't touch Principle IX either — it's not a UI, and Principle IX's CLI-substitute
allowance is what spec 4 relies on, not this spec.)*

**Gate result**: PASS. No Complexity Tracking entries required — every principle in direct
scope is a PASS by construction (this spec exists specifically to implement them), and the
N/A entries reflect intentional scope exclusion per the PRD's own spec breakdown, not a
violation needing justification.

**Post-Phase-1 re-check**: Design artifacts (research.md, data-model.md, contracts/,
quickstart.md) introduce no new dependency, schema, or architectural element outside what the
table above already accounts for — `RendererConfig`'s own schema version (Principle X),
`OutputAssignment`/`PendingChange`'s coalescing behavior (Principle VI/VII), and the wgpu/SCTK
bridging risk flagged in research.md R3 (tracked as an implementation risk, not a constitution
gap) are all already reflected above. Gate result unchanged: PASS.

## Project Structure

### Documentation (this feature)

```text
specs/003-wallpaper-renderer/
├── plan.md              # This file (/speckit-plan command output)
├── research.md          # Phase 0 output (/speckit-plan command)
├── data-model.md        # Phase 1 output (/speckit-plan command)
├── quickstart.md        # Phase 1 output (/speckit-plan command)
├── contracts/            # Phase 1 output (/speckit-plan command)
│   └── renderer-config-schema.md
│       # (Amendment 2026-08-13: the two new contracts FR-015/FR-016 implement against —
│       #  LocationConfig schema, wallpaperd D-Bus interface — live in spec 4's own
│       #  contracts/ dir as the authoritative source, referenced from here rather than
│       #  duplicated, to avoid drift between the two specs' copies)
└── tasks.md              # Phase 2 output (/speckit-tasks command - NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
Cargo.toml                        # workspace root (spec 1); this spec adds a third member
crates/
├── schedule-engine/               # spec 1 (dependency, not modified here)
├── pack-loader/                   # spec 2 (dependency, not modified here)
└── renderer/
    ├── Cargo.toml
    ├── src/
    │   ├── lib.rs                 # public re-exports of the headless-testable surface (contracts/renderer-config-schema.md, data-model.md)
    │   ├── bin/
    │   │   └── wallpaperd.rs      # daemon entry point — the binary specs 4/5 target
    │   ├── assignment.rs          # OutputAssignment resolution: explicit override vs. "same everywhere" toggle, change coalescing (FR-005–FR-007, FR-014) — pure, headless-testable
    │   ├── config.rs              # cosmic-config schema + watch integration for assignments/toggle (FR-005–FR-007, research.md R4); also watches spec 4's LocationConfig (FR-015, Amendment 2026-08-13)
    │   ├── output.rs              # ManagedOutput lifecycle via SCTK's OutputHandler — hotplug/resize/rescale (FR-008–FR-010)
    │   ├── surface.rs             # per-output wlr-layer-shell surface + wp_viewporter/fractional-scale setup (research.md R1)
    │   ├── gpu.rs                 # wgpu instance/device setup, raw-window-handle bridge from SCTK surfaces (research.md R3)
    │   ├── crossfade.rs           # WGSL two-texture blend pipeline, frame-callback-paced draw loop (FR-001–FR-004, FR-011, FR-012)
    │   ├── texture.rs             # full-resolution image decode (via `image`) → GPU texture upload (research.md R5)
    │   ├── scheduler_bridge.rs    # per-output calloop timer wired to spec 1's next_transition_after (FR-003, constitution Principle VI); passes config.rs's location into solar-anchored queries (FR-015, Amendment 2026-08-13)
    │   ├── dbus_service.rs        # NEW (Amendment 2026-08-13): FR-016/User Story 7 — zbus server implementing specs/004-cli-control-surface/contracts/wallpaperd-dbus-interface.md
    │   └── error.rs               # RendererError — Result-typed, no panics (constitution Principle VIII)
    └── tests/
        ├── assignment_resolution.rs   # FR-005–FR-007, FR-014 coalescing — pure, no Wayland/GPU dependency
        ├── dbus_response_mapping.rs   # NEW (Amendment 2026-08-13): FR-016 response construction — pure, no real D-Bus connection
        └── fixtures/                   # small test images for texture.rs unit tests where feasible headlessly
```

**Structure Decision**: `renderer` joins `schedule-engine` and `pack-loader` as the third
member of the workspace spec 1 established, with path dependencies on both. Unlike those two
crates, `renderer` produces a binary (`wallpaperd`) as well as a library surface — the binary
is this spec's actual runnable deliverable (manually run for QA per quickstart.md), while the
library surface is what stays unit-testable in CI without a real compositor. Wayland/GPU-
touching modules (`output.rs`, `surface.rs`, `gpu.rs`, `crossfade.rs`) are deliberately kept
separate from the pure `assignment.rs` module so the constitution's testability expectations
(Principle V's spirit, even though this spec's domain is rendering, not solar/time logic)
apply to as much of this crate as the domain allows.

## Complexity Tracking

*No entries — Constitution Check passed with no unjustified violations (see table above).*
