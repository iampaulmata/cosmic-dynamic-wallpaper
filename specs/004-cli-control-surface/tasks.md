---

description: "Task list template for feature implementation"
---

# Tasks: CLI Control Surface

**Input**: Design documents from `/specs/004-cli-control-surface/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/wallpaperctl-cli.md, contracts/location-config-schema.md, contracts/wallpaperd-dbus-interface.md, quickstart.md

**Tests**: Included, in two tiers (plan.md Technical Context, research.md R6). Config-only
commands (register, list *packs*, remove, assign, location) get real `tempfile`-backed
integration tests against `pack-loader`/`cosmic-config`, matching spec 2's precedent.
Daemon-dependent commands (list *outputs*, query, reevaluate) get unit tests against a mock
D-Bus service (`dbus_mock.rs`) — end-to-end verification against a real `wallpaperd` waits on
spec 3's Phase 10 (Amendment 2026-08-13) being implemented, and stays a manual QA item until
then (quickstart.md).

**Organization**: Tasks are grouped by user story (spec.md) to enable independent
implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: Which user story this task belongs to (US1–US7)
- File paths are relative to the repository root

## Path Conventions

Fourth workspace crate (plan.md Structure Decision): `crates/wallpaperctl/src/`,
`crates/wallpaperctl/tests/`, with path dependencies on `crates/schedule-engine` (spec 1) and
`crates/pack-loader` (spec 2) only — deliberately **not** `crates/renderer` (spec 3); this
crate talks to `wallpaperd` only via `cosmic-config` or D-Bus, never by linking.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Add `wallpaperctl` as the workspace's fourth crate.

- [ ] T001 Add `crates/wallpaperctl` as a new member of the workspace root `Cargo.toml` (plan.md Project Structure)
- [ ] T002 Create `crates/wallpaperctl/Cargo.toml` with `clap` (4.6.x, derive), `serde_json`, `zbus` (5.x), `cosmic-config` (git dependency) dependencies, and path dependencies on `schedule-engine` and `pack-loader` only — no dependency on `renderer` (research.md R1–R4, plan.md Technical Context/Structure Decision)
- [ ] T003 [P] Add `[lints]` denying `clippy::unwrap_used` and `clippy::expect_used` outside `#[cfg(test)]` to `crates/wallpaperctl/Cargo.toml` (constitution Principle VIII)
- [ ] T004 [P] Add a CI workflow running `cargo test` and `cargo clippy` for the crate in `.github/workflows/wallpaperctl-ci.yml` (covers the config-only and mock-D-Bus tests; true daemon-dependent verification is manual QA, research.md R6)

**Checkpoint**: `cargo build` succeeds on an empty crate depending on `schedule-engine` and `pack-loader`; CI pipeline is defined.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Shared types/infrastructure every command needs. No user story work starts before
this phase is done.

**⚠️ CRITICAL**: Blocks Phases 3–9.

- [ ] T005 Create the `CliError` type and its exit-code mapping in `crates/wallpaperctl/src/error.rs` (data-model.md CliError, contracts/wallpaperctl-cli.md Exit codes, FR-012; `std::error::Error` + `Debug` + `Display`, no panics — constitution Principle VIII)
- [ ] T006 [P] Implement `output.rs` — human-readable vs. `serde_json` machine-readable rendering shared across every data-returning command (FR-013, research.md R2)
- [ ] T007 [P] Implement `dbus_client.rs` — `zbus` client wrapper connecting to `wallpaperd`'s session-bus interface (contracts/wallpaperd-dbus-interface.md), mapping an unreachable bus to `CliError::DaemonUnreachable` (FR-011, research.md R3; depends on T005)
- [ ] T008 Wire up the `clap` CLI skeleton — top-level subcommand enum (`register`, `list`, `remove`, `assign`, `location`, `query`, `reevaluate`) plus the global `--json` flag — in `crates/wallpaperctl/src/main.rs`, matching contracts/wallpaperctl-cli.md (research.md R1; depends on T005)

**Checkpoint**: Crate compiles; `wallpaperctl --help` lists every subcommand; nothing is implemented yet.

---

## Phase 3: User Story 1 - Register a Pack So It Can Be Used (Priority: P1) 🎯 MVP

**Goal**: Point the CLI at a directory or image file and have it become a known pack.

**Independent Test**: Point the CLI at a valid pack directory (or a single image file) and
confirm it is subsequently reported as known, without any other CLI command having run first.

### Tests for User Story 1

