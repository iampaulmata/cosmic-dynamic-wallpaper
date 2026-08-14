---

description: "Task list template for feature implementation"
---

# Tasks: Location Portal Integration

**Input**: Design documents from `/specs/006-location-portal-integration/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md,
contracts/location-config-schema-v2.md, contracts/wallpaperctl-location-cli.md, quickstart.md

**Tests**: Included, split the same way plan.md's Technical Context describes. The schema/
migration/`effective_location()`/CLI logic is fully headless-`cargo test`-able. The live portal
subscription itself (session creation, `LocationUpdated` stream, calloop integration) is
manual-QA-verified against this project's real COSMIC session — the same split spec 3 already
established for Wayland/GPU code, with research.md R1/R2's live spike as the first data point,
captured before any of this spec's code existed.

**Organization**: Tasks are grouped by user story (spec.md) to enable independent
implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: Which user story this task belongs to (US1–US4)
- File paths are relative to the repository root

## Path Conventions

No new workspace crate (plan.md Structure Decision). Every task lands in one of the two existing
crates spec 6's FRs name directly: `crates/renderer/src/` (daemon-side: portal client, schema
read side, scheduling amendment) and `crates/wallpaperctl/src/` (CLI-side: mode toggle commands,
schema write side).

---

## Phase 1: Setup

**Purpose**: Add the one new external dependency this spec needs.

- [X] T001 Add `ashpd = { version = "0.13", default-features = false, features = ["location", "async-io"] }` to `crates/renderer/Cargo.toml` (research.md R3 — `default-features = false` is required to drop `ashpd`'s own `tokio` default and keep it on the same `async-io` backend `zbus` already uses in this crate)

**Checkpoint**: `cargo build -p renderer` succeeds with the new dependency present but unused. No change needed to `crates/wallpaperctl/Cargo.toml` — its new subcommands are pure `cosmic-config` reads/writes, same posture as its existing `get`/`set`/`clear` (plan.md Technical Context). Existing per-crate CI (`.github/workflows/renderer-ci.yml`, `wallpaperctl-ci.yml`) already runs `cargo test`/`cargo clippy` for both crates and needs no changes to pick up this spec's new code.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The shared v2 schema and its pure resolution rule — every user story below reads or
writes this shape.

**⚠️ CRITICAL**: Blocks Phases 3–6.

- [X] T002 [P] Define `LocationMode` (`Manual`/`Automatic`) and `AutomaticStatus`
      (`Unresolved`/`Resolved`/`Unavailable { reason: String }`) enums, and extend
      `LocationConfigEntry` to v2 (`mode`, `location`, `automatic_location`,
      `automatic_status` fields, `#[version = 2]`, a `Default` impl matching data-model.md's
      Migration mapping) in `crates/wallpaperctl/src/config.rs` (data-model.md,
      contracts/location-config-schema-v2.md — write side)
