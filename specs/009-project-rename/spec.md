# Feature Specification: Rename Project to "Cosmic Dynamic Wallpaper"

**Feature Branch**: `009-project-rename`

**Created**: 2026-08-15

**Status**: Draft

**Input**: User description: "I want to rename this project to "Cosmic Dynamic Wallpaper" throughout every aspect of the project. Everything from the folder name, to every reference in every document, and the github repo."

## Clarifications

### Session 2026-08-15

- Q: Should the renamed `.deb` package declare itself as replacing the old `dynamic-wallpaper` package so a normal `apt install`/upgrade removes the old one automatically, or should that be a manual step? → A: Declare `Replaces`/`Conflicts`/`Breaks` against `dynamic-wallpaper` in the new package, so `apt install`/upgrade cleanly removes the old one first (its existing `prerm`/`postrm` run automatically, disabling its systemd unit) — a real upgrade path.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Consistent Public Branding (Priority: P1)

A prospective user or contributor finds the project on GitHub, clones it, or reads its
documentation, and sees "Cosmic Dynamic Wallpaper" as the project's name everywhere —
the repository name, the README title, every spec/plan document, and the local folder
name — with no leftover references to the old name ("Dynamic Wallpaper" /
"rust-dynamic-wallpaper") anywhere a reader would naturally encounter them.

**Why this priority**: This is the entire point of the request — a rename that's only
partially applied (e.g. GitHub renamed but the README still says the old name) is worse
than not renaming at all, since it reads as an unfinished or abandoned project.

**Independent Test**: Clone the repository fresh under its new URL, open the README, and
confirm the project name is consistent across the repo name, folder name, and every
document's title/prose — deliverable and verifiable without touching any code behavior.

**Acceptance Scenarios**:

1. **Given** the renamed GitHub repository, **When** a visitor opens its main page,
   **Then** the repository name and README title both read "Cosmic Dynamic Wallpaper"
   with no old name visible above the fold.
2. **Given** a fresh local clone, **When** the clone completes, **Then** the top-level
   folder name reflects the new project name, not `dynamic-wallpaper`.
3. **Given** any spec/plan/tasks document under `specs/`, **When** it references the
   project by name, **Then** it uses the new name.

---

### User Story 2 - Existing Installations Keep Working (Priority: P2)

Someone who already has the daemon installed and configured (location, registered
packs, per-display assignment) upgrades to a build produced after the rename, and their
existing setup keeps working exactly as before, with no reconfiguration needed — even
though every system-facing identifier (D-Bus name, `cosmic-config` application IDs,
package name) has changed underneath them.

**Why this priority**: A rename that quietly orphans every existing user's configuration
turns a cosmetic change into a regression — including on the maintainer's own dev
machine, which already has a real configured installation from beta testing.

**Independent Test**: Install the pre-rename package, configure a location and a pack
assignment, then upgrade to the post-rename package, and confirm the configuration
survives untouched with no user action.

**Acceptance Scenarios**:

1. **Given** an existing installation with a configured location and assigned packs,
   **When** the user upgrades to the renamed build, **Then** their settings are migrated
   automatically and are visible/effective immediately, with nothing to redo.

---

### User Story 3 - Contributor-Facing Consistency (Priority: P3)

A contributor working in the codebase (reading the constitution, specs, crate READMEs,
or code comments that name the project) never hits a reference to the old name that
makes them wonder if they're in a stale checkout or the wrong repo.

**Why this priority**: Lower-stakes than the public-facing rename (P1) or the
existing-user impact (P2), but a half-renamed codebase is a recurring source of
confusion for anyone who opens more than the README.

**Independent Test**: Full-text search the repository for the old project name outside
of explicitly-historical content (e.g. old changelog entries, already-published release
notes) and confirm no unintentional matches remain.

**Acceptance Scenarios**:

1. **Given** the fully renamed codebase, **When** a contributor searches for the old
   project name, **Then** the only matches are in explicitly historical content (already
   published release notes/tags), not in current docs or active code comments.

---

### Edge Cases

- What happens to already-published GitHub releases and their download assets (e.g.
  `dynamic-wallpaper_0.1.0-3_amd64.deb`) after the rename — are they renamed
  retroactively, or left as historical artifacts under the old name?
- What happens to the local git remote URL on any existing clone (including this dev
  machine) once the GitHub repository itself is renamed?
- What happens to open issues/PRs whose title or body text mentions the old project
  name in prose?
- A machine with the old `dynamic-wallpaper` package still installed and enabled
  installs the new package: `apt` MUST remove the old package first (FR-004b's
  `Replaces`/`Conflicts`/`Breaks`), which disables its systemd unit via its own
  `prerm`, before the new package's unit is enabled — the two daemons MUST NOT both be
  enabled at once.
- What happens if the one-time config migration (FR-004a) can't find an old-ID store to
  migrate from (e.g. a genuinely fresh install) — it MUST be a silent no-op, not an error,
  indistinguishable from any other first run.
- What happens if a migration is attempted twice (e.g. the daemon restarts mid-migration,
  or both the daemon and the GUI settings app race to migrate the same store on first
  run) — it MUST be idempotent, not duplicate or corrupt data.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The project's display name MUST read "Cosmic Dynamic Wallpaper" everywhere
  it appears in human-readable prose — the README, the constitution, every spec/plan/
  tasks/checklist document, crate-level READMEs, code comments that name the project,
  CLI `--help`/about text, the GUI's window title, and the `.desktop` entry's `Name`
  field.
- **FR-002**: The local top-level project folder MUST be renamed from `dynamic-wallpaper`
  to a filesystem-safe slug of the new name.
- **FR-003**: The GitHub repository MUST be renamed to match the new project name, and
  any local git remote pointing at its old URL MUST be updated to the new one.
