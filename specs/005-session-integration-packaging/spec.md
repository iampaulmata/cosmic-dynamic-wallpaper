# Feature Specification: Session Integration & Packaging

**Feature Branch**: `005-session-integration-packaging`

**Created**: 2026-08-14

**Status**: Draft

**Input**: User description: "Session integration & packaging: the daemon must ship as a
proper autostart/session component (systemd user unit and/or cosmic-session integration)
rather than requiring the user to manually launch it and keep a terminal open (FR-24), and the
install/uninstall flow must cleanly handle cosmic-bg's background role on outputs this daemon
takes over — disabling cosmic-bg's role on install so the two renderers never fight over the
same surface, and restoring a sane default background on uninstall rather than leaving the
user with a black screen (FR-25). Governed by constitution Principles I (exclusive layer-shell
ownership) and XI (session integration, including cleanly superseding cosmic-bg); packaging
(distro package and/or Flatpak) is called out in Principle XI as a release requirement, not a
nice-to-have. This is spec 5 of 6 in the project's PRD breakdown (docs/PRD.md section 8),
depending on spec 3's wallpaperd binary and spec 4's wallpaperctl binary already existing (both
implemented)."

## Clarifications

### Session 2026-08-14

- Q: How quickly must the daemon be up and rendering after a COSMIC session starts, for Success
  Criteria SC-001 to count as met? → A: ≤5 seconds — generous headroom over `wallpaperd`'s
  observed sub-second real startup cost, while still feeling instant to a user.
- Q: How many automatic restart attempts should the daemon get after a crash before it's left
  stopped rather than retried again, per FR-003/SC-003's crash-loop bound? → A: 5 attempts within
  a rolling window — systemd's common default burst convention, absorbing a transient hiccup
  without masking a truly broken install.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - The Daemon Starts Itself When You Log In (Priority: P1)

A user installs the project, logs into (or reboots into) their COSMIC session, and the daemon
is already running and rendering their configured wallpapers — no terminal, no manual command,
no "did I remember to start it" uncertainty.

**Why this priority**: This is the entire point of "session integration." Every other
capability in specs 1–4 (scheduling, packs, rendering, the CLI) is worthless to an everyday
user if they have to manually relaunch `wallpaperd` after every login — the project's own
README goal is "install, configure, and forget," and this story is what makes "forget"
literally true.

**Independent Test**: With the daemon installed and at least one pack already assigned (via
`wallpaperctl`, spec 4), log out and back in (or reboot), and confirm the daemon is running and
the assigned pack is rendering — without running any command by hand.

**Acceptance Scenarios**:

1. **Given** the daemon is installed and a pack is assigned, **When** the user starts a new
   COSMIC session, **Then** the daemon is running and rendering that pack within 5 seconds of
   the session starting — no manual launch step.
2. **Given** the daemon is already running as part of the session, **When** the user logs out,
   **Then** the daemon stops cleanly (no orphaned process left running outside the session it
   belonged to).
3. **Given** the daemon was running and is then terminated unexpectedly (crash, killed), **When**
   the session is still active, **Then** it is automatically restarted without the user having
   to notice or intervene (bounded — see Edge Cases for the crash-loop case).
4. **Given** a user who has never touched a terminal, **When** they finish installing the
   package through their normal software-install workflow and log in, **Then** they see their
   wallpaper packs working with zero command-line interaction of any kind.

---

### User Story 2 - Installing Doesn't Leave Two Wallpaper Daemons Fighting (Priority: P1)

A user installs the project on a stock COSMIC system where `cosmic-bg` (the existing wallpaper
daemon this project replaces) is already running the default desktop background. After install,
only this project's renderer is visibly active — no flicker, no two backgrounds racing, no
wasted `cosmic-bg` process still trying to draw over outputs this daemon now owns.

**Why this priority**: Named directly in PRD FR-25 and constitution Principle I/XI. Without
this, every new user's very first experience after installing is a visibly broken or confusing
desktop (competing backgrounds, or an ambiguous "which one actually controls my wallpaper now"
state) — a first-run regression severe enough to undermine trust in the whole project.

**Independent Test**: On a system with `cosmic-bg` active and showing a background, install
this project and confirm — without any manual step beyond the install itself — that `cosmic-bg`
is no longer setting the background on any output this daemon manages, and no visual artifact
(flicker, double image, black flash) appears during the handoff.

**Acceptance Scenarios**:

1. **Given** `cosmic-bg` is running and actively managing the desktop background, **When** the
   user installs this project, **Then** `cosmic-bg`'s background-setting role is disabled as
   part of install, before this daemon's own autostart (User Story 1) takes over — the user
   never sees both active at once.
