# Feature Specification: Pack Format & Loading

**Feature Branch**: `002-pack-format-loading`

**Created**: 2026-08-11

**Status**: Draft

**Input**: User description: "spec 2 — from docs/PRD.md, spec breakdown item 2: pack format & loading, covering FR-1 (a wallpaper pack is an ordered set of time-anchored images plus pack-level metadata), FR-2 (a pack is loadable from a local directory containing images plus a manifest file, schema owned and versioned by this project), FR-3 (a static mode for a single image with no time anchors, at feature parity with 'just set a normal wallpaper'), FR-4 (per-pack or per-image scaling/fit behavior — Fill, Fit, Stretch, Center — with a configurable fallback fill color), and FR-20 (all state persisted via cosmic-config, versioned). Resolves PRD Open Question OQ-3 (manifest format) as part of this spec, since the PRD flags it as needing a decision before FR-2 is specced precisely. Depends on spec 1 (core scheduling engine) for the time-anchor and pack-validation contract this spec's loaded packs must satisfy."

## Clarifications

### Session 2026-08-11

- Q: Should the loader reject a manifest entry whose image filename tries to escape the pack's own directory, rather than following it wherever it points? → A: Reject any image entry whose resolved path falls outside the pack directory — same clear-error path as FR-006.
- Q: Should a user be able to explicitly remove a previously-loaded pack from the persisted registry, so it's forgotten rather than just marked unavailable? → A: Explicit removal is in scope for this spec — the registry supports deleting a known pack's entry outright, distinct from FR-011's automatic "unavailable" state.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Load a Multi-Image Time-Anchored Pack From a Directory (Priority: P1)

A user points the daemon at a directory containing a set of images and a manifest file. The
daemon reads the manifest, matches each declared image to a file on disk, and produces a
fully-formed wallpaper pack — ready to hand to the scheduling engine (spec 1) — without the
user writing any code or hand-editing a config file beyond the manifest itself.

**Why this priority**: This is the feature's primary content-loading path and the direct
prerequisite for everything the scheduling engine (spec 1) and renderer (spec 3) do — neither
can act on a pack that was never successfully loaded.

**Independent Test**: Author a manifest referencing a small set of real image files with a
mix of time anchors, point the loader at that directory, and verify the resulting in-memory
pack has the correct images, anchors, and pack-level metadata — independent of any renderer
or daemon process.

**Acceptance Scenarios**:

1. **Given** a directory with a valid manifest and all referenced image files present,
   **When** the pack is loaded, **Then** the loader returns a pack whose images, time
   anchors, and metadata (name, scaling default, author/license note) match the manifest.
2. **Given** a manifest that references an image file that does not exist in the directory,
   **When** the pack is loaded, **Then** loading fails with a clear, specific error naming
   the missing file, and no partial or corrupted pack is returned.
3. **Given** a manifest whose declared time anchors would be rejected by the scheduling
   engine's own validation (spec 1) — for example mixed anchor types, or more than 64
   anchors — **When** the pack is loaded, **Then** the loader surfaces that same validation
   error rather than silently truncating or guessing.
4. **Given** a directory containing image files not referenced anywhere in the manifest,
   **When** the pack is loaded, **Then** those extra files are ignored and do not cause an
   error.

---

### User Story 2 - Zero-Config Static Wallpaper (Priority: P1)

A user who just wants a normal, unchanging desktop background points the daemon at a single
image file — no manifest, no time anchors, no pack directory structure required. This is the
same baseline capability every other desktop's background setting offers, since this daemon
takes over the background role entirely (per the project constitution) and must not regress
that basic case.

**Why this priority**: Without this, adopting the daemon at all would be a downgrade for
anyone not ready to build a full time-anchored pack — it's the floor every user starts from.

**Independent Test**: Point the loader at a single image file path with no manifest present,
and verify it produces a valid one-image pack with no time anchors, usable exactly like any
other loaded pack.

**Acceptance Scenarios**:

1. **Given** a path to a single valid image file and no manifest, **When** the pack is
   loaded, **Then** the loader returns a static one-image pack with no time anchors.
2. **Given** a path to a file that is not a readable image, **When** the pack is loaded,
   **Then** loading fails with a clear error rather than silently producing a broken pack.

---

### User Story 3 - Configure Scaling & Fit Behavior (Priority: P2)

A user (or pack author) sets how an image fills the screen — Fill, Fit, Stretch, or Center —
either once for an entire pack or overridden per individual image, plus a fallback fill color
for any letterboxed edges, matching the scaling options users already expect from `cosmic-bg`.

**Why this priority**: Important for visual correctness across mixed-aspect-ratio image sets,
but a pack can be loaded and scheduled without it (a sane default scaling mode still renders
something reasonable) — it doesn't block Stories 1 or 2.

