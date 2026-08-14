---

description: "Task list template for feature implementation"
---

# Tasks: GUI Usability Improvements

**Input**: Design documents from `/specs/008-gui-usability-improvements/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md,
contracts/gui-usability-improvements.md, quickstart.md

**Tests**: Included for every pure-logic behavior (message dispatch, state transitions, name/
thumbnail resolution, disclosure text). Dialogs, tooltips, dropdowns, and scrolling are
rendering/interaction behavior — not practically unit-testable outside a real compositor — and
are covered instead by quickstart.md's manual smoke checks (Polish phase), the same split this
project's GUI work already established in spec 7.

**Organization**: Tasks are grouped by user story (spec.md), in **priority order** (P1, P1, P1,
P2, P2, P2) rather than spec.md's narrative order (US1–US6) — spec.md's own Assumptions note that
US5 was added in a follow-up round at P1 (same priority as US1/US2, since it's as core a gap as
add/remove) and that US4/US5 are complementary, with US5's implementation satisfying US4 by
construction. This list implements the three P1 stories consecutively for a real MVP checkpoint,
same precedent as spec 007's own tasks.md reordering.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: Which user story this task belongs to (US1–US6)
- File paths are relative to the repository root

## Path Conventions

No new workspace members. Every task touches one already-shipped crate: `wallpaper-settings`
(the GUI, most of this spec's work), `wallpaper-ipc` (one relocated constant), or `wallpaperctl`
(one cosmetic wording fix).

---

## Phase 1: Setup

**Purpose**: Baseline sanity check only — research.md confirms this spec adds zero new Cargo
dependencies and zero new workspace members, so there is no scaffolding to do.

- [ ] T001 Confirm `cargo build --workspace && cargo test --workspace` passes cleanly on `main`
      before starting (repo root `Cargo.toml`)

**Checkpoint**: Clean baseline confirmed; no new crates or dependencies needed for this spec.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: `resolve_pack_name` is the one piece of shared infrastructure two different user
stories (US1's Packs page, US5's Assignment dropdowns) both depend on — implemented once here
rather than twice. (`resolve_thumbnail_path`, US6's own need, is added later in the same file by
US6's own phase — it isn't shared by any other story, so it doesn't block them.)

**⚠️ CRITICAL**: US1's name display (T006) and all of US5 (T021–T024) depend on this phase.

- [ ] T002 Add `crates/wallpaper-settings/src/pack_display.rs` implementing `resolve_pack_name(
      source: &PackSource) -> Option<String>` (data-model.md: `pack_loader::load_pack`'s manifest
      `name` for a directory pack, its filename with the extension stripped via
      `Path::file_stem()` for a static-file pack, `None` if `load_pack` fails) with unit tests
      covering all three branches; register the module in
      `crates/wallpaper-settings/src/main.rs` (`mod pack_display;`)

**Checkpoint**: `resolve_pack_name` implemented and tested — both US1 and US5 can now consume it.

---

## Phase 3: User Story 1 - Manage packs without leaving the GUI (Priority: P1) 🎯 MVP

**Goal**: Add and remove packs entirely from the Packs page, with no terminal command required
(FR-001–FR-004, FR-012).

**Independent Test**: Open the settings application with no packs registered, add a pack through
the Packs page, confirm it appears (by name, not path) and is usable, then remove it through the
same page and confirm it disappears.

### Tests for User Story 1

- [ ] T003 [US1] Unit test: `AddResult(Ok(path))` calls `pack_loader::Registry::register`, is a
      no-op (not a duplicate) when the path is already registered, and refreshes `rows` with
      `pack_display::resolve_pack_name`-derived names (spec.md Acceptance Scenarios 1, 4) in
      `crates/wallpaper-settings/src/pages/packs.rs`
- [ ] T004 [US1] Unit test: `AddResult(Err(reason))` leaves `rows` unchanged and sets `add_error`
      to the specific failure reason — no partial registration (spec.md Acceptance Scenario 3) in
      `crates/wallpaper-settings/src/pages/packs.rs`
- [ ] T005 [US1] Unit test: the removal state machine — `RemoveRequested(source)` sets
      `pending_removal`; `RemoveConfirmed` calls `pack_loader::Registry::remove` and clears it;
      `RemoveCancelled` clears it with no registry change (data-model.md state transitions;
      spec.md Acceptance Scenario 2) in `crates/wallpaper-settings/src/pages/packs.rs`

### Implementation for User Story 1

- [ ] T006 [US1] Replace `rows_from_registry`'s path-based `PackRow.name` with
      `pack_display::resolve_pack_name(&entry.source).unwrap_or_else(|| "(unnamed
      pack)".to_string())` (FR-012) in `crates/wallpaper-settings/src/pages/packs.rs` (depends on
      T002)
