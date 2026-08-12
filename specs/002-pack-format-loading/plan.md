# Implementation Plan: Pack Format & Loading

**Branch**: `002-pack-format-loading` | **Date**: 2026-08-11 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/002-pack-format-loading/spec.md`

**Note**: This template is filled in by the `/speckit-plan` command; its definition describes the execution workflow.

## Summary

A Rust library (`pack-loader`) that turns a directory of images plus a TOML manifest — or a
single image file with no manifest at all — into the in-memory `LoadedPack` shape specs 1
(scheduling) and 3 (renderer) consume. It parses and validates the manifest (schema version,
scaling config, path-containment against a malicious/shared manifest), delegates all
time-anchor correctness to spec 1's `WallpaperPack::validate` rather than re-implementing it,
and persists the set of known pack locations via `cosmic-config` so they survive a daemon
restart, with explicit user-initiated removal distinct from automatic "unavailable" marking
when a pack's source disappears. No rendering, output assignment, or Wayland code is part of
this spec.

## Technical Context

**Language/Version**: Rust, stable toolchain, edition 2021 — same workspace as spec 1; no
new MSRV constraint introduced by this spec's dependencies.

**Primary Dependencies**: [`toml`](https://crates.io/crates/toml) + `serde` (manifest
parsing, research.md R1), [`image`](https://crates.io/crates/image) (header-only readability
validation, R2), [`cosmic-config`](https://github.com/pop-os/libcosmic) (git dependency, pack
registry persistence, R4), spec 1's `schedule-engine` crate (workspace path dependency, for
`TimeAnchor`/`WallpaperPack::validate`, FR-003).

**Storage**: `cosmic-config` (RON-based, XDG config dir) for the pack registry only
(FR-010–FR-012). Pack manifests and images themselves are read-only local files, not written
by this crate.

**Testing**: `cargo test` against committed fixture directories (valid packs, every FR-006/
FR-006a rejection case) plus `tempfile`-backed registry round-trip tests (R6).

**Target Platform**: Linux (COSMIC desktop). This crate does real filesystem I/O (unlike
spec 1's pure computation), so its tests are not portable to environments without a
filesystem, but remain independent of any Wayland/display session.

**Project Type**: Library — second crate (`crates/pack-loader`) in the same Cargo workspace
spec 1 established, depending on spec 1's crate.

**Performance Goals**: Loading a 64-image manifest pack (the max size, inherited from spec
1's FR-001) completes in well under 500ms on local storage, excluding full pixel decode of
every image (research.md R7 — resolves the Outstanding performance item from
`/speckit-clarify`).

**Constraints**: No panics outside `#[cfg(test)]` code on any load/parse/registry path
(constitution Principle VIII); a manifest is treated as untrusted input (FR-006a) since packs
are meant to be shared between people, not just authored and consumed by the same user.

**Scale/Scope**: Single-user, single-machine. Pack size capped at 64 images (inherited from
spec 1). Registry size is not bounded — no PRD signal that it needs to be.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|---|---|---|
| I. Independent Renderer / Exclusive Output Ownership | N/A | No layer-shell/renderer code in this spec. |
| II. Wayland-Native, No X11 | N/A | No windowing/protocol code. |
| III. GPU-Accelerated Crossfade | N/A | No rendering; this crate only validates/loads content. |
| IV. Settings Live in cosmic-config | **PASS** | The pack *registry* (which locations are known, FR-010) is persisted via `cosmic-config` (research.md R4) — the constitution's mandated layer. The manifest TOML file itself is treated as authored *content* (like the image files it sits beside), not daemon *state* — it is never the runtime-read persistence layer for anything cosmic-config itself would own, and Principle IV's own text explicitly allows non-native formats as *content/import sources*, which is the role a manifest plays here. |
| V. Solar/Time Logic Pure, Deterministic, Unit-Tested | N/A (consumed, not re-implemented) | This spec deliberately does not re-implement anchor-correctness logic — FR-003 hands every anchor to spec 1's already-tested `WallpaperPack::validate`. Duplicating that logic here would itself be a Principle V violation (two implementations of the same pure logic drifting apart). |
| VI. Two Scheduling Modes | N/A | No scheduling/render-loop code in this spec. |
| VII. Per-Output Correctness | N/A | No output assignment in this spec (spec 3). |
| VIII. Failures Contained, Never Fatal | **PASS** | Every fallible path (bad manifest, missing/unreadable/out-of-directory image, registry I/O failure) returns a typed `Result` (data-model.md Error types); no `unwrap()`/`expect()` outside tests, same CI lint gate as spec 1. |
| IX. Native COSMIC UI | N/A | No UI in this spec; it's a library other specs' UIs will call into. |
| X. Config Schema Versioned | **PASS** | Two distinct, explicitly-scoped versioning stories: `cosmic-config`'s own versioning for the registry (R4), and this spec's own `schema_version` field + migration function for the manifest TOML itself (FR-007, R5) — documented separately so neither is silently assumed to cover the other. |
| XI. Session Integration | N/A | No daemon/session wiring in this spec. |

**Gate result**: PASS. No Complexity Tracking entries required. The one principle worth a
closer look (IV) is addressed directly above with the content-vs-state distinction, rather
than left as an unexamined N/A.

## Project Structure

### Documentation (this feature)

```text
specs/002-pack-format-loading/
├── plan.md              # This file (/speckit-plan command output)
├── research.md          # Phase 0 output (/speckit-plan command)
├── data-model.md         # Phase 1 output (/speckit-plan command)
├── quickstart.md         # Phase 1 output (/speckit-plan command)
├── contracts/            # Phase 1 output (/speckit-plan command)
│   └── pack-loader-api.md
└── tasks.md              # Phase 2 output (/speckit-tasks command - NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
Cargo.toml                        # workspace root (established by spec 1; this spec adds a member)
crates/
├── schedule-engine/               # spec 1 (dependency of this crate, not modified here)
└── pack-loader/
    ├── Cargo.toml
    ├── src/
    │   ├── lib.rs                 # public API re-exports (contracts/pack-loader-api.md)
    │   ├── manifest.rs            # PackManifest/ManifestImage TOML shape + parsing (FR-002, FR-006, FR-007)
    │   ├── path_safety.rs         # containment check (FR-006a, research.md R3)
    │   ├── image_check.rs         # header-only readability validation (FR-006, research.md R2)
    │   ├── load.rs                 # load_pack(): ties manifest.rs + path_safety.rs + image_check.rs + schedule-engine together (FR-001, FR-003, FR-004, FR-005)
    │   ├── registry.rs             # Registry: register/remove/known_packs/reload_all via cosmic-config (FR-009–FR-012)
    │   └── error.rs                # ManifestError, RegistryError
    └── tests/
        ├── fixtures/
        │   ├── valid_pack/          # User Story 1 happy path
        │   ├── static_image/         # User Story 2
        │   ├── scaling_overrides/    # User Story 3
        │   └── invalid/               # one subdir per FR-006/FR-006a rejection case
        ├── load_pack.rs
        └── registry.rs               # tempfile-backed, User Story 4
```

**Structure Decision**: `pack-loader` joins `schedule-engine` as the second member of the
workspace spec 1 established, with a straight path dependency on it — no circular
dependency, since `schedule-engine` has no knowledge of manifests, files, or `cosmic-config`.

## Complexity Tracking

*No entries — Constitution Check passed with no unjustified violations (see table above).*
