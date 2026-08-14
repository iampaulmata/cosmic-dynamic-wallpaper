# Feature Specification: GUI Usability Improvements

**Feature Branch**: `008-gui-usability-improvements`

**Created**: 2026-08-14

**Status**: Draft

**Input**: User description: "Need to write new specs to address a few things that I want to update. 1. The users need a way to add/remove Packs from the GUI. 2. The notification about the IP-geolocation should be a hover notification for the user when they hover over the radio button. It should also use proper sentence case. 3. The window should either be appropriately sized to show the "Set Manual Location" button or it should scroll. 4. The assignment tab should show the Pack name, not the location."

## Clarifications

### Session 2026-08-14

- Q: Where should the "pack name" shown on the Assignment page actually come from, given the Packs page doesn't display a human-readable name today either — it currently shows the file path, not the manifest's `name` field? → A: Load each registered pack's manifest `name` field (directories) with a sensible fallback for single-image packs (e.g. filename without extension); also switch the Packs page to show this same name instead of the path, for consistency
- Q: When a user adds a pack from the Packs page, how should they provide its location — typed path or a native file/folder picker dialog? → A: A button opens the desktop's native file chooser (via the XDG desktop portal) so the user browses to the pack instead of typing a path
- Q: When a user removes a registered pack from the Packs page, should removal happen immediately on click, or require a confirmation step first? → A: Clicking remove opens a small confirmation dialog ("Remove <pack name>? This cannot be undone.") that the user must confirm before it's removed
- Q: For touch-only users who can't hover, how should they discover the IP-geolocation disclosure before selecting that option? → A: A small (i) info icon sits next to the IP-geolocation option at all times; tapping/clicking it shows the same disclosure text

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Manage packs without leaving the GUI (Priority: P1)

A user browsing their wallpaper packs in the settings application wants to add a newly
downloaded or authored pack, or remove one they no longer want, entirely from within the
application — without switching to a terminal.

**Why this priority**: The settings application currently lets users *browse* registered
packs but not manage which packs are registered at all — for that one action, users are
forced back to the command line, undermining the point of having a GUI. This is the single
biggest capability gap in the current settings application.

**Independent Test**: Can be fully tested by opening the settings application with no packs
registered, adding a pack through the Packs page, confirming it appears and is usable, then
removing it through the same page and confirming it disappears — with no terminal command
run at any point.

**Acceptance Scenarios**:

1. **Given** the Packs page is open, **When** the user opens the native file/folder picker,
   browses to the location of a valid, well-formed pack, and confirms, **Then** the pack
   appears in the list immediately and is available for assignment elsewhere in the
   application.
2. **Given** a pack is already registered, **When** the user selects remove and confirms in
   the resulting dialog, **Then** it disappears from the list immediately and is no longer
   available for assignment.
3. **Given** the user attempts to add a pack whose contents are invalid or malformed,
   **When** they confirm, **Then** the application shows a clear, specific error and the
   pack is not added.
4. **Given** the user attempts to add a pack that is already registered, **When** they
   confirm, **Then** the application succeeds without creating a duplicate entry.
5. **Given** a pack that was bundled with the application (not added by the user) is
   removed, **When** the user later reinstalls or updates the application, **Then** that
   pack does not silently reappear.

---

### User Story 2 - Every control on a page stays reachable (Priority: P1)

A user opens the Location page to enter a manual latitude/longitude and finds the button to
confirm it is cut off by the bottom of the window, with no way to reach it.

**Why this priority**: A control a user cannot reach is a control that does not exist, from
the user's perspective — this blocks an already-shipped capability (setting a manual
location) regardless of how correct the underlying logic is.

**Independent Test**: Can be fully tested by opening the application at its default window
size and confirming every control on every page — especially the manual-location confirm
button — is either fully visible or reachable by scrolling, with no manual window resizing
required.

**Acceptance Scenarios**:

1. **Given** the application is opened at its default size, **When** the user navigates to
   the Location page, **Then** the manual-location entry fields and their confirm button are
   either fully visible or reachable by scrolling within the page.
