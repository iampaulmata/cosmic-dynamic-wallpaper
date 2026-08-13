# Feature Specification: CLI Control Surface

**Feature Branch**: `004-cli-control-surface`

**Created**: 2026-08-13

**Status**: Draft

**Input**: User description: "the next spec so we can get to an MVP — per docs/PRD.md's spec
breakdown item 4: CLI control surface, covering FR-21 (a CLI control binary exists for
scripting: list packs, assign a pack to an output, query current/next transition, force an
immediate re-evaluation). This is the spec that turns specs 1–3 (already planned but not yet
usable by an actual person) into something a user can actually drive — the only settings
surface until the Future-tagged GUI (FR-22) exists. Also folds in registering/removing a pack
in spec 2's registry (nothing in specs 1–3 exposes that to a user otherwise) and manual
location get/set/clear for solar-anchored packs (FR-9) — resolved during authoring per the
Clarifications below, since neither has an assigned home in the PRD's own 6-spec breakdown."

## Clarifications

### Session 2026-08-13

- Q: Should this CLI spec also own getting/setting the user's manual location (lat/long) for
  solar-anchored packs, or should that wait for spec 6? → A: Include it now. Without it,
  solar-anchored packs — the project's actual visual differentiator (astronomically-anchored
  periods) — have no way to get a location at all until spec 6's portal work lands. Spec 6
  remains scoped to *automatic* portal-based location (FR-10) only.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Register a Pack So It Can Be Used (Priority: P1)

A user points the CLI at a directory containing a pack manifest and images (or a single image
file) and it becomes known to the daemon — validated via spec 2's loader, and remembered
across restarts via spec 2's registry.

**Why this priority**: Nothing else in this spec has any value without this — you cannot
assign, list, or query a pack that was never registered. This is the true entry point to using
the whole system, even though the PRD's own FR-21 text doesn't name it explicitly (see
Assumptions).

**Independent Test**: Point the CLI at a valid pack directory (or a single image file) and
confirm it is subsequently reported as known, without needing any other CLI command to have
run first.

**Acceptance Scenarios**:

1. **Given** a valid pack directory with a manifest and images, **When** the user registers
   it, **Then** the daemon's known-pack registry includes it, using spec 2's own load/validate
   path — any manifest error is surfaced verbatim, not re-wrapped.
2. **Given** a single valid image file with no manifest, **When** the user registers it,
   **Then** it becomes known as a static pack (spec 2's zero-config static mode).
3. **Given** a source that is already registered, **When** the user registers it again,
   **Then** the registry is unchanged and no duplicate or error results (idempotent).
4. **Given** a directory that fails spec 2's validation (malformed manifest, missing image,
   path-traversal attempt), **When** the user attempts to register it, **Then** registration
   fails with a clear, specific error naming the problem, and nothing is added to the registry.

---

### User Story 2 - Assign a Pack to an Output (Priority: P1)

A user picks a known pack (or static image) and assigns it to a specific monitor, or turns on
"same pack everywhere" for all of them — the action that actually makes something appear on
screen.

**Why this priority**: This is the payoff action of the whole project — the moment a
registered pack actually starts being scheduled and rendered. PRD FR-21 names it directly.

**Independent Test**: With at least one pack already registered (User Story 1) and at least
one output known, assign that pack to a specific output and confirm the assignment is
reflected in spec 3's output-assignment configuration.

**Acceptance Scenarios**:

1. **Given** a registered pack and a known output, **When** the user assigns that pack to that
   output, **Then** spec 3's per-output assignment reflects the new pack (spec 3 FR-005/FR-007
   apply from here — reaction time and cancellation are spec 3's contract, not re-specified
   here).
2. **Given** two or more known outputs, **When** the user enables "same pack on all outputs"
   with a chosen pack, **Then** every output without its own explicit override follows that
   pack (spec 3 FR-006).
3. **Given** an output that already has an explicit assignment, **When** the user assigns a
   different pack to it, **Then** the new assignment replaces the old one.
