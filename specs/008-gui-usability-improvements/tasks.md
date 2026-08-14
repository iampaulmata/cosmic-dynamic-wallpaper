---

description: "Task list template for feature implementation"
---

# Tasks: GUI Usability Improvements

**Input**: Design documents from `/specs/008-gui-usability-improvements/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md,
contracts/gui-usability-improvements.md, quickstart.md

**Tests**: Included for every pure-logic behavior (message dispatch, state transitions, name
resolution, disclosure text). Dialogs, tooltips, and scrolling are rendering/interaction
behavior — not practically unit-testable outside a real compositor — and are covered instead by
quickstart.md's manual smoke checks (Polish phase), the same split this project's GUI work
already established in spec 7.

**Organization**: Tasks are grouped by user story (spec.md), in **priority order** (P1, P1, P2,
P2) matching spec.md's own narrative order (US1, US2, US3, US4) — no reordering needed here,
unlike spec 007.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: Which user story this task belongs to (US1–US4)
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
stories (US1's Packs page, US4's Assignment page) both depend on — implemented once here rather
than twice.

**⚠️ CRITICAL**: US1's name display (T006) and all of US4 (T024) depend on this phase.

- [ ] T002 Add `crates/wallpaper-settings/src/pack_name.rs` implementing `resolve_pack_name(source:
      &PackSource) -> Option<String>` (data-model.md: `pack_loader::load_pack`'s manifest `name`
      for a directory pack, its filename with the extension stripped via `Path::file_stem()` for a
      static-file pack, `None` if `load_pack` fails) with unit tests covering all three branches;
      register the module in `crates/wallpaper-settings/src/main.rs` (`mod pack_name;`)

**Checkpoint**: `resolve_pack_name` implemented and tested — both US1 and US4 can now consume it.

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
      `resolve_pack_name`-derived names (spec.md Acceptance Scenarios 1, 4) in
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
      `resolve_pack_name(&entry.source).unwrap_or_else(|| "(unnamed pack)".to_string())` (FR-012)
      in `crates/wallpaper-settings/src/pages/packs.rs` (depends on T002)
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
      `crates/wallpaper-settings/src/pages/packs.rs` (research.md R5; depends on T011 — after
      US1's packs.rs changes land)
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

## Phase 5: User Story 3 - Understand IP-geolocation's one external touchpoint before opting in (Priority: P2)

**Goal**: The IP-geolocation disclosure is discoverable by hover or by tap, before that option is
selected, as a properly capitalized sentence (FR-007–FR-009).

**Independent Test**: Open the Location page, hover the IP-geolocation option without selecting
it, and confirm the explanatory text appears, reads as a proper sentence, and disappears
predictably; separately, confirm the same text is reachable via the info icon without hovering.

### Tests for User Story 3

- [ ] T018 [P] [US3] Unit test: `wallpaper_ipc::IP_GEOLOCATION_DISCLOSURE` starts with an
      uppercase letter and ends with terminal punctuation (FR-009) in
      `crates/wallpaper-ipc/src/lib.rs`
- [ ] T019 [P] [US3] Unit test: `location::Message::ToggleIpDisclosure` flips
      `show_ip_disclosure`, independent of `entry.mode` (data-model.md) in
      `crates/wallpaper-settings/src/pages/location.rs`

### Implementation for User Story 3

- [ ] T020 [US3] Move `IP_GEOLOCATION_DISCLOSURE` into `crates/wallpaper-ipc/src/lib.rs` with the
      new sentence-case wording (research.md R4, data-model.md), removing the duplicated copies
      from `crates/wallpaperctl/src/commands/location.rs` and
      `crates/wallpaper-settings/src/pages/location.rs` (depends on T018)
- [ ] T021 [P] [US3] Update `crates/wallpaperctl/src/commands/location.rs`'s `location ip`
      message to import the relocated constant and avoid the "IP-geolocation enabled
      (IP-geolocation…)" repetition (data-model.md) (depends on T020)
- [ ] T022 [P] [US3] Add `show_ip_disclosure: bool` to `location::State` and the
      `ToggleIpDisclosure` message to `location::Message`, importing the relocated constant, in
      `crates/wallpaper-settings/src/pages/location.rs` (depends on T020, T015, T019)
- [ ] T023 [US3] Wrap the IP-geolocation radio row in `widget::tooltip(...)` (FR-007) and add a
      persistent `dialog-information-symbolic` info-icon button that toggles
      `show_ip_disclosure` and reveals the same text inline (FR-008), gated on the option not
      being selected yet rather than on `entry.mode == IpGeolocation` (research.md R4) in
      `crates/wallpaper-settings/src/pages/location.rs` (depends on T022)

**Checkpoint**: The disclosure is discoverable via hover or tap, before opting into
IP-geolocation, in properly-cased sentence form, from a single source of truth.

---

## Phase 6: User Story 4 - Recognize assignments by pack name (Priority: P2)

**Goal**: The Assignment page shows pack names, not file paths, for every output and the
"same pack everywhere" toggle (FR-010, FR-011).

**Independent Test**: Assign a registered pack (with a human-readable name) to an output and
confirm the Assignment page displays that name, not its file location.

`resolve_pack_name`'s correctness is already covered by Foundational's tests (T002); this phase
only wires the existing, tested function into `assignment::view` — see quickstart.md's manual
smoke check 4 for the rendered-output confirmation.

### Implementation for User Story 4

- [ ] T024 [US4] Replace `source.path().display()` / `current.path().display()` in
      `assignment::view` with `resolve_pack_name(source).unwrap_or_else(|| "(unnamed
      pack)".to_string())`, for both per-output rows and the "same pack everywhere" toggle label
      (FR-010, FR-011) in `crates/wallpaper-settings/src/pages/assignment.rs` (depends on T002,
      T014)