2. **Given** a page's content is taller than the available window space, **When** the user
   scrolls within that page, **Then** every control on the page becomes reachable without
   needing to resize the window.
3. **Given** the user resizes the window smaller than the default, **When** they view any
   page, **Then** no control becomes permanently unreachable (scrolling remains available).

---

### User Story 3 - Understand IP-geolocation's one external touchpoint before opting in (Priority: P2)

A user considering the IP-geolocation option wants to understand what it actually does —
specifically, the one external network request it makes — before switching to it, not after.

**Why this priority**: This is a privacy-relevant disclosure. Showing it only after a user
has already switched to the mode undersells the "before you opt in" intent; discovering it
on hover, before committing to the choice, is a meaningfully better (if smaller-scope) fix
than the wording/casing issue alone.

**Independent Test**: Can be fully tested by opening the Location page, hovering the
IP-geolocation option without selecting it, and confirming the explanatory text appears,
reads as a proper sentence, and disappears predictably when no longer hovering.

**Acceptance Scenarios**:

1. **Given** the Location page is open and IP-geolocation is not the active mode, **When**
   the user hovers the IP-geolocation option, **Then** a message explaining its one external
   network touchpoint appears near the pointer.
2. **Given** the explanatory message is visible, **When** the user reads it, **Then** it is
   written as a properly capitalized sentence (not a lowercase-leading fragment).
3. **Given** the user moves the pointer away from the IP-geolocation option, **When** the
   message would otherwise remain, **Then** it disappears.

---

### User Story 4 - Recognize assignments by pack name (Priority: P2)

A user checking which pack is assigned to a given display wants to see something they
recognize — the pack's name — not a filesystem path they'd have to decode.

**Why this priority**: This is a clarity fix to an already-working page; it doesn't unblock
any action the way User Story 2 does, but it meaningfully reduces confusion every time a
user checks their assignments.

**Independent Test**: Can be fully tested by assigning a registered pack to an output and
confirming the Assignment page displays that pack's name rather than its file location.

**Acceptance Scenarios**:

1. **Given** a pack with a human-readable name is assigned to an output, **When** the user
   views the Assignment page, **Then** the pack's name is shown for that output, not its
   file location.
2. **Given** the "same pack everywhere" option is active, **When** the user views the
   Assignment page, **Then** the active pack's name is shown, not its file location.

---

### Edge Cases

- What happens when a user removes a pack that is currently assigned to one or more
  outputs? The removal proceeds (matching the existing command-line behavior); the output(s)
  keep using the pack's files directly until reassigned, so nothing currently displayed is
  interrupted.
- What happens when a user attempts to add a pack whose location doesn't exist or isn't
  readable? A clear, specific error is shown and nothing is added.
- What happens on a window resized small enough (e.g., in a tiling layout) that even a
  scrolled page's individual controls would be difficult to reach? Scrolling remains the
  fallback in every case; the application does not impose a hard minimum window size that
  blocks controls outright.
- What happens when a user's input device has no hover capability (e.g., touch-only)? A
  persistent info icon next to the IP-geolocation option makes the explanation discoverable
  by tap, not exclusively through hovering.
- What happens when a pack has no human-readable name available (e.g., a minimally-formed
  pack)? The Assignment page falls back to a clearly-labeled placeholder rather than reverting
  to a raw file location.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The settings application MUST let users register a new pack by browsing to its
  location with the desktop's native file/folder picker (the XDG desktop portal's file
  chooser), without using a separate command-line tool or typing a raw path.
- **FR-002**: The settings application MUST let users remove a registered pack, without
  using a separate command-line tool, and MUST require the user to confirm in a dialog before
  the removal takes effect.
- **FR-003**: Registering a pack through the settings application MUST behave identically to
  registering the same pack via the existing command-line tool (idempotent for
  already-registered packs, clear and specific errors for invalid packs, no partial
  registration on failure).
