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

Satisfied entirely as a side effect of US5's dropdown construction (below), not a separate code
path — see US5's table. A dropdown's option labels and its selected-value display are both
`resolve_pack_name` results, so once US5 lands there is no remaining `source.path().display()` or
`current.path().display()` anywhere in `pages::assignment::view` for US4 to separately fix.

## US5 — Assign packs to displays from the GUI (amends spec 7's `pages/assignment.rs` "assigns the
first registered pack" simplification, which this spec supersedes)

| State | Control shown | Selecting an option writes |
|---|---|---|
| Toggle on (`same_pack_everywhere.is_some()`, default) | One `widget::toggler` (checked) + one `widget::dropdown` listing every registered pack by name | `RendererConfig.same_pack_everywhere` — identical field `wallpaperctl assign --same-everywhere` writes |
| Toggle off | The toggler (unchecked) + one `widget::dropdown` per connected display, each independently selectable | `RendererConfig.overrides[output_id]` — identical field `wallpaperctl assign --output <id>` writes |
| No packs registered | Dropdown(s) replaced with a message directing the user to register a pack first (FR-016) | Nothing — no dropdown to interact with |

**Toggle-on transition** (FR-015, `set_same_everywhere_enabled`, data-model.md): switching the
toggle from off to on clears `RendererConfig.overrides` in the same write, so every display shows
the toggle's chosen pack unconditionally — **a deliberate GUI-specific behavior**, not shared with
`wallpaperctl assign --same-everywhere`, which continues to leave `overrides` untouched exactly as
it does today (research.md R6, spec.md Assumptions). If `same_pack_everywhere` has no value yet
when the toggle is switched on, the first registered pack is pre-selected rather than leaving the
toggle on with nothing chosen.

**Toggle-off transition**: sets `same_pack_everywhere = None`. A display with no `overrides` entry
of its own then resolves to `Unassigned` (spec 3's already-defined, non-error state) until the
user picks something from its now-visible dropdown.

Every write goes through the exact same `RendererConfig` fields `wallpaperctl assign` already
writes (contracts/gui-application.md's FR-007 interchangeability promise, unchanged) — FR-013's
"behave like the CLI" is enforced structurally, the toggle-clears-overrides behavior being the one
explicitly-scoped GUI-only exception (called out above, not silently different).

## US6 — Packs page thumbnail

| FR | Behavior |
|---|---|
| FR-018 | `PackRow` gains a `thumbnail: Option<PathBuf>`, rendered via `widget::image(path)` instead of the existing path-as-text preview. |
| FR-019 | `resolve_thumbnail_path` (data-model.md, research.md R7) picks the pack's solar-noon-anchored image if one exists, else its first image in manifest order. |
| FR-020 | `None` (a failed `load_pack`, or an entry already `RegistryStatus::Unavailable`) renders the same placeholder posture the Packs page already uses for a missing preview today. |

## Explicitly out of scope for this contract

- Editing a pack's manifest `name` from the GUI — out of scope; this spec only *reads* the
  existing name.
- Any change to `wallpaperctl`'s command surface or JSON output shape (beyond the disclosure
  string's wording, data-model.md, and the fact that the GUI's toggle-on transition — unlike the
  CLI's `--same-everywhere` — also clears `overrides`, US5 above) — the CLI's contract (spec 4)
  is otherwise untouched.
- A thumbnail carousel/gallery showing every image in a pack — out of scope; US6 asks for one
  representative thumbnail, chosen deterministically, not a full preview gallery.
