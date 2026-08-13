# Feature Specification: Wallpaper Renderer

**Feature Branch**: `003-wallpaper-renderer`

**Created**: 2026-08-13

**Status**: Draft

**Input**: User description: "spec 3 — from docs/PRD.md, spec breakdown item 3: renderer,
covering FR-14 (transitions crossfade over a configurable duration using GPU compositing),
FR-15 (outside of an active transition the daemon holds no render loop and wakes only at the
next scheduled instant), FR-17 (each Wayland output can be assigned an independent pack,
independent of other outputs), FR-18 (a 'same pack on all outputs' convenience toggle exists,
with per-output override still possible underneath), and FR-19 (output hotplug and fractional
scaling are handled without crashing or requiring a daemon restart). The highest-risk spec per
the PRD's own breakdown — depends on spec 1 (core scheduling engine) for the active/incoming/
outgoing image and progress-fraction answer per output, and spec 2 (pack format & loading) for
the loaded pack/static image each output is assigned. Also folds in FR-16 (a config or output
change immediately re-evaluates the current/next transition) as necessary supporting
infrastructure for FR-17–FR-19, even though the PRD's own spec breakdown table doesn't list it
under this spec explicitly — see Assumptions."

## Clarifications

### Session 2026-08-13

- Q: Should the crossfade transition duration be a single fixed value, or should it scale
  with the length of the upcoming period (PRD Open Question OQ-2)? → A: Fixed duration only
  — one configurable duration (default 45s) applies to every transition. Period-aware scaling
  is deferred as a possible future enhancement, not part of this spec's contract.
- Q: How quickly must an affected output visibly react to a hotplug event or a config/pack-
  assignment change? → A: Under 2 seconds for both — snappy but leaves room for a
  `cosmic-config` watch round-trip on the config-change path.
- Q: What's the largest number of simultaneously managed outputs this spec needs to handle
  correctly? → A: Up to 8 outputs — covers larger docking/multi-monitor-wall desktop setups
  with headroom, without over-engineering for an unbounded/video-wall scale.
- Q: If a second config/output change arrives for the same output while the first change's
  re-evaluation is still being processed, how should the daemon handle it? → A: Coalesce
  rapid-fire changes to the same output — only the latest state as of when re-evaluation
  actually runs is applied; intermediate changes are superseded, not queued or individually
  processed.

### Amendment 2026-08-13 (spec 4 dependency)

While planning spec 4 (CLI control surface), two gaps in this spec surfaced: nothing here
reads a location for solar-anchored packs, and nothing here exposes any live interface for an
external control surface to query state or trigger re-evaluation. Both are necessary for
spec 4's own contract to work at all, so — same posture as this spec's own FR-16 scope
decision — they're folded in directly as FR-015 and FR-016 below, and User Story 7, rather
than left as a gap discovered mid-implementation. See spec 4's
`specs/004-cli-control-surface/contracts/location-config-schema.md` and
`.../wallpaperd-dbus-interface.md` for the exact shapes this spec now commits to.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Smooth Crossfade at a Scheduled Transition (Priority: P1)

At the moment the scheduling engine (spec 1) says a transition is due on a managed output, the
user sees the outgoing image smoothly blend into the incoming image over a short window,
rather than an abrupt hard cut — the project's core visual differentiator from a plain
slideshow.

**Why this priority**: This is the single feature the whole project exists to deliver (PRD
Goal G2, constitution Principle III). Nothing else in this spec matters if this doesn't work.

**Independent Test**: On a single managed output with a multi-image pack loaded, advance
(or wait for) a scheduled transition instant and observe the output smoothly blend from the
outgoing image to the incoming image over the configured duration, ending cleanly on the
incoming image with no flicker, tearing, or visible hard cut at either end.

**Acceptance Scenarios**:

1. **Given** a managed output showing image A with a transition to image B due at time T,
   **When** the clock reaches T, **Then** the output begins blending from A to B, reaching
   B fully and exclusively displayed once the configured crossfade duration has elapsed.
2. **Given** a crossfade in progress, **When** the transition completes, **Then** the
   renderer immediately stops any per-frame redraw activity for that output (Principle II/VI)
   rather than continuing to redraw an unchanging frame.
3. **Given** a pack with only one image (the static/degenerate case, spec 1), **When** time
   passes, **Then** no crossfade ever triggers and the single image is simply displayed
   continuously.

