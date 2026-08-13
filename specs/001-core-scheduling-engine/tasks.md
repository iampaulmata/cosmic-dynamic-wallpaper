---

description: "Task list template for feature implementation"
---

# Tasks: Core Scheduling Engine

**Input**: Design documents from `/specs/001-core-scheduling-engine/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/schedule-engine-api.md, quickstart.md

**Tests**: Included. The constitution requires solar/time logic to ship with unit tests
(Principle V, Development Workflow), and spec.md's SC-002/SC-003/SC-005 are only checkable
with an actual test suite — this is not an optional add-on for this spec.

**Organization**: Tasks are grouped by user story (spec.md) to enable independent
implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: Which user story this task belongs to (US1, US2, US3)
- File paths are relative to the repository root

## Path Conventions

Single Rust library crate in a workspace (plan.md Structure Decision):
`crates/schedule-engine/src/`, `crates/schedule-engine/tests/`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Stand up the workspace and crate this and later specs will build in.

- [X] T001 Create workspace root `Cargo.toml` at the repository root with `crates/schedule-engine` as its first member (plan.md Project Structure)
- [X] T002 Create `crates/schedule-engine/Cargo.toml` with `sunrise` and `chrono` dependencies and `proptest` as a dev-dependency (research.md R1–R3); leave the MSRV field for the toolchain present when first built (research.md R5)
- [X] T003 [P] Add `[lints]` denying `clippy::unwrap_used` and `clippy::expect_used` outside `#[cfg(test)]` to `crates/schedule-engine/Cargo.toml` (constitution Principle VIII)
- [X] T004 [P] Add a CI workflow running `cargo test`, `cargo clippy`, and `cargo llvm-cov` for the crate in `.github/workflows/schedule-engine-ci.yml`

**Checkpoint**: `cargo build` succeeds on an empty crate; CI pipeline is defined.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Shared types every user story needs. No user story work starts before this
phase is done.

**⚠️ CRITICAL**: Blocks Phases 3–5.

- [X] T005 Create `LocationError` and `PackError` types in `crates/schedule-engine/src/error.rs` (data-model.md Error types; `std::error::Error` + `Debug` + `Display`, no panics — constitution Principle VIII)
- [X] T006 [P] Create `TimeAnchor` and `SolarEventKind` types in `crates/schedule-engine/src/anchor.rs` (data-model.md TimeAnchor/SolarEventKind, FR-6)
- [X] T007 [P] Create `Location` type with `Location::new` range/finite validation in `crates/schedule-engine/src/location.rs` (data-model.md Location, FR-002a; depends on T005 for `LocationError`)
- [X] T008 Create `PackImage` and `WallpaperPack` container types with structural validation — max 64 anchors (FR-001), unique image ids, mixed-anchor-type rejection (FR-006) — in `crates/schedule-engine/src/pack.rs` (depends on T005, T006)
- [X] T009 [P] Create `ScheduleQueryResult` and `TransitionState` types in `crates/schedule-engine/src/query.rs` (data-model.md ScheduleQueryResult/TransitionState)
- [X] T010 Wire up public API re-exports (`Location`, `TimeAnchor`, `SolarEventKind`, `PackImage`, `WallpaperPack`, `ScheduleQueryResult`, `TransitionState`, `LocationError`, `PackError`) in `crates/schedule-engine/src/lib.rs` (depends on T005–T009; matches contracts/schedule-engine-api.md)

**Checkpoint**: Crate compiles with all shared types defined; user stories can now proceed.

---

## Phase 3: User Story 1 - Solar-Anchored Schedule Resolves Correctly for a Location (Priority: P1) 🎯 MVP

**Goal**: Given a solar-anchored pack and a manual lat/long, deterministically resolve which
image is active (and any crossfade progress) for any query instant.

**Independent Test**: Build a pack anchored to solar events, supply a fixed location/date,
query across a full day, and check results against independently-computed reference solar
times — no Wayland/rendering involved.

### Tests for User Story 1