- [ ] T009 [P] [US1] `tempfile`-backed integration test: register a valid multi-image pack directory and a single static image, confirm both are subsequently known (spec.md US1 Scenarios 1–2) in `crates/wallpaperctl/tests/register_list_remove.rs` (research.md R6)
- [ ] T010 [P] [US1] Integration test: registering an already-known source is idempotent — no duplicate, no error (spec.md US1 Scenario 3) in `crates/wallpaperctl/tests/register_list_remove.rs`
- [ ] T011 [P] [US1] Integration test: registering an invalid pack (malformed manifest, missing image, path-traversal attempt) fails with spec 2's error surfaced verbatim, nothing added to the registry (spec.md US1 Scenario 4) in `crates/wallpaperctl/tests/register_list_remove.rs`

### Implementation for User Story 1

- [ ] T012 [US1] Implement `register <path>` — call spec 2's `load_pack` + `Registry::register`, mapping `ManifestError` to `CliError::PackLoadFailed` — in `crates/wallpaperctl/src/commands/register.rs` (FR-001, FR-002; depends on T005)
- [ ] T013 [US1] Wire `register.rs` into `main.rs`'s dispatch with a human/JSON success confirmation (depends on T006, T008, T012)

**Checkpoint**: `wallpaperctl register <path>` works end to end against a real registry, independently testable (`cargo test --test register_list_remove`).

---

## Phase 4: User Story 2 - Assign a Pack to an Output (Priority: P1) 🎯 MVP

**Goal**: Bind a registered pack (or the "same everywhere" toggle) to a specific output —
config-only, no daemon required.

**Independent Test**: With at least one pack registered (Story 1), assign it to an output name
and confirm the write lands in spec 3's `RendererConfig` shape — with no daemon running.

### Tests for User Story 2

- [ ] T014 [P] [US2] `tempfile`-backed integration test: assign a registered pack to a named output — including a name that isn't currently connected (the "configure ahead of time" case, FR-007) — confirm the write matches spec 3's `RendererConfig.overrides` shape (spec.md US2 Scenarios 1, 4) in `crates/wallpaperctl/tests/assign_location.rs` (research.md R6)
- [ ] T015 [P] [US2] Integration test: enabling "same pack on all outputs" sets `RendererConfig.same_pack_everywhere` (spec.md US2 Scenario 2) in `crates/wallpaperctl/tests/assign_location.rs`
- [ ] T016 [P] [US2] Integration test: assigning an unregistered pack fails clearly with no write (spec.md US2 Scenario 5) in `crates/wallpaperctl/tests/assign_location.rs`

### Implementation for User Story 2

- [ ] T017 [US2] Implement `assign --output <id> <pack>` / `assign --same-everywhere <pack>` — validate the pack is registered via spec 2's `Registry::known_packs` (local, no daemon needed); write to spec 3's `RendererConfig` via `cosmic-config` regardless of whether the named output currently exists; if `dbus_client` happens to be reachable, emit a non-fatal warning (not a `CliError`) when the output name doesn't match a currently-managed one — in `crates/wallpaperctl/src/commands/assign.rs` (FR-006, FR-007; depends on T005, T007)
- [ ] T018 [US2] Wire `assign.rs` into `main.rs`'s dispatch with human/JSON confirmation (depends on T006, T008, T017)

**Checkpoint**: User Stories 1 and 2 — a pack can be registered and assigned purely via CLI, no daemon required.

---

## Phase 5: User Story 3 - Provide Your Location for Solar-Anchored Packs (Priority: P1) 🎯 MVP

**Goal**: Persist a manual latitude/longitude so solar-anchored packs have what they need —
config-only, new `LocationConfig` schema.

**Independent Test**: Set a valid latitude/longitude, then read it back and confirm it
matches — independent of any pack or output configuration.

### Tests for User Story 3

- [ ] T019 [P] [US3] `tempfile`-backed integration test: set a valid location, confirm a subsequent read matches (spec.md US3 Scenario 1) in `crates/wallpaperctl/tests/assign_location.rs`
- [ ] T020 [P] [US3] Integration test: setting a new location replaces the old value (spec.md US3 Scenario 2) in `crates/wallpaperctl/tests/assign_location.rs`
- [ ] T021 [P] [US3] Integration test: an out-of-range/malformed latitude or longitude is rejected via spec 1's `Location::new`, with no partial write (spec.md US3 Scenario 3) in `crates/wallpaperctl/tests/assign_location.rs`
- [ ] T022 [P] [US3] Integration test: clearing a location removes it (spec.md US3 Scenario 4) in `crates/wallpaperctl/tests/assign_location.rs`

