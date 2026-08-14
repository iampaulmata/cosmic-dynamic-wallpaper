# Implementation Plan: GUI Usability Improvements

**Branch**: `008-gui-usability-improvements` | **Date**: 2026-08-14 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/008-gui-usability-improvements/spec.md`

**Note**: This template is filled in by the `/speckit-plan` command; its definition describes the execution workflow.

## Summary

Four usability fixes to the standalone `wallpaper-settings` GUI shipped in spec 7, all scoped to
that one existing crate: pack add/remove via a native file/folder picker (US1), scrollable pages
so no control is ever unreachable (US2), the IP-geolocation disclosure surfaced on hover *and* by
tap before opt-in (US3), and the Assignment page showing pack names instead of file paths (US4).
**Every mechanism needed already ships inside the `libcosmic` dependency this crate already
pins** (`cosmic::dialog::file_chooser`, `cosmic::widget::dialog`, `cosmic::widget::tooltip`,
`cosmic::widget::scrollable`) — research.md's five decisions add zero new Cargo dependencies. One
real finding from `/speckit-clarify` carries into this plan: `IP_GEOLOCATION_DISCLOSURE` turned
out to be two independently-duplicated string literals, not a shared constant despite a doc
comment claiming otherwise — this plan folds it into `wallpaper-ipc` (research.md R4), the exact
class of fix that crate was created for in spec 7.

## Technical Context

**Language/Version**: Rust, stable toolchain, edition 2021 — same workspace as specs 1–7.

**Primary Dependencies**: No new Cargo dependencies. `libcosmic` (already pinned in
`crates/wallpaper-settings/Cargo.toml` with the `xdg-portal` feature already enabled) supplies
`cosmic::dialog::file_chooser`, `cosmic::widget::dialog`, `cosmic::widget::tooltip`, and
`cosmic::widget::scrollable` — all four mechanisms this spec needs (research.md R1/R3/R4/R5).
`wallpaper-ipc` (spec 7) gains one relocated constant (research.md R4); `pack-loader`'s existing
`load_pack` is reused, not extended (research.md R2).

**Storage**: No `cosmic-config` schema changes. `resolve_pack_name` (data-model.md) is a pure,
read-time-only derivation from data already on disk (`PackRegistryEntry.source` + the pack's own
manifest/filename) — nothing new is persisted, so constitution Principle X's migration
requirement doesn't apply anywhere in this spec.

**Testing**: `cargo test -p wallpaper-settings` for `resolve_pack_name`'s two branches (manifest
name, extension-stripped static filename) plus every new message's pure state transition
(add/remove dispatch, the removal confirmation state machine, the disclosure toggle) — same "pure
core, thin UI shell" split this crate already established in spec 7. The four manual smoke checks
in quickstart.md close the rendered/interactive half (dialogs, tooltips, scrolling) against a real
COSMIC session, the same posture spec 7's own GUI quickstart already used.

**Target Platform**: Linux, COSMIC desktop — same as spec 7's GUI; no new platform surface.

**Project Type**: Extends one already-shipped crate (`wallpaper-settings`) plus a small,
structurally-motivated amendment to `wallpaper-ipc` (relocating one constant) and a one-line
wording tidy-up in `wallpaperctl`. No new workspace members.

**Performance Goals**: None beyond what spec 7 already established — this is UI-interaction-only
work (dialogs, tooltips, scroll), not on any latency-sensitive path (no rendering/scheduling code
touched).

**Constraints**: No `unwrap()`/`expect()` outside `#[cfg(test)]` (constitution Principle VIII,
unchanged gate) — a cancelled file-chooser dialog and a `load_pack` failure are both modeled as
`Result`/`Option`, never assumed to succeed. The GUI and CLI MUST continue to never disagree about
persisted state (spec 7 FR-007) — this spec's add/remove actions call the identical
`pack_loader::Registry::register`/`remove` functions `wallpaperctl` already calls, not a
parallel path (contracts/gui-usability-improvements.md).

