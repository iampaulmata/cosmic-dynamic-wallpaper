# Implementation Plan: V1 Completion — GUI, Starter Packs, IP Fallback, and Gap Closure

**Branch**: `007-v1-completion` | **Date**: 2026-08-14 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/007-v1-completion/spec.md`

**Note**: This template is filled in by the `/speckit-plan` command; its definition describes the execution workflow.

## Summary

Four workstreams: a standalone libcosmic GUI settings app (FR-001–007), a bundled zero-config
starter pack (FR-008–011), an offline-database-backed IP-geolocation location mode (FR-012–015),
and closing specs 3/5/6's remaining reliability/verification gaps (FR-016–019). **Three findings
from this planning pass are flagged prominently, not silently absorbed** (research.md R2, R4; plan
Constitution Check finding 3): (1) the GUI is the natural moment to fix a real bug class this
project has already hit once — independently-duplicated `cosmic-config` schema types drifting
apart — by extracting a new shared `wallpaper-ipc` crate rather than adding a third independent
copy; (2) the user's own Clarification chose a bundled offline database specifically to avoid a
third-party network call for IP-geolocation, but discovering a NAT'd machine's own public IP
address unavoidably needs *some* external touchpoint — resolved via STUN (a narrower, purpose-
built exception, not a "geolocation API"), called out here for the user to weigh in on; (3)
crossfade duration is not actually configurable anywhere in already-shipped spec 3 code despite a
stray doc comment claiming otherwise — FR-006 requires real new plumbing, not just a GUI form.

## Technical Context

**Language/Version**: Rust, stable toolchain, edition 2021 — same workspace as specs 1–6.

**Primary Dependencies**: `libcosmic` (git, `pop-os/libcosmic`, same pin as `cosmic-config` —
research.md R1) for the new GUI crate; `maxminddb` 0.30.x (research.md R3) and `stunclient` 0.4.2
(research.md R4) added to `crates/renderer` for IP-geolocation; `wayland-server` 0.31.x
(research.md R7, `dev-dependencies` only) for the mock hotplug test harness; `image` (already a
workspace dependency) for the starter-pack generator tool. No new dependency in
`crates/wallpaperctl` beyond the new `wallpaper-ipc` path dependency (research.md R2).

**Storage**: Extends spec 6's `LocationConfig` v2→v3 (adds `ip_location`/`ip_status`, renames
`AutomaticStatus`→`ResolutionStatus` — research.md R9, no hand-written migration needed per spec
6's own R7 finding), spec 2's pack registry schema (adds `origin: PackOrigin` — research.md
R6, same no-migration-needed pattern), and spec 3's `RendererConfig` (adds
`crossfade_duration_secs`, previously not configurable at all despite a stray doc comment
claiming otherwise — Constitution Check finding 3, data-model.md). All three are versioned schema
bumps per constitution Principle X, satisfied the same way spec 6 already established: a version
bump plus a correct `Default`, not new imperative migration code.

**Testing**: `cargo test` for everything pure/headless: schema/migration round-trips, IP-
geolocation's `maxminddb` lookup against a small fixture database, the starter-pack registry
origin-tracking logic, and the GUI's own view-state logic (kept separate from `libcosmic`'s
rendering, same "pure core, thin UI shell" posture this project already uses elsewhere). The mock
hotplug harness (research.md R7) is a real `wayland-server`-backed integration test, not manual
QA — closing spec 3 tasks.md T043. STUN/`maxminddb` resolution and the GUI's actual rendered
appearance remain manual-QA items against this project's real COSMIC session, same posture as
spec 3/6's own Wayland/portal code.

**Target Platform**: Linux, COSMIC desktop. The GUI needs a running compositor session (same as
any COSMIC app); IP-geolocation needs outbound UDP (STUN) reachability as its one narrow
exception (research.md R4) but works fully offline for the lookup itself once a public IP is
known.

**Project Type**: Extends the workspace with two new crates (`wallpaper-ipc`, a shared
schema/D-Bus-client library; `wallpaper-settings`, the GUI binary) plus amendments to three
already-shipped crates (`renderer`, `wallpaperctl`, `pack-loader`) and spec 5's not-yet-
implemented packaging artifacts. One new non-crate directory: `tools/generate-starter-pack`
(maintainer-only, never built as part of the normal workspace build — research.md R5) and
`assets/starter-pack/` (its checked-in static output).

**Performance Goals**: GUI timeline/query views reflect live daemon state within the same
reaction bound already established (spec 3 FR-007, 2 seconds) — no new bound introduced, the GUI
reads the identical live state `wallpaperctl query` does. IP-geolocation's STUN lookup is cached
24 hours (research.md R4) — not repeated per solar-event resolution.

**Constraints**: No `unwrap()`/`expect()` outside `#[cfg(test)]` (constitution Principle VIII,
unchanged gate). The GUI and CLI MUST never disagree about persisted state (spec.md FR-007) —
enforced structurally by both depending on the same `wallpaper-ipc` crate rather than by
convention. IP-geolocation's one external touchpoint (STUN) MUST be disclosed to the user before
opt-in (spec.md FR-014) and MUST NOT be invoked more often than its cache TTL.