### Implementation for User Story 3

- [ ] T023 [US3] Implement the `LocationConfig` `cosmic-config` schema (`schema_version`, `location: Option<Location>`) in `crates/wallpaperctl/src/commands/location.rs` (data-model.md LocationConfig, contracts/location-config-schema.md, FR-008; depends on T005)
- [ ] T024 [US3] Implement `location get|set|clear` — `set` validates via spec 1's `Location::new` before writing anything (FR-008, FR-013; depends on T023)
- [ ] T025 [US3] Wire `location.rs` into `main.rs`'s dispatch with human/JSON output (depends on T006, T008, T024)

**Checkpoint**: User Stories 1–3 (all P1) — MVP complete: register, assign, and set a location, entirely via CLI, no daemon needed at any point.

---

## Phase 6: User Story 4 - See What's Currently Showing and What's Next (Priority: P2)

**Goal**: Query a live `wallpaperd` for an output's current image and next transition.

**Independent Test**: With an output actively scheduled, query it and confirm the reported
state matches the daemon's real internal state.

### Tests for User Story 4

- [ ] T026 [P] [US4] Mock-D-Bus unit test: `query` constructs the correct `QueryOutput`/`QueryAll` request and parses a mocked response into `ScheduleQueryResponse` (contracts/wallpaperd-dbus-interface.md) in `crates/wallpaperctl/tests/dbus_mock.rs` (research.md R6)
- [ ] T027 [P] [US4] Unit test: `query` fails fast with `CliError::DaemonUnreachable` when no bus connection is available, rather than hanging (spec.md US4 Scenario 3, FR-011) in `crates/wallpaperctl/tests/dbus_mock.rs`

### Implementation for User Story 4

- [ ] T028 [US4] Implement `dbus_client.rs`'s `QueryOutput`/`QueryAll` calls per contracts/wallpaperd-dbus-interface.md (research.md R3; depends on T007)
- [ ] T029 [US4] Implement `query [--output <id>]` — map the D-Bus response into `ScheduleQueryResponse`, including the `Unassigned` state (spec.md US4 Scenario 2), human/JSON output — in `crates/wallpaperctl/src/commands/query.rs` (FR-009, FR-013; depends on T028)
- [ ] T030 [US4] Wire `query.rs` into `main.rs`'s dispatch (depends on T006, T008, T029)

**Checkpoint**: User Stories 1–4 functional. `query` is verifiable end-to-end once spec 3's Phase 10 (D-Bus service, Amendment 2026-08-13) is implemented; until then it's exercised via T026/T027's mocks only.

---

## Phase 7: User Story 5 - Discover What You Can Assign (Priority: P2)

**Goal**: Browse known packs (config-only) and currently-managed outputs (daemon-required —
corrected, spec.md Assumptions).

**Independent Test**: With at least one pack registered, list packs and confirm accuracy —
independent of any daemon. Separately, with `wallpaperd` running, list outputs and confirm the
listing matches what the daemon actually manages.

### Tests for User Story 5

- [ ] T031 [P] [US5] Integration test: `list packs` shows name/source/status for registered packs, and reports an empty result clearly when none are registered (spec.md US5 Scenarios 1–2) in `crates/wallpaperctl/tests/register_list_remove.rs`
- [ ] T032 [P] [US5] Mock-D-Bus unit test: `list outputs` reuses the `QueryAll` call and displays only each entry's `output_id` (spec.md US5 Scenario 3, research.md R5) in `crates/wallpaperctl/tests/dbus_mock.rs`
- [ ] T033 [P] [US5] Unit test: `list outputs` fails fast with `CliError::DaemonUnreachable` when no daemon is running (spec.md US5 Scenario 4, FR-011 — corrected during task planning) in `crates/wallpaperctl/tests/dbus_mock.rs`

### Implementation for User Story 5

- [ ] T034 [US5] Implement `list packs` — spec 2's `Registry::known_packs()`, human/JSON output — in `crates/wallpaperctl/src/commands/list.rs` (FR-003; depends on T005, T006)
- [ ] T035 [US5] Implement `list outputs` — calls `dbus_client`'s `QueryAll` (T028) and displays only the output identifiers — in `crates/wallpaperctl/src/commands/list.rs` (FR-005, research.md R5; depends on T006, T028)
- [ ] T036 [US5] Wire `list.rs` into `main.rs`'s dispatch (depends on T008, T034, T035)

**Checkpoint**: User Stories 1–5 functional.