- **FR-004**: System-facing identifiers currently embedded in the codebase — the compiled
  binary names (`wallpaperd`, `wallpaperctl`, `wallpaper-settings`), the Cargo workspace's
  crate package names, the Debian package name (`dynamic-wallpaper`), the D-Bus bus/
  interface name (`com.system76.CosmicWallpaper1*`), the `cosmic-config` application IDs
  (`com.system76.CosmicWallpaper.*`, one per persisted settings store), and the
  `.desktop` entry's application ID (`com.system76.CosmicWallpaperSettings`) — MUST be
  renamed to match the new project name (resolved: full rename, Option A). Binary/daemon
  names follow the established COSMIC-ecosystem convention of a `cosmic-` prefix (matching
  `cosmic-bg`, `cosmic-comp`, `cosmic-panel`, etc. already referenced elsewhere in this
  project), and every `com.system76.CosmicWallpaper*` identifier is renamed to
  `com.system76.CosmicDynamicWallpaper*` (D-Bus bus/interface, every `cosmic-config`
  application ID, and the `.desktop` application ID alike) — the exact new names are an
  implementation-plan decision, but MUST consistently reflect "Cosmic Dynamic Wallpaper".
- **FR-004a**: Because each renamed `cosmic-config` application ID is a distinct on-disk
  store, upgrading to the renamed build MUST NOT lose a user's previously-configured
  location, pack registry, or per-display/renderer assignment. On first run after the
  rename, any settings found under each old application ID MUST be migrated (read once
  under the old ID, written under the new ID) automatically and without user action —
  mirroring this project's existing versioned config-migration precedent
  (`LocationConfigEntry`'s v2→v3 migration), extended here to also cover an application-ID
  change, not just an in-place schema-version bump.
- **FR-004b**: The renamed `.deb` package MUST declare `Replaces`/`Conflicts`/`Breaks`
  against the old `dynamic-wallpaper` package name, so a normal `apt install`/upgrade of
  the new package automatically removes the old one first (running its existing
  `prerm`/`postrm`, which disables its systemd unit) rather than leaving both packages —
  and both packages' systemd units and binaries — installed side by side. This is what
  makes FR-004a's config migration reachable through an ordinary upgrade instead of
  requiring users to know to manually purge the old package first, and it prevents two
  renderer daemons ever being enabled at once (constitution Principle I: exclusive
  ownership of the background surface).
- **FR-005**: All Markdown documentation under the repository (README.md, the
  constitution, every file under `specs/`, and any crate-level README) MUST be updated
  to use the new project name in place of "Dynamic Wallpaper" / "dynamic-wallpaper" /
  "rust-dynamic-wallpaper".
- **FR-006**: Packaging metadata that's user-visible — the `.deb`'s description/summary
  text and the `.desktop` entry's `Name`/`Comment` fields — MUST reflect the new display
  name, distinct from FR-004's renaming of the underlying package/identifier names
  themselves (both change, but this covers the human-readable text specifically).
- **FR-007**: The rename MUST NOT alter any functional behavior of the daemon, CLI, or
  GUI — this is a naming-only change. The existing automated test suite MUST continue to
  pass with no changes to test assertions (test names/comments that reference the old
  project name MAY be updated for consistency, but what they verify does not change).
- **FR-008**: A new build (packaging revision bump and/or version tag) MUST be produced
  under the new name once the rename is complete, so the next distributed artifact
  reflects it — not just the source tree.

### Key Entities

N/A — this is a naming/branding change with no data entities of its own. FR-004/FR-004a
change the *storage identifiers* of existing settings entities (location, pack registry,
renderer assignment), but no new entity is introduced by this feature.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A full-text search of all tracked documentation files for the old project
  name ("Dynamic Wallpaper", "dynamic-wallpaper", "rust-dynamic-wallpaper") returns zero
  matches outside of explicitly historical content (e.g. already-published release
  notes).
- **SC-002**: A new visitor to the GitHub repository sees "Cosmic Dynamic Wallpaper" as
  both the repository name and the README's title within the first screen, with no
  mixed old/new branding visible.
- **SC-003**: The project builds, packages, and installs end to end (workspace build,
  `.deb` packaging, install) using only the new names, with zero build or install
  failures caused by a lingering old-name reference.
- **SC-004**: 100% of existing installations upgrade to the renamed build with their
  configured location, pack registry, and per-display/renderer assignment intact and no
  user action required (FR-004a's migration).
- **SC-005**: A machine with the old package installed and enabled upgrades via a single
  `sudo apt install ./cosmic-dynamic-wallpaper_*.deb` (or `apt upgrade`) with no manual
  removal step, ending with exactly one renderer daemon package installed and enabled —
  never both at once (FR-004b).

## Assumptions

- The new folder/package slug is `cosmic-dynamic-wallpaper` (lowercase, hyphenated) —
  the conventional machine-readable form of "Cosmic Dynamic Wallpaper" — unless
  specified otherwise.
- Renaming the GitHub repository itself is an action only the repository owner can take
  (via GitHub's own settings UI, or an authenticated API call this environment currently
  has no access to perform) — implementation will need to hand that specific step to the
  user directly rather than executing it end-to-end.
- GitHub automatically redirects the old repository URL to the new one after a rename,
  so existing bookmarks and clone URLs keep working without further action.
- Already-published release assets (e.g. `dynamic-wallpaper_0.1.0-3_amd64.deb`) are
  historical artifacts and are NOT retroactively renamed — only new releases going
  forward use the new naming.
- Published git history (old commits mentioning "dynamic-wallpaper" in their messages)
  is not rewritten; rewriting already-pushed history is out of scope and would break
  every existing clone/fork.
- None of these crates are currently published to crates.io, so no crates.io name
  reservation/transfer concern applies.