- [X] T003 [P] Mirror the exact same `LocationMode`/`AutomaticStatus`/`LocationSource` v2 shape
      (renderer's existing name for its read-side struct) with `#[version = 2]` in
      `crates/renderer/src/config.rs` — field names and types MUST match T002 exactly (this
      project's own established lesson: a prior mismatch between these two crates' independently
      -defined "identical" types silently produced an empty map at runtime, see
      `crates/renderer/src/config.rs`'s existing regression test and doc comment)
- [X] T004 [P] Regression test confirming `cosmic-config`'s built-in previous-version fallback
      needs no hand-written migration code (research.md R7): hand-write a v1-shaped RON entry on
      disk, load it through the new v2 struct, and assert `mode: Manual`, unchanged `location`,
      `automatic_location: None`, `automatic_status: Unresolved` — add to both
      `crates/wallpaperctl/src/config.rs` and `crates/renderer/src/config.rs` test modules
      (depends on T002, T003)
- [X] T005 Implement `effective_location()` (data-model.md) as a pure function in
      `crates/renderer/src/config.rs`, with unit tests for all three branches: `Manual` mode
      returns `location`; `Automatic` mode with a resolved value returns `automatic_location`;
      `Automatic` mode unresolved/unavailable falls back to `location`, then `None` (depends on
      T003)

**Checkpoint**: The v2 schema compiles and round-trips correctly in both crates; migration and
resolution logic are fully unit-tested. No daemon or CLI wiring yet — nothing observable to a
user.

---

## Phase 3: User Story 1 - Get a Correct Schedule Without Ever Typing Coordinates (Priority: P1) 🎯 MVP

**Goal**: Enable automatic mode; the daemon resolves a location via the portal and schedules
solar-anchored packs against it.

**Independent Test**: On a system where the portal is available and permission is granted,
enable automatic location (no manual location ever entered) and confirm a solar-anchored pack's
active image/next-transition match what spec 1 computes for the resolved coordinates.

### Tests for User Story 1

- [X] T006 [P] [US1] Unit test: `location auto` sets `mode: Automatic` only, is idempotent on
      repeat calls, and never touches `location`/`automatic_location`/`automatic_status`
      (spec.md US1, contracts/wallpaperctl-location-cli.md) in
      `crates/wallpaperctl/src/commands/location.rs`
- [X] T007 [P] [US1] Unit test: given a successful resolved reading, `portal_location.rs`'s
      conversion path validates it through spec 1's `Location::new` and produces
      `automatic_location: Some(..)`, `automatic_status: Resolved` (spec.md US1 Scenarios 1–2) in
      `crates/renderer/src/portal_location.rs` — inject the reading through a small seam (a
      plain function taking `latitude`/`longitude`/`accuracy` rather than an `ashpd` type
      directly) so this stays a pure, no-D-Bus unit test

### Implementation for User Story 1

- [X] T008 [US1] Implement the `location auto` subcommand (writes `mode: Automatic` only) in
      `crates/wallpaperctl/src/commands/location.rs` (FR-001/FR-002/FR-003,
      contracts/wallpaperctl-location-cli.md; depends on T002)
- [X] T009 [US1] Wire `location auto` into `main.rs`'s dispatch (depends on T008)
- [X] T010 [US1] Implement `crates/renderer/src/portal_location.rs`: create an
      `ashpd::desktop::location::LocationProxy` session requesting `Accuracy::City`
      (research.md R4), call `Start` wrapped in a 5-second timeout (research.md R6), and on
      success convert the reading to spec 1's `Location` (validated via `Location::new`),
      writing `automatic_location`/`automatic_status: Resolved` back through the v2
      `LocationConfigEntry` (research.md R3/R4/R6; depends on T003, T001)
- [X] T011 [US1] Wire `portal_location.rs`'s resolution future into `wallpaperd.rs`'s existing
      single `calloop` loop, advanced the same `internal_executor(false)` + `block_on` way
      `dbus_service.rs` already is (research.md R5; depends on T010) — in
      `crates/renderer/src/bin/wallpaperd.rs`
- [X] T012 [US1] Amend `scheduler_bridge.rs` to call `effective_location()` (T005) instead of
      reading `LocationSource.location` directly, so a resolved automatic value actually reaches
      spec 1's `ValidatedPack::query` (plan.md Cross-Spec Dependency; depends on T005) — in
      `crates/renderer/src/scheduler_bridge.rs`

**Checkpoint**: On a machine with a working portal *and* GeoClue backend with location services
enabled, `wallpaperctl location auto` plus a running `wallpaperd` produces a correctly
solar-scheduled pack with zero typed coordinates. **Not independently verifiable end-to-end in
this project's own dev environment** (no GeoClue installed, research.md R2) — every component up
to the portal boundary is real, live-spiked, and unit-tested; the final resolved-value hop is
documented as an honest gap in quickstart.md, same posture as spec 3's own untested branches.

---

## Phase 4: User Story 2 - Nothing Breaks When the Portal Isn't There or Says No (Priority: P1) 🎯 MVP

**Goal**: Every portal/backend/permission failure degrades cleanly — no crash, no hang, no stuck
retry loop.

**Independent Test**: On a system with no portal service reachable (or permission declined),
enable automatic location and confirm: no crash/hang, a clear distinguishing status, and
existing manual/clock-anchored packs keep working exactly as before.

### Tests for User Story 2

- [X] T013 [P] [US2] Unit test: a portal error/timeout/absence maps to
      `AutomaticStatus::Unavailable { reason }` with the specific error string preserved
      verbatim — including this project's own live-observed `"Location services disabled"`
      string (research.md R1) as a literal test case, not a generic placeholder — in
      `crates/renderer/src/portal_location.rs`
- [X] T014 [P] [US2] Unit test extending T005's coverage: `effective_location()` with
      `mode: Automatic` and `automatic_status: Unavailable` falls back to `location` if present,
      else `None` — no panic, no new failure mode invented (spec.md FR-005) in
      `crates/renderer/src/config.rs`
- [X] T015 [P] [US2] Unit test: repeated resolution failures back off exponentially (30s start,
      5-minute cap), never a tight loop (research.md R6) in
      `crates/renderer/src/portal_location.rs`

### Implementation for User Story 2

- [X] T016 [US2] Implement the failure/timeout path in `portal_location.rs`: map every failure
      mode (portal absent, backend absent, permission declined, mid-session error, the 5s
      timeout) to `AutomaticStatus::Unavailable { reason }`, written back immediately with no
      grace period (spec.md FR-005 Clarifications; depends on T010)
- [X] T017 [US2] Implement the exponential-backoff retry timer (research.md R6) as a `calloop`
      timer alongside the resolution future, so a transient failure recovers automatically
      without user action (depends on T011, T016)
- [X] T018 [US2] Manual QA: run quickstart.md's "Manual smoke check" against this project's real
      COSMIC session once T008–T017 land, confirming `wallpaperctl location get` reports
      `status: unavailable (Location services disabled)` and manual/clock-anchored packs remain
      unaffected (spec.md US2, quickstart.md; depends on T009, T016, T017)

**Checkpoint**: User Stories 1–2 (both P1) complete — this spec's MVP. This dev environment can
fully validate US2 live end-to-end (research.md R1/R2 already demonstrated the exact failure
path before any code existed); US1's full success path needs a machine with GeoClue installed
(documented honestly in quickstart.md, not hidden).

---

## Phase 5: User Story 3 - Schedule Follows You When Your Location Actually Changes (Priority: P2)

**Goal**: A location update pushed by the portal while automatic mode is active re-evaluates
affected schedules without a restart.

**Independent Test**: With automatic location active and a schedule running, deliver an updated
location and confirm the schedule recomputes within the existing reaction bound, no restart.

### Tests for User Story 3

- [X] T019 [P] [US3] Unit test: a `LocationUpdated` value distinct from the currently-stored
      `automatic_location` triggers `Coalescer::record_change` for every output currently
      scheduling in automatic mode (spec.md US3 Scenario 1, FR-006) in
      `crates/renderer/src/portal_location.rs`
- [X] T020 [P] [US3] Unit test: several rapid `LocationUpdated` signals collapse to a single
      re-evaluation via the existing `Coalescer` (spec 3 FR-014, spec.md US3 Scenario 2) in
      `crates/renderer/src/config.rs`

### Implementation for User Story 3

- [X] T021 [US3] Subscribe to `ashpd`'s `receive_location_updated()` stream for the lifetime of
      automatic mode (not just the one-shot initial `Start` resolution), routing every distinct
      value through the same write-back and coalescing path T010/T016 already established —
      in `crates/renderer/src/portal_location.rs` (research.md R5, spec.md FR-006; depends on
      T010, T011)
- [X] T022 [US3] Confirm/adjust `wallpaperd.rs`'s event-loop wiring so the ongoing
      `LocationUpdated` stream (not just the initial resolution future) is polled every tick
      alongside the D-Bus server and existing timers (depends on T011, T021)

**Checkpoint**: User Stories 1–3 functional. Live-update behavior is structurally complete and
unit-tested; full live verification needs the same GeoClue-backed machine US1 does — a location
*change* can't be observed without a working initial resolution.

---

## Phase 6: User Story 4 - See and Control Which Mode Is Active (Priority: P2)

**Goal**: Query current mode/status/coordinates at any time; switch back to manual with no
re-entry.

**Independent Test**: Enable automatic location, query which mode is active, disable it, and
confirm the daemon reverts to the previously-stored manual location without re-entry.

### Tests for User Story 4

- [X] T023 [P] [US4] Unit test: `location manual` sets `mode: Manual` only, leaves `location`
      untouched, and handles the "no manual value was ever stored" case cleanly (spec.md US4
      Scenarios 2–3, contracts/wallpaperctl-location-cli.md) in
      `crates/wallpaperctl/src/commands/location.rs`
- [X] T024 [P] [US4] Unit test: `location get`'s human and `--json` output both report `mode`,
      `status`, and the effective location for every `(mode, status)` combination (spec.md US4
      Scenario 1, SC-004) in `crates/wallpaperctl/src/commands/location.rs`
- [X] T025 [P] [US4] Regression test: `location set` now also writes `mode: Manual`
      (research.md R8) and `location clear` continues to leave `mode` untouched (research.md
      R7) — both documented deliberate side effects, not accidental scope creep — in
      `crates/wallpaperctl/src/commands/location.rs`

### Implementation for User Story 4

- [X] T026 [US4] Implement the `location manual` subcommand (depends on T002)
- [X] T027 [US4] Extend `location get`'s human/JSON output with `mode`/`status`/
      `manual_location`/`automatic_location` fields per
      contracts/wallpaperctl-location-cli.md (depends on T002)
- [X] T028 [US4] Update `location set` to also write `mode: Manual` (research.md R8); confirm
      `location clear`'s existing behavior needs no code change (research.md R7) (depends on
      T002)
- [X] T029 [US4] Wire `location manual` into `main.rs`'s dispatch alongside the existing
      `location` subcommand group (depends on T009, T026)

**Checkpoint**: All four user stories functional and tested. The full CLI surface (`auto`/
`manual`/`get`/`set`/`clear`) works end-to-end whether or not a daemon happens to be running
(FR-012) — Stories 1–4 collectively close out this spec.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Close out the spec's success criteria and hand off a stable, documented feature.

- [X] T030 [P] Verify strong test coverage via `cargo llvm-cov` on `portal_location.rs`,
      `config.rs` (both crates), and `commands/location.rs`'s new/changed paths; add tests to
      close any real gap
- [X] T031 [P] Add rustdoc comments to every new public item; verify with
      `RUSTFLAGS="-W missing_docs" cargo build --workspace` (spec 4 T047's precedent — this
      full-workspace check has caught real cross-crate gaps before)
- [X] T032 [P] Update `crates/renderer/README.md`'s "What's simplified or not implemented"
      section: remove automatic location from the gap list, and add the honest caveat that
      full success-path verification needs a GeoClue-backed machine this dev environment
      doesn't have (research.md R2)
- [X] T033 [P] Update `crates/wallpaperctl/README.md` documenting the two new `location`
      subcommands and `get`'s extended output
- [X] T034 Run quickstart.md end-to-end (the automated `cargo test` portion, plus the manual
      smoke check against this project's real COSMIC session) and fix any drift between the
      doc and actual behavior

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — start immediately.
- **Foundational (Phase 2)**: Depends on Setup. Blocks all user stories.
- **User Story 1 (Phase 3)**: Depends only on Foundational — the first story to introduce
  `portal_location.rs` and the `scheduler_bridge.rs` amendment.
- **User Story 2 (Phase 4)**: Depends on Foundational **and** US1's `portal_location.rs`/
  `wallpaperd.rs` wiring (T010, T011) — it extends the same resolution attempt with its failure
  path, rather than being a parallel independent module (spec.md's own US1/US2 acceptance
  scenarios are the two outcomes of one resolution attempt, not two separate mechanisms).
- **User Story 3 (Phase 5)**: Depends on Foundational and US1's `portal_location.rs`/
  `wallpaperd.rs` wiring (T010, T011) — extends the one-shot resolution into an ongoing
  subscription.
- **User Story 4 (Phase 6)**: Depends only on Foundational (T002) — fully independent of
  Stories 1–3's daemon-side code; it's pure `cosmic-config` read/write, same posture as spec 4's
  original `get`/`set`/`clear`. Can be implemented in parallel with Stories 1–3 if staffed
  separately.
- **Polish (Phase 7)**: Depends on all four user stories being complete.

### Parallel Opportunities

- T002 and T003 (Foundational) — different files (though their shapes must match exactly — see
  T003's note).
- T006 and T007 (US1 tests) — different files.
- T013, T014, and T015 (US2 tests) — two different files, independent scenarios.
- T019 and T020 (US3 tests) — different files.
- T023, T024, and T025 (US4 tests) — same file but independent scenarios.
- T030–T033 (Polish) — different files.
- **User Story 4 can proceed fully in parallel with Stories 1–3** once Foundational is done — it
  shares no files with `portal_location.rs`/`wallpaperd.rs`/`scheduler_bridge.rs`.
- Stories 1, 2, and 3 are **sequential-in-practice** despite being separate phases: each extends
  the same `portal_location.rs` module and `wallpaperd.rs` wiring the previous one introduced,
  unlike this project's typical fully-parallel story structure (e.g. spec 4's Stories 1/2/3/7).

### Sequential-in-Practice Files

`crates/renderer/src/portal_location.rs` is built up across Stories 1, 2, and 3 (T010→T016/T017
→T021) — coordinate edits there even though each task is nominally story-scoped.
`crates/wallpaperctl/src/main.rs` is touched by both US1 (T009) and US4 (T029)'s wiring tasks.
`crates/wallpaperctl/src/commands/location.rs` is extended by both US1 (T008) and US4
(T026–T028).

---

## Parallel Example: Foundational Phase

```bash
# After Setup (T001), launch together:
Task: "Define LocationMode/AutomaticStatus/LocationConfigEntry v2 in crates/wallpaperctl/src/config.rs"
Task: "Mirror the same v2 shape as LocationSource in crates/renderer/src/config.rs"
```

## Parallel Example: User Story 4 alongside User Story 1

```bash
# Once Foundational is done, these have no shared files:
Task: "Implement location manual/get/set extensions (US4) in crates/wallpaperctl/src/commands/location.rs"
Task: "Implement portal_location.rs's resolution path (US1) in crates/renderer/src/portal_location.rs"
```

---

## Implementation Strategy

### MVP First (User Stories 1 and 2)

1. Phase 1: Setup
2. Phase 2: Foundational (blocks everything)
3. Phase 3: User Story 1 — the resolution success path
4. Phase 4: User Story 2 — the resolution failure/degrade path (extends US1's module, not
   parallel to it)
5. **STOP and VALIDATE**: `cargo test` green across both crates; quickstart.md's manual smoke
   check (T018) confirms the degrade path live against this project's real COSMIC session
6. This alone is the spec's MVP: automatic location works when the underlying stack supports it,
   and fails safe when it doesn't — both P1 stories, spec.md's own priority ordering

### Incremental Delivery

1. Setup + Foundational → schema/resolution-rule ready
2. Add User Story 1 → validate independently (live-verifiable only up to the portal boundary in
   this dev environment, per research.md R2)
3. Add User Story 2 → validate independently → **MVP** (both P1 stories done, fully live-testable
   here)
4. Add User Story 3 → validate independently (structurally; full live verification needs US1's
   same missing GeoClue dependency)
5. Add User Story 4 → validate independently — can be developed in parallel with 1–3 if staffed
   separately (no shared files)
6. Polish → coverage, docs, quickstart parity

---

## Notes

- [P] tasks touch different files, or independent scenarios within the same file, with no unmet
  dependency.
- Unlike this project's earlier CLI-heavy specs (4, 5), Stories 1–3 here are **not** mutually
  independent in implementation, only in what each one's acceptance scenarios exercise — they
  build up one shared module (`portal_location.rs`) incrementally. Story 4 is the one genuinely
  parallel-safe story, sharing no files with the daemon-side work.
- Full end-to-end validation of Stories 1 and 3's success path requires a machine with GeoClue2
  installed and location services enabled — not available in this project's own dev environment
  (research.md R2). This is a documented, honest gap, not a task this list can close on its own;
  see quickstart.md's "What 'done' looks like" section.
- The v1→v2 schema migration (T004) needs no hand-written migration function — verified against
  `cosmic-config`'s actual source (research.md R7) — don't add one; the task is to *verify* the
  built-in fallback, not implement new migration code.
- `unwrap()`/`expect()` outside `#[cfg(test)]` remains prohibited (constitution Principle VIII,
  same CI lint gate as every other crate in this workspace) — every portal/D-Bus failure path
  above must return a typed result, never panic.
- Commit after each task or logical group.