---

## Phase 8: User Story 6 - Force an Immediate Re-Evaluation (Priority: P3)

**Goal**: Trigger a live `wallpaperd` to recompute one or all outputs' schedules on demand.

**Independent Test**: With an output already running on a schedule, force re-evaluation and
confirm the daemon recomputes without any assignment or config value having changed.

### Tests for User Story 6

- [ ] T037 [P] [US6] Mock-D-Bus unit test: `reevaluate` constructs the correct `Reevaluate`/`ReevaluateAll` request for a named output or all outputs (spec.md US6 Scenarios 1–2) in `crates/wallpaperctl/tests/dbus_mock.rs`
- [ ] T038 [P] [US6] Unit test: `reevaluate` fails fast with `CliError::DaemonUnreachable` when no daemon is running (spec.md US6 Scenario 3, FR-011) in `crates/wallpaperctl/tests/dbus_mock.rs`

### Implementation for User Story 6

- [ ] T039 [US6] Implement `dbus_client.rs`'s `Reevaluate`/`ReevaluateAll` calls per contracts/wallpaperd-dbus-interface.md (research.md R3; depends on T007)
- [ ] T040 [US6] Implement `reevaluate [--output <id>]` — human/JSON acknowledgement — in `crates/wallpaperctl/src/commands/reevaluate.rs` (FR-010, FR-013; depends on T039)
- [ ] T041 [US6] Wire `reevaluate.rs` into `main.rs`'s dispatch (depends on T006, T008, T040)

**Checkpoint**: User Stories 1–6 functional.

---

## Phase 9: User Story 7 - Remove a Known Pack (Priority: P3)

**Goal**: Delete a pack's registry entry outright.

**Independent Test**: Register a pack, remove it, and confirm it no longer appears in `list
packs` nor can be newly assigned.

### Tests for User Story 7

- [ ] T042 [P] [US7] Integration test: removing a pack not currently assigned anywhere deletes its registry entry — it no longer lists or can be assigned (spec.md US7 Scenario 1) in `crates/wallpaperctl/tests/register_list_remove.rs`
- [ ] T043 [P] [US7] Integration test: removing a pack currently assigned to an output still succeeds — the affected output falls back to spec 2/3's existing unavailable-pack handling, no new behavior invented (spec.md US7 Scenario 2) in `crates/wallpaperctl/tests/register_list_remove.rs`

### Implementation for User Story 7

- [ ] T044 [US7] Implement `remove <pack-source>` — spec 2's `Registry::remove` — in `crates/wallpaperctl/src/commands/remove.rs` (FR-004; depends on T005)
- [ ] T045 [US7] Wire `remove.rs` into `main.rs`'s dispatch (depends on T006, T008, T044)

**Checkpoint**: All seven user stories functional; full quickstart.md automated portion green.

---

## Phase 10: Polish & Cross-Cutting Concerns

**Purpose**: Close out the spec's success criteria and hand off a stable CLI.

- [ ] T046 [P] Verify strong test coverage on `error.rs`, `output.rs`, and every config-only command path via `cargo llvm-cov` (SC-002, SC-004); add tests to close any gap
- [ ] T047 [P] Add rustdoc comments to every public item matching contracts/wallpaperctl-cli.md
- [ ] T048 [P] Add `crates/wallpaperctl/README.md` summarizing scope, the config-only-vs-daemon-required command split (spec.md Assumptions), and explicit non-scope (no GUI, no crossfade-duration control)
- [ ] T049 Document the manual QA checklist for the three daemon-dependent commands (`list outputs`, `query`, `reevaluate`) against a real `wallpaperd` — runnable only once spec 3's Phase 10 (Amendment 2026-08-13) is implemented — referencing quickstart.md's manual smoke check
- [ ] T050 Run quickstart.md end-to-end (`cargo test`, and the manual smoke check once a real `wallpaperd` exists) and fix any drift between the doc and the actual API/behavior

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — start immediately.
- **Foundational (Phase 2)**: Depends on Setup. Blocks all user stories.
- **User Story 1 (Phase 3)**: Depends only on Foundational.
- **User Story 2 (Phase 4)**: Depends only on Foundational — independent of Story 1's
  implementation, though its tests assume a pack is already registered (reuses Story 1's
  fixtures, not its code).
