---

description: "Task list template for feature implementation"
---

# Tasks: V1 Completion — GUI, Starter Packs, IP Fallback, and Gap Closure

**Input**: Design documents from `/specs/007-v1-completion/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/wallpaper-ipc-crate.md,
contracts/location-config-schema-v3.md, contracts/pack-registry-origin.md,
contracts/gui-application.md, quickstart.md

**Tests**: Included, per plan.md's Technical Context split. Schema/migration/`effective_location()`/
registry logic and the mock hotplug harness are real `cargo test` coverage. STUN/`maxminddb`
happy-path resolution and the GUI's actual rendered appearance remain manual-QA items against a
real COSMIC session, same posture as specs 3/6's own Wayland/portal code.

**Organization**: Tasks are grouped by user story (spec.md), in **priority order** (P1, P1, P2,
P3) rather than spec.md's narrative order (US1, US2, US3, US4) — spec.md itself notes US1 and
US4 are equal-priority P1 stories despite US4 being listed last, so this task list implements
them consecutively for a real MVP checkpoint.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: Which user story this task belongs to (US1–US4)
- File paths are relative to the repository root

## Path Conventions

Two new workspace crates (`wallpaper-ipc`, `wallpaper-settings`), amendments to three
already-shipped crates (`renderer`, `wallpaperctl`, `pack-loader`), a new maintainer-only tool
(`tools/generate-starter-pack`), its checked-in output (`assets/starter-pack/`), and amendments
to spec 5's planned (not yet implemented) `packaging/` artifacts.

---

## Phase 1: Setup

**Purpose**: Workspace scaffolding for the two new crates and every new dependency this spec
needs.

- [X] T001 Add `crates/wallpaper-ipc` as a new workspace member in the root `Cargo.toml`, with an
      empty crate depending on `serde`, `cosmic-config` (git, `features = ["macro"]`, no
      `"calloop"` — research.md R2), `zbus` (`features = ["async-io"]`), and path dependencies on
      `schedule-engine` and `pack-loader` (contracts/wallpaper-ipc-crate.md)
- [X] T002 [P] Add `crates/wallpaper-settings` as a new workspace member, empty crate depending on
      `libcosmic` (git, `pop-os/libcosmic`, same pin as `cosmic-config` — research.md R1),
      `wallpaper-ipc`, `schedule-engine`, and `pack-loader` — explicitly NOT `renderer`
      (plan.md Constitution Check finding 1)
- [X] T003 [P] Add `tools/generate-starter-pack` as a new workspace member, empty binary depending
      on `image` (already a workspace dependency) — research.md R5
- [X] T004 [P] Add `maxminddb` (0.30.x) and `stunclient` (0.4.2) to `crates/renderer/Cargo.toml`,
      plus a `wallpaper-ipc` path dependency (research.md R3/R4)
- [X] T005 [P] Add `wayland-server` (0.31.x) as a `dev-dependencies`-only entry in
      `crates/renderer/Cargo.toml` (research.md R7)
- [X] T006 [P] Add a `wallpaper-ipc` path dependency to `crates/wallpaperctl/Cargo.toml`
- [X] T007 [P] Add `.github/workflows/wallpaper-ipc-ci.yml` and
      `.github/workflows/wallpaper-settings-ci.yml` (`cargo test` + `cargo clippy`), matching
      this project's existing per-crate CI pattern

**Checkpoint**: Both new crates compile empty; every new dependency resolves; CI is defined for
the new crates.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Extract the shared schema/D-Bus-client crate (research.md R2) and land the two
schema extensions (v3 location config, crossfade duration) it carries. Blocks US1 and US3
directly; US2 and US4 do **not** depend on this phase (see Dependencies below) and may proceed in
parallel.

**⚠️ CRITICAL**: Blocks Phases 3 (US1) and 6 (US3).

- [X] T008 Move `RendererConfig`/`OutputAssignment`/`OutputId` from `crates/renderer/src/output.rs`
      into `crates/wallpaper-ipc/src/renderer_config.rs`, unchanged shape (contracts/
      wallpaper-ipc-crate.md; depends on T001)
- [X] T009 Move `LocationConfigEntry` into `crates/wallpaper-ipc/src/location_config.rs`, rename
      `AutomaticStatus`→`ResolutionStatus` (research.md R9), bump to v3, add `ip_location`/
      `ip_status` fields (data-model.md, contracts/location-config-schema-v3.md; depends on T001,
      T008)
- [X] T010 [P] Add `crossfade_duration_secs: u32` (default `45`) to `RendererConfig` in
      `wallpaper-ipc` (plan.md Constitution Check finding 3, data-model.md; depends on T008)
- [X] T011 Implement the three-way `effective_location()` in `wallpaper-ipc` with unit tests
      covering all nine `(mode, resolution-state)` combinations (data-model.md; depends on T009)
- [X] T012 Move `DbusClient` from `crates/wallpaperctl/src/dbus_client.rs` into
      `crates/wallpaper-ipc/src/dbus_client.rs`, unchanged protocol (contracts/
      wallpaperd-dbus-interface.md, spec 4, unchanged; depends on T001)
- [X] T013 [P] Regression test: a hand-written v2 RON `LocationConfig` entry loads via the new v3
      struct as `mode: Manual` (unchanged), `ip_location: None`, `ip_status: Unresolved` —
      confirms `cosmic-config`'s per-key fallback (spec 6 research.md R7) still works across the
      `AutomaticStatus`→`ResolutionStatus` rename (research.md R9; depends on T009)
- [X] T014 Amend `crates/renderer/src/config.rs` and `src/output.rs` to re-export
      `wallpaper-ipc`'s types instead of defining their own; delete the old independently-defined
      structs (depends on T008, T009)
- [X] T015 [P] Update `crates/renderer/src/scheduler_bridge.rs`'s `effective_location()` import
      path to `wallpaper-ipc` (no logic change — depends on T011, T014)
- [X] T016 Amend `crates/renderer/src/surface.rs`'s three `CROSSFADE_DURATION` call sites to read
      `RendererConfig.crossfade_duration_secs` instead of the constant, applied live via the
      existing config-watch mechanism (plan.md finding 3; depends on T010, T014)
- [X] T017 Amend `crates/wallpaperctl/src/config.rs` to re-export `wallpaper-ipc`'s types instead
      of defining its own; delete `crates/wallpaperctl/src/dbus_client.rs`, re-exporting from
      `wallpaper-ipc` instead (depends on T009, T012, T006)
- [X] T018 Regression test closing research.md R2's own real-bug precedent: a `RendererConfig`
      value written through `wallpaper-ipc` by a simulated `wallpaperctl` write path and read back
      through a simulated `wallpaperd` load path round-trips byte-for-byte — the exact class of
      bug now structurally prevented by a single shared type (depends on T014, T017)

**Checkpoint**: `wallpaper-ipc` is the sole source of truth for shared schema/D-Bus-client code;
`renderer`/`wallpaperctl` both compile against it with **zero behavior change** to any existing
test; crossfade duration is genuinely configurable for the first time.

---

## Phase 3: User Story 1 - Configure Everything Without the CLI (Priority: P1) 🎯 MVP

**Goal**: A standalone GUI covering pack browsing, assignment, location, timeline, and crossfade
duration — everything the CLI already does.

**Independent Test**: Complete a full "browse a pack, assign it, set a location, see it on the
timeline" flow entirely inside the GUI, with no terminal command run at any point.

### Tests for User Story 1

- [X] T019 [P] [US1] Unit test: Packs page's view-state maps a `pack-loader::Registry` listing
      into display rows with preview paths, independent of `libcosmic` rendering (spec.md
      Acceptance Scenario 1) in `crates/wallpaper-settings/src/pages/packs.rs`
- [X] T020 [P] [US1] Unit test: Assignment page writes the identical `RendererConfig.overrides`
      shape `wallpaperctl assign` does (spec.md Acceptance Scenario 2) in
      `crates/wallpaper-settings/src/pages/assignment.rs`
- [X] T021 [P] [US1] Unit test: Location page's mode switch writes the identical
      `LocationConfigEntry` shape `wallpaperctl location` does, for all three modes (spec.md
      Acceptance Scenario 3) in `crates/wallpaper-settings/src/pages/location.rs`
- [X] T022 [P] [US1] Unit test: Timeline page's data mapping matches
      `wallpaper-ipc::DbusClient`'s query response shape 1:1, including the "daemon unreachable"
      state (spec.md Acceptance Scenario 4) in `crates/wallpaper-settings/src/pages/timeline.rs`
- [X] T023 [P] [US1] Unit test: Crossfade page writes `RendererConfig.crossfade_duration_secs`
      (spec.md Acceptance Scenario 5) in `crates/wallpaper-settings/src/pages/crossfade.rs`

### Implementation for User Story 1

- [X] T024 [US1] Implement the `cosmic::Application` skeleton — window, sidebar navigation between
      the five pages — in `crates/wallpaper-settings/src/app.rs` + `src/main.rs` (research.md R1;
      depends on T002)
- [X] T025 [US1] Implement the Packs page (browse + preview) reading `pack-loader::Registry`
      (contracts/gui-application.md; depends on T024)
- [X] T026 [US1] Implement the Assignment page (per-output / same-everywhere) writing
      `wallpaper-ipc::RendererConfig` (depends on T024, T014)
- [X] T027 [US1] Implement the Location page (manual/automatic/IP-geolocation mode switch, plus
      the STUN-disclosure copy FR-014 requires) writing `wallpaper-ipc::LocationConfigEntry`
      (depends on T024, T009)
- [X] T028 [US1] Implement the Timeline page using `wallpaper-ipc::DbusClient` (depends on T024,
      T012, T017), showing the same "daemon unreachable" fallback `wallpaperctl query` uses
- [X] T029 [US1] Implement the Crossfade page writing `RendererConfig.crossfade_duration_secs`
      (depends on T024, T016)

**Checkpoint**: GUI functional end-to-end against real `cosmic-config`/D-Bus state, interchangeable
with the CLI (FR-007) **by construction** — both link the same `wallpaper-ipc` types, not by
convention or a regression test alone.

---

## Phase 4: User Story 4 - Harden and Verify Already-Shipped Specs Before Release (Priority: P1) 🎯 MVP

**Goal**: Close spec 3's mock-hotplug gap, spec 6's GeoClue packaging gap, and record the
remaining manual-QA-only gaps with dated evidence.

**Independent Test**: Run the automated suite with no physical display attached and confirm
output connect/disconnect/resize are all exercised; confirm dated evidence exists for each
remaining manual-only gap.

### Tests for User Story 4

- [X] T030 [P] [US4] Mock hotplug harness test: an output connect event via the `wayland-server`
      double reaches the same real SCTK client code path a physical compositor triggers (spec.md
      Acceptance Scenario 1) in `crates/renderer/tests/hotplug_mock.rs` (research.md R7)
- [X] T031 [P] [US4] Mock hotplug harness test: an output disconnect event — previously entirely
      untested on any hardware this project has access to — correctly tears down state without
      panicking (closes spec 3 tasks.md T043's disconnect gap) in
      `crates/renderer/tests/hotplug_mock.rs`
- [X] T032 [P] [US4] Mock hotplug harness test: an output resize/scale-change event correctly
      triggers reconfiguration (closes spec 3's previously-untested resize branch) in
      `crates/renderer/tests/hotplug_mock.rs`

### Implementation for User Story 4

- [X] T033 [US4] Implement the minimal `wayland-server`-backed fake compositor
      (`wl_registry`/`wl_output`/`xdg_output`/`wp_fractional_scale_v1`) driving the real,
      unmodified SCTK client code in `crates/renderer/src/surface.rs` over an in-memory
      socketpair (research.md R7; depends on T005)
- [X] T034 [US4] Add `recommends = "geoclue-2.0"` to `crates/renderer/Cargo.toml`'s
      `[package.metadata.deb]` section (research.md R8, closes spec 6 research.md R2's
      previously-unapplied suggestion)
- [X] T035 [US4] Manual QA, executed and dated: spec 5's real install/autostart/crash-restart
      lifecycle on a live session (spec.md FR-019) — record dated results in a new
      `docs/manual-qa-log.md`
- [X] T036 [US4] Manual QA, executed and dated: spec 6's full automatic-location success path
      against a real GeoClue backend with location services enabled (spec.md FR-019) — same
      recording location as T035

**Checkpoint**: **US1 + US4 together are this spec's MVP** (both P1, per spec.md's own framing —
neither matters alone for "confidently shippable"). Output hotplug/resize/disconnect now has real
automated coverage; the GeoClue soft-dependency is packaged; remaining manual-QA-only gaps are
closed with dated evidence, not left as an open caveat indefinitely.

---

## Phase 5: User Story 2 - Get Something Beautiful With Zero Configuration (Priority: P2)

**Goal**: A bundled starter pack, automatically registered and assigned on fresh install, whose
explicit removal is permanent.

**Independent Test**: On a fresh install with no prior `wallpaperctl` commands ever run, confirm
a bundled starter pack is already registered, assigned, and actively scheduled.

### Tests for User Story 2

- [X] T037 [P] [US2] Unit test: `PackRegistryEntry.origin` defaults to `User`; a pre-existing
      (v-old) registry entry loads unchanged (research.md R6) in
      `crates/pack-loader/src/registry.rs`
- [X] T038 [P] [US2] Unit test: removing a `Package`-origin entry appends its source to
      `RemovedStarterPacks`; removing a `User`-origin entry does not (spec.md FR-010) in
      `crates/pack-loader/src/registry.rs`
- [X] T039 [P] [US2] Unit test: a simulated `postinst` re-run skips registering a starter pack
      already listed in `RemovedStarterPacks` (spec.md US2 Scenario 2) in
      `crates/pack-loader/src/registry.rs`
- [X] T040 [P] [US2] Unit test: a starter pack's default assignment never overrides an existing
      user assignment (spec.md US2 Scenario 3) in `crates/renderer/src/scheduler_bridge.rs`

### Implementation for User Story 2

- [X] T041 [US2] Add `origin: PackOrigin` to `PackRegistryEntry` and a new, separate
      `RemovedStarterPacks` schema in `crates/pack-loader/src/registry.rs` (data-model.md,
      contracts/pack-registry-origin.md)
- [X] T042 [US2] Implement `tools/generate-starter-pack`: produce a fixed gradient/sky-art image
      sequence spanning a solar-anchored day cycle (research.md R5; depends on T003)
- [X] T043 [US2] Run the generator once; commit its static output
      (`assets/starter-pack/*.png` + `manifest.toml`) to the repository (depends on T042)
- [X] T044 [US2] Amend spec 5's `packaging/debian/postinst` to register `assets/starter-pack/`
      with `origin: Package`, checking `RemovedStarterPacks` first (contracts/
      pack-registry-origin.md; depends on T041, T043)
- [X] T045 [US2] Amend `wallpaperctl remove` to append to `RemovedStarterPacks` when removing a
      `Package`-origin entry (depends on T041)

**Checkpoint**: Fresh installs show a real, actively-scheduled wallpaper immediately with zero
configuration; an explicit removal is permanent across package upgrades.

---

## Phase 6: User Story 3 - Get Automatic Scheduling Without a Location Portal (Priority: P3)

**Goal**: An explicit, opt-in IP-geolocation location mode using a bundled offline database.

**Independent Test**: With no manual location and no working portal resolution, enable IP-based
location and confirm a solar-anchored pack schedules against the resolved approximate location.

### Tests for User Story 3

- [X] T046 [P] [US3] Unit test: `maxminddb` lookup against a small fixture `.mmdb` resolves known
      test IPs to expected coordinates, fully offline — no real STUN/network call in `cargo test`
      (research.md R3) in `crates/renderer/src/ip_geolocation.rs`
- [X] T047 [P] [US3] Unit test: STUN failure/timeout maps to `ResolutionStatus::Unavailable` with
      a specific reason string, never panics (research.md R4) in
      `crates/renderer/src/ip_geolocation.rs`
- [X] T048 [P] [US3] Unit test: the public-IP cache respects its 24-hour TTL — a second lookup
      within the window reuses the cached value, no repeat STUN call (research.md R4) in
      `crates/renderer/src/ip_geolocation.rs`
- [X] T049 [P] [US3] Unit test extending T011's coverage: `effective_location()` with
      `mode: IpGeolocation` falls back to `location` then `None` when unresolved/unavailable
      (spec.md FR-015) in `crates/wallpaper-ipc/src/location_config.rs`

### Implementation for User Story 3

- [X] T050 [US3] Implement `crates/renderer/src/ip_geolocation.rs`: STUN-based public-IP discovery
      (`stunclient`) with a 24-hour in-memory cache, `maxminddb` lookup against the bundled
      `.mmdb`, writing `ip_location`/`ip_status` back through `wallpaper-ipc::LocationConfigEntry`
      (research.md R3/R4; depends on T009, T012, T004)
- [X] T051 [US3] Package the bundled DB-IP Lite `.mmdb` database as a static asset alongside
      `wallpaperd` (spec 5's packaging, amended) — document the download-and-verify step in the
      release process, not a runtime dependency (research.md R3)
- [X] T052 [US3] Implement the `wallpaperctl location ip` subcommand (sets `mode: IpGeolocation`
      only) in `crates/wallpaperctl/src/commands/location.rs` (contracts/
      location-config-schema-v3.md; depends on T017)
- [X] T053 [US3] Wire `location ip` into `main.rs`'s dispatch (depends on T052)
- [X] T054 [US3] Confirm the GUI's Location page (T027) and CLI's `location get` both surface the
      STUN-disclosure copy FR-014 requires before a user opts in (depends on T027, T052)

**Checkpoint**: All four user stories functional. IP-geolocation's happy path is live-testable on
this dev machine (real network access, confirmed elsewhere in this project); its degrade path is
testable by disconnecting network access.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Close out the spec's success criteria and hand off a stable, documented feature set.

- [X] T055 [P] Verify strong test coverage via `cargo llvm-cov --workspace`; add tests to close
      any real gap across the two new crates and the three amended ones
- [X] T056 [P] Add rustdoc comments to every new public item; verify with
      `RUSTFLAGS="-W missing_docs" cargo build --workspace` (this full-workspace check has caught
      real cross-crate gaps before, per this project's own history)
- [X] T057 [P] Add `crates/wallpaper-ipc/README.md` and `crates/wallpaper-settings/README.md`
      documenting scope, matching this project's existing per-crate README convention
- [X] T058 [P] Update `crates/renderer/README.md`'s "What's simplified or not implemented"
      section: remove the mock-hotplug and non-configurable-crossfade gaps; add IP-geolocation's
      STUN caveat plainly (plan.md finding 2)
- [X] T059 Run quickstart.md end-to-end (automated `cargo test --workspace`, plus all three manual
      smoke checks) and fix any drift between the doc and actual behavior

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — start immediately.
- **Foundational (Phase 2)**: Depends on Setup. Blocks **US1 (Phase 3)** and **US3 (Phase 6)**
  only — see the exception below.
- **User Story 1 (Phase 3)**: Depends on Foundational (needs `wallpaper-ipc`'s types to exist).
- **User Story 4 (Phase 4)**: Depends **only on Setup** (specifically T005's `wayland-server` dev-
  dependency for T033) — **does not depend on Foundational at all**. Every US4 task can proceed
  in parallel with Foundational/US1/US2/US3 once Setup is done. Flagged explicitly, matching this
  project's established practice of calling out a story's real dependency shape rather than
  assuming uniform parallelism (spec 6 tasks.md made the same kind of call for its own US4).
- **User Story 2 (Phase 5)**: Depends only on Setup (T003 for the generator tool) — independent of
  Foundational, US1, US3, and US4.
- **User Story 3 (Phase 6)**: Depends on Foundational (needs `wallpaper-ipc`'s v3 schema) and T004
  (Setup's `maxminddb`/`stunclient` dependencies).
- **Polish (Phase 7)**: Depends on all four user stories being complete.

### Parallel Opportunities

- T002–T007 (Setup) — different files, all depend only on T001 or nothing.
- T010, T012, T013 (Foundational) — different files/independent scenarios.
- T019–T023 (US1 tests) — different files.
- T030–T032 (US4 tests) — same file, independent scenarios.
- T037–T040 (US2 tests) — same file, independent scenarios (T040 in a different file).
- T046–T049 (US3 tests) — mostly the same file, independent scenarios (T049 in a different file).
- T055–T058 (Polish) — different files.
- **User Stories 2 and 4 can proceed fully in parallel with Foundational and each other** — this
  is the big scheduling win in this spec's shape: a team isn't blocked on the `wallpaper-ipc`
  extraction to start either the starter pack or the hardening work.

### Sequential-in-Practice Files

`crates/wallpaper-ipc/src/location_config.rs` is touched by Foundational (T009, T011) and again
by US3 (T049, extending T011's test coverage) — coordinate edits there. `crates/renderer/src/
config.rs` and `crates/wallpaperctl/src/config.rs` are each touched once during Foundational
(T014, T017) and should be stable afterward for every later phase. `crates/wallpaper-settings/
src/pages/location.rs` is built in US1 (T027) and revisited in US3 (T054) for the STUN-disclosure
copy.

---

## Parallel Example: Setup Phase

```bash
# After T001 (wallpaper-ipc skeleton), launch together:
Task: "Add wallpaper-settings crate skeleton"
Task: "Add tools/generate-starter-pack skeleton"
Task: "Add maxminddb/stunclient/wallpaper-ipc deps to crates/renderer/Cargo.toml"
Task: "Add wayland-server dev-dependency to crates/renderer/Cargo.toml"
Task: "Add wallpaper-ipc dep to crates/wallpaperctl/Cargo.toml"
```

## Parallel Example: User Stories 2 and 4 alongside Foundational

```bash
# Once Setup is done, these need no Foundational work at all:
Task: "Implement tools/generate-starter-pack (US2)"
Task: "Implement the wayland-server mock hotplug harness (US4)"
Task: "Add recommends = geoclue-2.0 packaging metadata (US4)"
```

---

## Implementation Strategy

### MVP First (User Stories 1 and 4)

1. Phase 1: Setup
2. Phase 2: Foundational (blocks US1/US3 — but see Phase 4's exception, US4 can start in
   parallel with this phase)
3. Phase 3: User Story 1 — the GUI
4. Phase 4: User Story 4 — hardening/gap closure (may already be substantially done in parallel
   with Phases 2–3, per its dependency exception above)
5. **STOP and VALIDATE**: `cargo test --workspace` green; quickstart.md's GUI manual smoke check
   and the mock-hotplug automated suite both pass
6. This is the spec's MVP per spec.md's own P1 framing: a working settings GUI, plus real,
   automated hotplug/resize/disconnect coverage and a closed GeoClue packaging gap

### Incremental Delivery

1. Setup (+ Foundational, in parallel with User Story 4 starting immediately)
2. User Story 1 → validate independently
3. User Story 4 → validate independently → **MVP** (both P1 stories done)
4. User Story 2 → validate independently (fully parallel-safe, could actually land earlier)
5. User Story 3 → validate independently (needs Foundational's v3 schema)
6. Polish → coverage, docs, quickstart parity

---

## Notes

- [P] tasks touch different files, or independent scenarios within the same file, with no unmet
  dependency.
- This spec's shape is unusual for this project: **two** stories (US2, US4) are essentially fully
  independent of the Foundational phase, not just of each other — don't assume every story needs
  `wallpaper-ipc` to exist first just because Foundational conventionally blocks everything.
- The `wallpaper-ipc` extraction (T008–T018) is a refactor of already-shipped, tested code (specs
  3/4) — every existing test in `crates/renderer` and `crates/wallpaperctl` MUST still pass
  unchanged after T014/T017; a passing existing test suite is the acceptance bar for "the
  refactor didn't change behavior," not a new test written from scratch.
- Two findings from plan.md are worth re-reading before starting T050 (IP-geolocation) and T016
  (crossfade): the STUN public-IP-discovery tradeoff (research.md R4, the user has confirmed
  they're comfortable with this) and the crossfade-duration plumbing gap (finding 3) — both are
  real, not hypothetical, corrections to what was previously believed about already-shipped code.
- `unwrap()`/`expect()` outside `#[cfg(test)]` remains prohibited (constitution Principle VIII,
  same CI lint gate as every other crate in this workspace).
- Commit after each task or logical group.
