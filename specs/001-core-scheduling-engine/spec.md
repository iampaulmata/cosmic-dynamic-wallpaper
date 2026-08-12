# Feature Specification: Core Scheduling Engine

**Feature Branch**: `001-core-scheduling-engine`

**Created**: 2026-08-11

**Status**: Draft

**Input**: User description: "Core scheduling engine — from docs/PRD.md, spec breakdown item 1: the pure solar/time logic covering FR-6 (time anchors: solar event names with optional signed offsets, or absolute clock times), FR-8 (compute solar event times via a vetted astronomical crate, never hand-rolled), FR-9 (manual lat/long location input, works with zero external services), FR-11 (fully manual location-free clock-time schedule), and FR-13 (deterministically answer "which image is active right now, and what fraction through the current crossfade" as a pure, testable function). No Wayland/rendering/GPU code in this spec — buildable and fully unit-tested standalone, since specs 2 and 3 in the breakdown depend on this one existing first."

## Clarifications

### Session 2026-08-11

- Q: When two images in the same pack resolve to the exact same instant, which one does the engine treat as active? → A: Reject the pack at validation time if two anchors resolve to the identical instant — same validation-error path as FR-006.
- Q: What should the engine do when the manually-entered latitude or longitude is out of range? → A: Reject at validation time with a clear error (same boundary as FR-006/FR-006a) — the engine never attempts a solar calculation with an invalid coordinate.
- Q: Is there an upper bound on images/anchors per pack that the performance target should be scoped against? → A: Cap at 64 anchors per pack, target sub-millisecond query time.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Solar-Anchored Schedule Resolves Correctly for a Location (Priority: P1)

A user has a wallpaper pack whose images are each tied to a solar event (sunrise, solar
noon, sunset, civil/astronomical dawn or dusk), optionally offset by a signed duration, and
has entered their location as manual latitude/longitude. At any moment, the system can say
exactly which image should be showing, so the wallpaper always tracks the true solar day
for that location rather than a generic clock schedule.

**Why this priority**: This is the feature's core differentiator (per the PRD, astronomically
anchored periods vs. plain fixed-interval rotation). Nothing else in the project is useful if
this calculation is wrong or imprecise.

**Independent Test**: Build a pack with images anchored to a mix of solar events and offsets,
supply a fixed latitude/longitude and date, and query the engine at many timestamps across a
full day (including exact anchor instants). Verify the returned active image and crossfade
progress against independently-computed reference solar times for that location/date, with no
Wayland, rendering, or GPU dependency involved in the test.

**Acceptance Scenarios**:

1. **Given** a pack with images anchored to `sunrise`, `solar_noon`, and `sunset` for a known
   location/date, **When** queried at a timestamp strictly between two anchors and outside any
   crossfade window, **Then** the engine returns the image for the most recently passed anchor
   with 0% transition progress.
2. **Given** an image anchored to `civil_dawn-30m`, **When** queried at exactly that offset
   instant, **Then** the engine reports the transition into that image has begun.
3. **Given** a timestamp inside the crossfade window between two anchors, **When** queried,
   **Then** the engine returns both the outgoing and incoming image identifiers plus a
   fractional progress value strictly between 0.0 and 1.0.

---

### User Story 2 - Fully Manual, Location-Free Clock Schedule (Priority: P1)

A privacy-conscious user does not want to share their location by any means — not even a
manually-typed coordinate. They instead assign each image an absolute clock time (`HH:MM`).
The system schedules transitions purely from wall-clock time, with no location data used,
requested, or required anywhere in that path.

**Why this priority**: FR-11 explicitly calls this out as a baseline path that "must work
with zero external services" and zero shared location; it's equally load-bearing as the
solar path, not a lesser fallback.

**Independent Test**: Build a pack using only clock-time anchors and no location input at
all, then query the engine across a day and confirm results match the expected schedule
without any solar computation being invoked or any location value being read.

**Acceptance Scenarios**:

1. **Given** a pack with clock-time anchors `06:00` (A), `12:00` (B), `20:00` (C), **When**
   queried at `15:00`, **Then** image B is reported active.
2. **Given** a pack using only clock-time anchors, **When** the engine is queried, **Then** it
   returns a valid result without requiring, requesting, or reading any location value.
