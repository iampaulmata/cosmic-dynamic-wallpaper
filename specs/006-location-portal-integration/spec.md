# Feature Specification: Location Portal Integration

**Feature Branch**: `006-location-portal-integration`

**Created**: 2026-08-14

**Status**: Draft

**Input**: User description: "build out the spec for spec 6 as defined in the PRD — per
docs/PRD.md's spec breakdown item 6: Location portal integration, covering FR-10 (location can
optionally be provided automatically via the `org.freedesktop.portal.Location` D-Bus portal,
backed by GeoClue2, degrading gracefully to FR-9's manual entry if the portal or backend isn't
present). Kept separate from spec 1 specifically because of PRD Open Question OQ-1 — it was
unconfirmed at PRD-authoring time whether `xdg-desktop-portal-cosmic` implements the Location
portal backend at all."

## Clarifications

### Session 2026-08-14

- Q: PRD Open Question OQ-1 flags that it's unconfirmed whether `xdg-desktop-portal-cosmic`
  actually implements the Location portal backend — does this spec need that confirmed before
  it can be written? → A: No. This spec targets the standard `org.freedesktop.portal.Location`
  interface regardless of which backend (if any) answers it, and treats "no backend present" as
  just one more case of the same graceful-degradation path already required for "permission
  declined" (see FR-004). The actual spike — checking what a live COSMIC session's portal
  returns — is deferred to `/speckit-plan`/implementation, the same way spec 3 deferred its
  `wgpu`-vs-GL choice to its own plan rather than blocking spec authoring on it.
- Q: Should automatic location be the default the first time a user has no location configured
  at all, or must it be explicitly opted into? → A: Explicit opt-in, default OFF. This matches
  the PRD's own framing of FR-9 (manual entry) as "the baseline path" and FR-10 (automatic) as
  optional, and is the conventional privacy default for any feature that silently starts talking
  to a location service — a user should choose automatic location, not discover it was already
  on.
- Q: Should the automatically-resolved coordinates themselves be written to disk, or only the
  chosen mode (automatic/manual) and any manually-entered value? → A: Persist the resolved
  coordinates in `cosmic-config` alongside the mode, so a value is immediately available on
  restart before the portal answers again (see FR-010).
- Q: What should count as a "materially different" location while automatic mode is active, so
  the daemon knows when to re-evaluate a schedule versus ignore noise? → A: No new distance
  threshold — re-evaluate on every distinct value the portal reports, relying entirely on spec 3
  FR-014's existing change-coalescing to bound churn from rapid-fire updates (see FR-006).
- Q: If automatic location has already resolved successfully and then a location query
  transiently fails mid-session, should the daemon keep scheduling against the last
  successfully-resolved location, or fall back immediately to manual/no-location? → A: Fall
  back immediately, no grace period or staleness tracking — consistent with constitution
  Principle VIII's posture of degrading to an already-established fallback path (see FR-005).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Get a Correct Schedule Without Ever Typing Coordinates (Priority: P1)

