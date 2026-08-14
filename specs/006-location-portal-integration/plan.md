# Implementation Plan: Location Portal Integration

**Branch**: `006-location-portal-integration` | **Date**: 2026-08-14 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/006-location-portal-integration/spec.md`

**Note**: This template is filled in by the `/speckit-plan` command; its definition describes the execution workflow.

## Summary

Automatic location for solar-anchored packs via `org.freedesktop.portal.Location`, extending
`wallpaperd` (spec 3) with a live portal subscription and `wallpaperctl location` (spec 4) with
an `auto`/`manual` mode toggle — no new workspace crate. **PRD Open Question OQ-1 is resolved
for real, not just written around**: a live spike against this project's own dev COSMIC session
(research.md R1) confirms `xdg-desktop-portal-cosmic` genuinely implements the Location
interface — a real `CreateSession` call reaches actual portal logic and returns a specific
`"Location services disabled"` error, not a "no such interface" failure. The same spike also
confirms GeoClue2 itself isn't installed on this machine (research.md R2), which means FR-005's
graceful-degrade path isn't just spec'd, it's the path this dev environment will actually
exercise. [`ashpd`](https://docs.rs/ashpd) 0.13.x (`location` + `async-io` features, no
`tokio`) is the chosen portal client, integrated into `wallpaperd`'s existing single-`calloop`-
loop model rather than a second concurrency model (research.md R3/R5).

## Technical Context

**Language/Version**: Rust, stable toolchain, edition 2021 — same workspace as specs 1–5. No
new MSRV constraint (`ashpd` 0.13.x's own MSRV is well within this workspace's pinned 1.97).

**Primary Dependencies**: [`ashpd`](https://crates.io/crates/ashpd) 0.13.x, added to
`crates/renderer` only, with `default-features = false, features = ["location", "async-io"]` —
the `async-io` feature switches `ashpd`'s internal `zbus` dependency to the same non-`tokio`
backend `wallpaperd`/`wallpaperctl` already use (research.md R3), so no second async runtime
enters the workspace. No new dependency in `crates/wallpaperctl` — its `location auto`/`manual`
subcommands only read/write `cosmic-config` (FR-001/FR-007/FR-009), same posture as its existing
`get`/`set`/`clear`.

**Storage**: Extends spec 4's `LocationConfig` `cosmic-config` schema
(`specs/004-cli-control-surface/contracts/location-config-schema.md`, v1) to a new v2 shape
(this spec's contracts/location-config-schema-v2.md) adding `mode`, `automatic_location`, and
`automatic_status` fields alongside the existing `location` (manual) field — a versioned schema
bump (constitution Principle X) that needs no hand-written migration function: `cosmic-config`'s
own versioned-directory fallback carries the existing `location` value forward automatically
(research.md R7, verified against its vendored source), so this is a `#[version]` bump plus a
correct `Default` impl, not new migration code.

**Testing**: `cargo test` for the fully pure, headless-testable parts: `LocationConfigEntry`'s
v1→v2 migration, the `effective_location()` resolution rule (data-model.md), and
`wallpaperctl`'s new subcommand argument parsing/output — matching specs 1–4's precedent. The
live portal subscription itself (session creation, `LocationUpdated` stream, the calloop/async-
io integration) is **manual-QA-verified against this project's own real COSMIC session**, the
same split spec 3 already established for Wayland/GPU code that can't be meaningfully mocked —
research.md R1/R2's live spike results are the first data point for that manual QA, captured
before any code exists.

**Target Platform**: Linux, COSMIC desktop. Requires `xdg-desktop-portal` +
`xdg-desktop-portal-cosmic` for the Location *interface* to be reachable at all (confirmed
present on Pop!_OS 24.04, research.md R1); a GeoClue2 backend is additionally required for a
resolution to actually *succeed* (confirmed absent on this dev machine, research.md R2) — FR-005
is written, and now verified, to degrade cleanly without it.

