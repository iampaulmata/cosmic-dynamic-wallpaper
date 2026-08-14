---

description: "Task list template for feature implementation"
---

# Tasks: Wallpaper Renderer

**Input**: Design documents from `/specs/003-wallpaper-renderer/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/renderer-config-schema.md, quickstart.md. Phase 10 below additionally depends on spec 4's `specs/004-cli-control-surface/contracts/location-config-schema.md` and `.../wallpaperd-dbus-interface.md` (Amendment 2026-08-13).

**Tests**: Partial. plan.md's Technical Context and research.md R6 commit this spec to a
two-tier strategy: the pure `assignment.rs`/state-machine logic is `cargo test`-able like
specs 1–2 and gets real test tasks below; the Wayland/GPU-touching code (`output.rs`,
`surface.rs`, `gpu.rs`, `crossfade.rs`) is validated via quickstart.md's manual QA checklist
plus an exploratory CI smoke test, per the constitution's own explicit allowance for that gap
— it does not get `cargo test` unit-test tasks here.

**Organization**: Tasks are grouped by user story (spec.md) to enable independent
implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: Which user story this task belongs to (US1–US6)
- File paths are relative to the repository root

## Path Conventions

Third library-plus-binary crate in the workspace specs 1–2 established (plan.md Structure
Decision): `crates/renderer/src/`, `crates/renderer/tests/`, with path dependencies on
`crates/schedule-engine` (spec 1) and `crates/pack-loader` (spec 2).

---

## ⚠️ Implementation pass status (2026-08-13, same session as specs 1/2/4)

**Only the pure-logic subset of this spec is implemented** — see
`crates/renderer/README.md` for the full rationale. Summary: this pass ran in a
sandboxed dev environment with no Wayland compositor and no GPU rendering target, and
this is the one spec (per spec.md's own framing, "the highest-risk spec") whose actual
core deliverable — a smooth GPU crossfade on a real screen — cannot be verified to work
correctly without both. Rather than write ~30 tasks of Wayland/GPU integration code that
could not be run or checked here, this pass implemented and fully tested everything that
*is* pure logic (assignment resolution, crossfade progress math, config reading/
coalescing, the scheduler bridge, D-Bus response mapping — 21 tests, 93.99% line
coverage, clippy clean) and left the Wayland/GPU/D-Bus-server tasks explicitly open below
for a session with real compositor/GPU access. Task IDs and file paths below are
otherwise unchanged from the original plan; checked items reflect what actually landed,
not a renumbering. `crates/renderer/Cargo.toml` deliberately omits
`smithay-client-toolkit`/`wgpu`/`calloop`/`calloop-wayland-source`/`raw-window-handle`/
`zbus` as a result — T002 below is only partially done for exactly that reason.

**Real cross-spec bug found and fixed during this pass** (see `scheduler_bridge.rs`):
spec 1's `ValidatedPack::query` panics if called with `location: None` on a
solar-anchored pack (documented as a caller-contract violation there) — but this daemon
can legitimately reach that state at runtime, and naively calling it would crash the
whole daemon, violating this spec's own FR-013. Added `RendererError::LocationRequired`
(not itself listed in data-model.md, which says this case needs "not a new error
variant" but doesn't identify an existing one that actually fits) to degrade just that
one output instead. Documented in both `scheduler_bridge.rs` and `error.rs`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Add `renderer` as the workspace's third crate, wired to both prior specs' crates.

- [X] T001 Add `crates/renderer` as a new member of the workspace root `Cargo.toml` (plan.md Project Structure)
- [~] T002 Create `crates/renderer/Cargo.toml` with `smithay-client-toolkit` (0.20.x), `calloop`, `calloop-wayland-source` (0.3.x), `wgpu`, `raw-window-handle` (0.6.x), `image` (0.25.x), `cosmic-config` (git dependency) dependencies, and path dependencies on `schedule-engine` and `pack-loader`, producing both a library and a `wallpaperd` binary (`src/bin/wallpaperd.rs`) (research.md R1–R5, plan.md Project Structure) — **partial**: `cosmic-config` + the two path dependencies are present; the Wayland/GPU/image dependencies and the `wallpaperd` binary are not (see status note above). `image` also omitted from non-dev dependencies since nothing in the implemented subset decodes pixels yet.
- [X] T003 [P] Add `[lints]` denying `clippy::unwrap_used` and `clippy::expect_used` outside `#[cfg(test)]` to `crates/renderer/Cargo.toml` (constitution Principle VIII)
- [X] T004 [P] Add a CI workflow running `cargo test` and `cargo clippy` against the pure-logic portion of the crate in `.github/workflows/renderer-ci.yml` (the Wayland/GPU-touching portion's CI story is research.md R6's exploratory smoke test, added separately in Polish)