- [ ] T007 [US1] Add `pending_removal: Option<PackSource>` and `add_error: Option<String>` to
      `packs::State`, and the `AddFolderRequested` / `AddFileRequested` / `AddResult(Result
      <PathBuf, String>)` / `RemoveRequested(PackSource)` / `RemoveConfirmed` / `RemoveCancelled`
      variants to `packs::Message` (data-model.md) in
      `crates/wallpaper-settings/src/pages/packs.rs` (depends on T006; makes T003–T005 compile)
- [ ] T008 [P] [US1] Add "Add pack folder…" and "Add image file…" buttons, an error row bound to
      `add_error`, and a per-row "Remove" button to `packs::view`
      (contracts/gui-usability-improvements.md) in
      `crates/wallpaper-settings/src/pages/packs.rs` (depends on T007)
- [ ] T009 [P] [US1] Wire `AddFolderRequested`/`AddFileRequested` to `cosmic::Task`s running
      `cosmic::dialog::file_chooser::open::Dialog::new().open_folder()` /`.open_file()`, mapping a
      cancelled dialog to a no-op and any other outcome to
      `Message::Packs(packs::Message::AddResult(...))` (research.md R1) in
      `crates/wallpaper-settings/src/app.rs` (depends on T007)
- [ ] T010 [US1] Handle `AddResult` / `RemoveRequested` / `RemoveConfirmed` / `RemoveCancelled` in
      `App::update`, calling `pack_loader::Registry::register`/`remove` and refreshing
      `packs::State` (contracts/gui-usability-improvements.md) in
      `crates/wallpaper-settings/src/app.rs` (depends on T009)
- [ ] T011 [US1] Override `cosmic::Application::dialog(&self)` to render the removal confirmation
      dialog (`widget::dialog::dialog()`, primary "Remove" / secondary "Cancel") when
      `packs.pending_removal.is_some()`, titled with the pack's `resolve_pack_name` result
      (research.md R3, data-model.md) in `crates/wallpaper-settings/src/app.rs` (depends on T010)
- [ ] T012 [P] [US1] Update the "registration is out of this spec's GUI scope" note in
      `crates/wallpaper-settings/README.md` to reflect that add/remove is now supported
      (contracts/gui-usability-improvements.md) (depends on T011)

**Checkpoint**: Packs can be added and removed entirely from the GUI; the Packs page shows names,
not paths.

---

## Phase 4: User Story 2 - Every control on a page stays reachable (Priority: P1) 🎯 MVP

**Goal**: Every control on every page is visible or reachable by scrolling, at the default window
size and when resized smaller (FR-005, FR-006).

**Independent Test**: Open the application at its default size, navigate to every page, and
confirm every control — especially the Location page's manual-location confirm button — is
visible or reachable by scrolling, with no manual window resizing required.

No automated tests — layout/scroll behavior is rendering-only and covered by quickstart.md's
manual smoke check 2 (Polish phase).

### Implementation for User Story 2