3. **Given** a pack manifest that mixes solar-event anchors and clock-time anchors, **When**
   the pack is validated, **Then** the engine rejects it with a clear validation error rather
   than resolving a partial or ambiguous schedule.

---

### User Story 3 - Deterministic State Query for Downstream Consumers (Priority: P2)

A downstream component (the renderer, or the CLI control surface, both specced separately)
needs to ask two questions at any time: "what's active right now, including any in-progress
transition?" and "when is the next scheduled change?" so it can render the correct frame
immediately and sleep until the next real event instead of polling.

**Why this priority**: This is the seam the rest of the project builds on (renderer spec and
CLI spec both depend on this contract existing and being stable), but it has no standalone
user-facing value without Story 1 or 2 providing a real schedule to query.

**Independent Test**: For a variety of valid packs (solar and clock-anchored) and query
instants, call the engine directly (no daemon, no Wayland) and confirm the same input always
produces the same output, and that the reported "next transition instant" is a real future
timestamp consistent with the pack's anchors.

**Acceptance Scenarios**:

1. **Given** any valid pack and a fixed query instant, **When** the engine is queried twice
   with identical inputs, **Then** both calls return identical results.
2. **Given** any valid pack, **When** queried for the next transition instant, **Then** the
   engine returns a timestamp strictly after the query instant that matches the pack's next
   anchor (adjusted for any configured crossfade lead-in).

---

### Edge Cases

- What happens when a pack has only a single time anchor (or is in static mode with no
  anchors at all, per FR-3)? The one image MUST be reported active at every query, with no
  transition ever reported and no meaningful "next transition instant."
- What happens when the query instant falls before the day's first anchor (e.g., 2 AM with
  the first anchor at 6 AM)? The engine MUST wrap around and resolve against the previous
  day's last anchor rather than treating the period as undefined.
- What happens at a requested latitude/date where a solar event does not occur at all (polar
  day/night)? The engine MUST skip that day's missing anchor and hold the adjacent image
  active until the next anchor that does occur (see FR-007).
- What happens when two consecutive anchors are closer together than the configured crossfade
  duration (overlapping transition windows)? The engine MUST still produce a well-defined,
  monotonic progress value rather than a discontinuity or an out-of-range fraction.
- What happens when a manually-entered latitude or longitude is out of range or non-numeric
  (e.g. latitude 200)? The engine MUST reject it at validation time with a clear error rather
  than attempting a solar calculation or crashing (see FR-002a).
- What happens when two anchors in the same pack resolve to the exact same instant (e.g. two
  images both anchored to `sunrise` with no offset)? The engine MUST reject the pack at
  validation time rather than silently picking a winner (see FR-006a).
- What happens across a daylight-saving-time shift for clock-time-anchored packs? The engine
  MUST use the same wall-clock/local-time interpretation a user would expect from their
  system clock (i.e., `02:30` means whatever the OS considers `02:30` local time that day),
  and must not crash or produce an undefined result on the transition day.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The engine MUST accept a wallpaper pack as an ordered set of images, each
  associated with exactly one time anchor, where every anchor in a given pack is either a
  solar-event anchor (name plus optional signed offset) or an absolute clock-time anchor,
  never a mix of both within the same pack, and containing at most 64 anchors — the
  performance target in SC-001 is scoped to this bound, and the engine MUST reject a pack
  exceeding it at validation time with a clear error.
- **FR-002**: For solar-anchored packs, the engine MUST compute solar event times (sunrise,
  sunset, solar noon, solar midnight, civil dawn/dusk, astronomical dawn/dusk) for a given
  date and a manually-supplied latitude/longitude, using a vetted astronomical algorithm, with
  no network access required.
- **FR-002a**: The engine MUST validate a manually-entered latitude/longitude at
  configuration-validation time (latitude in [-90, 90], longitude in [-180, 180], both
  numeric) and MUST reject an out-of-range or non-numeric value with a clear validation
  error rather than attempting a solar calculation with it — per the project constitution's
  "failures are contained, never fatal" principle, this MUST NOT panic or crash the caller.
- **FR-003**: For clock-anchored packs, the engine MUST resolve the active image using only
  wall-clock time; no location input may be required, requested, or read anywhere on that
  code path.
