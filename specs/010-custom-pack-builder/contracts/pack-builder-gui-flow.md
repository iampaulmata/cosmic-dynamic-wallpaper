# Contract: Custom Pack Builder GUI flow

Amends `specs/008-gui-usability-improvements/contracts/gui-usability-improvements.md`'s US1 row
("Add a pack (folder)") — that mechanism is unchanged for a folder that already has a manifest.
This documents only the new branch and everything after it.

## Entry

| Action | Mechanism | Branch |
|---|---|---|
| "Add pack folder…" → pick a folder | Existing `cosmic::dialog::file_chooser::open::Dialog::new().open_folder()` (unchanged, research.md R1) | `PackSource::resolve` + `pack_loader::load_pack` fails with `ManifestError::ManifestNotFound` |
| → | | Opens `pages::pack_builder::State` at that path (`App.pack_builder = Some(...)`, research.md R9); every other `load_pack` failure keeps today's behavior (`add_error` shown, wizard not opened) |

## Mode choice (FR-004)

Two buttons, "By solar period" / "By specific time." Selecting one sets `State.mode` and scans
the folder (research.md R2) into `State.rows`, every row's assignment `None` (research.md R5).
A folder yielding zero usable images, or more than `schedule_engine::pack::MAX_ANCHORS`, sets
`State.scan_error` instead and shows FR-018's message with no rows and no Generate button.

## Configuration screen (FR-005–FR-009)

| Mode | Per-row control | Writes |
|---|---|---|
| Solar period | `widget::dropdown` of the 8 `SolarEventKind` labels, `selected: Option<usize>` | `row.solar.event` |
| Solar period | Two `widget::spin_button<i32/u32>` (signed hours, minutes) beside the dropdown | `row.solar.offset_hours`/`offset_minutes` (research.md R6, clamped ±12h) |
| Specific time | Two `widget::spin_button<u32>` (hour, minute) | `row.time` |

An author `widget::text_input` (FR-010) and the Generate button sit below the row list.
Generate's `on_press` is present only when `all_assigned(&rows, mode)` is `true` **and**
`State.conflict` is `None` (data-model.md); otherwise the button renders disabled with
`State.conflict`'s message shown beneath it, if any (FR-008, FR-009). `detect_conflict`
(data-model.md) re-runs after every row edit — no separate "check" action.

Switching `State.mode` after some rows are already touched clears every row's `solar`/`time`
field back to `None` (Edge Case) — the row list and thumbnails themselves are not re-scanned.

## Generate (FR-011, FR-012, FR-020)

```text
1. draft = build_draft(rows, mode, source_dir.file_name(), author)   // R10 defaults applied here
2. text  = pack_loader::manifest::render(&draft)
3. write text to source_dir/manifest.toml
4. pack_loader::load_pack(&source_dir)
     Err → delete the manifest.toml just written, show the specific error inline, stay on this
           screen with rows/author untouched (FR-017)
     Ok  → State.pending_placement = Some(GeneratedPlacement { generated_path: source_dir })
```

## Placement prompt (FR-013–FR-016, FR-014a)

A `cosmic::widget::dialog::dialog()` modal (the same mechanism spec 008 research.md R3 uses for
pack removal), shown whenever `pending_placement.is_some() || pending_collision.is_some()`:

| State | Dialog shown | Primary action | Secondary action |
|---|---|---|---|
| `pending_placement: Some(_)` | "Move this pack to the standard pack location?" | Move | Keep it here |
| `pending_collision: Some(_)` | "A pack named '{suggested_name}' already exists there — choose a different name." + text input pre-filled with `suggested_name` | Move (retry with the typed name) | Cancel move (falls back to "keep it here" — the folder already has a valid manifest either way) |

**Move** (research.md R8): recursively copy `generated_path` to
`dirs::data_dir()/cosmic-dynamic-wallpaper/packs/<name>` (or the collision-prompt's typed name),
call `pack_loader::load_pack` on the copy to confirm it, then remove `generated_path`. Any
failure at any step leaves `generated_path` untouched and shows a specific error (FR-017);
partial destination copies are cleaned up before reporting the error. A same-name folder already
present at the destination is the *only* condition that opens `pending_collision` instead of
completing the move outright.

**Keep it here**: no filesystem change beyond the manifest already written in Generate.

**Either outcome**: `PackSource::resolve(final_path)` + `Registry::register(...)` — the identical
call the existing "Add pack folder…" success path already makes (research.md R1) — then
`App.pack_builder = None`, returning to whichever nav page was active before the wizard opened,
with `pages::packs::State` refreshed so the new pack appears immediately (FR-016, SC-005).

## Cancel (FR-019)

Available from the mode-choice screen and the configuration screen (not once
`pending_placement`/`pending_collision` is `Some` — Generate has already written a real file by
that point, so "cancel" no longer means "nothing happened," only move-vs-keep does). Sets
`App.pack_builder = None` with **no** filesystem or registry change (SC-006).