- [ ] T013 [P] [US2] Wrap `packs::view`'s returned column in `widget::scrollable(...)` in
      `crates/wallpaper-settings/src/pages/packs.rs` (research.md R5; depends on T008 — after
      US1's packs.rs view changes land)
- [ ] T014 [P] [US2] Wrap `assignment::view`'s returned column in `widget::scrollable(...)` in
      `crates/wallpaper-settings/src/pages/assignment.rs` (research.md R5)
- [ ] T015 [P] [US2] Wrap `location::view`'s returned column in `widget::scrollable(...)` in
      `crates/wallpaper-settings/src/pages/location.rs` (research.md R5)
- [ ] T016 [P] [US2] Wrap `timeline::view`'s returned column in `widget::scrollable(...)` in
      `crates/wallpaper-settings/src/pages/timeline.rs` (research.md R5)
- [ ] T017 [P] [US2] Wrap `crossfade::view`'s returned column in `widget::scrollable(...)` in
      `crates/wallpaper-settings/src/pages/crossfade.rs` (research.md R5)

**Checkpoint**: Every page's controls are reachable at default size and when the window is
resized smaller (spec.md Acceptance Scenarios 1–3, SC-003).

---

## Phase 5: User Story 5 - Assign packs to displays from the GUI (Priority: P1) 🎯 MVP

**Goal**: Real pack assignment from the Assignment page — a "same pack everywhere" toggle
(default on) plus, when off, an independent per-display dropdown — replacing the "assigns the
first registered pack" placeholder (FR-013–FR-017).

**Independent Test**: Register two or more packs, switch the toggle off, assign a different pack
to each of two connected displays via their dropdowns, confirm each independently, then switch
the toggle back on and confirm a single chosen pack applies to every display.

### Tests for User Story 5

- [ ] T018 [US5] Unit test: `set_same_everywhere_enabled(config, true, default)` clears
      `config.overrides` and sets `config.same_pack_everywhere` to `default` only if it was
      already `None` (FR-014, FR-015, data-model.md) in
      `crates/wallpaper-settings/src/pages/assignment.rs`
- [ ] T019 [US5] Unit test: `set_same_everywhere_enabled(config, false, _)` sets
      `config.same_pack_everywhere = None` and leaves `config.overrides` untouched (data-model.md)
      in `crates/wallpaper-settings/src/pages/assignment.rs`
- [ ] T020 [US5] Unit test: `SameEverywherePackSelected`/`OutputPackSelected` write through the
      existing `apply_assignment` (spec 7) with the same `AssignTarget::SameEverywhere`/`Output`
      shapes `wallpaperctl assign` writes (FR-013, FR-017) in
      `crates/wallpaper-settings/src/pages/assignment.rs`

### Implementation for User Story 5

- [ ] T021 [US5] Add the `set_same_everywhere_enabled` pure helper and the
      `ToggleSameEverywhere(bool)` / `SameEverywherePackSelected(usize)` /
      `OutputPackSelected(String, usize)` variants to `assignment::Message` (data-model.md) in
      `crates/wallpaper-settings/src/pages/assignment.rs` (depends on T014; makes T018–T020
      compile)
- [ ] T022 [US5] Replace `assignment::view`'s "assigns the first registered pack" buttons with a
      `widget::toggler` plus `widget::dropdown`(s) — one when the toggle is on, one per
      `known_outputs` entry when off — option labels via `pack_display::resolve_pack_name`, and
      the FR-016 "register a pack first" message when `available_packs` is empty
      (contracts/gui-usability-improvements.md, research.md R6) in
      `crates/wallpaper-settings/src/pages/assignment.rs` (depends on T021, T002)
- [ ] T023 [US5] Wire `ToggleSameEverywhere`/`SameEverywherePackSelected`/`OutputPackSelected` in
      `App::update`, persisting via the same `RendererConfig::save` pattern the existing
      Assignment messages already use, in `crates/wallpaper-settings/src/app.rs` (depends on
      T022)
- [ ] T024 [P] [US5] Update `crates/wallpaper-settings/README.md`'s "assigns the first registered
      pack" simplification note to describe the real toggle/dropdown assignment
      (contracts/gui-usability-improvements.md) (depends on T023)

**Checkpoint**: Packs can be assigned to specific displays, or to every display via the toggle,
entirely from the GUI — the toggle's "on" state is unconditional (FR-015).

---

## Phase 6: User Story 3 - Understand IP-geolocation's one external touchpoint before opting in (Priority: P2)

**Goal**: The IP-geolocation disclosure is discoverable by hover or by tap, before that option is
selected, as a properly capitalized sentence (FR-007–FR-009).

**Independent Test**: Open the Location page, hover the IP-geolocation option without selecting
it, and confirm the explanatory text appears, reads as a proper sentence, and disappears
predictably; separately, confirm the same text is reachable via the info icon without hovering.

### Tests for User Story 3

- [ ] T025 [P] [US3] Unit test: `wallpaper_ipc::IP_GEOLOCATION_DISCLOSURE` starts with an
      uppercase letter and ends with terminal punctuation (FR-009) in
      `crates/wallpaper-ipc/src/lib.rs`
- [ ] T026 [P] [US3] Unit test: `location::Message::ToggleIpDisclosure` flips
      `show_ip_disclosure`, independent of `entry.mode` (data-model.md) in
      `crates/wallpaper-settings/src/pages/location.rs`

### Implementation for User Story 3

- [ ] T027 [US3] Move `IP_GEOLOCATION_DISCLOSURE` into `crates/wallpaper-ipc/src/lib.rs` with the
      new sentence-case wording (research.md R4, data-model.md), removing the duplicated copies
      from `crates/wallpaperctl/src/commands/location.rs` and
      `crates/wallpaper-settings/src/pages/location.rs` (depends on T025)
- [ ] T028 [P] [US3] Update `crates/wallpaperctl/src/commands/location.rs`'s `location ip`
      message to import the relocated constant and avoid the "IP-geolocation enabled
      (IP-geolocation…)" repetition (data-model.md) (depends on T027)
- [ ] T029 [P] [US3] Add `show_ip_disclosure: bool` to `location::State` and the
      `ToggleIpDisclosure` message to `location::Message`, importing the relocated constant, in
      `crates/wallpaper-settings/src/pages/location.rs` (depends on T027, T015, T026)
- [ ] T030 [US3] Wrap the IP-geolocation radio row in `widget::tooltip(...)` (FR-007) and add a
      persistent `dialog-information-symbolic` info-icon button that toggles
      `show_ip_disclosure` and reveals the same text inline (FR-008), gated on the option not
      being selected yet rather than on `entry.mode == IpGeolocation` (research.md R4) in
      `crates/wallpaper-settings/src/pages/location.rs` (depends on T029)

**Checkpoint**: The disclosure is discoverable via hover or tap, before opting into
IP-geolocation, in properly-cased sentence form, from a single source of truth.

---

## Phase 7: User Story 4 - Recognize assignments by pack name (Priority: P2)

**Goal**: The Assignment page shows pack names, not file paths, for every output and the
"same pack everywhere" toggle (FR-010, FR-011).

**Independent Test**: Assign a registered pack (with a human-readable name) to an output and
confirm the Assignment page displays that name, not its file location.

**Already satisfied by User Story 5's construction** (plan.md finding 3, contracts/
gui-usability-improvements.md): `widget::dropdown`'s option labels and its selected-value display
are both `pack_display::resolve_pack_name` results (T022) — there is no remaining
`source.path().display()`/`current.path().display()` anywhere in `assignment::view` for this
story to separately fix. This phase is a verification step, not new implementation.

