# Implementation Plan: Custom Pack Builder

**Branch**: `010-custom-pack-builder` | **Date**: 2026-08-15 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/010-custom-pack-builder/spec.md`

**Note**: This template is filled in by the `/speckit-plan` command; its definition describes the execution workflow.

## Summary

A GUI wizard, entered from the existing `wallpaper-settings` Packs page's "Add pack folder…"
button, that turns a plain folder of images with no `manifest.toml` into a fully valid custom
pack: choose solar-period or specific-time scheduling, assign every image via a thumbnail-plus-
control row, enter an author name (defaulting to "Artist Unknown"), and Generate a
self-validated `manifest.toml`, then choose whether to move the folder into the application's
standard per-user pack location or leave it in place. **No new top-level UI surface and no new
`cosmic-config` schema** — the wizard hooks into the exact folder path that today produces a
plain "no manifest found" error (research.md R1), and every duplicate/conflict/size check is the
existing, already-tested `schedule_engine::WallpaperPack::validate` (research.md R4), not new
logic. The one real new piece of business logic is a small, symmetric write-side addition to
`pack-loader`'s manifest module — `render`/`format_anchor`, the mirror image of its existing
`parse`/`parse_anchor` (research.md R3, data-model.md) — plus a copy-then-verify-then-delete move
routine that never leaves a folder half-moved on failure (research.md R8).

## Technical Context

**Language/Version**: Rust, stable toolchain, edition 2021 — same workspace as specs 1–9.

**Primary Dependencies**: No new Cargo dependencies except `dirs` becoming a *direct* dependency
of `wallpaper-settings` (research.md R8) — already resolved at v6.0.0 in `Cargo.lock` as a
transitive dependency of `cosmic-config`, so this adds zero new crates to the build. `image`
becomes a direct (non-dev) dependency of `wallpaper-settings` (research.md R2), same
version/feature set `pack-loader` already pins. `libcosmic` (already pinned, `xdg-portal`
feature already enabled) supplies everything else: `cosmic::dialog::file_chooser` (folder
picker, reused unchanged), `cosmic::widget::dialog` (placement/collision prompts),
`cosmic::widget::dropdown` (solar-event selector), `cosmic::widget::spin_button` (offset and
time-of-day entry, research.md R6/R7), `cosmic::widget::image` (thumbnails), `cosmic::widget::
text_input` (author field). `pack-loader` gains `render`/`format_anchor` (research.md R3,
contracts/pack-loader-manifest-writer.md); `schedule_engine::WallpaperPack::validate`/
`ValidatedPack::check_solar_duplicate_instant` are reused as-is, not extended (research.md R4).

**Storage**: No `cosmic-config` schema changes. `manifest.toml` is pack-loader's existing,
already-versioned on-disk format (`schema_version = 1`, unchanged) — this feature only adds a
writer for a format that already exists. The generated pack registers via the identical
`PackSource::resolve` + `Registry::register` call the existing "Add pack folder…" success path
already makes (research.md R1) — no new persisted shape there either. The only new filesystem
behavior is the optional move into `dirs::data_dir()/cosmic-dynamic-wallpaper/packs/` (research.md
R8), a plain directory, not a config store.

**Testing**: `cargo test -p pack-loader` for `render`/`format_anchor`'s round-trip and
TOML-escaping guarantees (data-model.md, contracts/pack-loader-manifest-writer.md);
`cargo test -p wallpaper-settings` for the new `pages::pack_builder` pure functions
(`all_assigned`, `detect_conflict`, `build_draft`, `combine_offset`, `effective_author` —
data-model.md) in the same pure-logic-separated-from-`view()` style every existing page in this
crate already uses. No new integration-test harness — manual COSMIC-session validation
(quickstart.md) covers the folder picker, dialogs, and spin buttons themselves, matching every
prior GUI spec's split (008 quickstart.md's own opening line).

**Target Platform**: Linux/Wayland (COSMIC desktop) — no change from the rest of the workspace;
this feature touches no rendering/layer-shell code at all.

**Project Type**: Desktop GUI feature within the existing `wallpaper-settings` binary
(`cosmic-wallpaper-settings`), plus a small library addition to the existing `pack-loader` crate.
Not a new crate.

**Performance Goals**: Not perf-critical. Folder scanning and per-row conflict re-checks operate
on at most 64 images (the pack format's own cap, FR-018) using header-only image reads (research.md
R2) — the same cost class `pack_loader::load_pack` already pays for every registered pack, several
times over, with no reported performance issue.

**Constraints**: `unwrap()`/`expect()` outside `#[cfg(test)]` remain review-blocking findings
(constitution Principle VIII) in both `pack-loader` (already `clippy::unwrap_used`/
`expect_used = "deny"`) and any new `wallpaper-settings` code touching this feature — every
fallible step (scan, write, self-validate, move) returns a `Result`/`Option` surfaced as a
specific message, never a panic (FR-017).

