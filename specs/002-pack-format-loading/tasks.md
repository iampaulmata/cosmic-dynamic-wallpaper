---

description: "Task list template for feature implementation"
---

# Tasks: Pack Format & Loading

**Input**: Design documents from `/specs/002-pack-format-loading/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/pack-loader-api.md, quickstart.md

**Tests**: Included. plan.md's Technical Context commits this spec to `cargo test` against
committed fixtures plus `tempfile`-backed registry round-trip tests (research.md R6), and
spec.md's SC-002 ("100% of malformed manifests... produce a specific, actionable error") is
only checkable with an actual test suite covering every FR-006/FR-006a rejection case.

**Organization**: Tasks are grouped by user story (spec.md) to enable independent
implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: Which user story this task belongs to (US1, US2, US3, US4)
- File paths are relative to the repository root

## Path Conventions

Second library crate in the workspace spec 1 established (plan.md Structure Decision):
`crates/pack-loader/src/`, `crates/pack-loader/tests/`, with a path dependency on
`crates/schedule-engine` (spec 1).

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Add `pack-loader` as the workspace's second crate, wired to spec 1's crate.

- [X] T001 Add `crates/pack-loader` as a new member of the workspace root `Cargo.toml` (plan.md Project Structure)
- [X] T002 Create `crates/pack-loader/Cargo.toml` with `toml` + `serde` (derive), `image`, `cosmic-config` (git dependency on `pop-os/libcosmic`) dependencies, a path dependency on `schedule-engine`, and `tempfile` as a dev-dependency (research.md R1, R2, R4, R6) — **deviation**: `cosmic-config` deliberately deferred to Phase 6 (US4), not added yet; see that phase's note. `humantime` added beyond the original list (needed for anchor-offset duration parsing, an implementation-detail gap the plan didn't specify a crate for).
- [X] T003 [P] Add `[lints]` denying `clippy::unwrap_used` and `clippy::expect_used` outside `#[cfg(test)]` to `crates/pack-loader/Cargo.toml` (constitution Principle VIII)
- [X] T004 [P] Add a CI workflow running `cargo test`, `cargo clippy`, and `cargo llvm-cov` for the crate in `.github/workflows/pack-loader-ci.yml`

**Checkpoint**: `cargo build` succeeds on an empty crate that depends on `schedule-engine`; CI pipeline is defined.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Shared types every user story needs. No user story work starts before this
phase is done.

**⚠️ CRITICAL**: Blocks Phases 3–6.

- [X] T005 Create `ManifestError` and `RegistryError` types in `crates/pack-loader/src/error.rs` (data-model.md Error types; `std::error::Error` + `Debug` + `Display`, no panics — constitution Principle VIII)
- [X] T006 [P] Create `PackManifest`, `ManifestImage`, `ScalingMode`, and `Color` TOML deserialization shapes in `crates/pack-loader/src/manifest.rs` (data-model.md PackManifest/ManifestImage/ScalingMode/Color, FR-001, FR-002; depends on T005 for `ManifestError`)
- [X] T007 [P] Create the `PackSource` tagged union (`Directory`/`StaticFile`) with canonicalizing constructors in `crates/pack-loader/src/pack_source.rs` (data-model.md PackSource, FR-009)
- [X] T008 [P] Implement the path containment check (canonicalize + `starts_with`) in `crates/pack-loader/src/path_safety.rs` (FR-006a, research.md R3; depends on T005 for `ManifestError`)
- [X] T009 [P] Implement header-only image readability validation (`ImageReader::open(..).with_guessed_format()?.into_dimensions()`) in `crates/pack-loader/src/image_check.rs` (FR-006, research.md R2; depends on T005)
- [X] T010 Create the `LoadedPack` type in `crates/pack-loader/src/load.rs` (data-model.md LoadedPack; depends on T006, T007)
- [X] T011 Wire up public API re-exports (`load_pack`, `LoadedPack`, `PackManifest`, `ManifestImage`, `ScalingMode`, `Color`, `PackSource`, `ManifestError`) in `crates/pack-loader/src/lib.rs` (depends on T005–T010; matches contracts/pack-loader-api.md)

**Checkpoint**: Crate compiles with all shared types defined; user stories can now proceed.

---

## Phase 3: User Story 1 - Load a Multi-Image Time-Anchored Pack From a Directory (Priority: P1) 🎯 MVP

**Goal**: Point the loader at a directory with a manifest and images and get back a fully
validated, spec-1-compatible in-memory pack.

**Independent Test**: Author a manifest referencing a small set of real image files with a
mix of time anchors, point the loader at that directory, and verify the resulting in-memory
pack has the correct images, anchors, and pack-level metadata.