- [ ] T011 [P] [US1] Golden reference accuracy tests (location/date pairs vs. published reference values, within 1 minute — SC-002, research.md R4) in `crates/schedule-engine/tests/solar_accuracy.rs`
- [ ] T012 [P] [US1] Acceptance scenario tests — mid-period resolution, offset-anchor transition start, in-window progress fraction (spec.md US1 scenarios 1–3), plus polar day/night fallback (FR-007) and solar-pack exact-instant tie rejection (FR-006a) — in `crates/schedule-engine/tests/schedule_resolution.rs`

### Implementation for User Story 1

- [ ] T013 [US1] Implement solar event time computation wrapping the `sunrise` crate for all eight `SolarEventKind` variants, including derived solar midnight (`solar_noon ± 12h`, research.md R1) and signed-offset application, in `crates/schedule-engine/src/solar.rs` (depends on T006, T007)
- [ ] T014 [US1] Implement solar-pack exact-instant duplicate detection, resolved per query date (FR-006a) in `crates/schedule-engine/src/pack.rs` (extends T008; depends on T013)
- [ ] T015 [US1] Implement solar-anchored resolution in `ValidatedPack::query` — active/outgoing/incoming image and progress fraction (FR-004), polar day/night hold-adjacent-image fallback (FR-007), midnight wraparound (FR-009) — in `crates/schedule-engine/src/query.rs` (depends on T013)
- [ ] T016 [US1] Implement `ValidatedPack::next_transition_after` for solar-anchored packs (FR-005) in `crates/schedule-engine/src/query.rs` (depends on T015)

**Checkpoint**: User Story 1 fully functional and testable independently (`cargo test --test solar_accuracy --test schedule_resolution`).

---

## Phase 4: User Story 2 - Fully Manual, Location-Free Clock Schedule (Priority: P1)

**Goal**: Given a clock-anchored pack and no location input at all, resolve the active image
purely from wall-clock time.

**Independent Test**: Build a pack with only clock-time anchors, no location, query across a
day, and confirm no solar computation runs and no location value is read.

### Tests for User Story 2

- [ ] T017 [P] [US2] Acceptance scenario tests — clock-time resolution, zero-location-required query, mixed-anchor-type rejection (spec.md US2 scenarios 1–3), plus DST-shift edge case and clock-pack exact-instant tie rejection (FR-006a) — in `crates/schedule-engine/tests/schedule_resolution.rs`

### Implementation for User Story 2

- [ ] T018 [US2] Implement clock-anchored resolution in `ValidatedPack::query` using `chrono::DateTime<Local>` end to end (no naive-datetime confusion, per research.md R2) and midnight wraparound (FR-003, FR-009) in `crates/schedule-engine/src/query.rs` (depends on T010)
- [ ] T019 [US2] Implement clock-pack exact-instant duplicate detection, static one-time check (FR-006a) in `crates/schedule-engine/src/pack.rs` (extends T008/T014)
- [ ] T020 [US2] Implement `ValidatedPack::next_transition_after` for clock-anchored packs (FR-005) in `crates/schedule-engine/src/query.rs` (depends on T018)

**Checkpoint**: User Stories 1 and 2 both independently functional.

---

## Phase 5: User Story 3 - Deterministic State Query for Downstream Consumers (Priority: P2)

**Goal**: Guarantee that `query()`/`next_transition_after()` are pure and deterministic
across both anchor types, so the renderer/CLI specs can build on a stable contract.

**Independent Test**: For a mix of solar- and clock-anchored packs, call the engine directly
(no daemon) and confirm identical inputs produce identical outputs, and next-transition
timestamps are real and consistent.

### Tests for User Story 3

- [ ] T021 [P] [US3] Determinism and monotonic-progress property tests — identical inputs → identical outputs; progress fraction is monotonic and stays within [0.0, 1.0) (SC-003) — in `crates/schedule-engine/tests/determinism.rs` using `proptest`
- [ ] T022 [P] [US3] Edge case tests — single-image/static-mode pack always active with no transition (FR-3 degenerate case), and overlapping crossfade windows produce a well-defined monotonic fraction — in `crates/schedule-engine/tests/schedule_resolution.rs`

### Implementation for User Story 3