4. **Given** an output name that doesn't match any output `wallpaperd` currently manages,
   **When** the user attempts an assignment while the daemon happens to be reachable, **Then**
   the CLI still writes the assignment (this command works without a daemon — Story 2's own
   Independent Test doesn't require one) but warns that the name isn't currently connected, so
   a typo is caught without blocking legitimate "assign ahead of time" configuration (e.g. for
   a docking-station monitor that isn't plugged in yet).
5. **Given** a pack identifier that is not currently registered, **When** the user attempts to
   assign it, **Then** it fails with a clear error suggesting registration first (User Story
   1), rather than silently no-op'ing — this check is against spec 2's local registry, so it
   applies whether or not a daemon is running.

---

### User Story 3 - Provide Your Location for Solar-Anchored Packs (Priority: P1)

A user who wants a solar-anchored pack (sunrise/sunset/civil twilight/etc.) enters their
latitude/longitude once via the CLI, so the daemon has what it needs to compute a schedule —
without this, only fully clock-anchored packs (FR-11) are usable at all.

**Why this priority**: Solar-anchored scheduling is the project's headline differentiator
(PRD Goal G1, "Astronomically-anchored periods"). Without a way to provide a location, that
entire capability is unreachable through this spec's CLI, even though the underlying engine
(spec 1) already supports it — this is purely a missing settings surface, resolved here per
Clarifications.

**Independent Test**: Set a valid latitude/longitude via the CLI, then read it back via the
same CLI and confirm it matches what was set — independent of any pack or output already
being configured.

**Acceptance Scenarios**:

1. **Given** no location has been set, **When** the user sets a valid latitude/longitude,
   **Then** it is persisted and a subsequent query reports the same value.
2. **Given** a location is already set, **When** the user sets a new one, **Then** it replaces
   the old value.
3. **Given** an out-of-range or malformed latitude/longitude, **When** the user attempts to
   set it, **Then** it is rejected using spec 1's own location-validity rule, with a clear
   error — the value is not partially applied.
4. **Given** a location is set, **When** the user clears it, **Then** subsequently only
   clock-anchored packs remain usable, and any output assigned a solar-anchored pack degrades
   per spec 1/3's existing failure-containment posture rather than crashing.

---

### User Story 4 - See What's Currently Showing and What's Next (Priority: P2)

A user checks, per output, which image is active right now and when the next transition is
scheduled — for confidence that the system is doing what's expected, and for debugging.

**Why this priority**: Valuable for trust and troubleshooting, and named directly in PRD
FR-21, but the daemon works correctly with or without anyone ever querying it — it doesn't
block the core "set it up and it works" path (Stories 1–3).

**Independent Test**: With an output already assigned a pack and actively being scheduled,
query that output and confirm the reported active image and next-transition time match what
spec 1/3 are actually doing (cross-checked against the pack's manifest).

**Acceptance Scenarios**:

1. **Given** an output with an active pack assignment, **When** the user queries it, **Then**
   the CLI reports the currently active image and the next scheduled transition instant.
2. **Given** an output with no assignment, **When** the user queries it, **Then** the CLI
   reports a clear "unassigned" state, not an error.
3. **Given** the daemon is not currently running, **When** the user queries any output,
   **Then** the CLI fails immediately with a clear "daemon unreachable" error rather than
   hanging.

---

### User Story 5 - Discover What You Can Assign (Priority: P2)

A user lists the packs currently known to the daemon and the outputs it currently manages,
before deciding what to assign where.

**Why this priority**: A real usability aid — named directly in PRD FR-21 ("list packs") —
but a user who already knows a pack's location and an output's name (from Story 1's
registration confirmation, or their own knowledge of their monitor setup) can complete Stories
2–3 without ever running a list command.

**Independent Test**: With at least one pack registered, list packs and confirm the listing is
accurate. Separately, with `wallpaperd` running and at least one output present, list outputs
and confirm the listing matches what the daemon actually manages.

**Note on this story's two halves**: "list packs" reads spec 2's registry — persisted state,
works with or without a daemon running, same posture as Stories 1–3 (FR-011). "list outputs"
is different: there is no persisted record of "every possible output" anywhere — it's live
Wayland/daemon state, the same as Story 4's query — so it requires a running `wallpaperd` and
fails the same way Story 4 does if none is reachable (Scenario 4 below). An earlier draft of
this spec grouped both under the daemon-optional bucket; corrected here before planning.

**Acceptance Scenarios**:

1. **Given** one or more registered packs, **When** the user lists packs, **Then** each is
   shown with enough identifying information (name, source location, availability status —
   spec 2's `Known`/`Unavailable` distinction) to choose one for assignment.
2. **Given** no packs are registered, **When** the user lists packs, **Then** the CLI reports
   an empty result clearly, not an error.
3. **Given** `wallpaperd` is running and one or more outputs it currently manages, **When**
   the user lists outputs, **Then** each is shown by the same identifier spec 3's assignment
   configuration uses, so it can be used directly in an assignment command (Story 2).
4. **Given** `wallpaperd` is not running, **When** the user lists outputs, **Then** the CLI
   fails immediately with the same "daemon unreachable" error Story 4 uses, rather than
   returning an empty or stale list.

---

### User Story 6 - Force an Immediate Re-Evaluation (Priority: P3)

A user asks the daemon to recompute an output's (or every output's) current schedule state
right now, without changing any assignment or setting — for recovery after something the
daemon wouldn't otherwise notice, like a pack's image files being edited on disk or the
system clock being corrected.

**Why this priority**: A real recovery/debugging convenience named in PRD FR-21, but the
daemon already self-corrects on its own schedule (spec 1) and reacts to actual assignment/
config changes within 2 seconds (spec 3 FR-007) — this command only matters for the narrower
case of an external change the daemon has no other way to learn about.

**Independent Test**: With an output already running on a schedule, force a re-evaluation and
confirm the daemon recomputes its current/next state without any assignment or config value
having changed.

**Acceptance Scenarios**:

1. **Given** an output with an active assignment, **When** the user forces re-evaluation,
   **Then** the daemon recomputes that output's current/next transition immediately.
2. **Given** no specific output is named, **When** the user forces re-evaluation, **Then**
   every managed output recomputes.
3. **Given** the daemon is not currently running, **When** the user attempts to force
   re-evaluation, **Then** the CLI fails immediately with a clear "daemon unreachable" error.

---

### User Story 7 - Remove a Known Pack (Priority: P3)

A user who no longer wants a pack remembered removes it from the registry outright — distinct
from it merely becoming unavailable because its source disappeared.

**Why this priority**: A completeness/cleanup action mirroring spec 2's own registry
capability (FR-012 there), but a user can fully use the system (Stories 1–4) indefinitely
without ever needing to remove anything.

**Independent Test**: Register a pack, remove it, and confirm it no longer appears in the pack
listing (Story 5) nor can be newly assigned (Story 2), independent of any output's existing
assignment.

**Acceptance Scenarios**:

1. **Given** a registered pack not currently assigned to any output, **When** the user removes
   it, **Then** it no longer appears in the pack listing and cannot be assigned going forward.
2. **Given** a registered pack currently assigned to one or more outputs, **When** the user
   removes it, **Then** removal still succeeds, and each affected output falls back to spec
   2/3's existing "unavailable" handling — this command does not invent new fallback behavior.

---

### Edge Cases

- What happens when a command that needs a running daemon (Story 4's query, Story 6's force
  re-evaluation) is run while no daemon is active? It MUST fail immediately with a clear,
  specific error rather than hanging or timing out silently (User Story 4/6 Scenario 3).
- What happens when registering, listing, assigning, or setting location while no daemon is
  running? These act directly on persisted config/registry state (constitution Principle IV)
  and MUST succeed independent of whether a daemon happens to be running at that moment — the
  daemon picks up the change per spec 3's existing watch/reaction contract once it starts or
  reconnects.
- What happens when two CLI invocations race (e.g. two assignments to the same output run
  concurrently)? The daemon's existing change-coalescing behavior (spec 3 FR-014) already
  resolves this to a well-defined latest-state outcome — this spec does not add separate
  locking on top of it.
- What happens when a user removes the pack that "same pack everywhere" currently points at?
  Every output following the toggle falls back to spec 2/3's existing unavailable-pack
  handling (same as User Story 7 Scenario 2), not a CLI-specific behavior.
- What happens when setting a location while solar-anchored packs are already actively
  scheduled on an output? The change is picked up the same way any other schedule-relevant
  setting change is (spec 3 FR-007's 2-second reaction bound) — not a new timing contract.
- What happens when a user assigns a pack to an output name that isn't connected yet (e.g.
  pre-configuring a docking-station monitor before plugging it in)? This MUST succeed — it's
  a legitimate, deliberately-supported case (FR-007), not an error; spec 3 resolves the
  assignment automatically once a matching output actually connects (spec 3 FR-009).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The CLI MUST allow a user to register a pack — a directory containing a
  manifest and images, or a single static image file — as known to the daemon, using spec 2's
  own load/validation path and surfacing its errors verbatim on failure.
- **FR-002**: Registering a source that is already registered MUST be idempotent — it MUST
  NOT create a duplicate entry or produce an error, consistent with spec 2's identity-by-
  source-location rule.
- **FR-003**: The CLI MUST allow a user to list all currently known packs, showing enough
  identifying information (name, source location, availability status) to choose one for
  assignment.
- **FR-004**: The CLI MUST allow a user to remove a known pack from the registry outright,
  distinct from it becoming automatically unavailable — an output assigned to a removed pack
  falls back to the existing unavailable-pack handling (specs 2–3) rather than new behavior
  this spec invents.
- **FR-005**: The CLI MUST allow a user to list the outputs `wallpaperd` currently manages, by
  the same identifier spec 3's assignment configuration uses. Unlike FR-001–FR-004, this
  requires a running daemon (corrected below — see FR-011 and User Story 5's Note).
- **FR-006**: The CLI MUST allow a user to assign a known pack (or static image) to a specific
  output, and to enable/disable "same pack on all outputs," writing to spec 3's
  output-assignment configuration.
- **FR-007**: Assigning an unregistered pack MUST fail with a clear, specific error naming
  what wasn't found, checked against spec 2's local registry — this check MUST NOT require a
  running daemon. Assigning to an output name MUST be accepted even if that name doesn't match
  any currently-connected output (a legitimate "configure ahead of time" case, spec 3 resolves
  it when/if a matching output appears); if the daemon happens to be reachable at assignment
  time, the CLI SHOULD warn (not fail) when the name doesn't match a currently-managed output,
  as a typo-catching convenience only.
- **FR-008**: The CLI MUST allow a user to set, view, and clear a manual location (latitude/
  longitude) for solar-anchored pack scheduling, reusing spec 1's location-validity rule
  rather than re-implementing it (resolves this spec's scope per Clarifications).
- **FR-009**: The CLI MUST allow a user to query, per output, which image is currently active
  and when the next scheduled transition will occur.
- **FR-010**: The CLI MUST allow a user to force an immediate re-evaluation of one named
  output or all managed outputs, without changing any assignment or setting.
- **FR-011**: Any command that requires a running daemon (query, force re-evaluation, **and
  list outputs** — corrected from an earlier draft that grouped it with the config-only
  commands; see User Story 5's Note) MUST fail immediately with a clear "daemon unreachable"
  error if none is running, rather than hanging. Commands that only read or write persisted
  config/registry state (register, list *packs*, remove, assign, location) MUST work whether
  or not a daemon is currently running.
- **FR-012**: Every CLI command MUST exit with a non-zero status and a specific, actionable
  error message on failure, so scripted callers can detect failure reliably.
- **FR-013**: Every command that returns data (list packs, list outputs, query current/next)
  MUST support a machine-readable output mode in addition to a human-readable default, so a
  scripted caller does not have to parse free-form text (PRD FR-21's "for scripting" intent).

### Key Entities

- **Pack Registration Request**: The source (a directory or a single image file path) a user
  points the CLI at to register — becomes spec 2's `PackSource` once validated.
- **Output Assignment Request**: An (output identifier, pack source) pair, or a "same pack
  everywhere" toggle value with its chosen pack — written into spec 3's output-assignment
  configuration.
- **Location Setting**: The manual latitude/longitude pair a user provides — spec 1's
  `Location`, persisted so solar-anchored packs can be scheduled.
- **Schedule Query Result (CLI-facing)**: Per output, the currently active image and next
  transition instant — a read-only projection of spec 1/3's live internal state, not a new
  computation this spec owns.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A user with no prior configuration can go from "daemon installed" to "a
  solar-anchored pack visibly scheduled on an output" using only CLI commands, with zero
  manual config-file editing.
- **SC-002**: 100% of CLI commands that fail (invalid input, unknown pack/output, unreachable
  daemon) exit with a non-zero status and a specific, actionable error message — never a
  silent no-op, a hang, or an unhandled crash.
- **SC-003**: Every command that returns data can be retrieved in a machine-readable form by a
  scripted caller with no free-text parsing required.
- **SC-004**: Registering an already-known pack, or re-running any read-only command
  (list, query), produces no unintended change in state on repeated invocation.
- **SC-005**: A user can set a location and immediately confirm it via a query, with the
  confirmed value matching exactly what was set, on the first attempt.

## Assumptions

- **Pack registration scope decision**: PRD FR-21's own text names "list packs, assign a pack
  to an output, query current/next transition, force an immediate re-evaluation" but not
  registration — yet nothing in specs 1–3 exposes spec 2's `Registry::register`/`remove` to an
  actual user, and the GUI that might otherwise own this is Future-tagged (FR-22). This spec
  folds registration/removal into its own scope (FR-001–FR-004) the same way spec 3 folded in
  FR-16 — a necessary supporting capability, not a silent scope expansion.
- **Location scope decision (resolves Clarifications)**: Manual location get/set/clear is
  included in this spec rather than deferred to spec 6, per the resolved clarification —
  spec 6 remains scoped to automatic portal-based location (FR-10) only.
- **Daemon-requirement correction (found during task planning)**: An earlier draft of this
  spec grouped "list outputs" with the config-only commands (register, list packs, remove,
  assign, location) under FR-011. That was wrong — there's no persisted record of "every
  possible output" anywhere in specs 1–3, so listing outputs is necessarily live daemon state,
  the same as Story 4's query. Corrected in FR-005/FR-011 and User Story 5 above before task
  generation, rather than generating a task list built on the contradiction. Separately, FR-007
  was tightened to make clear that assigning to a not-currently-connected output name is a
  valid, deliberately-supported "configure ahead of time" case, not a failure — the daemon
  resolves it later (spec 3 FR-009), so `assign` doesn't need live output validation to work
  correctly, preserving its daemon-optional status.
- **No IPC transport is fixed here**: FR-009's query and FR-010's force-re-evaluation need
  some live channel to a running daemon; FR-001–FR-008 (register, list, remove, assign,
  location) do not, since they only read/write persisted config/registry state (constitution
  Principle IV). The exact transport for the live-query path is a planning-phase decision
  (like spec 3 leaving `wgpu` vs. raw GL to its own plan.md), not fixed by this spec.
- **Crossfade duration remains out of scope**: spec 3 already establishes a sane fixed default
  (45s); this spec does not add a command to change it. Revisit only if real usage shows the
  default is a problem.
- **No GUI in this spec**: this is the CLI-only control surface the constitution explicitly
  allows as an interim substitute (Principle IX); any future GUI (FR-22) is a separate spec.
- This spec assumes specs 1–3 already exist as implemented, available dependencies (the
  scheduling engine, pack loader/registry, and renderer's output-assignment configuration) —
  it consumes their contracts rather than redefining them.
- Multi-user access control is out of scope — this CLI operates within a single user's own
  desktop session, the same trust boundary every other daemon in this project already assumes.