### Tests for User Story 1

- [X] T012 [P] [US1] Create the `valid_pack` fixture directory (manifest + small real images with mixed anchors) in `crates/pack-loader/tests/fixtures/valid_pack/` (research.md R6, spec.md US1 Independent Test)
- [X] T013 [P] [US1] Create `invalid/` fixture subdirectories for missing-image, malformed-manifest, and unsupported-schema-version rejection cases in `crates/pack-loader/tests/fixtures/invalid/` (FR-006) — also added path_traversal, unreadable_image, mixed_anchors, invalid_scaling_mode, malformed_color beyond the original 3, covering every FR-006/FR-006a case in one pass
- [X] T014 [US1] Acceptance scenario tests — valid load returns correct images/anchors/metadata, missing-image error naming the file, spec 1 validation errors surfaced verbatim, extra untracked files ignored (spec.md US1 scenarios 1–4) in `crates/pack-loader/tests/load_pack.rs` (depends on T012, T013)

### Implementation for User Story 1

- [X] T015 [US1] Implement manifest parsing with `schema_version` support-check (parse failure / unsupported version → `ManifestError`) in `crates/pack-loader/src/manifest.rs` (FR-002, FR-006; extends T006)
- [X] T016 [US1] Implement `load_pack`'s directory branch — parse manifest, resolve + containment-check every image path (`path_safety`), header-validate readability (`image_check`), build the resolved `(id, TimeAnchor)` list — in `crates/pack-loader/src/load.rs` (FR-001; depends on T008, T009, T015)
- [X] T017 [US1] Hand the resolved anchor list to spec 1's `WallpaperPack::validate`, surfacing its `PackError` through `ManifestError` rather than re-implementing anchor rules, in `crates/pack-loader/src/load.rs` (FR-003; depends on T016)
- [X] T018 [US1] Ignore image files present in the pack directory but not referenced by the manifest, in `crates/pack-loader/src/load.rs` (FR-008; depends on T016)

**Checkpoint**: User Story 1 fully functional and testable independently (`cargo test --test load_pack`).

---

## Phase 4: User Story 2 - Zero-Config Static Wallpaper (Priority: P1) 🎯 MVP

**Goal**: Point the loader at a single image file with no manifest and get back a valid
one-image, no-anchor pack — full parity with a traditional wallpaper picker.

**Independent Test**: Point the loader at a single image file path with no manifest present,
and verify it produces a valid one-image pack with no time anchors.

### Tests for User Story 2

- [X] T019 [P] [US2] Create the `static_image/` fixture (single valid image) and an unreadable/non-image fixture file under `crates/pack-loader/tests/fixtures/static_image/` and `crates/pack-loader/tests/fixtures/invalid/` (spec.md US2 Independent Test)
- [X] T020 [US2] Acceptance scenario tests — static single-image load with no anchors, unreadable-file error (spec.md US2 scenarios 1–2) in `crates/pack-loader/tests/load_pack.rs` (depends on T019)

### Implementation for User Story 2

- [X] T021 [US2] Implement `load_pack`'s single-file branch — header-validate readability, build a one-image no-anchor `LoadedPack` via spec 1's static/degenerate `WallpaperPack` case — in `crates/pack-loader/src/load.rs` (FR-004; depends on T007, T009, T010). Modeled the "no anchor at all" case as a single `Clock(00:00:00)` anchor, since spec 1's `ValidatedPack` has no anchor-less representation and never actually consults a single-image pack's anchor value (`is_static`/`query` short-circuit before reading it) — documented inline in `load.rs`.

**Checkpoint**: User Stories 1 and 2 (both P1) independently functional — MVP complete.

---

## Phase 5: User Story 3 - Configure Scaling & Fit Behavior (Priority: P2)

**Goal**: A pack-level default scaling mode plus fallback color, overridable per image.

**Independent Test**: Author a manifest with a pack-level scaling default and one image that
overrides it, load the pack, and verify the loaded pack reports the pack-level default for
unoverridden images and the per-image override where declared.

### Tests for User Story 3

- [X] T022 [P] [US3] Create the `scaling_overrides` fixture (pack-level default + one per-image override) in `crates/pack-loader/tests/fixtures/scaling_overrides/`
- [X] T023 [US3] Acceptance scenario tests — pack-level default applied to unoverridden images, per-image override honored, invalid scaling mode/malformed color error (spec.md US3 scenarios 1–3) in `crates/pack-loader/tests/load_pack.rs` (depends on T022)

### Implementation for User Story 3