- [ ] T031 [US4] Add a doc comment on `assignment::view` (or immediately above the dropdown
      construction) noting explicitly that FR-010/FR-011 are satisfied by US5's dropdown labels,
      so a future reader doesn't look for separate US4 code that doesn't exist; confirm via
      `grep -n 'path().display()' crates/wallpaper-settings/src/pages/assignment.rs` that no
      match remains, in `crates/wallpaper-settings/src/pages/assignment.rs` (depends on T022)

**Checkpoint**: Confirmed — the Assignment page shows names, not paths, with no dedicated code
path beyond US5's own.

---

## Phase 8: User Story 6 - See a thumbnail preview of each pack (Priority: P2)

**Goal**: The Packs page shows a representative thumbnail for each pack — the solar-noon-anchored
image if one exists, otherwise the first image in manifest order (FR-018–FR-020).

**Independent Test**: Register a pack with a solar-noon anchor and confirm its thumbnail is that
image; register one without and confirm its first image is shown instead.

### Tests for User Story 6

- [ ] T032 [US6] Unit test: `resolve_thumbnail_path` returns the solar-noon-anchored image's
      resolved path when the pack has one (spec.md Acceptance Scenario 1, FR-019) in
      `crates/wallpaper-settings/src/pack_display.rs`
- [ ] T033 [US6] Unit test: `resolve_thumbnail_path` falls back to the first image in manifest
      order when no solar-noon anchor exists, including the static single-image case (spec.md
      Acceptance Scenario 2, FR-019) in `crates/wallpaper-settings/src/pack_display.rs`
