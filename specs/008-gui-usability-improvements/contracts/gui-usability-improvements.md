# Contract: GUI usability improvements

Amends spec 7's `contracts/gui-application.md` — that contract's page-structure table and
daemon-optional posture are unchanged. This contract documents only what's new/different, one
user story at a time.

## US1 — Packs page gains add/remove (amends spec 7's "Registration is out of this spec's GUI
scope" note, which this spec supersedes)

| Action | Mechanism | Writes | Failure handling |
|---|---|---|---|
| Add a pack (folder) | "Add pack folder…" button → `cosmic::dialog::file_chooser::open::Dialog::new().open_folder()` | `pack_loader::Registry::register(PackSource::resolve(path)?)` — identical call `wallpaperctl register <dir>` makes | Cancelling the picker is a no-op, not an error. A path that fails `PackSource::resolve`/`register` shows the specific `ManifestError`/`RegistryError` message (FR-003); nothing is added. |
| Add a pack (single image) | "Add image file…" button → `.open_file()` | Same `Registry::register` call, static-file path | Same as above. |
| Remove a pack | Row's "Remove" button → confirmation dialog → confirm | `pack_loader::Registry::remove(&source)` — identical call `wallpaperctl remove <path>` makes, including the `Package`-origin → `RemovedStarterPacks` bookkeeping (spec 7 FR-010) | Cancelling the confirmation dialog is a no-op. An already-assigned pack's removal proceeds (spec.md Edge Cases) — the Assignment page's next refresh will show it via `resolve_pack_name` returning `None` → the FR-011 placeholder, same posture as any other unavailable source. |

Both add actions and the remove action call the exact same `pack_loader::Registry` methods
`wallpaperctl register`/`remove` already call (spec 4) — FR-003/FR-004's "behave identically to
the command-line tool" is enforced structurally (same function calls), not by convention, the
same posture spec 7's `wallpaper-ipc` extraction established for `RendererConfig`/
`LocationConfigEntry`.

## US2 — Every page is reachable

| Page | Change |
|---|---|
| Packs, Assignment, Location, Timeline, Crossfade | Each page's top-level `view()` output is wrapped in `widget::scrollable(...)` (research.md R5). No page's default-size appearance needs to change for this to satisfy FR-005/006 — scrolling activates only when content exceeds the available height, including after a user resizes the window smaller than the 900×700 default (Acceptance Scenario 3). |

No new messages; `widget::scrollable` manages its own internal scroll-offset state.

## US3 — IP-geolocation disclosure, before opt-in, on hover or tap

| Presentation | FR | Mechanism |
|---|---|---|
| Hover | FR-007 | `widget::tooltip(ip_geo_row, disclosure_text, Position::Bottom)` around the IP-geolocation radio row in `pages::location::view` |
| Non-hover (touch, or a mouse user who clicks instead of hovering) | FR-008 | A persistent `dialog-information-symbolic` icon button next to the row; toggles `location::State.show_ip_disclosure`, which reveals the identical `disclosure_text` inline when `true` |
| Wording | FR-009 | Both read the same `wallpaper_ipc::IP_GEOLOCATION_DISCLOSURE` constant (moved from two independently-duplicated copies, research.md R4) — one sentence-case, grammatically complete sentence |

Both presentations are available **before** the IP-geolocation mode is selected (`entry.mode`
never gates their visibility) — this is the change from spec 7's original post-selection-only
placement, per US3's own acceptance scenarios.

## US4 — Assignment page shows pack names, not paths

| Location in the page | Before | After |
|---|---|---|
| A per-output assignment row | `source.path().display()` | `resolve_pack_name(source).unwrap_or_else(\|\| "(unnamed pack)".into())` |
| The "same pack everywhere" toggle's active-pack label | `current.path().display()` | Same `resolve_pack_name` call |

`resolve_pack_name` (data-model.md) is the single implementation both this page and the Packs
page call — FR-010/FR-011/FR-012 by construction, not by two pages independently agreeing on the
same behavior.

## Explicitly out of scope for this contract

- A pack-picker *dropdown* replacing Assignment's existing "assigns the first registered pack"
  simplification (`pages/assignment.rs`'s own documented scope cut, spec 7) — untouched by this
  spec; FR-010/FR-011 only change *how a pack is displayed*, not which pack an action assigns.
- Editing a pack's manifest `name` from the GUI — out of scope; this spec only *reads* the
  existing name.
- Any change to `wallpaperctl`'s command surface or JSON output shape (beyond the disclosure
  string's wording, data-model.md) — the CLI's contract (spec 4) is otherwise untouched.