- [X] T024 [US3] Implement `ScalingMode` and `Color` parse-time validation (reject invalid mode name / malformed color value) in `crates/pack-loader/src/manifest.rs` (FR-005, FR-006; extends T006)
- [X] T025 [US3] Resolve per-image scaling (per-image override falls back to pack default) into `LoadedPack.image_scaling` in `crates/pack-loader/src/load.rs` (FR-005; depends on T016, T024)

**Checkpoint**: User Stories 1–3 independently functional.

---

## Phase 6: User Story 4 - Known Packs Persist Across Daemon Restarts (Priority: P3)

**Goal**: A registered pack's source location survives a daemon restart via `cosmic-config`,
with unreachable packs marked unavailable (not dropped) and explicit removal deleting an
entry outright.

**Independent Test**: Load a pack, persist its registration, restart the loading component
fresh, and verify the previously-loaded pack's location is still known without re-scanning.

**Un-deferred 2026-08-13, same session**: the initial pass deferred this phase (see git
history), judging the `cosmic-config` git dependency too risky/heavy to pull in for a P3
story. That was re-tested directly — with `default-features = false, features =
["macro"]` (dropping the default `subscription`/`iced_futures` pull-in this crate never
needed), it resolves and builds in ~15s, a small dependency footprint (`ron`, `notify`,
`atomicwrites`, `xdg`, `tracing`, `dirs`). Since spec 4 (CLI) also needs `cosmic-config`
for its own persisted config (location, output assignment) and can't reach its own MVP
without it either, the earlier deferral was reversed rather than carried forward as debt.

### Tests for User Story 4

- [X] T026 [P] [US4] `tempfile`-backed registry round-trip tests — register, then reopen a fresh `Registry` and confirm `known_packs()` still reports it (spec.md US4 scenario 1, research.md R6) in `crates/pack-loader/tests/registry.rs` — landed as `#[cfg(test)]` unit tests inside `src/registry.rs` itself (using the doc-hidden `Registry::open_at` test hook) rather than a separate `tests/registry.rs` file; equivalent coverage, noted as a minor structural deviation from the task's literal file path
- [X] T027 [P] [US4] Unavailable-marking and explicit-removal tests — a pack whose source vanished is marked `Unavailable` without affecting other known packs on `reload_all`, and explicit `remove` deletes the entry outright (spec.md US4 scenarios 2–3, FR-011 vs. FR-012) in `crates/pack-loader/tests/registry.rs` — same note as T026

### Implementation for User Story 4

- [X] T028 [US4] Create `PackRegistryEntry` and `RegistryStatus` types in `crates/pack-loader/src/registry.rs` (data-model.md PackRegistryEntry, FR-010, FR-011)
- [X] T029 [US4] Implement `Registry` backed by `cosmic-config`'s `CosmicConfigEntry` pattern — `Registry::open`, `register`, `known_packs` — in `crates/pack-loader/src/registry.rs` (FR-009, FR-010, research.md R4; depends on T028)
- [X] T030 [US4] Implement `Registry::reload_all` — attempt every known pack independently via `load_pack`, marking any whose source is unreachable `Unavailable` without failing the others — in `crates/pack-loader/src/registry.rs` (FR-011; depends on T029, T017, T021)
- [X] T031 [US4] Implement `Registry::remove` — delete a registry entry outright, distinct from `reload_all`'s automatic `Unavailable` marking — in `crates/pack-loader/src/registry.rs` (FR-012; depends on T029)
- [X] T032 [US4] Add `Registry` and `PackRegistryEntry` re-exports to `crates/pack-loader/src/lib.rs` (depends on T029–T031; extends T011)

**Checkpoint**: All four user stories independently functional and complete; full
acceptance suite green.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Close out the spec's success criteria and hand off a stable contract to specs 3–4.

