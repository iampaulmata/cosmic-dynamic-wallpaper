# Changelog

All notable changes to Cosmic Dynamic Wallpaper are documented here.
Format loosely follows [Keep a Changelog](https://keepachangelog.com/).

## [0.2.2] - 2026-08-17

### Added

- **Edit existing packs.** Directory-based packs can now be edited after creation,
  not just created once and left alone. The Packs screen's row actions changed from
  a single "Remove" text button to a pencil (edit) and trash-can (delete) icon pair;
  clicking the pencil reopens the same wizard used to create a pack — the same
  interface, not a separate one — pre-filled with the pack's current folder
  assignments, scaling, fallback color, author, and name.
- **Negative solar offsets.** When mapping an image to a solar period (sunrise,
  sunset, solar noon, etc.), you can now give it a negative hour/minute offset so
  the change happens *before* that solar event, not just at or after it. This works
  identically whether you're creating a new pack or editing an existing one.
- **Same-time conflict detection.** Assigning two images to the same effective
  display time is now caught and blocked, with the conflict surfaced inline, in both
  the add-folder and edit-pack flows — previously this check only ran in one of the
  two paths.
- **Pack and image naming.** Packs can now be given a display name from the wizard,
  and standalone single-image packs can be renamed from the Packs screen via a
  lightweight rename prompt (pencil icon → text field → Save/Cancel). This is
  display-only: it changes what's shown in the app, never the underlying file or
  folder name on disk.

### Fixed

- The new edit icon briefly rendered as a generic document glyph instead of a
  pencil, because the requested icon name wasn't in libcosmic's bundled icon set.
  Caught via live testing and switched to the icon that's actually bundled.
- A pack could previously be regenerated with a stale/bypassed scheduling conflict
  still in effect if the conflict state wasn't re-checked at save time; the save
  path now re-validates for conflicts immediately before writing, in both add and
  edit flows.

### Notes

- Upstream version jumps from 0.2.0 straight to 0.2.2. `0.2.1` was tagged against a
  version-bump commit that was never actually merged into `main` (an abandoned
  branch for the app-icon work) and was never released, so it's skipped here to
  avoid reusing that tag.
- Debian package revision resets to `1` for this new upstream version:
  `cosmic-dynamic-wallpaper_0.2.2-1_amd64.deb`.

## [0.2.0] - 2026-08-16

Initial 0.2.x line. See git history for details predating this changelog.