**Scale/Scope**: One new `pack-loader` module addition (`render`/`format_anchor` in the existing
`manifest.rs`, data-model.md), one new `wallpaper-settings` page-shaped module
(`pages/pack_builder.rs`), and the `app.rs` wiring to open/close it (research.md R9). No changes
to `renderer`, `wallpaperd`, `wallpaperctl`, or `wallpaper-ipc`.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Applies? | Assessment |
|---|---|---|
| I. Independent Renderer, Exclusive Ownership | No | GUI-only feature; touches no layer-shell surface or renderer code. |
| II. Wayland-Native, No X11 | No | No new windowing/protocol code — `wallpaper-settings` already runs under `libcosmic`/Wayland unchanged. |
| III. GPU-Accelerated Crossfade | No | No rendering path touched. |
| IV. Settings Live in `cosmic-config` | **Yes — pass** | The only new persisted artifact is `manifest.toml`, which is pack-loader's existing, already-`cosmic-config`-independent format (by design — packs are files, not config, per spec 2). No new bespoke *config* format is introduced; the registry entry the generated pack gets is the exact existing `cosmic-config`-backed `Registry::register` call, unchanged. |
| V. Solar/Time Logic Is Pure, Deterministic, Unit-Tested | **Yes — pass** | New anchor-formatting (`format_anchor`) and offset-combination (`combine_offset`) logic is pure, has no I/O, and ships with round-trip/unit tests (data-model.md, contracts/pack-loader-manifest-writer.md). No new solar-position math is added — duplicate/conflict detection reuses `schedule_engine`'s existing, already-vetted, already-tested functions rather than reimplementing anything (research.md R4). |
| VI. Two Scheduling Modes (daemon) | No | This feature runs entirely inside `wallpaper-settings`, a separate process from `wallpaperd`; it never touches the daemon's idle-wait/active-transition states. |
| VII. Per-Output Correctness | No | No multi-output/rendering surface touched. |
| VIII. Failures Are Contained, Never Fatal | **Yes — pass** | Every fallible step (scan, write, self-validate via `load_pack`, move) returns `Result`, matching `pack-loader`'s existing `clippy::unwrap_used`/`expect_used = "deny"` lint gate; a failed move leaves the source folder untouched rather than corrupting or losing it (FR-017, research.md R8). |
| IX. Native COSMIC Look and Feel | **Yes — pass** | Every wizard control is a `libcosmic` widget (`dropdown`, `spin_button`, `dialog`, `text_input`, `image`) — no GTK/Qt/web view, no new toolkit dependency. |
| X. Config Schema Versioned With Migration | No | No `cosmic-config` schema changes anywhere in this feature (see Storage above) — nothing to version or migrate. |
| XI. Session Integration | No | No packaging/autostart/session changes. |

No violations. No entries needed in Complexity Tracking.

## Project Structure

### Documentation (this feature)

```text
specs/010-custom-pack-builder/
├── plan.md              # This file (/speckit-plan command output)
├── research.md          # Phase 0 output (/speckit-plan command)
├── data-model.md         # Phase 1 output (/speckit-plan command)
├── quickstart.md        # Phase 1 output (/speckit-plan command)
├── contracts/            # Phase 1 output (/speckit-plan command)
│   ├── pack-loader-manifest-writer.md
│   └── pack-builder-gui-flow.md
├── checklists/
│   └── requirements.md
└── tasks.md              # Phase 2 output (/speckit-tasks command - NOT created by /speckit-plan)
```

### Source Code (repository root)

This is an existing Rust Cargo workspace (`Cargo.toml` `[workspace] members`), not a fresh
project — the structure below lists only what this feature adds or touches inside it.

```text
crates/
├── pack-loader/                        # existing crate — gains a write-side
│   └── src/
│       └── manifest.rs                 # + ManifestDraft, render(), format_anchor() (data-model.md)
│
├── schedule-engine/                    # existing crate — unchanged, reused as-is (research.md R4)
│
└── wallpaper-settings/                 # existing crate — gains the wizard
    ├── Cargo.toml                      # + direct deps: `image`, `dirs` (research.md R2, R8)
    └── src/
        ├── app.rs                      # + `pack_builder: Option<pages::pack_builder::State>` field,
        │                                #   folder-picker branch on `ManifestNotFound` (research.md R1, R9)
        └── pages/
            └── pack_builder.rs         # new — State, Message, pure functions, view() (data-model.md)

tests/ (in-crate, existing convention — `#[cfg(test)] mod tests` per module, no separate tests/ dir)
```

**Structure Decision**: Extends two existing crates (`pack-loader`, `wallpaper-settings`) in
place; introduces no new crate and no new workspace member. Matches every prior GUI-touching spec
in this repository (7, 8), which added pages/modules to `wallpaper-settings` rather than standing
up a separate crate for GUI-only logic.

## Complexity Tracking

*No entries — Constitution Check above found no violations to justify.*