2. **Given** the install has completed, **When** the user's session next starts this daemon
   (immediately if already logged in, or at next login), **Then** every output this daemon
   manages shows only this project's rendering, never `cosmic-bg`'s.
3. **Given** `cosmic-bg` was already disabled or is not installed at all (e.g. a minimal COSMIC
   setup), **When** the user installs this project, **Then** install still succeeds — this step
   is a no-op, not an error, when there is nothing to disable.
4. **Given** the install runs a second time (e.g. a package reinstall or upgrade), **When**
   `cosmic-bg` is already disabled from a prior install, **Then** the outcome is unchanged
   (idempotent) — no error, no attempt to "disable an already-disabled" role treated as a
   failure.

---

### User Story 3 - Uninstalling Gives You Your Desktop Background Back (Priority: P2)

A user removes the project. Their desktop does not go black or blank — `cosmic-bg` (or whatever
was providing the background before this project took over) resumes doing its job, the same way
it would have if this project had never been installed.

**Why this priority**: Named directly in PRD FR-25 and constitution Principle XI as the
symmetric counterpart to User Story 2 — install must hand off cleanly *from* `cosmic-bg`, and
uninstall must hand back *to* it. A user left with a black screen after removing an
unsatisfactory piece of software is the single worst impression an uninstall can leave, and
directly contradicts this project's stated goal of never regressing the base desktop experience.

**Independent Test**: With the project installed and actively rendering, uninstall it and
confirm — without any manual step beyond the uninstall itself, and without needing to log out
first — that a normal, non-black background reappears on every output.

**Acceptance Scenarios**:

