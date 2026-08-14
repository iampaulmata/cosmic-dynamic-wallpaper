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

## R6: Assigning packs from the GUI — toggle semantics and the overrides-clearing decision

**Decision**: `RendererConfig.same_pack_everywhere: Option<PackSource>` *is* the toggle's on/off
state already (`crates/wallpaper-ipc/src/renderer_config.rs`) — `None` means off, `Some(pack)`
means on with that pack chosen. No schema change needed; the GUI toggle (`widget::toggler`,
already available, no new dependency) reads/writes this field directly:

- **Toggle off → on**: if `same_pack_everywhere` is already `Some`, leave it; if `None`,
  pre-select a pack (the first registered one) rather than leaving the toggle visually "on" with
  nothing chosen (spec.md Edge Case). **Also clear `overrides` in the same write** — per the
  `/speckit-clarify`-style follow-up answer (2026-08-14): `resolve_assignment`'s existing,
  already-shipped precedence rule gives an explicit per-output override priority over the toggle
  (`crates/wallpaper-ipc/src/renderer_config.rs`'s own doc comment: "always takes precedence over
  the toggle") — left alone, a display with a stale override would silently keep showing its old
  individual pack even though the toggle now reads "on," breaking the toggle's own promise.
- **Toggle on → off**: set `same_pack_everywhere = None`. Any display without its own override
  then resolves to `Unassigned` (already a well-defined, non-error state,
  `renderer_config.rs`'s own doc comment, FR-009 in spec 3) until the user picks something from
  its now-visible individual dropdown.

**A real, deliberate divergence from already-shipped CLI behavior** (flagged per this project's
practice, plan.md Constitution Check): `wallpaperctl assign --same-everywhere` does **not** clear
`overrides` today (`crates/wallpaperctl/src/commands/assign.rs` — confirmed by reading its
implementation directly) and this spec does not change that command. Only the GUI's toggle
interaction clears overrides, because a toggle switch is a stronger UI promise ("this fully
controls behavior when on") than a CLI flag is — spec.md's Assumptions section records this
explicitly so it isn't mistaken for an oversight later.

**Selection widgets**: `widget::dropdown(&labels, selected_index, on_selected)` (already available,
`src/widget/dropdown/mod.rs`) for both the single "same everywhere" dropdown and each per-display
dropdown when the toggle is off. Labels are `resolve_pack_name` results (research.md R2) — so
FR-010/FR-011 (User Story 4) end up satisfied by construction once User Story 5 lands: a
dropdown's own closed-state display *is* the selected pack's resolved name. No separate read-only
label needs to be built or maintained (spec.md Assumptions).

**State design**: no new persisted or transient state needed beyond what `assignment::State`
already holds (`known_outputs`, `available_packs`, `current_config`) — the toggle's checked state
and each dropdown's selected index are both derived at render time directly from
`current_config`, the same "single source of truth, no duplicated cache" pattern this crate
already uses elsewhere (e.g. the Location page's mode radios read `entry.mode` directly).

**Alternatives considered**: Storing a separate `toggle_enabled: bool` in GUI-only state,
independent of `same_pack_everywhere`'s `Option`-ness — rejected as redundant: the `Option` already
*is* the boolean, and a separate flag risks the two disagreeing (e.g. toggle "on" in GUI state
with `same_pack_everywhere` still `None` after a config reload).

## R7: Packs page thumbnail

**Decision**: Extend the Foundational display-derivation module (research.md R2's
`resolve_pack_name`, now generalized to a `pack_display` module) with
`resolve_thumbnail_path(source: &PackSource) -> Option<PathBuf>`:

```text
fn resolve_thumbnail_path(source: &PackSource) -> Option<PathBuf> {
    let loaded = pack_loader::load_pack(source.path()).ok()?;
    let chosen = loaded.pack.images().iter()
        .find(|img| matches!(img.anchor, TimeAnchor::Solar { event: SolarEventKind::SolarNoon, .. }))
        .or_else(|| loaded.pack.images().first())?;
    loaded.image_paths.get(&chosen.id).cloned()
}
```

`ValidatedPack::images()` (`crates/schedule-engine/src/pack.rs`) preserves manifest declaration
order, so `.first()` is exactly "the first image in the pack" (spec.md FR-019). A single-image
static pack has no `Solar` anchor at all (it's `Clock`-anchored per `load.rs`'s degenerate case),
so the `find` naturally falls through to `.first()` — its one and only image — with no special
casing needed for that pack type.

**Rendering**: `widget::image(path)` (`pub use iced::widget::{Image, image}`,
`src/widget/mod.rs:69`) — already re-exported, no new dependency (the `image` crate itself is
already a workspace dependency, used by `pack-loader`). `PackRow` gains a `thumbnail:
Option<PathBuf>` field (`None` renders the FR-020 placeholder — a generic icon plus "(no preview
available)" text, reusing the placeholder posture `packs.rs` already has for a missing preview).

**Alternatives considered**: Rendering every image in the pack as a strip/carousel — rejected as
scope creep beyond what spec.md actually asks (a single representative thumbnail, chosen
deterministically). Caching decoded thumbnails across app restarts — rejected as premature; the
Packs page loads the full registry once already (`State::load`), and thumbnail decoding is a
one-time cost per session, not a hot path.

## Summary: no new Cargo dependencies

Every mechanism above (`cosmic::dialog::file_chooser`, `cosmic::widget::dialog`,
`cosmic::widget::tooltip`, `cosmic::widget::scrollable`, `cosmic::widget::button::icon`,
`cosmic::widget::toggler`, `cosmic::widget::dropdown`, `cosmic::widget::image`) already ships
inside the `libcosmic` git dependency this crate already pins, with the feature flags
(`xdg-portal`) already enabled in `crates/wallpaper-settings/Cargo.toml`. The only non-GUI-crate
change is moving one existing `pub const` from two crates into `wallpaper_ipc` (R4).
