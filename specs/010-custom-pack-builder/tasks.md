---

description: "Task list template for feature implementation"
---

# Tasks: Custom Pack Builder

**Input**: Design documents from `/specs/010-custom-pack-builder/`

**Prerequisites**: [plan.md](./plan.md) (required), [spec.md](./spec.md) (required for user stories), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/](./contracts/)

**Tests**: Not explicitly requested as a separate TDD phase. This codebase's own convention
(every existing module, e.g. `pages/assignment.rs`, `pack-loader/src/manifest.rs`) colocates
unit tests with the function they cover in the same `#[cfg(test)] mod tests` block, written
alongside the implementation rather than front-loaded red/green — task descriptions below follow
that pattern, folding test coverage into each implementation task instead of a separate phase.

**Organization**: Tasks are grouped by user story (spec.md's US1/US2/US3) to enable independent
implementation and testing of each.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1, US2, US3)
- Every task names its exact file path(s)

## Path Conventions

Existing Cargo workspace (plan.md's Project Structure) — no new crate. All paths are relative to
the repository root.

---

## Phase 1: Setup

**Purpose**: Give the new code somewhere to live before any logic is written.

- [ ] T001 [P] Add direct (non-dev) dependencies `image` (same version/feature set `pack-loader`
      already pins: `jpeg, png, gif, webp, bmp, tiff`) and `dirs` (already resolved at v6.0.0 via
      `cosmic-config`, so no new crate enters the build) to `crates/wallpaper-settings/Cargo.toml`
      (research.md R2, R8)
- [ ] T002 [P] Create `crates/wallpaper-settings/src/pages/pack_builder.rs` with a module-level
      doc comment describing its scope (spec.md, data-model.md §2), and register
      `pub mod pack_builder;` in `crates/wallpaper-settings/src/pages/mod.rs`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The write-side manifest API and the wizard's core data/pure-logic shapes every
story builds on.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [ ] T003 Add `ManifestDraft` and `ManifestDraftImage` structs (data-model.md §1) to
      `crates/pack-loader/src/manifest.rs`, next to the existing `PackManifest`/`ManifestImage`
- [ ] T004 Implement `pub fn format_anchor(anchor: &schedule_engine::TimeAnchor) -> String` in
      `crates/pack-loader/src/manifest.rs` as the exact inverse of the existing private
      `parse_anchor`; add unit tests asserting the round-trip property
      `parse_anchor(&format_anchor(&a)) == Ok(a)` for a clock anchor, a bare solar event, and a
      solar event with a positive and a negative offset (contracts/pack-loader-manifest-writer.md)
- [ ] T005 Implement `pub fn render(draft: &ManifestDraft) -> String` in
      `crates/pack-loader/src/manifest.rs` via a local `#[derive(Serialize)]` raw shape and the
      `toml` crate (never hand-built string interpolation — contracts/pack-loader-manifest-writer.md);
      omit the `author` key when `draft.author` is `None`; add unit tests where an author name
      containing `"` and non-ASCII text, fed through `render` then back through the existing
      `parse`, comes back byte-identical (spec.md Edge Cases)
- [ ] T006 Export `ManifestDraft`, `ManifestDraftImage`, `render`, and `format_anchor` from
      `crates/pack-loader/src/lib.rs`
- [ ] T007 Implement folder image scanning in `crates/wallpaper-settings/src/pages/pack_builder.rs`:
      list a directory's entries and keep only those passing a header-only
      `image::ImageReader::open(path)?.with_guessed_format()?.into_dimensions()` check
      (research.md R2), silently skipping non-image files and flagging unreadable ones; returns
      `Vec<ImageRow>` with `solar`/`time` both `None`
- [ ] T008 Define `AssignmentMode`, `SolarAssignment`, `ImageRow`, `State`, `PendingCollision`,
      and `GeneratedPlacement` in `crates/wallpaper-settings/src/pages/pack_builder.rs`
      (data-model.md §2)
- [ ] T009 Implement `fn combine_offset(hours: i32, minutes: u32) -> chrono::TimeDelta` in
      `crates/wallpaper-settings/src/pages/pack_builder.rs`, clamping `minutes` to `0` whenever
      `hours.abs() == 12` (research.md R6, the ±12h clarification cap); add unit tests covering
      the clamp boundary and an ordinary in-range value
- [ ] T010 Implement `fn effective_author(input: &str) -> String` in
      `crates/wallpaper-settings/src/pages/pack_builder.rs` — blank or whitespace-only input
      returns `"Artist Unknown"` (FR-010); add unit tests for both the blank and non-blank cases
- [ ] T011 Implement `fn all_assigned(rows: &[ImageRow], mode: AssignmentMode) -> bool` in
      `crates/wallpaper-settings/src/pages/pack_builder.rs` (research.md R5, FR-009); add unit
      tests for both modes with all-assigned, none-assigned, and partially-assigned rows
- [ ] T012 Implement
      `fn build_draft(rows: &[ImageRow], mode: AssignmentMode, folder_name: &str, author: &str) -> ManifestDraft`
      in `crates/wallpaper-settings/src/pages/pack_builder.rs`, applying the R10 defaults
      (`name` = `folder_name`, `default_scaling = ScalingMode::Fill`,
      `fallback_color = #000000`) and calling `effective_author`; add a unit test asserting the
      produced draft's shape for a small fixed row set
- [ ] T013 Add `pack_builder: Option<pages::pack_builder::State>` to `App` in
      `crates/wallpaper-settings/src/app.rs`; change the existing `AddResult` handling so that a
      directory failing specifically with `pack_loader::ManifestError::ManifestNotFound` opens
      the wizard (via T007's scan, `mode: None`) instead of setting `packs::State.add_error`
      — every other outcome (success, or any other error) is unchanged (research.md R1, R9,
      FR-001, FR-002)
- [ ] T014 Implement the mode-choice screen (`State.mode == None`) — two buttons, "By solar
      period" and "By specific time" — and Cancel handling (clears `App.pack_builder`, no
      filesystem change) in `crates/wallpaper-settings/src/pages/pack_builder.rs` (FR-004,
      FR-019)

**Checkpoint**: The wizard opens on a manifest-free folder and shows the mode-choice screen;
`pack-loader` can render a valid `manifest.toml` from a `ManifestDraft`. Neither story's
configuration screen exists yet.

---

## Phase 3: User Story 1 - Build a pack by solar period (Priority: P1) 🎯 MVP

**Goal**: A user assigns every image in a folder to a solar event (with an optional clamped
offset) and generates a real, loadable pack.

**Independent Test**: Point the wizard at a folder with several images and no `manifest.toml`,
choose "By solar period," assign each image a distinct event, click Generate, and confirm
`pack_loader::load_pack` reads the result back with every image scheduled exactly as configured
(spec.md User Story 1's own Independent Test).

### Implementation for User Story 1

- [ ] T015 [US1] Implement the solar-period configuration screen view in
      `crates/wallpaper-settings/src/pages/pack_builder.rs`: one row per scanned image showing
      its thumbnail (`widget::image`), a `widget::dropdown` of the 8 `SolarEventKind` labels
      (`selected: Option<usize>`, no default selection), and two `widget::spin_button`s for
      signed offset hours (`-12..=12`) and minutes (`0..=59`) (FR-005, FR-006, FR-009, User
      Story 1 Acceptance Scenarios 2, 3, 6)
- [ ] T016 [US1] Implement solar-mode conflict detection in
      `crates/wallpaper-settings/src/pages/pack_builder.rs`: a location-independent literal
      `(SolarEventKind, Option<TimeDelta>)` equality check across rows, plus
      `schedule_engine::ValidatedPack::check_solar_duplicate_instant` (via
      `wallpaper_ipc::effective_location`, reused exactly as `pages::location`/`pages::timeline`
      already do) when a location is configured (FR-008, research.md R4); add unit tests for the
      literal-duplicate case and (with a fixed `Location`) the location-aware case
- [ ] T017 [US1] Wire dropdown-selection and offset spin-button messages, and gate the Generate
      button on `all_assigned(&rows, mode) && conflict.is_none()`, in
      `crates/wallpaper-settings/src/app.rs` and
      `crates/wallpaper-settings/src/pages/pack_builder.rs` (User Story 1 Acceptance Scenarios 4,
      5)
- [ ] T018 [US1] Implement the Generate action in
      `crates/wallpaper-settings/src/pages/pack_builder.rs`: `build_draft` →
      `pack_loader::manifest::render` → write `manifest.toml` into the source folder → self-validate
      via `pack_loader::load_pack`, deleting the just-written file and showing a specific error on
      failure rather than treating it as committed (FR-011, FR-012, FR-017,
      contracts/pack-loader-manifest-writer.md's consumer flow); on success, sets
      `State.pending_placement`
- [ ] T019 [US1] Add an integration test in
      `crates/wallpaper-settings/src/pages/pack_builder.rs` (tempfile-backed, mirroring
      `pack-loader`'s own test style): a full solar-mode draft, run through T018's Generate path,
      produces a `manifest.toml` that `pack_loader::load_pack` reads back with every image
      scheduled to its chosen event and offset

**Checkpoint**: User Story 1 is fully functional and independently testable — solar-period packs
can be authored and generated end-to-end.

---

## Phase 4: User Story 2 - Build a pack by specific time of day (Priority: P2)

**Goal**: A user assigns every image an exact clock time instead of a solar event, and generates
a real, loadable pack the same way.

**Independent Test**: Point the wizard at a folder of images, choose "By specific time," assign
a distinct time to each image, click Generate, and confirm `pack_loader::load_pack` reads the
result back with every image scheduled to its exact time (spec.md User Story 2's own Independent
Test).

### Implementation for User Story 2

- [ ] T020 [US2] Implement the specific-time configuration screen view in
      `crates/wallpaper-settings/src/pages/pack_builder.rs`: one row per scanned image showing
      its thumbnail and two `widget::spin_button`s (hour `0..=23`, minute `0..=59`, no default
      selection), with no event dropdown or offset control present (FR-007, FR-009, User Story 2
      Acceptance Scenarios 1, 4)
- [ ] T021 [US2] Extend `detect_conflict`/`all_assigned` in
      `crates/wallpaper-settings/src/pages/pack_builder.rs` to cover `SpecificTime` rows, reusing
      `schedule_engine::WallpaperPack::validate`'s `PackError::DuplicateInstant` for the
      clock-anchor case (FR-008, research.md R4); add unit tests for the duplicate-time and
      distinct-times cases
- [ ] T022 [US2] Wire time spin-button messages, and mode-switch handling that clears every row's
      `solar`/`time` field back to `None` when `State.mode` changes (spec.md Edge Cases), in
      `crates/wallpaper-settings/src/app.rs` and
      `crates/wallpaper-settings/src/pages/pack_builder.rs`
- [ ] T023 [US2] Add an integration test in
      `crates/wallpaper-settings/src/pages/pack_builder.rs`: a full specific-time draft, run
      through the Generate path (T018's routine, mode-agnostic), produces a `manifest.toml` that
      `pack_loader::load_pack` reads back with every image scheduled to its exact time, and two
      identical times block Generate before any file is written (User Story 2 Acceptance
      Scenario 3)

**Checkpoint**: User Stories 1 AND 2 both work independently — the mode choice fully determines
which control set drives the identical Generate path.

---

## Phase 5: User Story 3 - Name the pack's author and choose where it lives (Priority: P3)

**Goal**: The user supplies (or skips) an author name and decides whether the generated pack
moves into the application's standard pack location or stays put — either way it's immediately
usable.

**Independent Test**: With a fully-assigned draft ready to generate (from either US1 or US2's
configuration screen), verify the author prompt's blank/filled behavior and that both the "move"
and "keep in place" choices leave the pack registered and working afterward (spec.md User Story
3's own Independent Test).

### Implementation for User Story 3

- [ ] T024 [US3] Add the author `widget::text_input` to the configuration screen (both modes,
      shared layout) in `crates/wallpaper-settings/src/pages/pack_builder.rs`, labeled to make
      clear that leaving it blank results in "Artist Unknown," and read through
      `effective_author` at Generate time (FR-010, User Story 3 Acceptance Scenarios 1-3)
- [ ] T025 [US3] Implement standard-pack-location resolution
      (`dirs::data_dir().join("cosmic-dynamic-wallpaper").join("packs")`) in
      `crates/wallpaper-settings/src/pages/pack_builder.rs` (research.md R8)
- [ ] T026 [US3] Implement the copy-then-verify-then-delete move routine in
      `crates/wallpaper-settings/src/pages/pack_builder.rs`: recursively copy the generated
      folder to the destination (T025, or a collision-prompt-supplied name), call
      `pack_loader::load_pack` on the copy to confirm it, then delete the source only after that
      succeeds; on any failure, remove a partial destination copy and leave the source completely
      untouched (FR-013, FR-014, FR-017, research.md R8); add a tempfile-backed unit test for the
      failure-leaves-source-untouched case
- [ ] T027 [US3] Implement the placement dialog (move vs. keep) via `cosmic::widget::dialog` and
      an `Application::dialog()` override in `crates/wallpaper-settings/src/app.rs`, shown
      whenever `State.pending_placement.is_some()` (FR-013, contracts/pack-builder-gui-flow.md)
- [ ] T028 [US3] Implement the destination-name-collision prompt
      (`State.pending_collision`: a dialog with a text input pre-filled with the suggested name,
      retrying the move with the typed name on confirm) in
      `crates/wallpaper-settings/src/pages/pack_builder.rs` and
      `crates/wallpaper-settings/src/app.rs` (FR-014a, User Story 3 Acceptance Scenario 6); add a
      tempfile-backed unit test confirming a same-name destination opens this prompt instead of
      overwriting anything
- [ ] T029 [US3] Wire final registration in `crates/wallpaper-settings/src/app.rs`:
      `pack_loader::PackSource::resolve(final_path)` + `Registry::register(...)` — the identical
      call the existing "Add pack folder…" success path already makes — followed by refreshing
      `pages::packs::State` and clearing `App.pack_builder`, on both the move-success and
      keep-in-place branches (FR-015, FR-016, SC-005)

**Checkpoint**: All three user stories are independently functional — the full wizard flow
(mode choice → assignment → author → Generate → placement) works end to end in either mode.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Edge cases and verification that span more than one story.

- [ ] T030 Add a tempfile-backed test in
      `crates/wallpaper-settings/src/pages/pack_builder.rs` confirming Cancel — from either the
      mode-choice or the configuration screen — leaves the source folder byte-for-byte unchanged
      (FR-019, SC-006)
- [ ] T031 [P] Add a test in `crates/wallpaper-settings/src/app.rs` confirming a folder that
      already contains `manifest.toml` registers exactly as it does today and never opens the
      wizard (FR-002, spec.md Edge Cases)
- [ ] T032 Add tests in `crates/wallpaper-settings/src/pages/pack_builder.rs` for FR-018's scan
      failures: zero usable images, and more than `schedule_engine::pack::MAX_ANCHORS` (64)
      images, each producing `State.scan_error` with no rows and no Generate button
- [ ] T033 Run `cargo clippy -p pack-loader -p wallpaper-settings --all-targets -- -D warnings`
      and `cargo test -p pack-loader -p wallpaper-settings`, fixing any `unwrap()`/`expect()`
      outside `#[cfg(test)]` (constitution Principle VIII)
- [ ] T034 [P] Walk through [quickstart.md](./quickstart.md)'s manual validation steps end-to-end
      on a real COSMIC session

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — can start immediately.
- **Foundational (Phase 2)**: Depends on Setup completion — BLOCKS all user stories.
- **User Stories (Phase 3-5)**: All depend on Foundational phase completion.
  - US1 and US2 can proceed in parallel (if staffed) — both consume the same `State`/`ImageRow`
    shapes from Phase 2 but touch disjoint mode-specific view/conflict code.
  - US3 touches the same file (`pages/pack_builder.rs`) as US1/US2's configuration-screen work
    (author field lives on that screen) and needs a generated pack to place, so in practice do it
    after at least one of US1/US2, even though its own acceptance criteria don't name either by
    ID.
- **Polish (Phase 6)**: Depends on all three user stories being complete.

### User Story Dependencies

- **User Story 1 (P1)**: Can start after Foundational (Phase 2) — no dependency on US2/US3.
- **User Story 2 (P2)**: Can start after Foundational (Phase 2) — independently testable; shares
  `detect_conflict`/`all_assigned`'s function signatures with US1 but adds its own match arms,
  not edits to US1's.
- **User Story 3 (P3)**: Can start after Foundational (Phase 2), but its Independent Test needs a
  fully-assigned draft to generate from — exercise it against whichever of US1/US2 is done first.

### Within Each User Story

- Configuration-screen view before message wiring before Generate/placement logic.
- Story complete (its own Checkpoint) before moving to the next priority.

### Parallel Opportunities

- T001/T002 (Setup) can run in parallel — different files.
- Within Foundational, T003-T006 (pack-loader) and T007-T012 (wallpaper-settings pure logic) are
  two independent chains that can proceed in parallel with each other, though each chain is
  internally sequential (same file). T013/T014 depend on T007-T012's types existing.
- Once Foundational is done, US1 (T015-T019) and US2 (T020-T023) can be staffed in parallel — see
  the note above about US3.
- T031 and T034 in Polish can run in parallel with the rest of Phase 6.

---

## Parallel Example: Foundational Phase

```bash
# These two chains touch different files and can proceed at the same time:
Task: "Add ManifestDraft/ManifestDraftImage structs, format_anchor, render, and lib.rs exports
       in crates/pack-loader/src/manifest.rs and crates/pack-loader/src/lib.rs (T003-T006)"
Task: "Implement folder scanning, the core State/ImageRow types, and combine_offset/
       effective_author/all_assigned/build_draft in
       crates/wallpaper-settings/src/pages/pack_builder.rs (T007-T012)"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup.
2. Complete Phase 2: Foundational (blocks everything else).
3. Complete Phase 3: User Story 1 (solar-period packs).
4. **STOP and VALIDATE**: run the automated tests plus quickstart.md's US1 manual steps.
5. This alone is a usable feature — a folder of images becomes a real, loadable, solar-scheduled
   pack via the GUI, author defaulting to "Artist Unknown" and staying in its original folder
   (US3 not yet built means no move option, which is a smaller but still coherent MVP surface).

### Incremental Delivery

1. Setup + Foundational → foundation ready.
2. Add User Story 1 → validate independently → this is the MVP.
3. Add User Story 2 → validate independently (mode switching, specific-time conflicts).
4. Add User Story 3 → validate independently (author naming, move/keep, collision handling).
5. Phase 6 polish (cancel-safety, already-has-manifest edge case, scan-failure messaging, lint).

### Parallel Team Strategy

With two or three developers:

1. Team completes Setup + Foundational together (or splits along the T003-T006 /
   T007-T012 chains noted above).
2. Once Foundational is done:
   - Developer A: User Story 1 (solar period).
   - Developer B: User Story 2 (specific time) — coordinate with A on
     `detect_conflict`/`all_assigned`'s shared match statement to avoid clobbering each other's
     arm.
   - Developer C: starts User Story 3's location/move/dialog plumbing (T025-T029), which doesn't
     depend on which mode screen lands first, then wires T024's author field in once either
     configuration screen exists.
3. Stories complete and integrate at the shared `Generate` action (T018) they all call into.

---

## Notes

- [P] tasks = different files, no dependencies. Most tasks here share one of two files
  (`crates/pack-loader/src/manifest.rs` or `crates/wallpaper-settings/src/pages/pack_builder.rs`)
  and are therefore intentionally left unmarked/sequential — only genuinely disjoint-file tasks
  carry `[P]`.
- Every conflict/validation check (FR-008, FR-018) reuses `schedule_engine::WallpaperPack::validate`
  and `ValidatedPack::check_solar_duplicate_instant` rather than introducing new duplicate-detection
  logic — there is deliberately no task that writes new instant-collision math.
- Commit after each task or logical group.
- Stop at any Checkpoint to validate a story independently before continuing.
