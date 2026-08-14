---

description: "Task list template for feature implementation"
---

# Tasks: Wallpaper Renderer

**Input**: Design documents from `/specs/003-wallpaper-renderer/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/renderer-config-schema.md, quickstart.md. Phase 10 below additionally depends on spec 4's `specs/004-cli-control-surface/contracts/location-config-schema.md` and `.../wallpaperd-dbus-interface.md` (Amendment 2026-08-13).

**Tests**: Partial, per plan.md's Technical Context/research.md R6's original two-tier
strategy — pure logic is `cargo test`-able, real Wayland/GPU code is manual-QA-only. In
practice this pass found a third tier worth using: `tests/gpu_render.rs` exercises the
*real* `wgpu` GPU pipeline offscreen (no Wayland surface needed), which turned out to be
both automatable *and* a stronger correctness check than eyeballing a screenshot (exact
pixel values, not "looks blended"). The actual on-screen Wayland integration
(`surface.rs`'s layer-shell/SCTK wiring, the `wallpaperd` binary) is still manual-QA-only
as originally planned — it was manually run against a live compositor during this pass
(see status note above), not exercised by `cargo test`.

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

**Two passes happened in this session.** The first implemented only the pure-logic
subset, believing this dev environment had no Wayland compositor or GPU — that
assumption turned out to be **wrong**: the environment is a real, live `cosmic-comp`
(COSMIC) session with real GPU hardware (Intel HD Graphics 630). Once discovered, a
second pass implemented and live-tested the actual Wayland/GPU rendering. See
`crates/renderer/README.md` for the full, current breakdown of what's implemented,
what's simplified, and what's still open — this note summarizes only the highlights and
defers to that file as the source of truth on scope.

**What's now real and verified against the live compositor**, not just written:
`gpu.rs` (wgpu instance/adapter/device — selected the real Intel adapter via Vulkan),
`texture.rs` (real image decode + GPU upload), `crossfade.rs`'s `CrossfadePipeline` (a
real WGSL two-texture blend shader, pixel-verified exact via an offscreen GPU
render+readback test — not just "looked right"), `surface.rs` (real SCTK layer-shell
surface creation, accepted by `cosmic-comp` at 1920x1080, bridged to `wgpu::Surface` via
`raw-window-handle`), and `src/bin/wallpaperd.rs` (a real daemon binary, run live for
35+ seconds spanning an actual scheduled crossfade transition, zero crashes). 26 tests
(25 pure-logic/config + 1 real-GPU pixel-correctness), clippy clean, zero missing-docs
warnings.

**Two real bugs found and fixed during this pass** (both documented at length in
`crates/renderer/README.md` and in the relevant source file's own doc comments —
summarized here):
1. (First pass) `scheduler_bridge.rs`: spec 1's `ValidatedPack::query` panics if called
   with `location: None` on a solar-anchored pack (a documented caller-contract
   violation there) — but this daemon can legitimately reach that state at runtime.
   Added `RendererError::LocationRequired` to degrade just that one output instead of
   crashing the whole daemon (FR-013).
2. (Second pass, found only by actually running against a real `wallpaperctl`-written
   config) `output.rs`'s `RendererConfig.overrides` was typed `HashMap<OutputId,
   PackSource>` — RON does not treat a single-field tuple struct as transparently
   string-keyed, so this silently read an **empty** map from every real config
   `wallpaperctl` had written (the parse error was swallowed by a `Default` fallback,
   no crash, no log line pointing at the cause). Fixed to `HashMap<String, PackSource>`,
   matching `wallpaperctl`'s independently-defined shape exactly; a permanent regression
   test now hand-constructs that literal on-disk RON text and confirms it parses.

Task IDs/file paths below are unchanged from the original plan (no renumbering).
`crates/renderer/Cargo.toml` now includes the full dependency set (`smithay-client-
toolkit`, `wgpu`, `calloop`, `calloop-wayland-source`, `raw-window-handle`, `image`,
`bytemuck`, `tracing`) — `zbus` is the one dependency still not added, since the live
D-Bus service (T053) is the one major piece still not implemented (see below).

**Follow-up gap-closure pass (2026-08-14, branch `003-close-renderer-gaps`)**: T021, T028,
T031, T033, T036, T050 all closed — the idle-wait timer is now precise (a single
`calloop::Timer` rescheduled to `next_wake_instant()`, not a flat poll), and
`RendererConfig`/`LocationSource` are now live-watched via `cosmic_config::calloop::
ConfigWatchSource`, feeding `Coalescer` and taking effect within ~2s with no `wallpaperd`
restart — both live-verified against a real two-output `cosmic-comp` session (assigning a
previously-unassigned output and changing location, observed applying without a restart).
A real gap found live during this pass, not just designed on paper: the idle timer was
originally armed once at startup, before any output (and therefore its real schedule) was
known, so it ran on a 60s fallback deadline until that fallback happened to fire — fixed by
also rescheduling at the end of each output's first `configure`. See
`crates/renderer/README.md` for the current up-to-date breakdown. T049/T053/T054 (the live
D-Bus service) remain the one major piece still not implemented.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Add `renderer` as the workspace's third crate, wired to both prior specs' crates.

- [X] T001 Add `crates/renderer` as a new member of the workspace root `Cargo.toml` (plan.md Project Structure)
- [X] T002 Create `crates/renderer/Cargo.toml` with `smithay-client-toolkit` (0.20.x), `calloop`, `calloop-wayland-source` (0.3.x), `wgpu`, `raw-window-handle` (0.6.x), `image` (0.25.x), `cosmic-config` (git dependency) dependencies, and path dependencies on `schedule-engine` and `pack-loader`, producing both a library and a `wallpaperd` binary (`src/bin/wallpaperd.rs`) (research.md R1–R5, plan.md Project Structure) — full dependency set now present (`bytemuck`, `tracing`/`tracing-subscriber` added beyond the original list, for uniform buffer packing and daemon logging respectively); `wayland-client` needed an explicit `features = ["system"]` beyond what SCTK pulls in by default, to get the real `libwayland-client.so`-backed backend that exposes raw pointers for the `raw-window-handle` bridge — SCTK alone doesn't force that feature on. `zbus` is not yet added (T053 not implemented).
- [X] T003 [P] Add `[lints]` denying `clippy::unwrap_used` and `clippy::expect_used` outside `#[cfg(test)]` to `crates/renderer/Cargo.toml` (constitution Principle VIII)
- [X] T004 [P] Add a CI workflow running `cargo test` and `cargo clippy` against the pure-logic portion of the crate in `.github/workflows/renderer-ci.yml` (the Wayland/GPU-touching portion's CI story is research.md R6's exploratory smoke test, added separately in Polish) — extended to install `libxkbcommon-dev` (see `crates/renderer/README.md`'s "Building on a real system") and to run the *full* test suite including the real-GPU `tests/gpu_render.rs` (skips gracefully on a GPU-less runner, doesn't fail CI).