A user who wants solar-anchored wallpaper packs (sunrise, sunset, civil twilight, etc.) enables
automatic location instead of hand-entering latitude/longitude (spec 4's existing manual path).
The daemon obtains a location from the system's location portal and uses it to compute the same
solar schedule spec 1 already produces for a manual location.

**Why this priority**: This is the entire point of the spec and the last piece of PRD Goal G1
("real solar events for the user's location") that doesn't yet have a zero-typing path — FR-10
directly.

**Independent Test**: On a system where the location portal is available and the user grants
permission, enable automatic location (with no manual location ever entered) and confirm a
solar-anchored pack's active image and next-transition time match what spec 1 would compute for
the resolved coordinates.

**Acceptance Scenarios**:

1. **Given** no location is configured at all, **When** the user enables automatic location and
   grants the portal's permission prompt, **Then** the daemon resolves a location and
   solar-anchored packs schedule correctly against it, with no coordinates ever typed.
2. **Given** automatic location is enabled and resolved, **When** the user queries the current
   location (spec 4's query surface, extended), **Then** the reported coordinates match what the
   portal supplied, not a stale or default value.
3. **Given** automatic location is enabled and was previously resolved, **When** the daemon
   restarts, **Then** it immediately schedules against the last persisted resolved location
   (no gap where a solar-anchored pack has no location at all) while it re-resolves a fresh
   value from the portal in the background, without the user re-enabling anything.

---

### User Story 2 - Nothing Breaks When the Portal Isn't There or Says No (Priority: P1)

A user enables automatic location on a system where the portal service doesn't exist, has no
backend implementing the Location interface, or where the user declines the permission prompt.
The daemon behaves exactly as if automatic location were never touched: it falls back to any
manual location already on file, or to the existing "no location" degraded state for
solar-anchored packs — never a crash, hang, or stuck retry loop.

**Why this priority**: PRD Open Question OQ-1 leaves it genuinely unconfirmed whether
`xdg-desktop-portal-cosmic` backs this interface today. If this path isn't solid, the feature is
unshippable on an unknown fraction of real COSMIC systems — this is as load-bearing as User
Story 1, not a secondary concern.

**Independent Test**: On a system with no portal service running (or with the permission prompt
declined), enable automatic location and confirm: (a) no crash or hang, (b) a clear status is
reported distinguishing "automatic requested but unavailable" from "working," and (c) any
existing manual location or clock-anchored packs continue working exactly as before.

**Acceptance Scenarios**:

1. **Given** no portal service is reachable at all, **When** the user enables automatic
   location, **Then** the daemon falls back to the last manual location on file (if any) and
   reports automatic mode as unavailable, without crashing.
2. **Given** the portal is reachable but the user declines its permission prompt, **When**
   automatic location is enabled, **Then** the same fallback in Scenario 1 applies.
3. **Given** no manual location was ever set either, **When** automatic location is enabled and
   unavailable, **Then** solar-anchored packs degrade exactly the way they already do today with
   no location configured (spec 1's existing contract) — this spec does not invent a new failure
   mode — while clock-anchored packs (FR-11) continue unaffected.
4. **Given** the portal briefly errors or the backing service crashes mid-session, **When** the
   daemon next needs a location, **Then** it treats this the same as Scenario 1 (degrade, don't
   crash) and retries with a sane backoff rather than looping tightly.

---

### User Story 3 - Schedule Follows You When Your Location Actually Changes (Priority: P2)

With automatic location active, the portal reports an updated location (e.g., a laptop changes
networks/location, or the backend simply re-resolves more precisely after startup). The daemon
picks up the new location and re-evaluates affected schedules without a restart.

**Why this priority**: Real value for mobile hardware, and a natural extension of automatic mode
— but a user who never leaves one location gets full value from Stories 1–2 alone, so this is
not required for a usable MVP of this spec.

**Independent Test**: With automatic location active and a solar-anchored pack already
scheduled, deliver an updated location from the portal and confirm the schedule recomputes
against the new coordinates within the daemon's existing live-reconfiguration reaction bound,
with no restart.

**Acceptance Scenarios**:

1. **Given** automatic location is active and a schedule is running, **When** the portal reports
   a distinct location value, **Then** affected solar-anchored outputs re-evaluate within
   the same reaction bound already established for other live config changes (spec 3 FR-007's
   2 seconds).
2. **Given** several location updates arrive in rapid succession, **When** the daemon processes
   them, **Then** only the latest is acted on — reusing the existing change-coalescing behavior
   (spec 3 FR-014) rather than re-evaluating once per update.

---

### User Story 4 - See and Control Which Mode Is Active (Priority: P2)

A user checks whether automatic or manual location is currently in effect and what coordinates
are being used, and can switch back to manual (or off) at any time without losing a previously
entered manual value.

**Why this priority**: Transparency and reversibility matter for a feature that talks to an
external location service, but the daemon already works correctly (Stories 1–2) without a user
ever checking or toggling this after initial setup.

**Independent Test**: Enable automatic location, then query which mode is active; disable it and
confirm the daemon reverts to the previously-stored manual location (if one existed) without
requiring it to be re-entered.

**Acceptance Scenarios**:

1. **Given** either mode is active, **When** the user queries location status, **Then** the
   response clearly states which mode (automatic or manual) is in effect and the coordinates
   currently used for scheduling.
2. **Given** automatic location is active and a manual location was set before it, **When** the
   user disables automatic mode, **Then** the daemon reverts to that previously-stored manual
   value immediately, with no re-entry required.
3. **Given** automatic location is active and no manual location was ever set, **When** the user
   disables automatic mode, **Then** the daemon returns to the existing "no location" state
   (same as never having configured location at all).

---

### Edge Cases

- What happens if the portal takes a long time to resolve a first location (cold GeoClue
  lookup)? The daemon MUST keep behaving on whatever it already has (prior manual value, or the
  existing no-location degrade path) in the meantime, rather than blocking startup or scheduling
  on the outcome.
- What happens if permission is granted, then later revoked through the portal's own permission
  settings while the daemon is running? The next location query MUST fail cleanly and degrade
  immediately (User Story 2's path, FR-005) rather than continuing to schedule against the
  last-known-good automatic reading — no crash, and no silent use of a now-stale value.
- What happens on a non-Linux or non-D-Bus environment? Out of scope — this project is
  COSMIC-only (PRD NG3), and the portal is a Linux desktop mechanism by definition.
- What happens if a user enables automatic location while no daemon is running? The mode
  preference persists (same daemon-optional posture as spec 4's manual location, constitution
  Principle IV) and takes effect once a daemon starts or reconnects — it does not require the
  daemon to be live at the moment the setting is changed.
- What happens if the resolved automatic location is identical to the already-stored manual
  value? No unnecessary re-evaluation churn beyond the existing coalescing rule (spec 3 FR-014).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST allow a user to enable an "automatic" location mode, as an
  alternative to spec 4's existing manual latitude/longitude entry, through the same location
  control surface (extended, not replaced).
- **FR-002**: Automatic location mode MUST default to OFF; it is never enabled implicitly on a
  user's behalf (resolves this spec's own Clarifications).
- **FR-003**: When automatic mode is enabled, the daemon MUST obtain a location via the
  `org.freedesktop.portal.Location` D-Bus portal, honoring whatever permission/consent flow that
  portal itself presents — this project MUST NOT implement its own separate consent UI for this.
- **FR-004**: The system MUST request only the coarsest location accuracy sufficient for
  solar-event calculation, not GPS-exact precision, as a data-minimization default.
- **FR-005**: If the portal service, a supporting backend, or the permission grant is
  unavailable for any reason (not installed, no backend implements the interface, user declines,
  mid-session failure — including a location query that fails after automatic mode was
  previously resolved successfully) the system MUST degrade gracefully and immediately, with no
  grace period or staleness tracking of the last-known-good value: fall back to the existing
  manual location if one is stored (spec 4 FR-008), or to the existing no-location behavior for
  solar-anchored packs (spec 1's contract) if none is stored (resolves this spec's
  Clarifications). This MUST NOT crash, hang, or block clock-anchored packs (FR-11), and MUST
  NOT retry in a tight loop.
- **FR-006**: While automatic mode is active, the system MUST accept live location updates from
  the portal and re-evaluate affected schedules using the daemon's existing live-reconfiguration
  path and reaction-time bound (spec 3 FR-007). No separate distance/significance threshold is
  introduced: every distinct value the portal reports is acted on, with rapid successive updates
  coalesced the same way other rapid config changes already are (spec 3 FR-014) — this is the
  sole mechanism bounding re-evaluation churn (resolves this spec's Clarifications).
- **FR-007**: Switching from automatic to manual location MUST NOT discard a previously-stored
  manual value — disabling automatic mode reverts to that value immediately with no re-entry.
- **FR-008**: A user MUST be able to query, at any time, which location mode (automatic or
  manual) is currently active and the coordinates currently being used for scheduling.
- **FR-009**: A user MUST be able to explicitly disable automatic mode and revert to manual (or
  no) location at any time.
- **FR-010**: All location-mode state (automatic vs. manual, and the last resolved/entered value
  of each — including automatic mode's resolved coordinates, per this spec's Clarifications)
  MUST be persisted via `cosmic-config`, extending spec 3/4's existing location schema rather
  than introducing a separate one, preserving its versioned-migration story (constitution
  Principle X). Persisting the resolved automatic value lets a restarted daemon schedule
  immediately against the last-known location rather than waiting for a fresh portal
  resolution before any solar-anchored pack can render.
- **FR-011**: Location-mode changes (enabling/disabling automatic mode, or the portal delivering
  an updated location) MUST be picked up by an already-running daemon without requiring a
  restart, consistent with spec 3's existing config-watch behavior.
- **FR-012**: The mode preference (automatic vs. manual, and manual's stored value) MUST persist
  and be settable whether or not a daemon is currently running, the same daemon-optional posture
  spec 4 established for manual location.

### Key Entities

- **Location Mode**: Which source currently supplies the location used for solar scheduling —
  automatic (portal-backed) or manual (spec 4's user-entered value) — plus an implicit "none"
  state when neither has ever produced a value.
- **Automatic Location Reading**: The most recently resolved (latitude, longitude) pair obtained
  from the portal while automatic mode is active, feeding the same downstream `Location` value
  spec 1's scheduling engine already consumes — this spec does not introduce a new location
  representation. Persisted via `cosmic-config` (per this spec's Clarifications and FR-010) so
  it survives a daemon restart while a fresh value is re-resolved in the background.
- **Location Availability Status**: A read-only, queryable state distinguishing "automatic mode
  active and resolved," "automatic mode requested but unavailable/degraded," and "manual mode,"
  so a user can tell why a given location is in effect.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On a system where the location portal is available and permission is granted, a
  user goes from "enable automatic location" to a correctly solar-scheduled pack without ever
  typing coordinates.
- **SC-002**: On a system where the portal is unavailable or permission is declined, enabling
  automatic location produces identical behavior to never having enabled it (existing
  manual/no-location handling) — zero crashes or hangs across repeated attempts.
- **SC-003**: A location change delivered while automatic mode is active is reflected in the
  active schedule within the same reaction-time bound already guaranteed for other live config
  changes.
- **SC-004**: A user can determine which location mode is active and what coordinates are
  currently used with a single query, at any time.
- **SC-005**: Switching from automatic back to manual restores any previously-entered manual
  coordinates with zero re-entry, on the first attempt, every time.

## Assumptions

- **OQ-1 (portal backend availability) resolution**: per this spec's Clarifications, the spec is
  written against the standard `org.freedesktop.portal.Location` interface regardless of whether
  `xdg-desktop-portal-cosmic` currently implements a backend for it. "No backend present" and
  "backend present but declined" are handled by the same FR-005 degrade path, so this spec does
  not need OQ-1 confirmed before being written or planned — confirming it live is deferred to
  `/speckit-plan`/implementation as an early research/spike step, the same way spec 3 deferred
  its `wgpu`-vs-GL choice.
- **Opt-in default (resolves this spec's own Clarifications)**: automatic location defaults to
  OFF and requires an explicit user action to enable, consistent with PRD FR-9 being described
  as "the baseline path" and FR-10 as optional.
- **Accuracy level**: requesting city/neighborhood-level accuracy rather than GPS-exact
  precision is sufficient, since sub-kilometer differences don't meaningfully change sunrise/
  sunset timing beyond the ~3-minute accuracy band spec 1 already accepts (see spec 1's SC-002),
  and minimizes exposed precision as a privacy default.
- **Builds on spec 3 and spec 4, doesn't re-implement them**: this spec extends spec 4's manual
  location control surface (FR-008 there) and spec 3's live `LocationConfig` watch/coalescing
  (FR-014/FR-015 there) rather than introducing parallel mechanisms for either.
- **No new UI surface**: control remains the existing CLI location commands from spec 4,
  extended with an automatic-mode toggle — no GUI is introduced here (Future FR-22 unaffected).
- **Single-user desktop session**: same trust boundary as every other spec in this project — no
  multi-user or multi-session portal handling.
- **IP-geolocation fallback remains out of scope**: PRD FR-12 (IP-based location when no portal
  and no manual entry) is explicitly `[Future]` and is not pulled into this spec's degrade path;
  FR-005's fallback stops at "manual value if present, else no-location," matching FR-12's own
  deferral rationale in the PRD.