**Project Type**: Extends two existing crates, no new workspace member. `crates/renderer` gains
a `portal_location.rs` module plus a `LocationConfigEntry` v2 read (replacing its current v1
`LocationSource` read). `crates/wallpaperctl` gains two new `location` subcommands and a v2
write path in its `config.rs`/`commands/location.rs`.

**Performance Goals**: Live location updates re-evaluate affected schedules within spec 3's
existing 2-second reaction bound (spec 3 FR-007, spec.md SC-003) — reused, not redefined. A new,
this-spec-owned bound: the initial (or any post-failure) resolution attempt is capped at a
5-second timeout (research.md R6) before FR-005's immediate-degrade applies, so a slow or hung
backend can't stall scheduling — distinct from, and not to be confused with, the reaction bound
above.

**Constraints**: No `unwrap()`/`expect()` outside `#[cfg(test)]` (constitution Principle VIII,
same CI lint gate as specs 1–5). The portal integration MUST run inside `wallpaperd`'s existing
single `calloop` event loop (research.md R5) — no dedicated OS thread for portal I/O, preserving
the one-concurrency-model posture spec 3 gap 3 already established for the D-Bus service.
Retry-after-failure MUST use backoff, not a tight loop (spec.md FR-005, research.md R6).

**Scale/Scope**: Single-user desktop session, one automatic location value at a time (no
multi-location, no per-output location) — same trust/scope boundary as spec 4's manual location.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|---|---|---|
| I. Independent Renderer / Exclusive Output Ownership | N/A | No rendering or output-ownership change in this spec. |
| II. Wayland-Native, No X11 | N/A | No windowing/protocol code — this spec is D-Bus-only. |
| III. GPU-Accelerated Crossfade | N/A | No rendering in this spec. |
| IV. Settings Live in cosmic-config | **PASS** | The new `mode`/`automatic_location`/`automatic_status` fields extend the existing `LocationConfig` `cosmic-config` entry (data-model.md) — no bespoke format introduced, consistent with spec 4's original schema. |
| V. Solar/Time Logic Pure, Deterministic, Unit-Tested | **PASS** (supports, doesn't touch) | This spec adds one new pure function, `effective_location()` (data-model.md) — which manual/automatic value feeds spec 1's scheduling — fully unit-testable with no I/O. Spec 1's own solar math is untouched. |
| VI. Two Scheduling Modes: Idle-Wait / Active-Transition | **PASS** | The portal's `LocationUpdated` signal is an event-driven D-Bus subscription, not a poll — subscribing to it while idle doesn't add a busy loop, the same posture already established for the existing D-Bus service (spec 3 gap 3). No new timer-based polling is introduced (research.md R6's backoff is itself timer-scheduled, not tight-looped). |
| VII. Per-Output Correctness Under Hotplug/Scaling | N/A | Location is a single global value, not per-output (spec.md Assumptions). |
| VIII. Failures Are Contained, Never Fatal | **PASS** | FR-005's immediate-degrade path (spec.md Clarifications) is the concrete implementation of this principle for portal/backend failures; no `unwrap()`/`expect()` outside tests (Technical Context: Constraints). |
| IX. Native COSMIC Look and Feel | N/A | No new UI surface — the portal's own consent prompt (rendered by whichever backend answers it) is explicitly out of this project's UI responsibility (spec.md FR-003, Assumptions). |
| X. Config Schema Versioned With Migration Path | **PASS** | `LocationConfig` bumps v1→v2 with a documented migration behavior (data-model.md, contracts/location-config-schema-v2.md, research.md R7) — old values are not silently misinterpreted, satisfying the gate without new imperative migration code (`cosmic-config`'s own versioned-directory fallback does it). |
| XI. Session Integration, Including Superseding cosmic-bg | N/A | No autostart/packaging change — spec 5's territory, untouched here. |

**Gate result**: PASS. No Complexity Tracking entries required — every principle in scope is a
PASS or an intentional, justified N/A.

### ⚠️ Cross-Spec Dependency (flagged, not yet applied to spec 3's shipped code)

Spec 3 is **already implemented** (unlike spec 4's plan, which flagged gaps in spec 3 before it
existed). `crates/renderer/src/config.rs`'s `LocationSource` currently reads the *v1* shape
(`{ location: Option<Location> }`, live in this dev machine's actual `cosmic-config` store).
This spec's v2 schema (data-model.md) is a superset with a migration function, so existing v1
data upgrades cleanly — but `scheduler_bridge.rs`'s current call site
(`crates/renderer/src/config.rs`'s `LocationSource`, consumed per spec 3's FR-015) must be
repointed at the new `effective_location()` resolution rule (data-model.md) instead of reading
`location` directly, or automatic mode will be silently ignored by actual scheduling even though
the config value is correctly persisted. This is a small, mechanical amendment to already-shipped
code — not a redesign — flagged explicitly here per this project's established practice (spec
3's FR-16 addition, spec 4's two cross-spec gaps) rather than left as a surprise for
implementation to discover. This plan does not modify spec 3's shipped code itself — that's a
decision for the user, reported at completion.

**Post-Phase-1 re-check**: Design artifacts (research.md, data-model.md, contracts/,
quickstart.md) confirm the same shape the table above already accounts for — the v1→v2 migration
(Principle X), the event-driven (not polling) portal subscription (Principle VI), and the single-
calloop-loop integration model (research.md R5) with no second concurrency model introduced. The
Cross-Spec Dependency above remains exactly as described, not expanded by Phase 1 design. Gate
result unchanged: PASS.

## Project Structure

### Documentation (this feature)

```text
specs/006-location-portal-integration/
├── plan.md                          # This file (/speckit-plan command output)
├── research.md                      # Phase 0 output (/speckit-plan command)
├── data-model.md                    # Phase 1 output (/speckit-plan command)
├── quickstart.md                    # Phase 1 output (/speckit-plan command)
├── contracts/                       # Phase 1 output (/speckit-plan command)
│   ├── location-config-schema-v2.md
│   └── wallpaperctl-location-cli.md
├── checklists/
│   └── requirements.md
└── tasks.md                         # Phase 2 output (/speckit-tasks command - NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
crates/
├── schedule-engine/                 # spec 1 (dependency, not modified here)
├── pack-loader/                     # spec 2 (dependency, not modified here)
├── renderer/                        # spec 3 — this spec's main daemon-side work
│   ├── Cargo.toml                    # + ashpd 0.13.x (features = ["location", "async-io"])
│   └── src/
│       ├── config.rs                 # LocationConfigEntry v2 read + migration (was LocationSource v1)
│       ├── portal_location.rs        # NEW — ashpd session, LocationUpdated stream, calloop integration (research.md R5)
│       ├── scheduler_bridge.rs       # amended: consumes effective_location() instead of LocationSource.location directly (Cross-Spec Dependency above)
│       └── bin/wallpaperd.rs         # amended: wires portal_location.rs's async source into the existing calloop loop alongside dbus_service.rs's
└── wallpaperctl/                    # spec 4 — this spec's CLI-side work
    └── src/
        ├── config.rs                 # LocationConfigEntry v2 write
        └── commands/
            └── location.rs           # + auto (FR-001/002/003), manual (FR-007/009), get extended with mode + status (FR-008)
```

**Structure Decision**: No new workspace crate. All changes land inside the two crates spec 6's
own FRs name directly — `renderer` (the daemon that actually talks to the portal and schedules
against its result) and `wallpaperctl` (the CLI surface that toggles the mode) — matching this
project's established pattern of extending an existing crate's schema/CLI rather than
introducing a parallel one (e.g. spec 4 extending spec 3's `RendererConfig` pattern for its own
`LocationConfig`).

## Complexity Tracking

*No entries — Constitution Check passed with no unjustified violations. The Cross-Spec
Dependency noted above is a real, small amendment to already-shipped spec 3 code, not a
violation of this spec's own constitution compliance — called out for visibility, matching this
project's established practice, not filed as complexity this spec must justify.*