---

### User Story 2 - Idle Between Transitions Costs Nothing (Priority: P1)

Outside of an active crossfade, the daemon does not poll, does not hold an active render loop,
and does not consume meaningful CPU or GPU — it computes the next transition instant per
output and sleeps until then.

**Why this priority**: This is the non-negotiable efficiency counterpart to User Story 1
(constitution Principle VI, PRD Goal G5) — without it, "smooth crossfade" would come at the
cost of a background process that never stops working, which defeats the point of the
project's own differentiation from a naive polling slideshow.

**Independent Test**: With no transition due for an extended window, observe the daemon's
CPU/GPU activity and confirm it performs no periodic redraw or polling work, waking only when
a timer fires at the pre-computed next-transition instant (or an external change per User
Story 5 interrupts it early).

**Acceptance Scenarios**:

1. **Given** a managed output with no transition due for the next several hours, **When** the
   daemon is observed over that window, **Then** it performs no redraw work and no periodic
   polling — a single sleeping timer accounts for its only scheduled activity.
2. **Given** a transition instant is reached, **When** the crossfade begins, **Then** the
   daemon transitions from idle-wait to active-transition state for exactly the duration of
   that blend, then returns to idle-wait (constitution Principle VI's two-state model).
3. **Given** multiple managed outputs with different pack schedules, **When** one output's
   transition fires, **Then** only that output enters active-transition state — the others
   remain idle-wait, each on its own independent timer.

---

### User Story 3 - Each Output Shows Its Own Independently-Scheduled Wallpaper (Priority: P2)

A user with multiple monitors assigns a different pack (or static image) to each one, and each
output's active image, transition timing, and crossfade state are computed and rendered
completely independently of the others.

**Why this priority**: Multi-output correctness is explicitly the highest-risk area of this
spec (constitution Principle VII) — it's no longer inherited from `cosmic-bg` now that this
project owns the renderer outright — but a single-output setup (Stories 1–2) is still a fully
functional, demonstrable product on its own.

**Independent Test**: Assign two different packs with different schedules to two outputs,
observe both over a period spanning at least one transition on each, and confirm each output's
active image and transition timing match its own assigned pack's schedule, with no
cross-contamination between outputs.

**Acceptance Scenarios**:

1. **Given** two managed outputs each assigned a different pack, **When** either output's
   transition instant is reached, **Then** only that output's image changes — the other
   output's currently-displayed image is unaffected.
2. **Given** two managed outputs, **When** one is actively crossfading, **Then** the other
   output's idle-wait or active-transition state is entirely unaffected by that activity.

---

### User Story 4 - Config and Assignment Changes Take Effect Immediately (Priority: P2)

When a user swaps which pack is assigned to an output, edits location or crossfade settings,
or an output is added, removed, or resized, the affected output(s) immediately reflect the new
state rather than waiting for whatever transition instant the *old* schedule would have woken
up for next.

**Why this priority**: Without this, User Story 3's per-output assignment and User Story 6's
hotplug handling would both feel broken in practice — a pack swap or newly connected monitor
that silently does nothing until some arbitrary future instant reads as a bug, not a working
feature. This is foundational to making Stories 3, 5, and 6 actually usable, but the project's
core crossfade/idle behavior (Stories 1–2) doesn't depend on it, so it is not itself P1.

**Independent Test**: While an output is in idle-wait with its next transition hours away,
change that output's pack assignment (or a schedule-relevant setting) and confirm the output
re-evaluates and updates its displayed image/next-wake timer immediately, without waiting for
the original scheduled instant.

**Acceptance Scenarios**:

1. **Given** an output in idle-wait with its next transition hours away, **When** its pack
   assignment changes, **Then** the output immediately re-evaluates against the new pack and
   updates its displayed image and next-wake timer accordingly.
2. **Given** an output mid-crossfade, **When** a change arrives that affects that output,
   **Then** the in-progress crossfade is cancelled cleanly (no dangling GPU resources, no
   visible glitch) and re-evaluation starts fresh from the new state.
3. **Given** a change that affects only one of several managed outputs, **When** that change
   is applied, **Then** only the affected output re-evaluates — unrelated outputs are
   untouched (consistent with User Story 3's independence guarantee).

---

### User Story 5 - One Toggle for "Same Everywhere," Override Still Available (Priority: P3)

A user with multiple monitors who wants the same pack on all of them flips a single toggle
instead of assigning it output-by-output, while still being able to override any individual
output afterward without fighting the toggle.

**Why this priority**: A real convenience (PRD FR-18) for the common case of not wanting
per-output configuration, but every output already has a well-defined assignment without it
(explicit per-output assignment, Story 3) — it's a layer on top, not a blocker.

**Independent Test**: Enable the "same pack everywhere" toggle with two or more outputs
present, confirm all outputs converge on the same pack, then override one output individually
and confirm it diverges while the toggle remains otherwise in effect for the rest.

**Acceptance Scenarios**:

1. **Given** multiple managed outputs, **When** the "same pack on all outputs" toggle is
   enabled with a chosen pack, **Then** every managed output immediately shows that pack's
   schedule (User Story 4 applies).
2. **Given** the toggle is enabled, **When** one output is given an explicit per-output
   override, **Then** that output follows its own override while the remaining outputs
   continue following the toggle's pack.
3. **Given** an output has an explicit override in place, **When** the toggle's pack
   selection changes, **Then** the overridden output is unaffected by that change.

---

### User Story 6 - Outputs Can Come, Go, Resize, or Rescale Without a Restart (Priority: P3)

Monitors get connected, disconnected, resized, or have their scale factor changed while the
daemon is running (docking/undocking a laptop, changing display settings) — none of it
requires restarting the daemon, and none of it disrupts the other, unaffected outputs.

**Why this priority**: A real quality-of-life and stability requirement (PRD FR-19,
constitution Principle VII) that matters most in exactly the multi-output world Stories 3–5
already establish — a single always-on-one-monitor setup barely exercises it, so it's ordered
after the behaviors it builds on.

**Independent Test**: With one or more outputs already being managed with active pack
assignments, connect a new output, disconnect an existing one, and resize/rescale a remaining
one — in each case, confirm the daemon keeps running, the unaffected outputs are undisturbed,
and the changed output reaches a well-defined, correctly-rendered state without a restart.

**Acceptance Scenarios**:

1. **Given** the daemon is running with one or more managed outputs, **When** a new output is
   connected, **Then** the daemon begins managing it without restarting or disrupting existing
   outputs, and it reaches a well-defined assignment state (per the "same pack everywhere"
   toggle if enabled, User Story 5; otherwise unassigned until explicitly configured).
2. **Given** a managed output is disconnected (mid-idle-wait or mid-crossfade), **When** the
   disconnection is detected, **Then** its resources are cleaned up without affecting any
   other managed output, and the daemon does not crash or hang.
3. **Given** a managed output changes resolution or fractional scale factor at runtime,
   **When** the change is detected, **Then** rendering continues correctly at the new
   resolution/scale with no crash, without requiring a daemon restart.

---

### User Story 7 - External Control Surfaces Can Query and Trigger This Daemon Live (Priority: P2)

A running instance of this daemon can be asked, from outside its own process, "what's active
on this output right now and when's the next transition" and "recompute this output's
schedule now" — the live counterpart to the config-file-based control every other user story
already supports.

**Why this priority**: Added per the Amendment above — spec 4 (CLI control surface) cannot
fulfill its own "query current/next transition" and "force an immediate re-evaluation"
commands without this daemon exposing *something* live to call into; every other form of
control in this spec (assignment, toggle, now location) already works purely through
`cosmic-config`, which has no mechanism for a reader to ask about in-memory-only state. Ranked
P2, not P1, because the daemon is already fully functional and correct (Stories 1–6) with
nobody ever querying it — this is a control-surface enabler, not a rendering behavior.

**Independent Test**: With the daemon running and at least one output actively scheduled, use
the exposed interface directly (independent of spec 4's CLI actually existing yet) to query
that output's current image/next transition and to trigger a re-evaluation, and confirm both
calls succeed and reflect the daemon's real internal state.

**Acceptance Scenarios**:

1. **Given** the daemon is running with an assigned, actively-scheduled output, **When** an
   external caller queries that output, **Then** it receives the currently active image and
   next scheduled transition instant, matching the daemon's actual internal state.
2. **Given** an output with no assignment, **When** an external caller queries it, **Then**
   the response clearly indicates "unassigned" rather than an error or stale data.
3. **Given** the daemon is running, **When** an external caller triggers re-evaluation for one
   named output or for all outputs, **Then** the daemon recomputes accordingly without any
   assignment or config value having changed (the same effect FR-007 already produces for a
   real config change, but on demand).
4. **Given** an external caller queries or names an output the daemon doesn't currently
   manage, **When** the call is made, **Then** it fails with a clear, specific error rather
   than a hang or a crash.

---

### Edge Cases

- What happens when two scheduled transitions on the same output are closer together than the
  configured crossfade duration (a pathologically dense pack)? The second transition MUST
  cleanly cancel/supersede the first's in-progress blend rather than stacking, queuing, or
  corrupting the visible result — the output always ends up in a well-defined state.
- What happens when a config/output change (User Story 4) arrives while a crossfade for that
  same output is already in progress? The in-progress blend MUST be cancelled cleanly (no
  dangling GPU resources, no visible tearing/flash) before re-evaluation begins, per User
  Story 4 Scenario 2.
- What happens when a newly connected output (User Story 6) has no pack assignment yet and the
  "same pack everywhere" toggle (User Story 5) is off? It MUST reach a well-defined idle state
  (e.g. no managed content yet, cleanly not crashing) rather than an undefined or crashing one
  — the exact default presentation is an implementation decision, not a product requirement.
- What happens if a pack assigned to an output becomes invalid or its source disappears while
  already assigned (spec 2's loader/registry surfaces this as an error or "unavailable" pack)?
  That one output MUST degrade independently (constitution Principle VIII) — e.g. hold its
  last-good frame — without affecting any other managed output or crashing the daemon.
- What happens on integrated graphics rather than a discrete GPU? The crossfade MUST remain
  smooth and low-cost (constitution Principle III, NFR-3) — this is a required test
  configuration, not just a nice-to-have.
- What happens when a crossfade is in progress and the daemon is asked to shut down (session
  logout)? It MUST exit cleanly without corrupting on-disk state or leaving GPU resources
  dangling — no product requirement exists for resuming a mid-flight crossfade after restart.
- What happens when a second config/output change for the same output arrives before the
  first one's re-evaluation has finished processing (e.g. rapidly toggling "same pack
  everywhere," or a flaky/bouncing hotplug event)? The daemon MUST coalesce them — only the
  latest state as of when re-evaluation actually runs is applied, rather than processing each
  intermediate change individually (FR-014).
- What happens when a solar-anchored pack is assigned to an output but no location has been
  provided (FR-015)? That output MUST degrade per the existing invalid/unavailable-pack
  posture (FR-013) rather than crash or silently guess a location — the same contained-failure
  treatment as any other pack the output can't currently render correctly.
- What happens when an external caller (spec 4's CLI) tries to query or trigger
  re-evaluation while no daemon is running at all? There's nothing for this spec to do about
  that case — it's the caller's responsibility to detect an unreachable daemon (spec 4 FR-011)
  — this spec only needs to expose the interface correctly whenever it *is* running.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: When a scheduled transition instant is reached for a managed output (per spec
  1's schedule query), the renderer MUST display a smooth crossfade blend between the outgoing
  and incoming images over a configurable duration, composited on the GPU rather than by
  per-frame CPU pixel blending (PRD FR-14, constitution Principle III).
- **FR-002**: The crossfade duration MUST be a single configurable value applying uniformly to
  every transition (default 45 seconds), not scaled per-transition by the length of the
  upcoming period — see Clarifications.
- **FR-003**: Outside of an active crossfade, a managed output MUST hold no active render loop
  and MUST NOT subscribe to per-frame redraw callbacks — it computes its next transition
  instant and sleeps until then (PRD FR-15, constitution Principles II, VI).
- **FR-004**: On the completion of a crossfade, the renderer MUST immediately return that
  output to the idle-wait state (FR-003) rather than continuing any redraw activity.
- **FR-005**: Each managed Wayland output MUST support an independent pack (or static image)
  assignment, with independent scheduling and crossfade state, fully isolated from every other
  managed output (PRD FR-17, constitution Principle VII).
- **FR-006**: A "same pack on all outputs" toggle MUST exist: when enabled, every managed
  output without its own explicit override follows the toggle's chosen pack; when an output
  has an explicit per-output override, that override takes precedence over the toggle for that
  output only (PRD FR-18).
- **FR-007**: A change to an output's pack assignment, to a schedule-relevant setting (e.g.
  location), or to the "same pack everywhere" toggle MUST cause the affected output(s) to
  re-evaluate their current/next transition within 2 seconds of the change being detected,
  rather than waiting for the previously-computed next-wake instant (PRD FR-16 — included
  here as necessary supporting infrastructure for FR-005/FR-006/FR-008; see Assumptions;
  timing bound resolved in Clarifications).
- **FR-008**: Output hotplug events (connect, disconnect) and runtime resolution or fractional
  scale-factor changes MUST be handled without crashing the daemon and without requiring a
  restart; unaffected outputs MUST be undisturbed by another output's hotplug/resize event
  (PRD FR-19, constitution Principle VII).
- **FR-009**: A newly connected output MUST reach a well-defined state (assigned per the
  "same pack everywhere" toggle if enabled, or an explicit not-yet-assigned state otherwise)
  within 2 seconds of the output being detected, without crashing or blocking the daemon's
  handling of other outputs (timing bound resolved in Clarifications).
- **FR-010**: A disconnected output's resources (render state, timers, in-progress crossfade)
  MUST be released without affecting any other managed output.
- **FR-011**: If two scheduled transitions on the same output occur closer together than the
  configured crossfade duration, the later transition MUST cleanly supersede any in-progress
  blend from the earlier one rather than stacking, queuing, or producing an undefined visual
  result.
- **FR-012**: If a config/output change (FR-007) arrives while a crossfade is already in
  progress on the affected output, that in-progress crossfade MUST be cancelled cleanly (no
  dangling GPU resources, no visible corruption) before re-evaluation begins.
- **FR-013**: If an assigned pack becomes invalid or unreadable after assignment (surfaced by
  spec 2's loader/registry), the affected output MUST degrade independently — continuing to
  show its last-known-good frame rather than crashing or corrupting other outputs' state
  (constitution Principle VIII).
- **FR-014**: If a second config/output change for the same output arrives while an earlier
  change's re-evaluation (FR-007) is still being processed, the daemon MUST coalesce them —
  only the latest state as of when re-evaluation actually runs is applied; intermediate
  changes are superseded rather than queued or individually processed (timing/conflict
  resolution decided in Clarifications).
- **FR-015**: When computing a schedule for a solar-anchored pack, the daemon MUST use a
  manually-provided location read from a persisted source external to this spec's own
  configuration (spec 4's `LocationConfig`, Amendment 2026-08-13) — this spec never collects
  or validates a location itself, only consumes one, the same "consumed, not re-implemented"
  posture it already applies to spec 1's solar/clock logic generally.
- **FR-016**: The daemon MUST expose a live, read/trigger-only interface (not a second way to
  change persisted state) that lets an external caller query a managed output's current image
  and next transition instant, and trigger an immediate re-evaluation of one or all outputs —
  backing User Story 7, and the specific shape spec 4's CLI depends on (Amendment 2026-08-13).

### Key Entities

- **Managed Output**: A Wayland output the daemon has taken exclusive background-rendering
  ownership of (constitution Principle I) — carries its own pack/static-image assignment,
  idle-wait/active-transition state, and crossfade progress, fully independent of every other
  managed output.
- **Output Assignment**: The binding between a managed output and the pack (or static image,
  spec 2) it displays — either an explicit per-output assignment or inherited from the "same
  pack on all outputs" toggle when no override exists for that output.
- **Crossfade Transition**: The active-transition state for one output — outgoing image,
  incoming image, start instant, configured duration, and progress fraction — built directly
  from spec 1's per-output schedule query result (`ScheduleQueryResult`) at the moment a
  transition instant is reached.
- **Idle-Wait State**: The sleeping state for one output between transitions — a single
  pending-wake instant (derived from spec 1's `next_transition_after`) and no other scheduled
  activity, per constitution Principle VI.
- **Location Source** (added, Amendment 2026-08-13): The manually-provided latitude/longitude
  this daemon reads (but never collects or validates itself) to compute solar-anchored
  schedules — spec 4's `LocationConfig` entry, spec 1's `Location` type reused by reference.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On any scheduled transition, a user observes a smooth blend with no hard cut,
  completing within the configured crossfade duration (default 45s) to within a small,
  consistent tolerance, on 100% of observed transitions across an extended run.
- **SC-002**: Between transitions, the daemon's CPU and GPU usage is indistinguishable from a
  fully idle desktop over an extended observation window containing no scheduled transitions.
- **SC-003**: On a multi-monitor setup with up to 8 outputs each carrying a different pack,
  each output's active image and transition timing independently match its own assigned
  pack's schedule across a full day-long observation, with zero instances of cross-output
  contamination.
- **SC-004**: Connecting, disconnecting, resizing, or rescaling an output takes effect and
  reaches a stable, correctly-rendered state within 2 seconds, with zero daemon crashes and
  zero disruption to other outputs' currently-displayed state, across repeated hotplug
  cycles.
- **SC-005**: A pack reassignment or relevant setting change is visibly reflected on the
  affected output(s) within 2 seconds, rather than waiting for a previously-scheduled
  transition instant that may be hours away.
- **SC-006**: The crossfade path performs smoothly (no visible stutter or dropped-frame
  artifacts) on integrated graphics hardware, not only on a discrete GPU.
- **SC-007** (added, Amendment 2026-08-13): An external caller (spec 4's CLI) can query any
  managed output's current/next-transition state and trigger a re-evaluation, with the
  response always reflecting this daemon's real internal state at the moment of the call —
  never stale or fabricated data.

## Assumptions

- **FR-16 scope decision**: PRD §8's own spec-breakdown table lists FR-14, FR-15, and
  FR-17–FR-19 under this spec but does not list FR-16 anywhere. Since FR-16 ("a config or
  output change immediately re-evaluates the current/next transition") is necessary
  supporting infrastructure for FR-17/FR-18 (assignment changes) and FR-19 (hotplug) to be
  meaningfully "handled" at all — without it, a pack swap or newly connected monitor would
  silently do nothing until an arbitrary future instant — this spec incorporates it directly
  as FR-007/FR-012 above, the same way spec 2 resolved an unassigned PRD open question (OQ-3)
  during its own authoring.
- **Crossfade duration default (resolves OQ-2)**: 45 seconds, a fixed value for all
  transitions (Clarifications). Falls within the PRD's own suggested 30–60s range; the
  period-length-scaling idea from OQ-2 is deferred as a possible future enhancement, not part
  of this spec's contract.
- **Location and live-query/re-evaluation interface (Amendment 2026-08-13, FR-015/FR-016)**:
  Neither was in this spec's original scope — both surfaced while planning spec 4, whose own
  contract (query current/next transition, force re-evaluation, solar-anchored scheduling via
  a CLI-set location) is unreachable without them. Folded in directly rather than left as a
  gap discovered during implementation, the same posture as this spec's own FR-16 decision
  above. The location's *validation* rule still belongs to spec 1 (`Location::new`) and its
  *collection* from a user still belongs to spec 4 (`wallpaperctl location set`) — this spec
  only ever reads the already-validated result. Likewise, FR-016's interface is read/trigger-
  only by design (research note: mirrors spec 4's own R5 reasoning) so it never becomes a
  second way to change persisted state alongside `cosmic-config`.
- This spec builds directly on spec 1's `ScheduleQueryResult`/`next_transition_after` contract
  (per output) and spec 2's `LoadedPack`/static-image shape — it does not redefine or
  re-validate either; an output's schedule and image data are already correct by the time this
  spec's logic runs.
- Where a pack is actually assigned to a specific output (the UI/CLI action a user takes) is
  out of scope here — that's spec 4 (CLI control surface, FR-21) and the future GUI (FR-22).
  This spec owns the *runtime behavior* once an assignment exists or changes, not the surface
  used to make that change.
- Persisting output assignments (which pack goes to which output, and the "same pack
  everywhere" toggle state) follows the same `cosmic-config`, versioned-schema approach
  established by spec 2 (constitution Principle IV/X) — the exact schema is a planning-phase
  detail, not re-litigated here.
- A newly connected output with no applicable assignment (toggle off, no prior override) has
  its exact default visual presentation (blank/black/last-known-default) left as an
  implementation decision — no product requirement mandates a specific placeholder.
- Video, animated, or GPU-shader wallpaper content is out of scope (PRD Non-Goal NG1) — this
  spec crossfades between still images only.
- **Output count bound (resolves scale ambiguity)**: correctness (independent per-output
  scheduling/crossfade state, isolation guarantees in User Story 3/FR-005) is required for up
  to 8 simultaneously managed outputs (Clarifications) — a generous bound for docking/
  multi-monitor desktop setups, not an unbounded video-wall scale.