1. **Given** this daemon is actively rendering on one or more outputs, **When** the user
   uninstalls the project, **Then** this daemon's rendering stops and `cosmic-bg`'s
   background-setting role is restored to whatever state it was in before install (User Story
   2's disable is reversed).
2. **Given** uninstall has completed, **When** the user's session next needs a background drawn
   (immediately if the restore is live, or at next login if not), **Then** a normal background
   is shown — never a black or blank output.
3. **Given** the user manually disabled `cosmic-bg` themselves *before* ever installing this
   project (unrelated to this project's own install step), **When** they later uninstall this
   project, **Then** uninstall does not silently override that user choice by force-re-enabling
   `cosmic-bg` — see Edge Cases for how this is distinguished.

---

### User Story 4 - You Can Actually Install It Without Building From Source (Priority: P3)

A user obtains and installs the project through their distribution's normal package-install
workflow (a native package or a Flatpak), the same way they'd install any other desktop
application — not by cloning the repository, running `cargo build`, and manually copying
binaries into place.

**Why this priority**: Called out explicitly in constitution Principle XI as a release
requirement ("not a nice-to-have"), and it's the prerequisite that makes User Stories 1–3
reachable by an ordinary user at all — everything above this assumes installation already
happened through a normal path. Ranked P3 relative to 1–2 only because, during this project's
own development, a source build has been the actual install path in use, and packaging can be
finished once the session-integration behavior it wraps (Stories 1–3) is proven correct.

**Independent Test**: Starting from a clean target system with none of this project's source
code present, install the project using only its packaged distribution (native package or
Flatpak) and confirm the daemon and CLI (`wallpaperd`, `wallpaperctl`) are both available and
working, with no `cargo`/Rust toolchain step performed by the installer.

**Acceptance Scenarios**:

1. **Given** a target system with this project not yet installed, **When** the user installs it
   via a native distribution package or a Flatpak, **Then** both `wallpaperd` and `wallpaperctl`
   (specs 3–4) are available and functional afterward.
2. **Given** the package is installed, **When** the user checks how to remove it, **Then** they
   can do so through the same package manager they used to install it (no separate manual
   cleanup script required for the basic case).

---

### Edge Cases

- What happens if the daemon crashes repeatedly right after being (re)started (e.g. a corrupted
  config value)? It MUST NOT restart in a tight, unbounded loop consuming CPU indefinitely — after
  5 automatic restart attempts within a rolling window (FR-003), it stays stopped and the failure
  is discoverable (e.g. via the session's normal service-status tooling), rather than retried
  again (User Story 1 Scenario 3's bound).
- What happens if `cosmic-bg` is not installed at all when this project is installed (a minimal
  or customized COSMIC setup)? Install MUST still succeed (User Story 2 Scenario 3) — resolved
  during planning (research.md R3) as inherently true rather than needing a defensive check:
  install performs no `cosmic-bg`-specific action at all, so there is nothing that could fail
  based on whether `cosmic-bg` happens to be present.
- What happens if the user had already manually disabled `cosmic-bg` themselves before ever
  installing this project (if some future mechanism ever makes that possible)? Uninstall MUST
  NOT interpret that as "this project's install must have disabled it" and force-re-enable
  something the user deliberately turned off on their own. Resolved during planning
  (research.md R3): moot for the mechanism this spec actually ships — install never disables
  `cosmic-bg` in the first place (there is no external lever to do so at all), so uninstall
  never needs to distinguish "who turned it off" or record anything to restore later (User
  Story 3 Scenario 3).
- What happens if a user uninstalls while `wallpaperd` is actively mid-crossfade on an output?
  The transition simply stops when the process is removed — no special teardown sequence is
  required beyond User Story 3's background-restoration guarantee applying immediately after.
- What happens if the user runs `wallpaperd` manually from a terminal (as has been done during
  this project's own development and testing) alongside — or instead of — the session-managed
  instance? This spec's autostart mechanism and a manually-launched instance are not required to
  coexist safely (running the same layer-shell-owning daemon twice is already an unsupported
  configuration by constitution Principle I); this spec only guarantees the *session-managed*
  path works without manual launching, not that manual launching stops being possible for
  development use.
- What happens on a system where the install target isn't COSMIC at all, or `cosmic-session`
  itself isn't present? Out of scope — this project is COSMIC-only (README Non-goals), so
  install is only ever expected to run on a COSMIC system where session integration has a
  session to integrate with.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The project MUST ship a session-integrated autostart mechanism (a systemd user
  unit, activated as part of a COSMIC session) that starts `wallpaperd` automatically when a
  COSMIC session begins, with no manual command required by the user.
- **FR-002**: The autostart mechanism MUST stop `wallpaperd` cleanly when the session ends, and
  MUST NOT leave an orphaned process running outside the session's lifecycle.
- **FR-003**: If `wallpaperd` exits unexpectedly while the session is still active, the autostart
  mechanism MUST restart it automatically, bounded by a limit of 5 restart attempts within a
  rolling window, after which it MUST stay stopped rather than keep retrying, so a
  persistently-crashing daemon does not consume resources in an unbounded restart loop (Edge
  Cases).
- **FR-004**: Installing the project MUST disable `cosmic-bg`'s background-setting role on every
  output this daemon manages, before this daemon's own rendering becomes active, so the two are
  never both drawing a background at once.
- **FR-005**: The install step covered by FR-004 MUST be idempotent and MUST succeed as a no-op
  when `cosmic-bg` is not installed, not running, or already disabled — none of those states is
  an install failure.
- **FR-006**: Uninstalling the project MUST restore `cosmic-bg`'s background-setting role,
  unless the install step (FR-004) never actually performed the disable (e.g. `cosmic-bg` wasn't
  present, or was already off before this project was installed) — uninstall MUST NOT override a
  `cosmic-bg` state the user set independently of this project (Edge Cases).
- **FR-007**: After uninstall completes, every output MUST show a normal background again (via
  the restored `cosmic-bg`, or an equivalent fallback if `cosmic-bg` itself is unavailable at
  uninstall time) — a black or blank output is never an acceptable end state of uninstalling.
- **FR-008**: The project MUST be installable through at least one standard packaging mechanism
  for the target platform (a native distribution package, a Flatpak manifest, or both) that
  provides both `wallpaperd` and `wallpaperctl` without requiring the end user to build from
  source.
- **FR-009**: The packaged install MUST be removable through the same package manager used to
  install it, without requiring a separate manual cleanup script for the basic install/uninstall
  case.

### Key Entities

- **Session Unit**: The autostart definition (a systemd user unit, per FR-001) that ties
  `wallpaperd`'s lifecycle to the COSMIC session's own — start-on-session-begin,
  stop-on-session-end, bounded auto-restart-on-crash.
- **Distribution Package**: The install artifact from FR-008 (native package and/or Flatpak)
  bundling `wallpaperd`, `wallpaperctl`, and the Session Unit together.

  *(Resolved during planning — no `cosmic-bg` Handoff State entity: research.md R3 found
  `cosmic-bg` cannot be externally disabled at all, so there is nothing for install to record
  and nothing for uninstall to restore — FR-004–FR-007 are satisfied structurally by the Session
  Unit alone. An earlier draft of this section named a "`cosmic-bg` Handoff State" entity for
  exactly that now-unnecessary bookkeeping; removed rather than left stale.)*

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A user who has just installed the project and configured at least one pack
  assignment sees that pack rendering within 5 seconds of starting a new COSMIC session — with
  zero commands typed.
- **SC-002**: Across install, active use, and uninstall, no output is ever observed showing two
  overlapping backgrounds at once, nor a black/blank background, at any point a background
  should normally be visible.
- **SC-003**: 100% of daemon crashes while a session is active result in an automatic restart
  within a bounded time, without user intervention — up to the 5-attempt crash-loop bound
  (FR-003), past which the daemon staying stopped is itself discoverable rather than silent.
- **SC-004**: A user can install and later fully uninstall the project using only their
  platform's normal package-management workflow, with no manual file editing or cleanup steps
  required for the basic case.
- **SC-005**: Re-running install (e.g. upgrading to a new version) or re-running uninstall never
  produces a different outcome than running it once — both are idempotent.

## Assumptions

- **`cosmic-bg` suppression scope**: PRD FR-25 scopes the disable to "outputs this daemon takes
  over." In practice, `wallpaperd` (spec 3) creates its exclusive background layer-shell surface
  for every output it detects as soon as it starts, regardless of whether that output has a pack
  assigned yet — there is no notion in specs 1–4 of an output this daemon is running but
  deliberately *not* managing. This spec therefore treats "outputs this daemon takes over" as
  equivalent to "system-wide, whenever this daemon's session component is active," rather than
  needing a new per-output `cosmic-bg` exclusion mechanism spec 3 doesn't have a way to express
  today. If a future spec introduces true per-output opt-out of this daemon entirely, this
  assumption should be revisited.
- **Uninstall's fallback if `cosmic-bg` itself is gone**: FR-007 allows "an equivalent fallback"
  because `cosmic-bg` could theoretically have been removed independently between this project's
  install and uninstall. Resolved during planning (research.md R3, verified live against this
  project's own dev COSMIC session): in practice `cosmic-bg` is never actually stopped by install
  at all (see next bullet), so this fallback path only matters in the already-unusual case of a
  user separately uninstalling `cosmic-bg` itself — genuinely out of this spec's scope to handle,
  since there would be no background renderer of any kind left on the system regardless of what
  this project does.
- **`cosmic-bg` disable/restore mechanism — resolved during planning, not left open**:
  research.md R1/R3 found, verified against this project's own live COSMIC session and
  `cosmic-session`'s upstream source, that `cosmic-bg` is unconditionally spawned by
  `cosmic-session` itself with no config/environment/CLI lever any external package can use to
  stop it — there is no "disable cosmic-bg" toggle to implement. FR-004–FR-007's outcomes (no
  visible double background; no black screen on uninstall) are instead satisfied structurally:
  `wallpaperd`'s pre-existing exclusive, opaque `Layer::Background` surface (spec 3) already
  makes `cosmic-bg`'s output invisible the moment `wallpaperd` is running, and since `cosmic-bg`
  was never stopped, uninstall needs no explicit "restore" step — `cosmic-bg`'s
  already-running surface simply becomes visible again once `wallpaperd.service` stops. This
  matches constitution Principle I's own second framing ("or fully superseding it as the
  session's background service") rather than its first ("disabling cosmic-bg's background
  role"). FR-004–FR-007's *text* is unchanged — the outcome they require still holds — only the
  previously-open mechanism question is now resolved.
- **Packaging format choice is a planning decision**: constitution Principle XI itself says "at
  least one of a distro package or Flatpak manifest," deliberately not picking one — this spec
  preserves that openness (FR-008) rather than prematurely committing to a specific format.
- **"systemd user unit" and "cosmic-session integration" are the same mechanism — confirmed,
  not just assumed**: research.md R1 verified live that a real `cosmic-session.target` systemd
  target exists precisely for this purpose (session-scoped autostart components bind
  `WantedBy=`/`PartOf=` it), distinct from `cosmic-session`'s own hardcoded first-party
  component list (which a third-party project cannot join). PRD FR-24's "and/or" phrasing is
  resolved as one mechanism, not two to support separately.
- This spec assumes specs 3–4 already exist as implemented, working binaries (`wallpaperd`,
  `wallpaperctl`) — it wraps their lifecycle and distribution, it does not change their behavior.
- Crossfade/scheduling correctness, multi-output behavior, and the CLI's own command surface are
  entirely out of scope here (specs 1–4's concern) — this spec is only about how the already-
  working system starts itself, stops itself, and reaches a user's machine in the first place.
- Multi-user/multi-session machines are out of scope beyond "each user's own session manages its
  own instance" — no cross-user coordination is assumed or required.