- [ ] T034 [US6] Unit test: `resolve_thumbnail_path` returns `None` when `load_pack` fails
      (spec.md Acceptance Scenario 3, FR-020) in `crates/wallpaper-settings/src/pack_display.rs`

### Implementation for User Story 6

- [ ] T035 [US6] Add `resolve_thumbnail_path(source: &PackSource) -> Option<PathBuf>` to
      `crates/wallpaper-settings/src/pack_display.rs` (research.md R7) (depends on T032–T034,
      T002)
- [ ] T036 [US6] Add `thumbnail: Option<PathBuf>` to `PackRow`, populated via
      `pack_display::resolve_thumbnail_path` in `rows_from_registry` (FR-018, FR-019) in
      `crates/wallpaper-settings/src/pages/packs.rs` (depends on T035, T013)
- [ ] T037 [US6] Render each row's thumbnail via `widget::image(path)` in `packs::view`, falling
      back to the existing "(no preview available)" placeholder posture when `thumbnail` is
      `None` (FR-020) in `crates/wallpaper-settings/src/pages/packs.rs` (depends on T036)

**Checkpoint**: All six user stories independently functional — Packs add/remove, every page
scrollable, real GUI-driven assignment, IP-geolocation disclosure hover/tap, Assignment shows
names, Packs shows thumbnails.

---

## Phase 9: Polish & Cross-Cutting Concerns

**Purpose**: Close out the spec's success criteria and hand off a stable, documented feature set.

- [ ] T038 [P] Run `cargo clippy --workspace -- -D warnings` across every touched crate and fix
      any new lint findings
- [ ] T039 [P] Run `cargo test --workspace`, confirming both this spec's new tests (T003–T005,
      T018–T020, T025–T026, T032–T034) and every pre-existing test still pass unchanged
- [ ] T040 Execute quickstart.md's six manual smoke checks against a real COSMIC session
      (add/remove, scrolling, GUI assignment, hover/tap disclosure, Assignment names, thumbnails)
      and record the results in `crates/wallpaper-settings/README.md`, matching this project's
      established "confirmed live" documentation posture (spec 7 quickstart.md precedent)
      (depends on T038, T039)

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — start immediately.
- **Foundational (Phase 2)**: Depends on Setup. Blocks US1's name display (T006) and all of US5
  (T021–T024) — but **not** US2, US3, US4, or US6 directly.
- **User Story 1 (Phase 3)**: Depends on Foundational (T002, for T006).
- **User Story 2 (Phase 4)**: Depends only on Setup, plus T008 specifically for its `packs.rs`
  task (T013) since both touch the same file — otherwise independent of Foundational/US1/US3/
  US5/US6.
- **User Story 5 (Phase 5)**: Depends on Foundational (T002) and T014 (US2's `assignment.rs`
  scrollable wrap, same file).
- **User Story 3 (Phase 6)**: Depends only on Setup, plus T015 specifically for its `location.rs`
  tasks (T029, T030) since both touch the same file — otherwise independent of Foundational/
  US1/US5/US6.
- **User Story 4 (Phase 7)**: Depends entirely on User Story 5 (T022) — no independent
  implementation of its own (plan.md finding 3).