**Scale/Scope**: Single-user desktop session, same trust boundary as every prior spec. The GUI is
a single-window settings app, not a multi-window/multi-document application.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|---|---|---|
| I. Independent Renderer / Exclusive Output Ownership | N/A | This spec doesn't touch layer-shell surface ownership. |
| II. Wayland-Native, No X11 | **PASS** | The mock hotplug harness (research.md R7) tests real Wayland client code against a real (if minimal) `wayland-server` double — no X11 anywhere, including in tests. |
| III. GPU-Accelerated Crossfade | N/A | Untouched by this spec. |
| IV. Settings Live in cosmic-config | **PASS** | Every new persisted field (`LocationConfig` v3, pack registry's `origin`) extends existing `cosmic-config` schemas (research.md R6/R9) — no bespoke format, and the GUI writes through the same schemas the CLI does (research.md R2), not a parallel store. |
| V. Solar/Time Logic Pure, Deterministic, Unit-Tested | N/A (consumed, not touched) | IP-geolocation supplies a `Location` input the same way spec 6's automatic mode does — spec 1's solar math itself is untouched. |
| VI. Two Scheduling Modes | **PASS** | IP-geolocation's STUN/`maxminddb` resolution reuses spec 6's existing idle-friendly resolution/retry model (research.md R4's 24h cache is itself timer-scheduled, not polling); the GUI's live views subscribe to existing config-watch/D-Bus mechanisms, not a new poll loop. |
| VII. Per-Output Correctness Under Hotplug/Scaling | **PASS** | Directly served by this spec's FR-016/017 — the mock harness (research.md R7) is new, real coverage for exactly this principle's own required scenario, not a regression risk. |
| VIII. Failures Are Contained, Never Fatal | **PASS** | IP-geolocation failure degrades exactly like spec 6 FR-005 (spec.md FR-015); a missing/corrupt starter pack falls back to spec 2's existing `Unavailable` handling (spec.md Edge Cases); no `unwrap()`/`expect()` outside tests anywhere new. |
| IX. Native COSMIC Look and Feel | **PASS** | The entire reason this spec's GUI exists — libcosmic widgets/theme tokens throughout (research.md R1), standalone app per spec.md's Clarifications. |
| X. Config Schema Versioned With Migration Path | **PASS** | Both schema extensions (`LocationConfig` v3, pack registry `origin`) are versioned bumps with no silent misinterpretation of old data (research.md R6/R9, reusing spec 6 R7's verified no-migration-code-needed mechanism). |
| XI. Session Integration, Including Superseding cosmic-bg | **PASS** (extends spec 5) | Starter-pack registration happens in spec 5's `postinst` (research.md R5/R6) — this spec's packaging additions are amendments to spec 5's not-yet-implemented artifacts, not a parallel install path. |

**Gate result**: PASS. No Complexity Tracking entries for constitution violations — the two new
crates (`wallpaper-ipc`, `wallpaper-settings`) are additive workspace members, not exceptions to
any principle above; research.md R2 explains why `wallpaper-ipc` specifically is the *lower*-
complexity choice (one shared definition vs. a third independently-drifting copy).

### ⚠️ Findings requiring the user's attention (not gate failures — flagged per this project's established practice)

1. **research.md R2 — architecture correction to already-shipped code.** `crates/renderer` and
   `crates/wallpaperctl` currently each independently define `LocationConfigEntry`/
   `RendererConfig`-shaped types that must stay byte-for-byte compatible; this project has already
   had one real bug from exactly that drifting (documented in `crates/renderer/src/config.rs`'s
   own comments). This plan extracts a new `wallpaper-ipc` crate as the single source of truth,
   refactoring both existing crates to depend on it rather than adding a third copy for the GUI.
   This is a real amendment to already-shipped spec 3/4 code, called out explicitly rather than
   silently folded in, matching spec 6's own precedent of flagging amendments to shipped code.
2. **research.md R4 — a real tension with the user's own privacy-motivated Clarification choice.**
   "Bundled offline database" (chosen specifically to avoid a live third-party geolocation call)
   does not, by itself, solve discovering a NAT'd machine's own public IP address — some external
   touchpoint is unavoidable for the general case. This plan proposes STUN (a narrow, purpose-
   built exception, cached 24h, disclosed to the user before opt-in per FR-014) as the smallest
   reasonable resolution, but this is a real design tradeoff the user did not explicitly weigh in
   on during the original Clarification and may want to revisit.
3. **Crossfade duration is not actually configurable today, despite a doc comment claiming it
   is.** `crates/renderer/src/crossfade.rs` carries the comment "Fixed 45s default (FR-002),
   configurable," but `crates/renderer/src/surface.rs:55` defines it as a plain Rust
   `pub const CROSSFADE_DURATION: Duration = Duration::from_secs(45)` — a compile-time constant,
   read from no config anywhere. FR-006 (the GUI's crossfade duration control) is therefore real
   new work on already-shipped spec 3 code, not just a GUI-side form field: `RendererConfig`
   needs a new `crossfade_duration_secs` field (contracts/gui-application.md), and `surface.rs`'s
   three call sites reading the constant directly need to read the config value instead. Flagged
   here since the existing doc comment could otherwise mislead a future contributor into thinking
   this plumbing already exists.

**Post-Phase-1 re-check**: Design artifacts (data-model.md, contracts/, quickstart.md) build
directly on the decisions above with no new gaps surfaced. Gate result unchanged: PASS.

## Project Structure

### Documentation (this feature)

```text
specs/007-v1-completion/
├── plan.md                          # This file (/speckit-plan command output)
├── research.md                      # Phase 0 output (/speckit-plan command)
├── data-model.md                    # Phase 1 output (/speckit-plan command)
├── quickstart.md                    # Phase 1 output (/speckit-plan command)
├── contracts/                       # Phase 1 output (/speckit-plan command)
│   ├── wallpaper-ipc-crate.md
│   ├── location-config-schema-v3.md
│   ├── pack-registry-origin.md
│   └── gui-application.md
├── checklists/
│   └── requirements.md
└── tasks.md                         # Phase 2 output (/speckit-tasks command - NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
crates/
├── schedule-engine/                 # spec 1 (dependency, not modified here)
├── pack-loader/                     # spec 2 — gains PackOrigin field (research.md R6)
│   └── src/registry.rs
├── wallpaper-ipc/                   # NEW — shared schema + D-Bus client (research.md R2)
│   └── src/
│       ├── renderer_config.rs        # moved from crates/renderer/src/output.rs's config half
│       ├── location_config.rs        # moved from renderer+wallpaperctl's independent copies; v3 (research.md R9)
│       └── dbus_client.rs            # moved from crates/wallpaperctl/src/dbus_client.rs
├── renderer/                        # spec 3 — now depends on wallpaper-ipc; gains ip_geolocation.rs
│   ├── Cargo.toml                    # + maxminddb, stunclient; + wallpaper-ipc path dep
│   │                                  # + [package.metadata.deb] recommends = "geoclue-2.0" (research.md R8)
│   ├── src/
│   │   ├── config.rs                  # amended: re-exports wallpaper-ipc types instead of defining its own
│   │   └── ip_geolocation.rs          # NEW — maxminddb lookup + stunclient public-IP discovery + 24h cache
│   └── tests/
│       └── hotplug_mock.rs            # NEW — wayland-server-backed harness (research.md R7, closes T043)
├── wallpaperctl/                    # spec 4 — now depends on wallpaper-ipc instead of defining its own copies
│   └── src/
│       ├── config.rs                  # amended: re-exports wallpaper-ipc types
│       └── dbus_client.rs             # removed — moved into wallpaper-ipc
└── wallpaper-settings/              # NEW — the GUI (research.md R1)
    ├── Cargo.toml                    # libcosmic (git), wallpaper-ipc, schedule-engine, pack-loader — NOT renderer
    └── src/
        ├── main.rs                    # cosmic::app::run entry point
        ├── app.rs                     # cosmic::Application impl, top-level view/update
        └── pages/
            ├── packs.rs                # FR-002 pack browser + preview
            ├── assignment.rs           # FR-003 per-output / same-everywhere assignment
            ├── location.rs             # FR-004 manual/automatic/IP-geo mode switch
            ├── timeline.rs             # FR-005 today's schedule visualization
            └── crossfade.rs            # FR-006 duration control

tools/
└── generate-starter-pack/           # NEW — maintainer-only, not a normal build target (research.md R5)
    └── src/main.rs

assets/
└── starter-pack/                    # NEW — checked-in static output of the generator tool
    ├── manifest.toml
    └── *.png

packaging/                          # spec 5 (not yet implemented) — this spec amends its planned contents
├── debian/
│   └── postinst                     # amended: registers assets/starter-pack/ with origin: Package
└── (geoclue Recommends lands in crates/renderer/Cargo.toml's [package.metadata.deb], not here)
```

**Structure Decision**: Two new workspace crates (`wallpaper-ipc`, `wallpaper-settings`) plus
amendments to three already-shipped crates and spec 5's planned packaging. `wallpaper-ipc` is
deliberately dependency-light (no Wayland/GPU) so both `wallpaperctl` and the new GUI stay free of
those dependencies, preserving spec 4's original design goal (plan.md Constitution Check finding
1). The starter-pack generator (`tools/`) and its output (`assets/`) are new top-level directories
outside `crates/`, matching how spec 5's `packaging/` already sits outside `crates/` for the same
reason — neither is a normal Rust library/binary consumed by the workspace's own build.

## Complexity Tracking

*No entries — Constitution Check passed with no unjustified violations. The two new crates are
additive, not exceptions; research.md R2 explains in detail why the shared-crate extraction is
the lower-complexity choice, not a violation this spec needs to justify away.*