**Checkpoint**: All four user stories independently functional — Packs add/remove, every page
scrollable, IP-geolocation disclosure hover/tap, Assignment shows names.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Close out the spec's success criteria and hand off a stable, documented feature set.

- [ ] T025 [P] Run `cargo clippy --workspace -- -D warnings` across every touched crate and fix
      any new lint findings
- [ ] T026 [P] Run `cargo test --workspace`, confirming both this spec's new tests (T003–T005,
      T018–T019) and every pre-existing test still pass unchanged
- [ ] T027 Execute quickstart.md's four manual smoke checks against a real COSMIC session
      (add/remove, scrolling, hover/tap disclosure, Assignment names) and record the results in
      `crates/wallpaper-settings/README.md`, matching this project's established "confirmed live"
      documentation posture (spec 7 quickstart.md precedent) (depends on T025, T026)

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — start immediately.
- **Foundational (Phase 2)**: Depends on Setup. Blocks US1's name display (T006) and all of US4
  (T024) — but **not** US2 or US3, which don't touch `resolve_pack_name` at all.
- **User Story 1 (Phase 3)**: Depends on Foundational (T002, for T006).
- **User Story 2 (Phase 4)**: Depends only on Setup, plus T011 specifically for its `packs.rs`
  task (T013) since both touch the same file — otherwise independent of Foundational/US1/US3/US4.
- **User Story 3 (Phase 5)**: Depends only on Setup, plus T015 specifically for its `location.rs`
  tasks (T022, T023) since both touch the same file — otherwise independent of Foundational/
  US1/US4.
- **User Story 4 (Phase 6)**: Depends on Foundational (T002) and T014 (US2's `assignment.rs`
  scrollable wrap, same file).
- **Polish (Phase 7)**: Depends on all four user stories being complete.

### Parallel Opportunities

- T008, T009 (US1 implementation) — different files (`packs.rs`, `app.rs`), both depend only on
  T007.
- T012 (US1) — different file from T011, can proceed once T011 lands.
- T013–T017 (US2) — five different files, fully parallel with each other (subject to each one's
  own same-file dependency on an earlier story's task, noted above).
- T018, T019 (US3 tests) — different files (`wallpaper-ipc`, `wallpaper-settings`).
- T021, T022 (US3 implementation) — different files (`wallpaperctl`, `wallpaper-settings`), both
  depend only on T020.
- T025, T026 (Polish) — independent commands, no shared file.

### Sequential-in-Practice Files

`crates/wallpaper-settings/src/pages/packs.rs` is touched by Foundational indirectly (via T002's
new `pack_name.rs`), then US1 (T006–T008), then US2 (T013) — coordinate edits in that order.
`crates/wallpaper-settings/src/pages/assignment.rs` is touched by US2 (T014) then US4 (T024).
`crates/wallpaper-settings/src/pages/location.rs` is touched by US2 (T015) then US3
(T022–T023). `crates/wallpaper-settings/src/app.rs` is touched by US1 only (T009–T011) — stable
for every later phase.

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
Task: "Wrap packs::view in scrollable (after US1's T011)"
Task: "Wrap assignment::view in scrollable"
Task: "Wrap location::view in scrollable"
Task: "Wrap timeline::view in scrollable"
Task: "Wrap crossfade::view in scrollable"
```

---

## Implementation Strategy

### MVP First (User Stories 1 and 2)

1. Phase 1: Setup
2. Phase 2: Foundational (blocks US1's name display and US4 only)
3. Phase 3: User Story 1 — add/remove packs from the GUI
4. Phase 4: User Story 2 — every control reachable
5. **STOP and VALIDATE**: `cargo test --workspace` green; quickstart.md's manual smoke checks 1
   and 2 both pass
6. This is the spec's MVP per spec.md's own P1 framing: pack management and layout reachability,
   the two capability-blocking fixes; US3/US4 are clarity/disclosure improvements, not blockers

### Incremental Delivery

1. Setup + Foundational
2. User Story 1 → validate independently
3. User Story 2 → validate independently → **MVP** (both P1 stories done)
4. User Story 3 → validate independently (needs only Setup + US2's `location.rs` scrollable wrap)
5. User Story 4 → validate independently (needs Foundational + US2's `assignment.rs` scrollable
   wrap)
6. Polish → lint, full suite, quickstart parity

---

## Notes

- [P] tasks touch different files, or independent scenarios within the same file, with no unmet
  dependency.
- This spec adds **zero new Cargo dependencies** (research.md) — every widget used
  (`file_chooser`, `dialog`, `tooltip`, `scrollable`) already ships in the `libcosmic` dependency
  `wallpaper-settings` already pins.
- `IP_GEOLOCATION_DISCLOSURE`'s relocation (T020) is a duplication fix, not new functionality —
  both `wallpaperctl`'s and `wallpaper-settings`' existing behavior around it must be unchanged in
  meaning after the move, only in wording/casing (FR-009) and location.
- `unwrap()`/`expect()` outside `#[cfg(test)]` remains prohibited (constitution Principle VIII,
  same CI lint gate as every other crate in this workspace) — a cancelled file-chooser dialog and
  a `load_pack` failure during name resolution are both `Result`/`Option`, never assumed to
  succeed.
- Commit after each task or logical group.