- **User Story 3 (Phase 5)**: Depends only on Foundational — fully independent of Stories 1–2.
- **User Story 4 (Phase 6)**: Depends on Foundational and `dbus_client.rs` (T007).
- **User Story 5 (Phase 7)**: `list packs` depends only on Foundational; `list outputs`
  depends on Foundational and reuses US4's `QueryAll` client call (T028) — so Story 5's
  `list outputs` half is implemented after Story 4, even though the two stories are otherwise
  independent.
- **User Story 6 (Phase 8)**: Depends on Foundational and `dbus_client.rs` (T007) — independent
  of Stories 4–5's own D-Bus calls, though all three share the same client module.
- **User Story 7 (Phase 9)**: Depends only on Foundational.
- **Polish (Phase 10)**: Depends on all seven user stories being complete.

### Parallel Opportunities

- T003 and T004 (Setup) — different files.
- T006 and T007 (Foundational) — different files, both depend only on T005.
- T009, T010, and T011 (US1 tests) — same file but independent scenarios.
- T014, T015, and T016 (US2 tests) — same file but independent scenarios.
- T019–T022 (US3 tests) — same file but independent scenarios.
- T026 and T027 (US4 tests) — same file but independent scenarios.
- T031, T032, and T033 (US5 tests) — two different files, independent scenarios.
- T037 and T038 (US6 tests) — same file but independent scenarios.
- T042 and T043 (US7 tests) — same file but independent scenarios.
- T046, T047, and T048 (Polish) — different files.
- Once Foundational (Phase 2) is done, Stories 1, 2, 3, and 7 can all proceed fully in
  parallel (no shared files, no cross-dependencies) — Stories 4, 5, and 6 share
  `dbus_client.rs` and should be coordinated if staffed separately.

### Sequential-in-Practice Files

`main.rs` is touched by every story's final wiring task (T013, T018, T025, T030, T036, T041,
T045) — coordinate edits there even though each task is nominally story-scoped. `dbus_client.rs`
is extended by Stories 4, 5 (partially), and 6 — the same file, different methods.

---

## Parallel Example: Foundational Phase

```bash
# After T005 (error.rs) lands, launch together:
Task: "Implement output.rs — human vs. machine-readable rendering"
Task: "Implement dbus_client.rs — zbus client wrapper, daemon-unreachable handling"
```

## Parallel Example: MVP Stories (1, 2, 3)

```bash
# Once Foundational is done, these three stories have no cross-dependencies:
Task: "Implement register.rs — FR-001, FR-002"
Task: "Implement assign.rs — FR-006, FR-007"
Task: "Implement location.rs — FR-008"
```

---

## Implementation Strategy

### MVP First (User Stories 1, 2, and 3)

1. Phase 1: Setup
2. Phase 2: Foundational (blocks everything)
3. Phase 3: User Story 1 — register a pack
4. Phase 4: User Story 2 — assign a pack to an output
5. Phase 5: User Story 3 — set a location
6. **STOP and VALIDATE**: `cargo test --test register_list_remove --test assign_location`
   green; spec.md SC-001's config-only half is achievable (the daemon-dependent half needs
   spec 3's Phase 10)
7. This alone is a demonstrable MVP: register a pack, assign it, set a location — all via CLI,
   no daemon required at any point

### Incremental Delivery

1. Setup + Foundational → foundation ready
2. Add User Story 1 → validate independently
3. Add User Story 2 → validate independently
4. Add User Story 3 → validate independently → MVP (all three P1 stories done)
5. Add User Story 4 → validate independently (mocked; real validation waits on spec 3)
6. Add User Story 5 → validate independently (`list packs` fully; `list outputs` mocked)
7. Add User Story 6 → validate independently (mocked)
8. Add User Story 7 → validate independently
9. Polish → coverage, docs, manual QA checklist, quickstart parity

---

## Notes

- [P] tasks touch different files, or independent scenarios within the same test file, with no
  unmet dependency.
- Three commands (`list outputs`, `query`, `reevaluate`) cannot be verified end-to-end until
  spec 3's Phase 10 (Amendment 2026-08-13: `dbus_service.rs` in `crates/renderer`) is
  implemented — their tests here are mock-D-Bus unit tests, not a gap in this task list.
- `assign` deliberately does not require a running daemon to succeed, even when targeting an
  output name that isn't currently connected — see FR-007 and US2 T014's test. Don't add a
  live-output-existence check that would make `assign` daemon-required; that was corrected out
  of this spec before task generation.
- A pack or config value is untrusted-by-the-time-it's-read input in the same spirit as spec
  2/3's own postures — every command must keep failing closed (specific error, non-zero exit)
  as the default, per constitution Principle VIII.
- Commit after each task or logical group.