**Independent Test**: Author a manifest with a pack-level scaling default and one image that
overrides it, load the pack, and verify the loaded pack reports the pack-level default for
unoverridden images and the per-image override where declared.

**Acceptance Scenarios**:

1. **Given** a manifest with a pack-level scaling mode and no per-image override, **When**
   the pack is loaded, **Then** every image reports that pack-level scaling mode.
2. **Given** a manifest where one image declares its own scaling mode, **When** the pack is
   loaded, **Then** that image reports its own mode while the rest report the pack default.
3. **Given** a manifest with an invalid scaling mode name or a malformed fallback color
   value, **When** the pack is loaded, **Then** loading fails with a clear, specific error.

---

### User Story 4 - Known Packs Persist Across Daemon Restarts (Priority: P3)

Once a user has pointed the daemon at a pack, it stays known and available the next time the
daemon starts — the user does not need to re-select the same directory every session.

**Why this priority**: A real quality-of-life requirement (FR-20), but the daemon is still
fully usable session-to-session without it if a caller re-supplies the pack location — it's
a persistence convenience layered on top of Stories 1–3, not a blocker for them.

**Independent Test**: Load a pack, persist its registration, restart the loading component
fresh, and verify the previously-loaded pack's location is still known without re-scanning
or re-specifying it.

**Acceptance Scenarios**:

1. **Given** a pack that has been successfully loaded once, **When** the daemon restarts,
   **Then** the pack's source location is still known without the user re-specifying it.
2. **Given** a previously-known pack whose source directory has since been deleted or moved,
   **When** the daemon restarts and tries to reload it, **Then** that one pack is reported as
   unavailable rather than crashing or silently dropping all other known packs.
3. **Given** a known pack the user no longer wants remembered, **When** the user explicitly
   removes it, **Then** its registry entry is deleted outright and it is no longer reported
   among known packs on the next restart — distinct from merely being unavailable.

---

### Edge Cases

- What happens when the manifest file itself is malformed (fails to parse at all)? Loading
  MUST fail with a clear, specific parse error rather than crashing or silently falling back.
- What happens when a manifest declares a schema version newer than this version of the
  loader understands? Loading MUST fail with a clear "unsupported schema version" error
  rather than guessing at the newer format's meaning.
- What happens when a manifest declares an older, previously-supported schema version? The
  loader MUST apply the documented migration path (constitution Principle X / NFR-5) rather
  than rejecting it outright.
- What happens when two separately-loaded packs share the same declared name? Pack identity
  is keyed by source location, not declared name, so this MUST NOT cause a collision — both
  remain independently loadable and distinguishable.
- What happens when an image file exists but is corrupt or an unreadable format? That single
  image reference MUST fail loading with a clear error identifying the file, consistent with
  the project constitution's "failures are contained, never fatal" principle — this is the
  same contained-failure posture as a missing file (Story 1, Scenario 2).
- What happens with non-UTF-8 file or directory names? The loader MUST reject them with a
  clear error rather than risk silently mishandling or corrupting a path.
- What happens when a manifest's image entry resolves to a path outside the pack's own
  directory (`..` traversal, an absolute path, or a symlink pointing elsewhere)? The loader
  MUST reject that entry with a clear error rather than reading the out-of-directory file
  (see FR-006a) — this matters specifically because packs are meant to be shared between
  people, so a manifest is untrusted input by the time it reaches someone else's machine.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: A wallpaper pack MUST be an ordered set of images, each associated with the
  time anchor contract defined by the scheduling engine (spec 1), plus pack-level metadata:
  a display name, a default scaling mode, and an optional author/license note.
- **FR-002**: A pack MUST be loadable from a local directory containing image files plus a
  manifest file written in TOML (resolves PRD Open Question OQ-3 — chosen over RON and JSON
  for familiarity outside the Rust ecosystem and comment support; see Assumptions),
  validated against a schema this project owns and versions (FR-20, constitution
  Principle X).
- **FR-003**: The loader MUST hand off every declared image's time anchor to the scheduling
  engine's own pack-validation contract (spec 1's `WallpaperPack::validate`) rather than
  re-implementing anchor-correctness rules — mixed-anchor-type rejection, the 64-anchor cap,
  and exact-instant tie rejection all apply here by inheritance, not duplication.
- **FR-004**: A static mode MUST exist: pointing the loader at a single image file with no
  manifest present MUST produce a valid one-image pack with no time anchors, requiring no
  additional configuration — full parity with "just set a normal wallpaper."
- **FR-005**: The loader MUST support a scaling/fit mode of Fill, Fit, Stretch, or Center,
  settable at the pack level and overridable per individual image, plus a configurable
  fallback fill color for letterboxed edges.