- **User Story 6 (Phase 8)**: Depends on Foundational (T002, same file as `resolve_thumbnail_path`
  lands in) and T013 (US2's `packs.rs` scrollable wrap, same file).
- **Polish (Phase 9)**: Depends on all six user stories being complete.

### Parallel Opportunities

- T008, T009 (US1 implementation) — different files (`packs.rs`, `app.rs`), both depend only on
  T007.
- T012 (US1) — different file from T011, can proceed once T011 lands.
- T013–T017 (US2) — five different files, fully parallel with each other (subject to each one's
  own same-file dependency on an earlier story's task, noted above).
- T024 (US5) — different file from T023, can proceed once T023 lands.
- T025, T026 (US3 tests) — different files (`wallpaper-ipc`, `wallpaper-settings`).
- T028, T029 (US3 implementation) — different files (`wallpaperctl`, `wallpaper-settings`), both
  depend only on T027.
- T038, T039 (Polish) — independent commands, no shared file.

### Sequential-in-Practice Files

`crates/wallpaper-settings/src/pages/packs.rs` is touched by Foundational indirectly (via T002's
new `pack_display.rs`), then US1 (T006–T008), then US2 (T013), then US6 (T036–T037) —
coordinate edits in that order. `crates/wallpaper-settings/src/pages/assignment.rs` is touched by
US2 (T014) then US5 (T021–T022) then US4 (T031, doc-comment only). `crates/wallpaper-settings/
src/pages/location.rs` is touched by US2 (T015) then US3 (T029–T030).
`crates/wallpaper-settings/src/pack_display.rs` is touched by Foundational (T002) then US6
(T035). `crates/wallpaper-settings/src/app.rs` is touched by US1 (T009–T011) then US5 (T023).

---

## Parallel Example: User Story 1 Implementation

```bash
# Once T007 (packs.rs State/Message additions) lands, launch together:
Task: "Add file-chooser buttons and error display to packs::view (packs.rs)"
Task: "Wire file-chooser Tasks in app.rs"
```

## Parallel Example: User Story 2

```bash
# Once each story's own same-file prerequisite lands, launch together:
Task: "Wrap packs::view in scrollable (after US1's T008)"
Task: "Wrap assignment::view in scrollable"
Task: "Wrap location::view in scrollable"
Task: "Wrap timeline::view in scrollable"
Task: "Wrap crossfade::view in scrollable"
```

---

## Implementation Strategy

### MVP First (User Stories 1, 2, and 5)

1. Phase 1: Setup
2. Phase 2: Foundational (blocks US1's name display and all of US5)
3. Phase 3: User Story 1 — add/remove packs from the GUI
4. Phase 4: User Story 2 — every control reachable
5. Phase 5: User Story 5 — real pack assignment (toggle + dropdowns)
6. **STOP and VALIDATE**: `cargo test --workspace` green; quickstart.md's manual smoke checks 1,
   2, and 5 all pass
7. This is the spec's MVP per spec.md's own P1 framing: pack management, layout reachability,
   and real assignment are the three capability-blocking fixes; US3/US4/US6 are clarity/
   disclosure/visual improvements, not blockers — and US4 needs no separate work once US5 lands

### Incremental Delivery

1. Setup + Foundational
2. User Story 1 → validate independently
3. User Story 2 → validate independently
4. User Story 5 → validate independently → **MVP** (all three P1 stories done)
5. User Story 3 → validate independently (needs only Setup + US2's `location.rs` scrollable wrap)
6. User Story 4 → validate as a byproduct of US5 (no independent implementation)
7. User Story 6 → validate independently (needs Foundational + US2's `packs.rs` scrollable wrap)
8. Polish → lint, full suite, quickstart parity

---

## Notes

- [P] tasks touch different files, or independent scenarios within the same file, with no unmet
  dependency.
- This spec adds **zero new Cargo dependencies** (research.md) — every widget used
  (`file_chooser`, `dialog`, `tooltip`, `scrollable`, `toggler`, `dropdown`, `image`) already
  ships in the `libcosmic` dependency `wallpaper-settings` already pins.
- `IP_GEOLOCATION_DISCLOSURE`'s relocation (T027) is a duplication fix, not new functionality —
  both `wallpaperctl`'s and `wallpaper-settings`' existing behavior around it must be unchanged in
  meaning after the move, only in wording/casing (FR-009) and location.
- User Story 5's toggle-on behavior (T021, `set_same_everywhere_enabled`) is a **deliberate,
  user-confirmed divergence** from `wallpaperctl assign --same-everywhere`, which does not clear
  `overrides` — do not "fix" this into matching the CLI without checking research.md R6 first.
- `unwrap()`/`expect()` outside `#[cfg(test)]` remains prohibited (constitution Principle VIII,
  same CI lint gate as every other crate in this workspace) — a cancelled file-chooser dialog and
  a `load_pack` failure during name/thumbnail resolution are both `Result`/`Option`, never assumed
  to succeed.
- Commit after each task or logical group.