- [ ] T023 [US3] Handle the degenerate single-image/static-mode pack in `ValidatedPack::query` and `next_transition_after` (always active, `transition: None`, `next_transition_at: None`) in `crates/schedule-engine/src/query.rs` (depends on T015, T018)
- [ ] T024 [US3] Audit `query.rs` and `solar.rs` to confirm no hidden state or implicit clock reads remain — `at` must be the only source of "now" (SC-003) — in `crates/schedule-engine/src/query.rs` and `crates/schedule-engine/src/solar.rs`

**Checkpoint**: All three user stories independently functional; full acceptance suite green.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Close out the spec's success criteria and hand off a stable contract to specs 2–4.

- [ ] T025 [P] Verify ≥90% line coverage on `src/solar.rs`, `src/pack.rs`, `src/location.rs`, `src/query.rs` via `cargo llvm-cov` (SC-005); add tests to close any gap
- [ ] T026 [P] Add rustdoc comments to every public item matching contracts/schedule-engine-api.md
- [ ] T027 [P] Add `crates/schedule-engine/README.md` summarizing scope and explicit non-scope (rendering/persistence/portal-location are later specs, per spec.md Assumptions)
- [ ] T028 Run `quickstart.md` end-to-end (build, `cargo test`, `cargo llvm-cov`, manual smoke snippet) and fix any drift between the doc and the actual API

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — start immediately.
- **Foundational (Phase 2)**: Depends on Setup. Blocks all user stories.
- **User Story 1 (Phase 3)**: Depends only on Foundational.
- **User Story 2 (Phase 4)**: Depends only on Foundational — independent of Phase 3's `solar.rs` work, though both phases touch `src/query.rs` (T015/T016 vs. T018/T020), so treat that file as sequential across phases even though the stories are logically independent.
- **User Story 3 (Phase 5)**: Depends on Foundational **and** exercises both Phase 3 and Phase 4's `query.rs` code paths (its Independent Test explicitly covers "solar and clock-anchored" packs) — implement after US1 and US2, even though its own test file (`determinism.rs`) is new and story-scoped.
- **Polish (Phase 6)**: Depends on all three user stories being complete.

### Parallel Opportunities

- T003 and T004 (Setup) — different files.
- T006, T007, and T009 (Foundational) — different files, no cross-dependency.
- T011 and T012 (US1 tests) — different files.
- T021 and T022 (US3 tests) — different files.
- T025, T026, and T027 (Polish) — different files.
- Once Foundational (Phase 2) is done, Phase 3 and Phase 4 implementation work can proceed
  in parallel if staffed by different people, provided `src/query.rs` edits are coordinated
  (both phases extend it) — Phase 5 still waits for both to land.

---

## Parallel Example: Foundational Phase

```bash
# After T005 (error.rs) lands, launch together:
Task: "Create TimeAnchor and SolarEventKind types in crates/schedule-engine/src/anchor.rs"
Task: "Create Location type with validation in crates/schedule-engine/src/location.rs"
Task: "Create ScheduleQueryResult and TransitionState types in crates/schedule-engine/src/query.rs"
```

## Parallel Example: User Story 1 Tests

```bash
Task: "Golden reference accuracy tests in crates/schedule-engine/tests/solar_accuracy.rs"
Task: "Acceptance scenario tests in crates/schedule-engine/tests/schedule_resolution.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Phase 1: Setup
2. Phase 2: Foundational (blocks everything)
3. Phase 3: User Story 1 — solar-anchored resolution
4. **STOP and VALIDATE**: `cargo test --test solar_accuracy --test schedule_resolution` green, SC-002 accuracy holds
5. This alone is a demonstrable MVP: a location + solar pack in, correct active image out

### Incremental Delivery

1. Setup + Foundational → foundation ready
2. Add User Story 1 → validate independently → MVP
3. Add User Story 2 → validate independently (confirms the privacy-preserving path works)
4. Add User Story 3 → validate independently (locks in the determinism contract specs 2–4 depend on)
5. Polish → coverage, docs, quickstart parity

---

## Notes

- [P] tasks touch different files with no unmet dependency.
- Tests are written before their corresponding implementation tasks within each phase, per
  constitution Principle V's test-first emphasis on this pure/deterministic logic.
- `src/query.rs` is shared across US1, US2, and US3 — coordinate edits there even when tasks
  are nominally story-scoped; it's the one file where cross-story sequencing actually matters.
- Commit after each task or logical group.