**Scale/Scope**: Single-user desktop session, unchanged from spec 7 — no new trust boundary, no
new external network touchpoint (the file chooser is a local desktop portal call, not network
I/O).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|---|---|---|
| I. Independent Renderer / Exclusive Output Ownership | N/A | This spec doesn't touch layer-shell surface ownership or `wallpaperd` at all — GUI-only. |
| II. Wayland-Native, No X11 | N/A | No rendering/output code touched. |
| III. GPU-Accelerated Crossfade | N/A | Untouched by this spec. |
| IV. Settings Live in cosmic-config | **PASS** | No new persisted state anywhere (Technical Context, Storage) — add/remove write through the exact same `pack_loader::Registry` `cosmic-config` entry `wallpaperctl` already uses; nothing new to keep in sync. |
| V. Solar/Time Logic Pure, Deterministic, Unit-Tested | N/A | Untouched. |
| VI. Two Scheduling Modes | N/A | This spec touches no scheduling/render-loop code. |
| VII. Per-Output Correctness Under Hotplug/Scaling | N/A | Untouched. |
| VIII. Failures Are Contained, Never Fatal | **PASS** | A cancelled file-chooser dialog, an invalid pack path, and a `load_pack` failure during name resolution are all handled as `Result`/`Option` with a specific, shown message (data-model.md) — never a panic, never a silent partial write. |
| IX. Native COSMIC Look and Feel | **PASS** | Every new widget (`file_chooser`, `dialog`, `tooltip`, `scrollable`) is a stock `libcosmic` widget using the shared theme tokens — no foreign toolkit, no bespoke dialog implementation. |
| X. Config Schema Versioned With Migration Path | **PASS (N/A in practice)** | No schema change in this spec (Technical Context, Storage) — nothing to version or migrate. |
| XI. Session Integration, Including Superseding cosmic-bg | N/A | This spec doesn't touch packaging, autostart, or `cosmic-bg` interaction. |

**Gate result**: PASS. No Complexity Tracking entries — every mechanism this spec needs already
exists in a dependency already present in the crate it extends; the one cross-crate change
(relocating `IP_GEOLOCATION_DISCLOSURE` into `wallpaper-ipc`) *reduces* duplication rather than
adding a new exception to any principle above.

### ⚠️ Finding requiring the user's attention (not a gate failure — flagged per this project's established practice)

1. **`IP_GEOLOCATION_DISCLOSURE` was never actually a shared constant** (research.md R4). Its own
   doc comment in `crates/wallpaper-settings/src/pages/location.rs` claims it's kept identical to
   `wallpaperctl`'s copy "so the two control surfaces never say different things about the one
   external touchpoint" — but it's two independent string literals that merely happened to match.
   This is the exact bug class `wallpaper-ipc` was created to prevent (spec 7 research.md R2).
   Since FR-009 requires editing both copies anyway (the sentence-case fix), this plan relocates
   the constant into `wallpaper-ipc` instead of editing two copies in lockstep again. Called out
   explicitly rather than silently folded in, matching this project's precedent (spec 7's own
   Constitution Check findings 1–3).

**Post-Phase-1 re-check**: Design artifacts (data-model.md, contracts/, quickstart.md) build
directly on the decisions above with no new gaps surfaced. Gate result unchanged: PASS.

## Project Structure

### Documentation (this feature)

```text
specs/008-gui-usability-improvements/
├── plan.md                          # This file (/speckit-plan command output)
├── research.md                      # Phase 0 output (/speckit-plan command)
├── data-model.md                    # Phase 1 output (/speckit-plan command)
├── quickstart.md                    # Phase 1 output (/speckit-plan command)
├── contracts/                       # Phase 1 output (/speckit-plan command)
│   └── gui-usability-improvements.md
├── checklists/
│   └── requirements.md
└── tasks.md                         # Phase 2 output (/speckit-tasks command - NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
crates/
├── wallpaper-ipc/                   # spec 7 (existing) — gains one relocated constant
│   └── src/
│       └── lib.rs                    # + `pub const IP_GEOLOCATION_DISCLOSURE` (research.md R4)
├── pack-loader/                     # spec 2 (existing) — unchanged; `load_pack` reused as-is (research.md R2)
├── wallpaperctl/                    # spec 4 (existing) — small amendment only
│   └── src/commands/location.rs      # imports the relocated constant; rewords one message (data-model.md)
└── wallpaper-settings/              # spec 7 (existing) — this spec's main surface
    ├── src/
    │   ├── main.rs                    # unchanged (window size left as-is, research.md R5)
    │   ├── app.rs                     # + `dialog()` override (research.md R3); + file-chooser Task dispatch (research.md R1)
    │   └── pages/
    │       ├── packs.rs                # + add/remove messages & state (data-model.md); name resolution (research.md R2)
    │       ├── assignment.rs           # view() uses resolve_pack_name instead of path (research.md R2)
    │       ├── location.rs             # + tooltip/info-icon disclosure (research.md R4); scrollable wrap
    │       ├── timeline.rs             # scrollable wrap only
    │       └── crossfade.rs            # scrollable wrap only
    └── README.md                      # amended: US1's add/remove note supersedes spec 7's "registration out of
                                        # scope" note (contracts/gui-usability-improvements.md)
```

**Structure Decision**: No new workspace members. All four user stories are scoped to
`wallpaper-settings` (spec 7's existing GUI crate), with one small, well-justified amendment to
`wallpaper-ipc` (a constant relocation, not new functionality) and a cosmetic one-line change in
`wallpaperctl`. This is the minimal-footprint shape: every mechanism needed is already available
to the crate that needs it (research.md's "no new dependencies" finding).

## Complexity Tracking

*No entries — Constitution Check passed with no unjustified violations. The `wallpaper-ipc`
constant relocation is a duplication *reduction*, not a new exception to justify.*