- [X] T033 [P] Verify strong line coverage on `src/manifest.rs`, `src/load.rs`, `src/path_safety.rs`, `src/image_check.rs`, `src/registry.rs` via `cargo llvm-cov` (SC-002); add tests to close any gap, especially remaining FR-006/FR-006a rejection cases not yet covered (unreadable image, path-traversal `..`/absolute-path/symlink variants, non-UTF-8 names) — 95.66% aggregate across all 7 source files (error.rs 100%, pack_source.rs 100%, manifest.rs 98.77%, registry.rs 92.04%, load.rs 94.81%, image_check.rs 97.50%, path_safety.rs 84.75%)
- [X] T034 [P] Add rustdoc comments to every public item matching contracts/pack-loader-api.md — verified via `RUSTFLAGS="-W missing_docs" cargo doc`, zero warnings
- [X] T035 [P] Add `crates/pack-loader/README.md` summarizing scope and explicit non-scope (no output assignment or rendering — spec 3; no network-sourced packs — PRD NG2), plus a Registry section covering FR-010-012 and the `cosmic-config` git-dependency note
- [X] T036 [P] Author the published, source-independent manifest schema documentation (SC-005) in `docs/pack-manifest-schema.md`, based on contracts/pack-loader-api.md's schema example
- [X] T037 Run quickstart.md end-to-end (build, `cargo test`, `cargo llvm-cov`, manual smoke snippet) and fix any drift between the doc and the actual API — no drift found this time; the smoke-check snippet (adapted to a scratch directory) is now a real passing doctest in `src/lib.rs`

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — start immediately.
- **Foundational (Phase 2)**: Depends on Setup. Blocks all user stories.
- **User Story 1 (Phase 3)**: Depends only on Foundational.
- **User Story 2 (Phase 4)**: Depends only on Foundational — independent of Phase 3, though both phases touch `src/load.rs` (T016–T018 vs. T021), so treat that file as sequential across phases even though the stories are logically independent.
- **User Story 3 (Phase 5)**: Depends on Foundational **and** on Phase 3's directory-loading path (T016) to attach resolved scaling to — implement after US1.
- **User Story 4 (Phase 6)**: Depends on Foundational **and** on both `load_pack` branches (T017 US1, T021 US2) since `reload_all` calls `load_pack` for every known pack regardless of its kind — implement after US1 and US2.
- **Polish (Phase 7)**: Depends on all four user stories being complete.

### Parallel Opportunities

- T003 and T004 (Setup) — different files.
- T006, T007, T008, and T009 (Foundational) — different files, no cross-dependency beyond T005.
- T012 and T013 (US1 fixtures) — different directories.
- T019 (US2 fixtures) — independent of US1's fixture tasks.
- T022 (US3 fixture) — independent of US1/US2 fixture tasks.
- T026 and T027 (US4 registry tests) — same file but logically independent scenarios; treat as sequential in practice since both edit `tests/registry.rs`.
- T033, T034, T035, and T036 (Polish) — different files.
- Once Foundational (Phase 2) is done, Phase 3 and Phase 4 implementation work can proceed in parallel if staffed by different people, provided `src/load.rs` edits are coordinated (both phases extend it) — Phases 5 and 6 still wait for the relevant earlier phases to land.

---

## Parallel Example: Foundational Phase

```bash
# After T005 (error.rs) lands, launch together:
Task: "Create PackManifest, ManifestImage, ScalingMode, and Color shapes in crates/pack-loader/src/manifest.rs"
Task: "Create the PackSource tagged union in crates/pack-loader/src/pack_source.rs"
Task: "Implement the path containment check in crates/pack-loader/src/path_safety.rs"
Task: "Implement header-only image readability validation in crates/pack-loader/src/image_check.rs"
```

## Parallel Example: User Story 1 Fixtures

```bash
Task: "Create the valid_pack fixture directory in crates/pack-loader/tests/fixtures/valid_pack/"
Task: "Create invalid/ fixture subdirectories in crates/pack-loader/tests/fixtures/invalid/"
```

---

## Implementation Strategy

### MVP First (User Stories 1 and 2)

1. Phase 1: Setup
2. Phase 2: Foundational (blocks everything)
3. Phase 3: User Story 1 — directory/manifest loading
4. Phase 4: User Story 2 — zero-config static loading
5. **STOP and VALIDATE**: `cargo test --test load_pack` green; both P1 stories satisfy their spec.md acceptance scenarios
6. This alone is a demonstrable MVP: any pack a user points the daemon at — multi-image or single-file — loads correctly

### Incremental Delivery

1. Setup + Foundational → foundation ready
2. Add User Story 1 → validate independently
3. Add User Story 2 → validate independently → MVP (both P1 stories done)
4. Add User Story 3 → validate independently (visual correctness across mixed aspect ratios)
5. Add User Story 4 → validate independently (registry survives a restart)
6. Polish → coverage, docs, published schema doc, quickstart parity

---

## Notes

- [P] tasks touch different files with no unmet dependency.
- Tests are written before their corresponding implementation tasks within each phase, matching spec 1's precedent even though this spec's constitution gate doesn't mandate test-first as strictly as Principle V does for solar/time logic — SC-002's "100%" claim still needs the coverage.
- `src/load.rs` is shared across US1, US2, and US3 (and called by US4's `reload_all`) — coordinate edits there even when tasks are nominally story-scoped.
- A manifest is untrusted input (FR-006a) — every US1/US3 implementation task must keep failing closed (reject, don't guess) as the default posture, per constitution Principle VIII.
- Commit after each task or logical group.