- **FR-004**: Given any valid pack and a query instant, the engine MUST deterministically
  report which image is active and, when that instant falls inside a crossfade window, the
  outgoing image, the incoming image, and a fractional progress value in the range 0.0–1.0.
- **FR-005**: Given any valid pack and a current instant, the engine MUST report the next
  transition instant, so a caller can schedule a single wake-up rather than polling.
- **FR-006**: The engine MUST reject, at pack-validation time, any pack that mixes solar and
  clock-time anchors, returning a clear validation error rather than a silently partial or
  ambiguous schedule.
- **FR-006a**: The engine MUST reject, at pack-validation time, any pack where two or more
  anchors resolve to the exact same instant (for the same reference date), rather than
  silently picking a winner — an exact tie is a pack-authoring error, not a runtime decision.
- **FR-007**: When a solar event does not occur for the requested date/location (polar
  day/night), the engine MUST skip that day's missing anchor and hold the adjacent image
  active straight through to the next anchor that does occur, rather than inventing a
  substitute time or surfacing an error for that date.
- **FR-008**: All solar-event and schedule-resolution calculations MUST be implemented as
  pure functions of (date, latitude, longitude, pack definition, query instant) — no I/O,
  network access, rendering dependency, or hidden global state — and MUST be independently
  unit-testable without a Wayland session or GPU.
- **FR-009**: The engine MUST wrap correctly around midnight: a query instant earlier than the
  day's first anchor MUST resolve against the previous day's last anchor rather than being
  treated as undefined.
- **FR-010**: The crossfade window duration MUST be accepted by the engine as an external
  parameter per query/pack (not hardcoded), since owning that value is the responsibility of
  a later spec (the renderer); this engine only performs the progress-fraction math once a
  duration is supplied.

### Key Entities

- **Wallpaper Pack**: An ordered set of (image, time anchor) pairs plus pack-level metadata;
  constrained to a single anchor-type (solar or clock) per pack.
- **Time Anchor**: Either a named solar event with an optional signed offset, or an absolute
  clock time (`HH:MM`).
- **Location**: A manually-entered latitude/longitude pair (FR-9's baseline path). Automatic,
  portal-based location resolution (FR-10) is a separate spec and out of scope here.
- **Schedule Query Result**: The answer to "what's active now" — an active image identifier,
  an optional in-progress-transition state (outgoing id, incoming id, progress fraction), and
  the next transition instant.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For any valid pack of up to 64 anchors and any query instant, the engine
  returns a result in under 1 millisecond, since this is pure in-memory computation with
  no I/O.
- **SC-002**: Computed solar event times match an independent reference source (e.g. a
  published solar calculator) to within one minute, for any tested location and date,
  including high-latitude locations short of full polar day/night.
- **SC-003**: Identical (pack, query instant) inputs produce identical results across
  repeated calls, 100% of the time — no dependency on call order, caching, or hidden state.
- **SC-004**: A fully manual, clock-time-only schedule can be defined and correctly evaluated
  end-to-end with zero location fields populated, zero location-related prompts, and zero
  location-related errors.
- **SC-005**: The pure scheduling/solar logic achieves at least 90% unit test line coverage,
  reflecting its status as the project's highest-test-priority code per the project
  constitution.

## Assumptions

- The host system's clock and timezone are assumed correct; correcting clock skew or handling
  leap seconds is out of scope for this engine.
- "Location" in this spec means only the manually-entered latitude/longitude path (FR-9).
  Automatic location via the `org.freedesktop.portal.Location` portal (FR-10) is a separate
  spec per the PRD's suggested breakdown and is not implemented here.
- Rendering, GPU compositing, and Wayland output handling are fully out of scope; this engine
  produces only the data (active/outgoing/incoming image identifiers and a progress fraction)
  that a renderer spec would consume.
- Crossfade duration is treated as a parameter supplied to this engine, not a value this spec
  decides on — that decision (and whether it varies per-period) belongs to the renderer spec
  per PRD Open Question OQ-2.
- A pack always contains at least one time anchor; the zero-anchor "static mode" case (FR-3)
  is treated as a degenerate schedule with exactly one always-active image and no transitions.
