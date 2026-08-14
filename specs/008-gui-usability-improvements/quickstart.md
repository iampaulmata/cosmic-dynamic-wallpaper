# Quickstart: Validating GUI Usability Improvements

Same split this project's GUI work already established (spec 7's own quickstart.md): pure
display/state logic is headless-testable; the actual rendered behavior (dialogs, tooltips,
scrolling) needs a real COSMIC session.

## Prerequisites

- A stable Rust toolchain, same workspace as specs 1–7.
- A real COSMIC session for the manual checks below (any COSMIC app's baseline requirement, same
  as spec 7's own GUI quickstart).
- At least one registered pack (directory-based, with a `manifest.toml` `name`) and, ideally, one
  static single-image pack, to exercise both branches of `resolve_pack_name` (data-model.md).

## Run the automated test suite

```sh
cargo test -p wallpaper-settings
cargo test -p wallpaper-ipc
cargo test -p wallpaperctl
```

Expected coverage:

- `resolve_pack_name` (data-model.md): a directory pack returns its manifest `name`; a static
  pack returns its filename *without* extension; an unreadable/`Unavailable` source returns
  `None`.
- `pages::packs`: `AddResult(Err(_))` leaves `rows` unchanged and sets `add_error`;
  `AddResult(Ok(path))` registers and refreshes `rows`; `RemoveRequested`/`RemoveConfirmed`/
  `RemoveCancelled`'s state transitions (data-model.md) match the documented state machine
  exactly, without needing a real file-chooser round trip (the `Task` itself is a thin wrapper
  around `cosmic::dialog::file_chooser`, exercised manually below — the *dispatch* logic around
  it is what's unit-tested).
- `pages::assignment`: given a registered pack with a known name, `view`'s row (indirectly, via
  the same `resolve_pack_name` unit tests) never renders a path-shaped string.
- `pages::location`: `ToggleIpDisclosure` flips `show_ip_disclosure`; the disclosure text is
  shown regardless of `entry.mode`.
- `wallpaper_ipc::IP_GEOLOCATION_DISCLOSURE`: capitalized, ends with terminal punctuation, and
  `wallpaperctl`'s and `wallpaper-settings`' own uses both resolve to the same string (a
  same-crate-constant equality check, closing the drift risk research.md R4 found).

## Manual smoke check 1: add and remove a pack (US1)

```sh
cargo run -p wallpaper-settings
```

1. Open the Packs page with zero packs registered. Click "Add pack folder…", browse to a
   directory containing a valid `manifest.toml`, confirm. **Expected**: the pack appears
   immediately, named per its manifest, with no terminal command run.
2. Click "Add image file…", pick a single image. **Expected**: it appears too, named by its
   filename without extension.
3. Re-add the same folder pack again. **Expected**: no duplicate row (FR-003's idempotency).
4. Point the folder picker at a directory with a malformed/missing manifest. **Expected**: a
   specific error is shown (not a generic failure), and nothing is added.
5. Click "Remove" on a pack. **Expected**: a confirmation dialog appears, naming the pack (not
   its path). Cancel it — the pack remains. Remove it again and confirm — it disappears
   immediately and is no longer offered on the Assignment page.

## Manual smoke check 2: every control reachable (US2)

1. Open the application at its default size. Navigate to every page in turn (Packs, Assignment,
   Location, Timeline, Crossfade). **Expected**: the "Set manual location" button and every other
   control is either fully visible or reachable by scrolling.
2. Resize the window smaller than the default. **Expected**: scrolling still reaches every
   control on every page — nothing becomes permanently unreachable.

## Manual smoke check 3: IP-geolocation disclosure (US3)

1. On the Location page, without selecting IP-geolocation, hover the mouse over its row.
   **Expected**: a tooltip appears near the pointer, reading as a properly capitalized sentence,
   and disappears when the pointer moves away.
2. Click the info icon next to the same row (simulating a touch/no-hover interaction). **Expected**:
   the same disclosure text appears inline, without needing to hover or select the mode first.

## Manual smoke check 4: Assignment shows names (US4)

1. Register a pack with a distinctive manifest `name` (e.g. "Mountains"). Assign it to an output
   or enable "same pack everywhere." **Expected**: the Assignment page shows "Mountains", not a
   file path, for that output/toggle.

## What "done" looks like for this spec

See spec.md's Success Criteria (SC-001–SC-005). `cargo test` closes the pure-logic half of every
user story (name resolution, message dispatch, disclosure-text consistency). The four manual
smoke checks above close the rendered/interactive half — dialogs, tooltips, and scrolling are not
practically unit-testable outside a real compositor, same posture this project's GUI work already
established in spec 7's own quickstart.md.