- **FR-006**: The loader MUST reject, with a clear and specific error, any manifest that:
  fails to parse; references an image file not present in the directory; declares an invalid
  scaling mode or malformed fallback color; or declares a schema version newer than the
  loader supports. None of these conditions may crash the loader or produce a partial pack.
- **FR-006a**: The loader MUST resolve every image entry's path relative to the pack
  directory and MUST reject, with the same clear-error posture as FR-006, any entry whose
  resolved path (via `..` traversal, an absolute path, or a symlink) falls outside that
  directory — a shared, untrusted manifest MUST NOT be able to make the loader read a file
  the pack author didn't ship alongside it.
- **FR-007**: The loader MUST support migrating a manifest written against an older,
  previously-supported schema version to the current one, per a documented migration path
  (constitution Principle X / NFR-5).
- **FR-008**: Extra image files present in a pack's directory but not referenced by the
  manifest MUST be ignored, not treated as an error.
- **FR-009**: Pack identity MUST be keyed by source location (directory path for manifest
  packs, file path for static packs), not by the manifest's declared display name, so two
  packs may share a display name without colliding.
- **FR-010**: The set of known pack locations MUST be persisted via `cosmic-config`,
  versioned per constitution Principle X, so previously-loaded packs remain known across a
  daemon restart without the user re-specifying them (FR-20).
- **FR-011**: If a previously-known pack's source location becomes unavailable (deleted,
  moved, unreadable) at reload time, only that pack MUST be marked unavailable — this MUST
  NOT prevent loading of any other known pack.
- **FR-012**: A user MUST be able to explicitly remove a known pack's entry from the
  registry, distinct from FR-011's automatic "unavailable" marking — removal deletes the
  registry entry outright rather than leaving a stale/unavailable record behind.

### Key Entities

- **Pack Manifest**: The on-disk TOML file describing a pack — a schema version, pack-level
  metadata (name, default scaling mode, author/license note), and an ordered list of image
  entries (filename, time anchor, optional per-image scaling override).
- **Wallpaper Pack (loaded form)**: The in-memory result of successfully loading a manifest
  plus its directory, or a single static image — the shape spec 1's scheduling engine and
  spec 3's renderer consume.
- **Static Wallpaper**: The degenerate, manifest-free pack form (FR-004) — one image, no time
  anchors, always active — matching the single-image case spec 1's data model already
  anticipates.
- **Scaling Configuration**: One of Fill / Fit / Stretch / Center plus a fallback fill color,
  attachable at pack level or per-image, with per-image taking precedence when both are set.
- **Pack Registry Entry**: The persisted record (via `cosmic-config`) of a known pack's
  source location and identity, independent of whether that pack is currently loaded in
  memory. Has two distinct end states for a pack that's no longer loadable: automatically
  marked *unavailable* when its location disappears out from under it (FR-011, entry
  retained), versus explicitly *removed* by the user (FR-012, entry deleted outright).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A user with a valid manifest and image directory can have all of that pack's
  images, anchors, and metadata available to the rest of the system in a single load
  operation, with no manual re-entry of information already present in the manifest.
- **SC-002**: 100% of malformed manifests, missing images, or unsupported schema versions
  produce a specific, actionable error rather than a crash, a hang, or a silently incomplete
  pack.
- **SC-003**: Selecting a single image file with no manifest requires zero additional steps
  beyond choosing that file — full parity with a traditional single-wallpaper picker.
- **SC-004**: 100% of previously-loaded packs remain known after a daemon restart with no
  user action required to re-register them.
- **SC-005**: A person who has only read the published manifest schema documentation (no
  access to this project's source code) can hand-author a valid multi-image manifest on
  their first attempt.

## Assumptions

- **Manifest format (resolves OQ-3)**: TOML. Both RON and TOML support the comments a
  hand-authored, shareable format benefits from, but TOML is far more widely recognized
  outside the Rust ecosystem (it's the format of `Cargo.toml` and many other Linux config
  files), which matters directly for FR-1/FR-2's audience of pack authors who are not
  assumed to be Rust developers (PRD Primary User). `cosmic-config`'s own RON-based store
  (constitution Principle IV) governs the daemon's *internal* persisted state (FR-010's pack
  registry, FR-20) — that is a separate, already-settled decision and does not require pack
  manifests themselves to also be RON.
- This spec covers loading and validating packs into memory; it does not cover assigning a
  loaded pack to a specific output (spec 3, FR-17/FR-18) or rendering it.
- The manifest schema versioning and migration mechanics reuse the same approach the
  scheduling engine spec (spec 1) and the constitution (Principle X, NFR-5) already commit
  the project to — this spec does not invent a second migration strategy.
- Image file format support (which raster formats are decodable) is treated as an
  implementation detail of the loader, not a product decision requiring a functional
  requirement here.
- A pack's images are read from local disk only; no network-sourced packs are in scope (PRD
  Non-Goal NG2 — no marketplace/hosting service).
