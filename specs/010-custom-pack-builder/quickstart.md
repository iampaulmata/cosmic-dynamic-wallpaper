# Quickstart: Validating the Custom Pack Builder

Same split every GUI spec in this project uses: pure state/logic is headless-testable; the
actual rendered flow (folder picker, dialogs, spin buttons) needs a real COSMIC session.

## Prerequisites

- A stable Rust toolchain, same workspace as specs 1–9.
- A real COSMIC session for the manual checks below.
- A scratch folder of a handful (e.g. 4–8) of ordinary image files (PNG/JPEG are enough) with
  **no** `manifest.toml` in it — this is the folder the wizard is exercised against. Keep a copy
  or regenerate it between runs, since Generate writes into it.
- For the "already has a manifest" edge case: any existing registered pack directory (e.g.
  `assets/starter-pack/`, read-only — copy it somewhere writable first if you want to test that
  path without touching the repo).
- For the solar-mode duplicate-instant check's location-aware layer (research.md R4): a location
  already configured on the Location page (either mode); otherwise that specific layer is simply
  skipped, which is itself a state worth confirming once.

## Run the automated test suite

```sh
cargo test -p pack-loader
cargo test -p schedule-engine
cargo test -p wallpaper-settings
```

Expected coverage:

- `pack_loader::manifest::render`/`format_anchor` (data-model.md, contracts/
  pack-loader-manifest-writer.md): round-trips every constructible `TimeAnchor` through
  `format_anchor` → `parse_anchor`; an author name containing a `"` and non-ASCII text produces
  TOML that `pack_loader::manifest::parse` reads back identically; `author: None` omits the line
  entirely.
- `schedule_engine::WallpaperPack::validate`/`ValidatedPack::check_solar_duplicate_instant`: no
  new tests needed here — this feature adds no logic to this crate, only new *callers* of
  existing, already-covered functions (research.md R4).
- `pages::pack_builder` (new): `all_assigned`, `detect_conflict`, `build_draft`,
  `combine_offset`, `effective_author` (data-model.md) each get direct unit tests — e.g.
  `effective_author("")  == "Artist Unknown"`, `effective_author("  ") == "Artist Unknown"` (or
  documents why whitespace-only isn't special-cased, whichever the implementation lands on —
  pin the actual behavior in the test either way), `combine_offset(12, 30) == combine_offset(12,
  0)` (clamped), and the full state-machine transitions (`mode` choice → row assignment →
  Generate-blocked-while-unassigned → Generate-enabled → conflict-reintroduced-by-a-later-edit).

## Manual validation

1. **Launch**: `cargo run -p wallpaper-settings` (or the installed `cosmic-wallpaper-settings`).
2. **US1 — solar-period pack**:
   - Packs page → "Add pack folder…" → pick the manifest-free scratch folder.
   - Confirm the mode-choice screen appears (not a plain error) — FR-002, Acceptance Scenario 1.
   - Choose "By solar period." Confirm every image shows a thumbnail with an event dropdown
     showing **no** default selection, and Generate is disabled — FR-005, FR-009 (User Story 1
     Acceptance Scenario 6).
   - Assign each image a distinct solar event; confirm Generate becomes enabled once the last
     row is set.
   - Set a non-zero offset on one row (e.g. `sunset -30m`); confirm the control refuses to go
     past ±12h in either direction — FR-006 (clarification).
   - Assign two rows to the identical event with no offset; confirm Generate is blocked with a
     conflict message; fix one and confirm it re-enables — FR-008, Acceptance Scenario 5.
   - Leave the author field blank, click Generate. Confirm a `manifest.toml` now exists in the
     folder, and `wallpaperctl register <folder>` (or re-adding it via the Packs page) shows the
     author as "Artist Unknown" — FR-010, FR-012.
3. **US2 — specific-time pack**: repeat step 2 against a second scratch folder, choosing "By
   specific time" instead; confirm the per-row control is a time selector (no event dropdown, no
   offset control) and that two identical times block Generate the same way — FR-007, Acceptance
   Scenario 3.
4. **US3 — placement**:
   - After a successful Generate, confirm the move-vs-keep prompt appears.
   - Choose "Keep it here": confirm the folder didn't move, and the pack appears on the Packs
     page immediately with no separate "Add" step — FR-015, FR-016, SC-005.
   - Repeat Generate against another scratch folder and choose "Move": confirm the folder is no
     longer at its original path, now lives under the standard pack location, and appears on the
     Packs page — FR-013, FR-014.
   - Repeat once more with a folder whose *destination name* collides with a pack already moved
     there; confirm the app prompts for a different name rather than overwriting anything —
     FR-014a.
5. **Edge case — already-configured folder**: point "Add pack folder…" at a folder that already
   has a `manifest.toml`. Confirm the wizard does **not** appear and the folder registers exactly
   as it does today — FR-002, Edge Cases.
6. **Edge case — cancel**: open the wizard, make a few assignments, then cancel before Generate.
   Confirm the source folder has no new or modified files afterward (`git status`/`ls -la`
   timestamps, or a fresh `diff -r` against a saved copy) — FR-019, SC-006.
