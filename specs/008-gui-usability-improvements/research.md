# Research: GUI Usability Improvements

Five decisions, each closing a concrete unknown from plan.md's Technical Context. Every one
reuses a mechanism already present in this workspace — no new Cargo dependency is added anywhere
in this spec.

## R1: Add-pack file/folder picker

**Decision**: Use `cosmic::dialog::file_chooser::open::Dialog` (already available — `libcosmic`
is pinned with the `xdg-portal` feature in `crates/wallpaper-settings/Cargo.toml` already, the
same git pin `crates/renderer` uses `ashpd` directly from for spec 6's location portal). No new
dependency.

**A real API constraint surfaced here**: `xdg-desktop-portal`'s `OpenFile` request is either a
file picker or a folder picker (a single `directory: bool` on the request) — it cannot offer a
mixed "pick a file or a folder" dialog in one call. Since a pack source is *either* a directory
(manifest pack) or a single image file (static pack) — `pack_loader::PackSource`'s own two
variants — the Packs page needs two explicit actions, not one: **"Add pack folder…"** (calls
`.open_folder()`) and **"Add image file…"** (calls `.open_file()`), each producing a
`cosmic::Task` that resolves to a path, handed to the same `Registry::register(PackSource::
resolve(path))` `wallpaperctl register` already uses (`crates/wallpaperctl/src/commands/
register.rs`) — identical validation/idempotency behavior (FR-003) by construction, not by
convention.

**Alternatives considered**: A single text-input field mirroring `wallpaperctl register <path>`
verbatim — rejected per the `/speckit-clarify` session (2026-08-14): the native picker is more
discoverable and was the user's explicit choice over typing a path.

## R2: Pack name resolution

**Decision**: Reuse `pack_loader::load_pack(path)`'s existing `LoadedPack.name` field rather than
reimplementing name derivation — it already does exactly what's needed: the manifest's `name` for
a directory pack, or the file name for a static pack (`crates/pack-loader/src/load.rs:101,141`).
A thin, GUI-local wrapper (`resolve_pack_name` in `wallpaper-settings`) calls `load_pack` and, for
the static-file case only, strips the extension via `Path::file_stem()` — matching the
`/speckit-clarify` session's specific wording ("filename without extension") without duplicating
`load_pack`'s own parsing/validation logic. If `load_pack` fails outright (the entry's status is
already `Unavailable` — spec 2's existing reachability tracking), the wrapper returns `None` and
both the Packs and Assignment pages show the FR-011 placeholder instead.

**Where it's used**: Both `pages/packs.rs` (replacing its current `row.name = path.display()`)
and `pages/assignment.rs` (replacing `source.path().display()`) call the same wrapper — single
source of truth, no risk of the two pages drifting on what "the pack's name" means.

**Alternatives considered**: Storing a resolved name in `PackRegistryEntry` at registration time
(rejected — the clarify session's Option C) — would need a `cosmic-config` schema change
(constitution Principle X migration) for a value that's cheap to recompute from data already on
disk, and would go stale if a pack's manifest `name` is edited after registration without the
registry being told.

## R3: Remove-pack confirmation dialog

**Decision**: `cosmic::widget::dialog::dialog()` (an existing libcosmic widget,
`src/widget/dialog.rs`) rendered via `cosmic::Application::dialog(&self) -> Option<Element<...>>`
— a trait method this app doesn't currently override (defaults to `None`). Overriding it to
return `Some(...)` exactly when `packs::State.pending_removal: Option<PackSource>` is set gives a
standard libcosmic modal overlay with a primary ("Remove") and secondary ("Cancel") action, no new
window or dependency.

**Alternatives considered**: An inline "are you sure" row replacing the pack's list entry —
rejected; a real modal is the standard COSMIC pattern for a destructive, non-undoable action
(constitution Principle IX) and was the user's explicit choice in `/speckit-clarify`.

## R4: IP-geolocation disclosure — hover and non-hover

**Decision**: Two complementary presentations of the same disclosure text, both gated on "not yet
selected" (moving the *whole* disclosure earlier than today's post-selection-only display, per
US3):

- **Hover** (FR-007): `widget::tooltip(radio_row, caption_text, Position::Bottom)` — an existing
  libcosmic/iced widget (`src/widget/mod.rs:335`) that shows/hides itself automatically on
  pointer hover; no manual show/hide state needed for the mouse case.
- **Non-hover** (FR-008): a small, always-visible `widget::button::icon(...)` info icon
  (`dialog-information-symbolic`, already bundled in `cosmic-icons`) next to the IP-geolocation
  row. Tapping/clicking it toggles a `bool` in `location::State` that reveals the identical
  caption text inline — independent of hover, so it also works for a mouse user who prefers to
  click rather than hover.

Both read the same disclosure constant, so FR-009's wording/capitalization fix applies to both
paths at once, by construction.

**A real drift risk found while researching this**: `IP_GEOLOCATION_DISCLOSURE` is currently
*two independently duplicated string literals* — one in `crates/wallpaperctl/src/commands/
location.rs`, one in `crates/wallpaper-settings/src/pages/location.rs` — not a shared constant,
despite a doc comment on the GUI's copy claiming they're kept in sync "so the two control surfaces
never say different things." This is exactly the class of bug `wallpaper-ipc` was created to
prevent (spec 7 research.md R2), and both copies need editing anyway for FR-009's casing fix.
**This plan moves the constant into `wallpaper_ipc`** (both crates already depend on it) so there
is one definition, not two that happen to currently match. Flagged here per this project's
practice of surfacing amendments to already-shipped code rather than silently folding them in.

**New sentence-case wording** (meaning unchanged per spec.md's Assumptions): *"IP-geolocation uses
a bundled offline database for the location lookup, and briefly asks a STUN server for this
machine's public IP address first, since that's not something the bundled database can determine
on its own."* The CLI's own call site (`location.rs:156`, `format!("IP-geolocation enabled
({IP_GEOLOCATION_DISCLOSURE}) — resolving…")`) is adjusted in the same pass to avoid the
resulting "IP-geolocation enabled (IP-geolocation uses…)" repetition — a small, directly-adjacent
tidy-up, not a scope expansion.

**Alternatives considered**: Keeping the disclosure only reachable after selecting IP-geolocation
(today's behavior) — this is precisely what US3 exists to fix, so not viable. A single mechanism
serving both hover and touch (e.g., tooltip-only) — rejected per the spec's own edge case
(FR-008): a pointer-only affordance is not discoverable on touch-only input at all, whereas an
icon works for both.

## R5: Making every control reachable (US2)

**Decision**: Wrap each page's top-level `view()` return value in `widget::scrollable(...)`
(`src/widget/scrollable.rs`, already re-exported, zero new dependency) rather than only enlarging
the default window size. Scrolling is the one fix that also satisfies Acceptance Scenario 3 (the
window resized *smaller* than default must still have every control reachable) — a taller default
size alone only helps the unmodified-window case. The default window size
(`cosmic::iced::Size::new(900.0, 700.0)`, `main.rs`) is left unchanged; scrolling makes the exact
default size a non-issue regardless.

**Alternatives considered**: Increasing the default window height (e.g. to 800px) — rejected as
the sole fix per the reasoning above, though it remains a trivially compatible *addition* if a
future pass wants both; this plan doesn't need it to satisfy FR-005/006/SC-003.

## Summary: no new Cargo dependencies

Every mechanism above (`cosmic::dialog::file_chooser`, `cosmic::widget::dialog`,
`cosmic::widget::tooltip`, `cosmic::widget::scrollable`, `cosmic::widget::button::icon`) already
ships inside the `libcosmic` git dependency this crate already pins, with the feature flags
(`xdg-portal`) already enabled in `crates/wallpaper-settings/Cargo.toml`. The only non-GUI-crate
change is moving one existing `pub const` from two crates into `wallpaper_ipc` (R4).