**Checkpoint**: `cargo build` succeeds depending on `schedule-engine` and `pack-loader`; no `wallpaperd` binary exists yet (see status note). CI pipeline is defined for the pure-logic tests.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Shared types every user story needs. No user story work starts before this
phase is done.

**⚠️ CRITICAL**: Blocks Phases 3–8.

- [X] T005 Create the `RendererError` type in `crates/renderer/src/error.rs` (data-model.md RendererError; `std::error::Error` + `Debug` + `Display`, no panics — constitution Principle VIII). Includes the added `LocationRequired` variant (status note above).
- [X] T006 [P] Create the `OutputId` type (an `xdg-output` connector-name wrapper) in `crates/renderer/src/output.rs` (data-model.md OutputId)
- [X] T007 [P] Create the `OutputAssignment` tagged union (`Explicit`/`FollowsToggle`/`Unassigned`) and the `RendererConfig` shape (`schema_version`, `same_pack_everywhere`, `overrides`), reusing spec 2's `PackSource` by reference, in `crates/renderer/src/assignment.rs` (data-model.md OutputAssignment/RendererConfig, contracts/renderer-config-schema.md; depends on T006 for `OutputId`) — landed in `output.rs` rather than a separate `assignment.rs`, since it's a small, tightly-related set of types; the resolution rule itself (T034) is here too rather than deferred to Phase 7.
- [~] T008 [P] Create the `ManagedOutput` type and `RendererState`/`IdleWaitState` types in `crates/renderer/src/output.rs` (data-model.md ManagedOutput/RendererState/IdleWaitState; depends on T005, T006, T007) — **partial**: `RendererState`/`IdleWaitState` done; `ManagedOutput` itself omitted — its two non-pure fields (`wl_output`, an opaque SCTK handle; the `calloop` timer inside `IdleWaitState`) belong to the unimplemented Wayland integration, and a struct missing exactly its two most structural fields wouldn't be a meaningful stand-in.
- [X] T009 [P] Create the `CrossfadeTransition` type in `crates/renderer/src/crossfade.rs` (data-model.md CrossfadeTransition; depends on nothing else) — `outgoing_texture`/`incoming_texture` (GPU handles in the full data model) are `schedule_engine::ImageId`s here instead, per the same "pure subset" note as T008.
- [X] T010 Wire up public API re-exports (`OutputId`, `ManagedOutput`, `OutputAssignment`, `RendererConfig`, `RendererError`) in `crates/renderer/src/lib.rs` (depends on T005–T009; matches contracts/renderer-config-schema.md's referenced types) — no `ManagedOutput` to re-export per T008's note.

**Checkpoint**: Crate compiles with all implemented shared types defined; pure-logic work can now proceed (Wayland/GPU work remains blocked on a real compositor regardless of this phase's completeness).

---

## Phase 3: User Story 1 - Smooth Crossfade at a Scheduled Transition (Priority: P1) 🎯 MVP

**Goal**: At a scheduled transition instant on a managed output, blend smoothly from the
outgoing to the incoming image on the GPU, over the configured (default 45s) duration.

**Independent Test**: On a single managed output with a multi-image pack loaded, advance to a
scheduled transition instant and observe a smooth GPU blend with no hard cut, flicker, or
tearing (quickstart.md manual smoke check steps 1–2).

### Implementation for User Story 1

**Not implemented this pass (all need a real Wayland compositor/GPU — see status note at
top of this file)**:

- [ ] T011 [US1] Implement `wgpu` instance/device/adapter setup with automatic backend selection (Vulkan preferred, GL fallback) in `crates/renderer/src/gpu.rs` (FR-001, research.md R3; depends on T005)
- [ ] T012 [US1] Implement the `raw-window-handle` bridge from SCTK's `wl_surface`/`wl_display` to `wgpu::Surface` creation in `crates/renderer/src/gpu.rs` (research.md R3's flagged integration risk — smoke-test this early; depends on T011)
- [ ] T013 [US1] Implement per-output `wlr-layer-shell-unstable-v1` background surface creation (background layer, full-output anchor) plus `wp_viewporter` setup in `crates/renderer/src/surface.rs` (FR-001, constitution Principle I, research.md R1; depends on T008, T012)
- [ ] T014 [US1] Implement full-resolution image decode (via `image`) and GPU texture upload via `wgpu::Queue::write_texture` in `crates/renderer/src/texture.rs` (FR-001, research.md R5; depends on T011)
- [ ] T015 [US1] Implement the WGSL two-texture crossfade blend pipeline (vertex/fragment shaders, progress uniform) in `crates/renderer/src/crossfade.rs` (FR-001, FR-002 fixed 45s default; depends on T009, T011)
- [ ] T016 [US1] Implement the frame-callback-paced draw loop — subscribe to `wl_surface.frame` only during an active `CrossfadeTransition`, recompute progress from `started_at`/`duration` each callback, unsubscribe immediately on completion — in `crates/renderer/src/crossfade.rs` (FR-001, FR-004, constitution Principle II/III; depends on T013, T015)
- [ ] T017 [US1] Wire spec 1's `ScheduleQueryResult` into transition triggering: when a scheduled transition instant is reached for an output, build a `CrossfadeTransition` from the outgoing/incoming images and hand it to the draw loop, in `crates/renderer/src/crossfade.rs` (FR-001; depends on T014, T016) — **the pure half of this** (computing which images/progress a `ScheduleQueryResult` implies) is done in `scheduler_bridge.rs::evaluate`; only "hand it to the draw loop" remains.
- [ ] T018 [US1] Implement the degenerate single-image/static-mode case — no crossfade ever triggers, the one image is simply displayed continuously (spec.md US1 Scenario 3) in `crates/renderer/src/crossfade.rs` (depends on T017) — spec 1's own `ValidatedPack` already guarantees this at the query level (`transition: None` always for a static pack); only the "don't ever start a draw loop for it" wiring remains.
- [X] T019 [P] [US1] Pure unit tests for crossfade progress computation — monotonic, clamped to `[0.0, 1.0]`, deterministic given `started_at`/`duration`/now — in `crates/renderer/tests/crossfade_progress.rs`, no Wayland/GPU dependency (depends on T009) — landed as `#[cfg(test)]` unit tests in `src/crossfade.rs` itself rather than a separate `tests/` file; also covers FR-011 (a new `CrossfadeTransition` value cleanly supersedes an in-flight one — there's no stacking representation possible in this data shape at all).
- [ ] T020 [US1] Implement a minimal `wallpaperd` entry point wiring one managed output end-to-end (load config → query spec 1's schedule → render via crossfade.rs) in `crates/renderer/src/bin/wallpaperd.rs` (depends on T017, T018)

**Checkpoint**: User Story 1's pure logic (T009, T019) is done and tested; the Wayland/GPU rendering itself (T011-T018, T020) needs a real compositor session — not runnable here.

---

## Phase 4: User Story 2 - Idle Between Transitions Costs Nothing (Priority: P1) 🎯 MVP

**Goal**: Outside an active crossfade, hold no render loop — compute the next transition
instant per output and sleep until then.

**Independent Test**: With no transition due for an extended window, confirm no periodic
redraw/polling occurs and the daemon wakes only via a single timer at the pre-computed next
instant (quickstart.md manual smoke check step 3).

### Implementation for User Story 2

- [ ] T021 [US2] Implement a per-output `calloop` timer wired to spec 1's `next_transition_after`, firing the transition-trigger path (T017) and holding no other scheduled activity, in `crates/renderer/src/scheduler_bridge.rs` (FR-003, constitution Principle VI; depends on T008, T017) — not implemented (needs `calloop`, not a dependency here per status note); `scheduler_bridge.rs::evaluate` (implemented) is the pure computation this timer would call into on every fire.
- [ ] T022 [US2] Ensure the draw loop unsubscribes from `wl_surface.frame` immediately on crossfade completion and returns the output to `IdleWaitState` (FR-004; extends T016; depends on T021) — `CrossfadeTransition::is_complete_at` (implemented, tested) is the pure predicate this wiring would check each frame callback.
- [X] T023 [P] [US2] Pure unit tests for the idle-wait/active-transition state machine — transitions between `IdleWaitState` and `ActiveTransition`, no-op if already idle — in `crates/renderer/tests/renderer_state.rs`, no Wayland/GPU dependency (depends on T008) — `RendererState`/`IdleWaitState` are plain enum/struct values (no transition *methods* to unit-test independently of the draw-loop wiring that would call them); their construction/equality is exercised indirectly through `output.rs`'s and `scheduler_bridge.rs`'s own tests. No separate `renderer_state.rs` file was needed for what's implemented.
- [ ] T024 [US2] Wire `scheduler_bridge.rs` into `wallpaperd.rs` so idle-wait is the daemon's actual resting state between transitions (spec.md US2 Scenarios 1–2; depends on T020, T021, T022) — not implemented (no `wallpaperd.rs` yet).

**Checkpoint**: User Story 2's pure logic (crossfade completion predicate, T009/T019) is done; the actual idle-wait timer loop (T021, T022, T024) needs `calloop` and a real event loop — not runnable here.

---

## Phase 5: User Story 3 - Each Output Shows Its Own Independently-Scheduled Wallpaper (Priority: P2)

**Goal**: Every managed output has fully independent pack assignment, scheduling, and
crossfade state.

**Independent Test**: Assign two different packs with different schedules to two outputs and
confirm each transitions independently with no cross-contamination (quickstart.md manual
smoke check step 5).

### Implementation for User Story 3

- [ ] T025 [US3] Extend `wallpaperd.rs`/`output.rs` to manage N `ManagedOutput` instances concurrently, each with its own `RendererState`, timer, and crossfade pipeline (FR-005, constitution Principle VII; depends on T008, T021, T024) — not implemented (no `wallpaperd.rs`/`ManagedOutput` yet, per T008's note).
- [ ] T026 [US3] Audit `output.rs`/`crossfade.rs` to confirm no shared mutable state between outputs beyond the shared `wgpu` device/instance — one output's activity never touches another's (FR-005; depends on T025) — not applicable yet (nothing to audit until T025 exists); the pure logic that *is* implemented (`resolve_assignment`, `scheduler_bridge::evaluate`) already takes an `OutputId` per call with no shared mutable state at all, by construction.
- [X] T027 [P] [US3] Pure unit tests confirming independent `RendererState` per `OutputId` — two outputs' states never cross-mutate — in `crates/renderer/tests/renderer_state.rs` (extends T023; depends on T008) — landed as `output.rs`'s `two_outputs_resolve_independently` and `overridden_output_is_unaffected_by_toggle_changes` tests, which cover exactly this at the assignment-resolution level (the level that's actually implemented).

**Checkpoint**: User Story 3's assignment-resolution independence is done and tested; actually running N concurrent outputs (T025, T026) needs the daemon event loop — not runnable here.

---

## Phase 6: User Story 4 - Config and Assignment Changes Take Effect Immediately (Priority: P2)

**Goal**: A pack-assignment, schedule-relevant setting, or toggle change causes affected
output(s) to re-evaluate within 2 seconds, with rapid repeated changes coalesced.

**Independent Test**: While an output is in idle-wait with its next transition hours away,
change its pack assignment and confirm it re-evaluates and updates within 2 seconds
(quickstart.md manual smoke check step 4).

### Implementation for User Story 4

- [~] T028 [US4] Implement `RendererConfig` read plus `cosmic-config` change-watch integration in `crates/renderer/src/config.rs` (FR-007, research.md R4, contracts/renderer-config-schema.md; depends on T007) — **partial**: reading (`RendererConfig::open`/`load`) is done and tested; the live change-*watch* integration (`cosmic-config`'s `notify`-backed watcher feeding into `calloop`) is not — needs the event loop.
- [X] T029 [US4] Implement `PendingChange` coalescing — replace in-flight pending state wholesale on repeated changes to the same output, guaranteeing re-evaluation by the 2-second deadline (FR-007, FR-014; depends on T028) — landed as `config.rs`'s `Coalescer` type (named differently from data-model.md's `PendingChange`, same behavior: `record_change`/`due`/`is_pending`, wholesale-replace semantics, fully tested including the "reported exactly once even after 3 rapid changes" case).
- [ ] T030 [US4] Wire config-change re-evaluation to cleanly cancel an in-progress crossfade (no dangling GPU resources) before re-evaluating from the new state (FR-012; depends on T016, T029) — not implemented (needs the draw loop, T016).
- [ ] T031 [US4] Ensure a change affecting only one output re-evaluates only that output, leaving unrelated outputs untouched (FR-007 Scenario 3; depends on T025, T029) — the coalescing half of this is already true by construction (`Coalescer` is keyed per-`OutputId`, tested in `changes_to_different_outputs_are_independent`); the "only that output re-evaluates" half needs T025's multi-output wiring.
- [X] T032 [P] [US4] Pure unit tests for `PendingChange` coalescing — rapid repeated changes to the same output collapse to the latest state only, never queued or individually processed (FR-014, spec.md Clarifications) in `crates/renderer/tests/assignment_resolution.rs` (depends on T007) — landed as `config.rs`'s own `#[cfg(test)]` module (`repeated_changes_to_the_same_output_coalesce`, `changes_to_different_outputs_are_independent`).
- [ ] T033 [US4] Wire `config.rs` into `wallpaperd.rs` as the live path for assignment/toggle changes (depends on T028, T029, T030, T031) — not implemented (no `wallpaperd.rs` yet).

**Checkpoint**: FR-014's coalescing logic (T029, T032) is done and tested; the live change-watch and crossfade-cancellation wiring (T028's watch half, T030, T031, T033) needs the event loop — not runnable here.

---

## Phase 7: User Story 5 - One Toggle for "Same Everywhere," Override Still Available (Priority: P3)

**Goal**: A "same pack on all outputs" toggle exists; an explicit per-output override always
takes precedence over it.

**Independent Test**: Enable the toggle with two or more outputs, confirm convergence, then
override one output individually and confirm it diverges while others still follow the toggle.

### Implementation for User Story 5

- [X] T034 [US5] Implement the `OutputAssignment` resolution rule — an `overrides` entry wins, else `same_pack_everywhere` if `Some`, else `Unassigned` — in `crates/renderer/src/assignment.rs` (FR-006, data-model.md Resolution rule; depends on T007) — landed as `output.rs`'s `resolve_assignment`/`effective_pack` functions (see T007's note on file placement).
- [X] T035 [P] [US5] Pure unit tests for resolution precedence — explicit override beats the toggle, and a toggle-pack change doesn't affect an overridden output (spec.md US5 Scenarios 1–3) in `crates/renderer/tests/assignment_resolution.rs` (depends on T034) — landed as `output.rs`'s own `#[cfg(test)]` module (`explicit_override_always_wins`, `no_override_follows_toggle_when_set`, `no_override_and_toggle_off_is_unassigned`, `overridden_output_is_unaffected_by_toggle_changes`).
- [ ] T036 [US5] Wire assignment resolution into `config.rs`'s change detection so a toggle-only change re-evaluates every non-overridden output (FR-006; depends on T028, T034) — not implemented (needs T028's live watch half + T025's multi-output wiring).

**Checkpoint**: User Story 5's resolution-precedence logic (T034, T035) is done and fully tested; wiring it into live change detection (T036) needs the event loop — not runnable here.

---

## Phase 8: User Story 6 - Outputs Can Come, Go, Resize, or Rescale Without a Restart (Priority: P3)

**Goal**: Hotplug (connect/disconnect) and runtime resolution/scale changes are handled
without crashing or requiring a restart, and don't disturb other outputs.

**Independent Test**: With one or more outputs already managed, connect a new output,
disconnect an existing one, and resize/rescale a remaining one — confirm the daemon keeps
running and unaffected outputs are undisturbed (quickstart.md manual smoke check step 5).

### Implementation for User Story 6

**Not implemented this pass (all need a real Wayland compositor — see status note)**:

- [ ] T037 [US6] Implement SCTK's `OutputHandler` for `new_output`/`output_destroyed`/`update_output` events in `crates/renderer/src/output.rs` (FR-008, research.md R1; depends on T008)
- [ ] T038 [US6] On `new_output`: create a `ManagedOutput`, resolve its `OutputAssignment` (FR-009's well-defined-state requirement), and reach a stable state within 2 seconds without disrupting existing outputs (FR-009; depends on T025, T034, T037) — the assignment-resolution half (`resolve_assignment`) is done; the "create a `ManagedOutput`... within 2 seconds" half needs T037.
- [ ] T039 [US6] On `output_destroyed`: release that output's render state, timer, and any in-progress crossfade without affecting other outputs (FR-010; depends on T025, T037)
- [ ] T040 [US6] On `update_output` (resize/rescale): reconfigure that output's `wp_viewporter`/fractional-scale setup and continue rendering correctly at the new resolution/scale without a restart (FR-008 Scenario 3; depends on T013, T037)
- [ ] T041 [US6] Implement overlapping-transition supersession — if a new transition trigger fires for an output already mid-crossfade, cleanly cancel the in-progress blend and start the new one rather than stacking (FR-011; depends on T016, T017) — the data-level half of this is already true by construction: `CrossfadeTransition` is a plain value type with no way to represent two transitions "stacked" at once (see `crossfade.rs`'s `a_new_transition_value_cleanly_replaces_an_in_flight_one` test) — a new value simply *is* the replacement. What's missing is the actual draw-loop cancellation (freeing GPU resources of the old blend), which needs T016.
- [ ] T042 [US6] Implement invalid/unreadable-pack degradation — if an assigned pack becomes invalid after assignment, hold that output's last-known-good frame without affecting others (FR-013, constitution Principle VIII; depends on T017, T025) — not implemented; `scheduler_bridge.rs::evaluate` returning `Err` (e.g. `LocationRequired`) is the trigger point this would react to by holding the prior frame instead of erroring the whole output, but the "hold the last frame" behavior itself is a rendering concern.
- [ ] T043 [P] [US6] Pure unit tests for hotplug lifecycle bookkeeping — new/destroyed output entries added/removed from the managed set — using a fake/mock output source, in `crates/renderer/tests/renderer_state.rs` (depends on T037) — not implemented; there's no managed-output *set* to test lifecycle bookkeeping against without `ManagedOutput`/T025.

**Checkpoint**: Not reached — User Story 6 is entirely Wayland-integration work (hotplug, resize, GPU resource cleanup), none of which is runnable without a real compositor.

---

## Phase 9: Polish & Cross-Cutting Concerns

**Purpose**: Close out the spec's success criteria and hand off a stable, runnable daemon to
specs 4–5.

- [ ] T044 [P] Add the exploratory Weston-headless CI smoke test (daemon starts, creates a layer surface per output, no crash on a simulated hotplug) per research.md R6, extending `.github/workflows/renderer-ci.yml` — not implemented; nothing to smoke-test yet (no `wallpaperd` binary or Wayland integration, T011-T020).
- [X] T045 [P] Add rustdoc comments to every public item matching contracts/renderer-config-schema.md and data-model.md — verified via `RUSTFLAGS="-W missing_docs" cargo build --workspace`, zero warnings, for everything actually implemented.
- [X] T046 [P] Add `crates/renderer/README.md` summarizing scope, the pure-vs-manual-QA testing split (research.md R6), and explicit non-scope (no CLI/GUI/`cosmic-bg` supersession — specs 4/5) — also documents exactly what this pass implemented vs. deferred and why (the Wayland/GPU/no-compositor rationale), since that's the more load-bearing scope boundary for this particular spec right now.
- [ ] T047 Document the manual QA checklist as a standalone, repeatable procedure (integrated-graphics run + multi-output/mixed-scale run, constitution Principles III/VII) referencing quickstart.md's five smoke-check scenarios — not written; premature before there's a `wallpaperd` binary to run the checklist against. quickstart.md's existing manual smoke check (unchanged, already accurate for what it describes) remains the reference for whoever picks up T011 onward.
- [~] T048 Run quickstart.md end-to-end (`cargo test`, manual smoke check on a real COSMIC/nested-compositor session, optional Weston-headless run) and fix any drift between the doc and the actual API/behavior — **partial**: `cargo test --package renderer` is green (21 tests, 93.99% coverage, clippy clean); the manual smoke check needs the Wayland/GPU code this pass doesn't implement, so it wasn't run and no drift check against it was possible.

---

## Phase 10: Cross-Spec Amendment — Location Consumption & D-Bus Service (Amendment 2026-08-13)

**Purpose**: Close the two gaps spec 4 (CLI control surface) surfaced during its own planning
— see spec.md's Amendment note and User Story 7, and plan.md's Cross-Spec Dependencies. New
task IDs are appended here rather than renumbering T001–T048, per this project's convention of
keeping existing task IDs stable once written.

**Independent Test** (User Story 7, FR-015/FR-016): With the daemon running and a location
configured via spec 4's `LocationConfig`, confirm a solar-anchored pack schedules correctly
using that location, and that an external D-Bus caller can query/re-evaluate any managed
output and get a response matching the daemon's real state.

- [ ] T049 [P] Add `zbus` (5.x) as a dependency in `crates/renderer/Cargo.toml` (research.md R8; extends T002) — not added; nothing here yet needs it (T053, the actual D-Bus server, is the task that would).
- [~] T050 [US2] Extend `config.rs` to also watch spec 4's `LocationConfig` `cosmic-config` entry, coalescing location changes the same way `RendererConfig` changes are coalesced (FR-015, research.md R7; depends on T028, T049) — **partial**: `LocationSource` reading (`open`/`load`) is done and tested in `config.rs`, using the same `open_at` scratch-directory test pattern as `RendererConfig`; the live watch/coalescing half needs T028's watch integration, which isn't implemented.
- [X] T051 [US1] Wire `config.rs`'s current location value into `scheduler_bridge.rs`'s calls to spec 1's `ValidatedPack::query` for solar-anchored packs; a `None` location degrades that output per `RendererError`'s existing containment posture (FR-015, FR-013's pattern; depends on T017, T021, T050) — **done, and this is where the real bug (status note at top of this file) was found**: naively passing `location: None` straight through to `query()` for a solar-anchored pack would panic (spec 1's own documented caller-contract violation), not degrade — `scheduler_bridge.rs::evaluate` checks `anchor_kind() == Solar && location.is_none()` *before* calling `query()` and returns `RendererError::LocationRequired` instead. Fully unit-tested (`solar_pack_without_location_degrades_this_output_only`, `solar_pack_with_location_resolves_normally`, `clock_pack_never_needs_a_location`).
- [X] T052 [US7] Create the `QueryResponse` type and `OutputNotManaged` error variant in `crates/renderer/src/error.rs` and `crates/renderer/src/dbus_service.rs` (data-model.md QueryResponse/RendererError; depends on T005, T008) — landed as `error.rs`'s `OutputNotManaged` (as specified) and `dbus_types.rs`'s `QueryResponse` (not `dbus_service.rs`, since no service exists yet to house it alongside — see T053).
- [ ] T053 [US7] Implement the `dbus_service.rs` `zbus` server — `QueryOutput`, `QueryAll`, `Reevaluate`, `ReevaluateAll` per `specs/004-cli-control-surface/contracts/wallpaperd-dbus-interface.md` — integrated into the existing `calloop` event loop (FR-016, research.md R8; depends on T025, T049, T052) — not implemented; needs the event loop. Note for whoever picks this up: `wallpaperctl`'s `dbus_client.rs` (spec 4, already implemented and tested) is the exact interface this server needs to satisfy — its tests currently confirm "no service registered" and would start exercising the real request/response path the moment this task lands, with no changes needed on the CLI side.
- [ ] T054 [US7] Wire `dbus_service.rs` into `wallpaperd.rs`'s startup (register the session-bus name, serve requests alongside the Wayland/timer event sources) in `crates/renderer/src/bin/wallpaperd.rs` (depends on T020, T053) — not implemented (no `wallpaperd.rs`/`dbus_service.rs` yet).
- [X] T055 [P] [US7] Pure unit tests for `QueryResponse` construction from `ManagedOutput`/`RendererState` — no real D-Bus connection needed, just the data-mapping logic — in `crates/renderer/tests/dbus_response_mapping.rs` (depends on T052) — landed as `dbus_types.rs`'s own `#[cfg(test)]` module, built from `ScheduleQueryResult` directly rather than `ManagedOutput`/`RendererState` (neither exists — see T008's note); covers the unassigned case, the outside-a-transition case (`active_before`), and the mid-transition case (reports the *incoming* image, since that's what's actually becoming visible).

**Checkpoint**: Not fully reached. FR-015's location-consumption logic (T051, the reading half
of T050) is done, tested, and includes a real correctness fix. `QueryResponse`'s data mapping
(T052, T055) is done and tested. The live D-Bus server itself (T049, T053, T054) and the
change-watch half of T050 need the event loop this pass doesn't implement — so spec 4's CLI
(`wallpaperctl query`/`reevaluate`) still correctly reports "daemon unreachable" until a future
pass adds them.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — start immediately.
- **Foundational (Phase 2)**: Depends on Setup. Blocks all user stories.
- **User Story 1 (Phase 3)**: Depends only on Foundational. The single-output crossfade path
  — every other story builds on top of the modules it creates (`gpu.rs`, `surface.rs`,
  `texture.rs`, `crossfade.rs`).
- **User Story 2 (Phase 4)**: Depends on Foundational **and** US1's transition-trigger path
  (T017) — idle-wait is meaningless without something to wake up for.
- **User Story 3 (Phase 5)**: Depends on Foundational, US1, and US2 — multi-output support is
  a direct extension of the single-output plumbing those stories build.
- **User Story 4 (Phase 6)**: Depends on Foundational, US1 (crossfade cancellation, T016), and
  US3 (per-output re-evaluation, T025) — introduces `config.rs`.
- **User Story 5 (Phase 7)**: Depends on Foundational's `assignment.rs` (T007) for its
  resolution-rule logic, and on US4's `config.rs` (T028) for wiring the toggle into live
  change detection.
- **User Story 6 (Phase 8)**: Depends on US1 (crossfade cancellation), US3 (multi-output
  management), and US5 (assignment resolution for newly-connected outputs) — hotplug handling
  sits on top of everything else this spec builds.
- **Polish (Phase 9)**: Depends on all six user stories being complete.
- **Cross-Spec Amendment (Phase 10, Amendment 2026-08-13)**: T049 (Setup-like, no dependency
  beyond T002) can start anytime after Phase 1. T050/T051 depend on US4's `config.rs` (T028)
  and US1/US2's query-triggering path (T017, T021). T052–T055 depend on US3's multi-output
  management (T025) and Foundational's error/output types (T005, T008). Not required for the
  Phase 1–8 checkpoint ("all six user stories functional") — it's an independent ninth
  capability layered on top, not a blocker for anything before it.

### Parallel Opportunities

- T003 and T004 (Setup) — different files.
- T006, T007, T008, and T009 (Foundational) — different files, though T007/T008 have a
  same-phase dependency on T006 (`OutputId`).
- T019 (US1 pure tests) — independent of the rest of US1's Wayland/GPU implementation tasks,
  can proceed once T009 lands.
- T023 (US2 pure tests) — independent, can proceed once T008 lands.
- T027 (US3 pure tests) — independent, extends T023.
- T032 (US4 pure tests) — independent, can proceed once T007 lands.
- T035 (US5 pure tests) — independent, can proceed once T034 lands.
- T043 (US6 pure tests) — independent, can proceed once T037 lands.
- T044, T045, and T046 (Polish) — different files.

### Sequential-in-Practice Files

Unlike specs 1–2, most of this crate's files are touched by multiple phases in sequence
rather than being cleanly parallel across stories — `crossfade.rs` (US1 → US2 → US4 → US6),
`output.rs` (Foundational → US3 → US6), and `config.rs`/`assignment.rs` (US4 → US5) are the
ones to coordinate carefully; this mirrors spec 1's own note about `query.rs` being
shared-but-sequential despite nominal story independence.

---

## Parallel Example: Foundational Phase

```bash
# After T006 (OutputId in output.rs) lands, launch together:
Task: "Create OutputAssignment tagged union and RendererConfig shape in crates/renderer/src/assignment.rs"
Task: "Create CrossfadeTransition type in crates/renderer/src/crossfade.rs"
```

## Parallel Example: Pure-Logic Test Tasks

```bash
# Independent of the Wayland/GPU implementation work, once their respective types exist:
Task: "Pure unit tests for crossfade progress computation in crates/renderer/tests/crossfade_progress.rs"
Task: "Pure unit tests for PendingChange coalescing in crates/renderer/tests/assignment_resolution.rs"
```

---

## Implementation Strategy

### MVP First (User Stories 1 and 2)

1. Phase 1: Setup
2. Phase 2: Foundational (blocks everything)
3. Phase 3: User Story 1 — single-output GPU crossfade
4. Phase 4: User Story 2 — zero-cost idle-wait
5. **STOP and VALIDATE**: quickstart.md manual smoke check steps 1–3 pass on at least one
   integrated-graphics device (constitution Principle III, NFR-3)
6. This alone is a demonstrable MVP: a single managed output that crossfades correctly and
   costs nothing while idle — the project's core differentiator, working end to end

### Incremental Delivery

1. Setup + Foundational → foundation ready
2. Add User Story 1 → validate independently (single-output crossfade)
3. Add User Story 2 → validate independently → MVP (both P1 stories done)
4. Add User Story 3 → validate independently (multi-output isolation)
5. Add User Story 4 → validate independently (live reconfiguration)
6. Add User Story 5 → validate independently ("same everywhere" toggle + override)
7. Add User Story 6 → validate independently (hotplug/resize/rescale resilience)
8. Polish → CI smoke test, docs, manual QA checklist, quickstart parity

---

## Notes

- [P] tasks touch different files with no unmet dependency.
- No tests are written before implementation in this spec's Wayland/GPU-touching tasks
  (unlike spec 1's strict test-first posture) — research.md R6 explicitly scopes automated
  tests to the pure logic only; the rest is manual QA by design, not an oversight.
- `crossfade.rs` is the single most cross-cutting file in this crate (US1, US2, US4, US6 all
  extend it) — coordinate edits there even when tasks are nominally story-scoped.
- A pack assignment or config value is untrusted-by-the-time-it-changes input in the same
  spirit as spec 2's manifest handling (FR-013) — every US4/US6 implementation task must keep
  failing closed (degrade one output, never crash the daemon) as the default posture, per
  constitution Principle VIII.
- Phase 10 (T049–T055) was added 2026-08-13 while planning spec 4, whose own contract turned
  out to depend on this daemon reading a location and exposing a D-Bus service — neither
  existed when Phases 1–9 were originally written. Appended rather than interleaved, to keep
  T001–T048's IDs stable for anything already referencing them.
- Commit after each task or logical group.