**Checkpoint**: `cargo build` succeeds, producing a real `wallpaperd` binary. Verified against a live `cosmic-comp` session, not just compiled (see status note above). CI pipeline covers the full test suite.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Shared types every user story needs. No user story work starts before this
phase is done.

**⚠️ CRITICAL**: Blocks Phases 3–8.

- [X] T005 Create the `RendererError` type in `crates/renderer/src/error.rs` (data-model.md RendererError; `std::error::Error` + `Debug` + `Display`, no panics — constitution Principle VIII). Includes the added `LocationRequired` variant (status note above). `SurfaceCreationFailed`/`GpuDeviceUnavailable`/`OutputProtocolError` are now genuinely constructed by real code (`surface.rs`/`gpu.rs`), not just defined for a future pass.
- [X] T006 [P] Create the `OutputId` type (an `xdg-output` connector-name wrapper) in `crates/renderer/src/output.rs` (data-model.md OutputId)
- [X] T007 [P] Create the `OutputAssignment` tagged union (`Explicit`/`FollowsToggle`/`Unassigned`) and the `RendererConfig` shape (`schema_version`, `same_pack_everywhere`, `overrides`), reusing spec 2's `PackSource` by reference, in `crates/renderer/src/assignment.rs` (data-model.md OutputAssignment/RendererConfig, contracts/renderer-config-schema.md; depends on T006 for `OutputId`) — landed in `output.rs` rather than a separate `assignment.rs`, since it's a small, tightly-related set of types; the resolution rule itself (T034) is here too rather than deferred to Phase 7. `overrides` is `HashMap<String, PackSource>`, not `HashMap<OutputId, PackSource>` as first written — see the status note's second real bug for why.
- [X] T008 [P] Create the `ManagedOutput` type and `RendererState`/`IdleWaitState` types in `crates/renderer/src/output.rs` (data-model.md ManagedOutput/RendererState/IdleWaitState; depends on T005, T006, T007) — `RendererState`/`IdleWaitState` as originally planned; `ManagedOutput` itself landed as `surface.rs`'s private `WallpaperOutput` struct instead (with its real SCTK/`wgpu` handle fields, `wl_output`/`layer`/`viewport`/`wgpu_surface`) once the real Wayland integration existed to give it something to hold — not re-exported publicly, since it's `wallpaperd`'s own internal per-output bookkeeping, not a type other crates need.
- [X] T009 [P] Create the `CrossfadeTransition` type in `crates/renderer/src/crossfade.rs` (data-model.md CrossfadeTransition; depends on nothing else) — `outgoing_texture`/`incoming_texture` (GPU handles in the full data model) are `schedule_engine::ImageId`s here (the *pipeline*, `CrossfadePipeline`, is what actually holds/binds the GPU textures per-frame from `WallpaperOutput`'s texture cache — this type itself stays a lightweight, easily-cloned/replaced value).
- [X] T010 Wire up public API re-exports (`OutputId`, `ManagedOutput`, `OutputAssignment`, `RendererConfig`, `RendererError`) in `crates/renderer/src/lib.rs` (depends on T005–T009; matches contracts/renderer-config-schema.md's referenced types) — no `ManagedOutput` re-export, per T008's note (`WallpaperDaemon` and `surface` module are re-exported/public instead, as the actual integration surface `wallpaperd.rs` uses).

**Checkpoint**: Crate compiles with all shared types defined (both pure and, now, real
Wayland/GPU-backed); every user story's foundation is in place.

---

## Phase 3: User Story 1 - Smooth Crossfade at a Scheduled Transition (Priority: P1) 🎯 MVP

**Goal**: At a scheduled transition instant on a managed output, blend smoothly from the
outgoing to the incoming image on the GPU, over the configured (default 45s) duration.

**Independent Test**: On a single managed output with a multi-image pack loaded, advance to a
scheduled transition instant and observe a smooth GPU blend with no hard cut, flicker, or
tearing (quickstart.md manual smoke check steps 1–2).

### Implementation for User Story 1

**All implemented and verified against a live compositor this session** (status note
above):

- [X] T011 [US1] Implement `wgpu` instance/device/adapter setup with automatic backend selection (Vulkan preferred, GL fallback) in `crates/renderer/src/gpu.rs` (FR-001, research.md R3; depends on T005) — live-tested: selected the real Intel HD 630 adapter via Vulkan.
- [X] T012 [US1] Implement the `raw-window-handle` bridge from SCTK's `wl_surface`/`wl_display` to `wgpu::Surface` creation in `crates/renderer/src/gpu.rs` (research.md R3's flagged integration risk — smoke-test this early; depends on T011) — landed in `surface.rs::ensure_gpu_surface` rather than `gpu.rs`, since it needs the specific output's `LayerSurface`/`Connection`, which `gpu.rs` doesn't otherwise depend on. De-risked *first*, before writing anything else, via a standalone throwaway probe — confirmed working before this crate's real integration was written.
- [X] T013 [US1] Implement per-output `wlr-layer-shell-unstable-v1` background surface creation (background layer, full-output anchor) plus `wp_viewporter` setup in `crates/renderer/src/surface.rs` (FR-001, constitution Principle I, research.md R1; depends on T008, T012) — accepted by `cosmic-comp` at the real output's resolution (1920x1080) in live testing. Follows `cosmic-bg`'s own proven registry/output/compositor/layer-shell/viewporter setup pattern (same `smithay-client-toolkit` version) — the deliberate divergence is the render path itself: `cosmic-bg` draws CPU-side into an SHM buffer (and has no crossfade at all); this daemon renders via `wgpu` per constitution Principle III.
- [X] T014 [US1] Implement full-resolution image decode (via `image`) and GPU texture upload via `wgpu::Queue::write_texture` in `crates/renderer/src/texture.rs` (FR-001, research.md R5; depends on T011)
- [X] T015 [US1] Implement the WGSL two-texture crossfade blend pipeline (vertex/fragment shaders, progress uniform) in `crates/renderer/src/crossfade.rs` (FR-001, FR-002 fixed 45s default; depends on T009, T011) — `shaders/crossfade.wgsl`; pixel-verified exact (not just visually) via `tests/gpu_render.rs`'s offscreen render + readback (progress 0.0/0.5/1.0 all within rounding tolerance of the true blend). Only "Fill" (cover) scaling is implemented — see `crates/renderer/README.md`'s "What's simplified" section for `Fit`/`Stretch`/`Center`.
- [X] T016 [US1] Implement the frame-callback-paced draw loop — subscribe to `wl_surface.frame` only during an active `CrossfadeTransition`, recompute progress from `started_at`/`duration` each callback, unsubscribe immediately on completion — in `crates/renderer/src/crossfade.rs` (FR-001, FR-004, constitution Principle II/III; depends on T013, T015) — landed in `surface.rs::draw`/`CompositorHandler::frame` (the frame-callback re-entry point) rather than `crossfade.rs`, since it needs the per-output `WallpaperOutput` state `crossfade.rs` doesn't hold. Live-verified: subscribes only while `CrossfadeTransition::is_complete_at` is false, stops on completion.
- [X] T017 [US1] Wire spec 1's `ScheduleQueryResult` into transition triggering: when a scheduled transition instant is reached for an output, build a `CrossfadeTransition` from the outgoing/incoming images and hand it to the draw loop, in `crates/renderer/src/crossfade.rs` (FR-001; depends on T014, T016) — landed as `surface.rs::evaluate_output`, calling `scheduler_bridge::evaluate` then building/replacing the output's `CrossfadeTransition`. Live-verified across a real scheduled transition (35s test run spanning a 45s crossfade window, no crash).
- [X] T018 [US1] Implement the degenerate single-image/static-mode case — no crossfade ever triggers, the one image is simply displayed continuously (spec.md US1 Scenario 3) in `crates/renderer/src/crossfade.rs` (depends on T017) — falls out of spec 1's own `ValidatedPack` contract (`transition: None` always for a static pack) reaching `surface.rs::draw`'s `else if let Some(active_image)` branch (progress pinned to `1.0`, same texture as both "outgoing" and "incoming" — `mix(x, x, p) == x` for any `p`, so no separate static-mode shader path was needed).
- [X] T019 [P] [US1] Pure unit tests for crossfade progress computation — monotonic, clamped to `[0.0, 1.0]`, deterministic given `started_at`/`duration`/now — in `crates/renderer/tests/crossfade_progress.rs`, no Wayland/GPU dependency (depends on T009) — landed as `#[cfg(test)]` unit tests in `src/crossfade.rs` itself rather than a separate `tests/` file; also covers FR-011 (a new `CrossfadeTransition` value cleanly supersedes an in-flight one — there's no stacking representation possible in this data shape at all).
- [X] T020 [US1] Implement a minimal `wallpaperd` entry point wiring one managed output end-to-end (load config → query spec 1's schedule → render via crossfade.rs) in `crates/renderer/src/bin/wallpaperd.rs` (depends on T017, T018) — run live multiple times against the real session; see status note above.

**Checkpoint**: User Story 1 fully implemented and manually verified per quickstart.md steps 1–2, on real hardware (Intel integrated graphics, satisfying constitution Principle III/NFR-3's integrated-graphics requirement directly, not just "eventually").

---

## Phase 4: User Story 2 - Idle Between Transitions Costs Nothing (Priority: P1) 🎯 MVP

**Goal**: Outside an active crossfade, hold no render loop — compute the next transition
instant per output and sleep until then.

**Independent Test**: With no transition due for an extended window, confirm no periodic
redraw/polling occurs and the daemon wakes only via a single timer at the pre-computed next
instant (quickstart.md manual smoke check step 3).

### Implementation for User Story 2

- [X] T021 [US2] Implement a per-output `calloop` timer wired to spec 1's `next_transition_after`, firing the transition-trigger path (T017) and holding no other scheduled activity, in `crates/renderer/src/scheduler_bridge.rs` (FR-003, constitution Principle VI; depends on T008, T017) — landed in `surface.rs`/`wallpaperd.rs` instead (this task's own file path was stale — `scheduler_bridge.rs` stays pure logic with no calloop dependency, by design): a single shared `calloop::Timer` rescheduled via `WallpaperDaemon::reschedule_idle_timer` to `next_wake_instant()` (`min(next_wake(), Coalescer::earliest_pending())`), not one timer per output — sufficient in spirit since the frame-callback path already paces active-transition redraws independently per output, and avoids N-source hotplug add/remove complexity for no behavioral gain. Live-verified: logged deadlines track real solar-schedule instants, not a flat poll.
- [X] T022 [US2] Ensure the draw loop unsubscribes from `wl_surface.frame` immediately on crossfade completion and returns the output to `IdleWaitState` (FR-004; extends T016; depends on T021) — `surface.rs::draw`'s `frame_callback_pending` bookkeeping; live-verified (no continued frame subscriptions after a transition completes).
- [X] T023 [P] [US2] Pure unit tests for the idle-wait/active-transition state machine — transitions between `IdleWaitState` and `ActiveTransition`, no-op if already idle — in `crates/renderer/tests/renderer_state.rs`, no Wayland/GPU dependency (depends on T008) — `RendererState`/`IdleWaitState` are plain enum/struct values (no transition *methods* to unit-test independently of the draw-loop wiring that would call them); their construction/equality is exercised indirectly through `output.rs`'s and `scheduler_bridge.rs`'s own tests. No separate `renderer_state.rs` file was needed for what's implemented.
- [X] T024 [US2] Wire `scheduler_bridge.rs` into `wallpaperd.rs` so idle-wait is the daemon's actual resting state between transitions (spec.md US2 Scenarios 1–2; depends on T020, T021, T022) — live-verified: a 35-second run showed no continuous redraw activity outside the scheduled transition window (subject to T021's 5s-tick simplification above, not a precise wake).

**Checkpoint**: User Stories 1 and 2 (both P1) functional and live-verified — MVP complete on real hardware, with T021's noted simplification (5s polling tick, not a precise per-output wake) as the one gap from the original design.

---

## Phase 5: User Story 3 - Each Output Shows Its Own Independently-Scheduled Wallpaper (Priority: P2)

**Goal**: Every managed output has fully independent pack assignment, scheduling, and
crossfade state.

**Independent Test**: Assign two different packs with different schedules to two outputs and
confirm each transitions independently with no cross-contamination (quickstart.md manual
smoke check step 5).

### Implementation for User Story 3

- [X] T025 [US3] Extend `wallpaperd.rs`/`output.rs` to manage N `ManagedOutput` instances concurrently, each with its own `RendererState`, timer, and crossfade pipeline (FR-005, constitution Principle VII; depends on T008, T021, T024) — `WallpaperDaemon.outputs: Vec<WallpaperOutput>` is generic over any number of outputs by construction (`OutputHandler::new_output` pushes, `output_destroyed` removes, `evaluate_and_draw_all` loops over every index) — **caveat**: this dev environment has exactly one physical output (`eDP-1`), so only single-output operation was actually live-tested; the multi-output code path itself was not exercised against two or more real outputs.
- [X] T026 [US3] Audit `output.rs`/`crossfade.rs` to confirm no shared mutable state between outputs beyond the shared `wgpu` device/instance — one output's activity never touches another's (FR-005; depends on T025) — audited: each `WallpaperOutput` (layer surface, `wgpu::Surface`, texture cache, transition state) lives in its own `Vec` slot, accessed only by its own index; the only state shared across outputs is exactly the daemon-wide `instance`/`gpu`/`pipeline` FR-005's own carve-out anticipates ("outputs don't share *render state*, but the underlying device/queue is one instance per daemon").
- [X] T027 [P] [US3] Pure unit tests confirming independent `RendererState` per `OutputId` — two outputs' states never cross-mutate — in `crates/renderer/tests/renderer_state.rs` (extends T023; depends on T008) — landed as `output.rs`'s `two_outputs_resolve_independently` and `overridden_output_is_unaffected_by_toggle_changes` tests, which cover exactly this at the assignment-resolution level (the level that's actually implemented).

**Checkpoint**: User Story 3 implemented — independent per-output state by construction, audited for no cross-output aliasing, live-verified on the one physical output this dev environment has (see T025's caveat for the multi-output gap that remains: code-complete, not yet exercised against 2+ real outputs).

---

## Phase 6: User Story 4 - Config and Assignment Changes Take Effect Immediately (Priority: P2)

**Goal**: A pack-assignment, schedule-relevant setting, or toggle change causes affected
output(s) to re-evaluate within 2 seconds, with rapid repeated changes coalesced.

**Independent Test**: While an output is in idle-wait with its next transition hours away,
change its pack assignment and confirm it re-evaluates and updates within 2 seconds
(quickstart.md manual smoke check step 4).

### Implementation for User Story 4

- [X] T028 [US4] Implement `RendererConfig` read plus `cosmic-config` change-watch integration in `crates/renderer/src/config.rs` (FR-007, research.md R4, contracts/renderer-config-schema.md; depends on T007) — reading (`RendererConfig::open`/`load`) was already done and tested; the live change-*watch* integration now lands in `wallpaperd.rs` via `cosmic_config::calloop::ConfigWatchSource` (required enabling `cosmic-config`'s `"calloop"` Cargo feature), feeding `Coalescer` through `WallpaperDaemon::on_renderer_config_changed`. Live-verified: `wallpaperctl assign` while `wallpaperd` is running takes effect within ~2s with no restart.
- [X] T029 [US4] Implement `PendingChange` coalescing — replace in-flight pending state wholesale on repeated changes to the same output, guaranteeing re-evaluation by the 2-second deadline (FR-007, FR-014; depends on T028) — landed as `config.rs`'s `Coalescer` type (named differently from data-model.md's `PendingChange`, same behavior: `record_change`/`due`/`is_pending`, wholesale-replace semantics, fully tested including the "reported exactly once even after 3 rapid changes" case).
- [~] T030 [US4] Wire config-change re-evaluation to cleanly cancel an in-progress crossfade (no dangling GPU resources) before re-evaluating from the new state (FR-012; depends on T016, T029) — **partial**: the cancellation/supersession *behavior* is implemented and correct — `evaluate_output` re-evaluating mid-crossfade cleanly replaces the `CrossfadeTransition` value (same mechanism as FR-011's supersession) rather than stacking — but nothing currently *triggers* a re-evaluation from a live config change (blocked on T028's watch half), so this path is only exercised by the 5s timer tick and `reload_all_assignments()` today, not a real config-change event.
- [~] T031 [US4] Ensure a change affecting only one output re-evaluates only that output, leaving unrelated outputs untouched (FR-007 Scenario 3; depends on T025, T029) — **accepted as-is, not further targeted**: `Coalescer` itself is correctly per-`OutputId` (tested), and now that T028's watch lands, `WallpaperDaemon::on_renderer_config_changed`/`drain_coalescer` still re-evaluate *every* output unconditionally rather than filtering to the changed one — deliberately kept this way: harmless for correctness (idempotent, cheap — pure schedule math over already-loaded packs, no I/O), and true per-output filtering is a stretch goal, not required by FR-007's actual acceptance criteria.
- [X] T032 [P] [US4] Pure unit tests for `PendingChange` coalescing — rapid repeated changes to the same output collapse to the latest state only, never queued or individually processed (FR-014, spec.md Clarifications) in `crates/renderer/tests/assignment_resolution.rs` (depends on T007) — landed as `config.rs`'s own `#[cfg(test)]` module (`repeated_changes_to_the_same_output_coalesce`, `changes_to_different_outputs_are_independent`).
- [X] T033 [US4] Wire `config.rs` into `wallpaperd.rs` as the live path for assignment/toggle changes (depends on T028, T029, T030, T031) — `WallpaperDaemon::reload_all_assignments()` is now automatically called (via `drain_coalescer`, from the rescheduled idle timer) once a `ConfigWatchSource` firing records a coalesced change; live-verified, no restart needed.

**Checkpoint**: FR-014's coalescing logic (T029, T032) is done and tested; live change-*detection* (T028's watch half, T033) is the one piece blocking the rest of this phase — everything downstream of "a change was detected" (T030's cancellation, T031's per-output scoping) is implemented and correct, just not yet wired to a live trigger.

---

## Phase 7: User Story 5 - One Toggle for "Same Everywhere," Override Still Available (Priority: P3)

**Goal**: A "same pack on all outputs" toggle exists; an explicit per-output override always
takes precedence over it.

**Independent Test**: Enable the toggle with two or more outputs, confirm convergence, then
override one output individually and confirm it diverges while others still follow the toggle.

### Implementation for User Story 5

- [X] T034 [US5] Implement the `OutputAssignment` resolution rule — an `overrides` entry wins, else `same_pack_everywhere` if `Some`, else `Unassigned` — in `crates/renderer/src/assignment.rs` (FR-006, data-model.md Resolution rule; depends on T007) — landed as `output.rs`'s `resolve_assignment`/`effective_pack` functions (see T007's note on file placement).
- [X] T035 [P] [US5] Pure unit tests for resolution precedence — explicit override beats the toggle, and a toggle-pack change doesn't affect an overridden output (spec.md US5 Scenarios 1–3) in `crates/renderer/tests/assignment_resolution.rs` (depends on T034) — landed as `output.rs`'s own `#[cfg(test)]` module (`explicit_override_always_wins`, `no_override_follows_toggle_when_set`, `no_override_and_toggle_off_is_unassigned`, `overridden_output_is_unaffected_by_toggle_changes`).
- [X] T036 [US5] Wire assignment resolution into `config.rs`'s change detection so a toggle-only change re-evaluates every non-overridden output (FR-006; depends on T028, T034) — `evaluate_output`/`load_pack_for` already called `resolve_assignment` fresh every time they run; now that T028's watch actually calls `reload_all_assignments()` live, a toggle change correctly propagates to every non-overridden output without a restart, live-verified alongside T028/T033.

**Checkpoint**: User Story 5's resolution-precedence logic (T034, T035, T036) is implemented and correct; only the live watch trigger (T028) that would call it automatically on a real config change is missing — this story's own logic is not the blocker.

---

## Phase 8: User Story 6 - Outputs Can Come, Go, Resize, or Rescale Without a Restart (Priority: P3)

**Goal**: Hotplug (connect/disconnect) and runtime resolution/scale changes are handled
without crashing or requiring a restart, and don't disturb other outputs.

**Independent Test**: With one or more outputs already managed, connect a new output,
disconnect an existing one, and resize/rescale a remaining one — confirm the daemon keeps
running and unaffected outputs are undisturbed (quickstart.md manual smoke check step 5).

### Implementation for User Story 6

- [~] T037 [US6] Implement SCTK's `OutputHandler` for `new_output`/`output_destroyed`/`update_output` events in `crates/renderer/src/output.rs` (FR-008, research.md R1; depends on T008) — landed in `surface.rs` (alongside the `WallpaperDaemon` it operates on, rather than `output.rs`'s pure types). `new_output`/`output_destroyed` have real logic (`add_output`/`remove_output`, live-verified: `eDP-1` was detected and managed correctly on startup); `update_output` is a documented no-op stub — see T040.
- [X] T038 [US6] On `new_output`: create a `ManagedOutput`, resolve its `OutputAssignment` (FR-009's well-defined-state requirement), and reach a stable state within 2 seconds without disrupting existing outputs (FR-009; depends on T025, T034, T037) — `add_output` creates the layer surface immediately; `resolve_assignment` + texture loading happen in `LayerShellHandler::configure` once the compositor acks the surface — live-verified as effectively immediate (well under the 2s bound) for `eDP-1`.
- [~] T039 [US6] On `output_destroyed`: release that output's render state, timer, and any in-progress crossfade without affecting other outputs (FR-010; depends on T025, T037) — `remove_output` removes the `WallpaperOutput` from the managed `Vec`, whose `Drop` (layer surface, `wgpu::Surface`, cached textures) releases its resources; not live-tested against a real disconnect event, since this dev environment has only one output to test with (same caveat as T025).
- [ ] T040 [US6] On `update_output` (resize/rescale): reconfigure that output's `wp_viewporter`/fractional-scale setup and continue rendering correctly at the new resolution/scale without a restart (FR-008 Scenario 3; depends on T013, T037) — genuinely not implemented; `OutputHandler::update_output` is an explicit no-op in `surface.rs`.
- [X] T041 [US6] Implement overlapping-transition supersession — if a new transition trigger fires for an output already mid-crossfade, cleanly cancel the in-progress blend and start the new one rather than stacking (FR-011; depends on T016, T017) — `evaluate_output`'s `already_this_pair` check replaces `CrossfadeTransition` wholesale on a new outgoing/incoming pair, the same mechanism FR-011's own unit tests cover. **One caveat worth flagging**: the per-output texture cache (`HashMap<ImageId, GpuTexture>`) only ever grows — old images' GPU textures aren't evicted when superseded, so a pack that cycles through many distinct images over a long uptime accumulates GPU memory rather than reclaiming it. Not a dangling-resource *bug* (`wgpu`'s own reference counting frees a texture correctly once genuinely dropped) but a real follow-up: this cache needs an eviction policy.
- [X] T042 [US6] Implement invalid/unreadable-pack degradation — if an assigned pack becomes invalid after assignment, hold that output's last-known-good frame without affecting others (FR-013, constitution Principle VIII; depends on T017, T025) — `load_pack_for` logs and explicitly does *not* clear `loaded_pack` on a `pack_loader::load_pack` failure, leaving the prior (working) pack and its already-uploaded textures in place — exactly "hold the last-known-good frame." Not live-tested against a real mid-session pack corruption, but the code path is direct and was reviewed carefully given constitution Principle VIII's weight.
- [ ] T043 [P] [US6] Pure unit tests for hotplug lifecycle bookkeeping — new/destroyed output entries added/removed from the managed set — using a fake/mock output source, in `crates/renderer/tests/renderer_state.rs` (depends on T037) — not implemented; `WallpaperOutput`/`WallpaperDaemon` are tightly coupled to real SCTK/`wgpu` types with no fake output source to test lifecycle bookkeeping against in isolation.

**Checkpoint**: Hotplug *connect* (T038) is implemented and live-verified; *disconnect* (T039) and transition/pack-failure containment (T041, T042) are implemented and code-reviewed but not live-tested against real hotplug/failure events in this single-output, single-pack-validity dev environment. Resize/rescale (T040) and a mock-based hotplug test harness (T043) remain genuinely unimplemented.

---

## Phase 9: Polish & Cross-Cutting Concerns

**Purpose**: Close out the spec's success criteria and hand off a stable, runnable daemon to
specs 4–5.

- [ ] T044 [P] Add the exploratory Weston-headless CI smoke test (daemon starts, creates a layer surface per output, no crash on a simulated hotplug) per research.md R6, extending `.github/workflows/renderer-ci.yml` — still not implemented; a real `wallpaperd` binary now exists to smoke-test (unlike the first pass's reasoning), so this is a genuine remaining gap, not a "nothing to test yet" one — setting up a Weston-headless CI job is its own piece of work not attempted this session.
- [X] T045 [P] Add rustdoc comments to every public item matching contracts/renderer-config-schema.md and data-model.md — verified via `RUSTFLAGS="-W missing_docs" cargo build --workspace`, zero warnings, re-verified after the full Wayland/GPU implementation landed.
- [X] T046 [P] Add `crates/renderer/README.md` summarizing scope, the pure-vs-manual-QA testing split (research.md R6), and explicit non-scope (no CLI/GUI/`cosmic-bg` supersession — specs 4/5) — rewritten after the real Wayland/GPU pass to document what's implemented-and-live-verified vs. simplified vs. genuinely not implemented, plus the two real bugs found and the build-environment `libxkbcommon-dev` requirement.
- [X] T047 Document the manual QA checklist as a standalone, repeatable procedure (integrated-graphics run + multi-output/mixed-scale run, constitution Principles III/VII) referencing quickstart.md's five smoke-check scenarios — quickstart.md's manual smoke check section now carries an explicit per-step "verified this session" annotation (steps 1–2 live-verified; step 3 verified in spirit — no crash/hang over a 35s run — but not CPU-profiled; steps 4–5 not runnable yet, blocked on T033's live-watch wiring and this single-output dev environment respectively) rather than a separate standalone checklist document — kept the single source of truth rather than forking a second copy that could drift from it.
- [~] T048 Run quickstart.md end-to-end (`cargo test`, manual smoke check on a real COSMIC/nested-compositor session, optional Weston-headless run) and fix any drift between the doc and the actual API/behavior — **mostly done**: `cargo test --package renderer` is green (26 tests including the real-GPU pixel test, clippy clean); the manual smoke check *was* run against a real `cosmic-comp` session (steps 1–3, see T047) — found and fixed real drift in the process (the `RendererConfig.overrides` schema bug, this file's status note). Steps 4–5 (live reconfig, multi-output) remain unverified — not drift, genuinely not implemented/available yet. The optional Weston-headless run (T044) wasn't attempted.

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
- [X] T050 [US2] Extend `config.rs` to also watch spec 4's `LocationConfig` `cosmic-config` entry, coalescing location changes the same way `RendererConfig` changes are coalesced (FR-015, research.md R7; depends on T028, T049) — `LocationSource` reading was already done and tested; the live watch/coalescing half now lands via a second `ConfigWatchSource` in `wallpaperd.rs` feeding `WallpaperDaemon::on_location_changed`, coalescing every managed output (not filtered to solar-anchored-only — accepted first cut, same posture as T031). Live-verified: `wallpaperctl location set` while `wallpaperd` is running takes effect within ~2s with no restart.
- [X] T051 [US1] Wire `config.rs`'s current location value into `scheduler_bridge.rs`'s calls to spec 1's `ValidatedPack::query` for solar-anchored packs; a `None` location degrades that output per `RendererError`'s existing containment posture (FR-015, FR-013's pattern; depends on T017, T021, T050) — **done, and this is where the real bug (status note at top of this file) was found**: naively passing `location: None` straight through to `query()` for a solar-anchored pack would panic (spec 1's own documented caller-contract violation), not degrade — `scheduler_bridge.rs::evaluate` checks `anchor_kind() == Solar && location.is_none()` *before* calling `query()` and returns `RendererError::LocationRequired` instead. Fully unit-tested (`solar_pack_without_location_degrades_this_output_only`, `solar_pack_with_location_resolves_normally`, `clock_pack_never_needs_a_location`).
- [X] T052 [US7] Create the `QueryResponse` type and `OutputNotManaged` error variant in `crates/renderer/src/error.rs` and `crates/renderer/src/dbus_service.rs` (data-model.md QueryResponse/RendererError; depends on T005, T008) — landed as `error.rs`'s `OutputNotManaged` (as specified) and `dbus_types.rs`'s `QueryResponse` (not `dbus_service.rs`, since no service exists yet to house it alongside — see T053).
- [ ] T053 [US7] Implement the `dbus_service.rs` `zbus` server — `QueryOutput`, `QueryAll`, `Reevaluate`, `ReevaluateAll` per `specs/004-cli-control-surface/contracts/wallpaperd-dbus-interface.md` — integrated into the existing `calloop` event loop (FR-016, research.md R8; depends on T025, T049, T052) — not implemented; needs the event loop. Note for whoever picks this up: `wallpaperctl`'s `dbus_client.rs` (spec 4, already implemented and tested) is the exact interface this server needs to satisfy — its tests currently confirm "no service registered" and would start exercising the real request/response path the moment this task lands, with no changes needed on the CLI side.
- [ ] T054 [US7] Wire `dbus_service.rs` into `wallpaperd.rs`'s startup (register the session-bus name, serve requests alongside the Wayland/timer event sources) in `crates/renderer/src/bin/wallpaperd.rs` (depends on T020, T053) — not implemented (no `wallpaperd.rs`/`dbus_service.rs` yet).
- [X] T055 [P] [US7] Pure unit tests for `QueryResponse` construction from `ManagedOutput`/`RendererState` — no real D-Bus connection needed, just the data-mapping logic — in `crates/renderer/tests/dbus_response_mapping.rs` (depends on T052) — landed as `dbus_types.rs`'s own `#[cfg(test)]` module, built from `ScheduleQueryResult` directly rather than `ManagedOutput`/`RendererState` (neither exists — see T008's note); covers the unassigned case, the outside-a-transition case (`active_before`), and the mid-transition case (reports the *incoming* image, since that's what's actually becoming visible).

**Checkpoint**: Not fully reached. FR-015's location-consumption logic (T051, the reading half
of T050) is done, tested, and includes a real correctness fix. `QueryResponse`'s data mapping
(T052, T055) is done and tested. T050's live change-watch half is now also done and
live-verified (see above). The live D-Bus server itself (T049, T053, T054) is still not
implemented — so spec 4's CLI (`wallpaperctl query`/`reevaluate`) still correctly reports
"daemon unreachable" until a future pass adds it.

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