- **FR-004**: Removing a pack through the settings application MUST behave identically to
  removing the same pack via the existing command-line tool, including preserving the
  existing rule that an explicitly-removed bundled pack never silently reappears on a later
  update.
- **FR-005**: Every page in the settings application MUST present all of its controls such
  that each one is either fully visible at the application's default window size or
  reachable by scrolling — no control may be rendered fully off-screen with no way to reach
  it.
- **FR-006**: The Location page MUST make the manual-location entry fields and their confirm
  control satisfy FR-005 specifically, since this is the control users have found
  unreachable today.
- **FR-007**: The settings application MUST let a user discover the IP-geolocation
  explanatory message by hovering the IP-geolocation option, before that option is selected.
- **FR-008**: The settings application MUST also show a persistent, always-visible info icon
  next to the IP-geolocation option; tapping or clicking it reveals the same explanatory
  message, so users who cannot hover (e.g., touch-only input) can discover it before
  selecting that option too.
- **FR-009**: The IP-geolocation explanatory message MUST be written as a properly
  capitalized, complete sentence.
- **FR-010**: The Assignment page MUST display each output's assigned pack, and the
  "same pack everywhere" toggle's active pack, by the pack's name rather than its file
  location. The pack's name is its manifest's `name` field for directory-sourced packs, or a
  derived name (its filename without extension) for single-image packs, which have no
  manifest.
- **FR-011**: Any page displaying a pack's name (Assignment, Packs) MUST show a
  clearly-labeled placeholder for a pack that has no usable name, rather than falling back to
  a file location.
- **FR-012**: The Packs page MUST display each registered pack by the same name resolved per
  FR-010, instead of its file location, for consistency with the Assignment page.

### Key Entities

- **Pack registration action**: A user-initiated request, from within the settings
  application, to add or remove a known pack — carries the same identity and validation
  rules already established for the command-line tool's equivalent actions. Adding a pack is
  initiated via the native file/folder picker rather than a typed path.
- **IP-geolocation explanatory message**: The existing disclosure text describing
  IP-geolocation's one external network touchpoint; unchanged in meaning, changed in when
  and how it's presented and in its exact capitalization.
- **Pack name**: The human-readable label for a registered pack, shown instead of its file
  location on both the Packs and Assignment pages. Resolved from the pack's manifest `name`
  field when it has one (directory-sourced packs); derived from the filename (without
  extension) for single-image packs, which have no manifest; falls back to a clearly-labeled
  placeholder when neither is usable.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Users can add a new pack and see it available for assignment without leaving
  the settings application or opening a terminal.
- **SC-002**: Users can remove a registered pack and see it disappear from the application
  in a single confirmed action.
- **SC-003**: 100% of controls on every page of the settings application — at the
  application's default window size — are either visible or reachable by scrolling, verified
  by checking each page in turn.
- **SC-004**: Users can identify, at a glance and without decoding a file path, which pack is
  assigned to any given output.
- **SC-005**: Users can read the IP-geolocation external-touchpoint disclosure before
  switching to that mode, not only after.

## Assumptions

- This feature extends the standalone settings application delivered previously; it does not
  change the command-line tool's already-complete pack management commands, which remain
  fully supported.
- "The pack's name" is resolved as defined in Key Entities ("Pack name") — the manifest
  `name` field, or a filename-derived fallback for single-image packs. The Packs page (not
  just Assignment) is also updated to show this name instead of a path, per the 2026-08-14
  clarification.
- The IP-geolocation disclosure's actual content (what it says about the external touchpoint)
  is unchanged from what was already reviewed and approved previously; this feature only
  changes when/how it's shown and its capitalization.
- "Appropriately sized" and "or it should scroll" are both acceptable outcomes per the
  original request; the specific choice (a taller default window, per-page scrolling, or
  both) is left open for planning rather than mandated here.
- Removing a pack through the settings application does not automatically unassign it from
  any output it's currently assigned to, matching the command-line tool's existing behavior.
