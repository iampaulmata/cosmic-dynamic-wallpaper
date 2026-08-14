# Data Model: GUI Usability Improvements

No `cosmic-config` schema changes anywhere in this spec — every entity below is either transient
GUI-only state (never persisted) or a pure display-derivation function computed from data already
on disk. Constitution Principle X (versioned schema + migration path) doesn't apply; there is
nothing to migrate.

## Pack display resolution (`crates/wallpaper-settings/src/pack_display.rs`, new)

Two pure functions, not stored entities — the single source `pages/packs.rs` and
`pages/assignment.rs` both call (research.md R2, R7). Colocated in one module since both derive
from the same `pack_loader::load_pack` call on the same `PackSource`.

```text
fn resolve_pack_name(source: &PackSource) -> Option<String> {
    let loaded = pack_loader::load_pack(source.path()).ok()?;
    match source {
        PackSource::Directory(_) => Some(loaded.name),       // the manifest's `name` field
        PackSource::StaticFile(_) => {
            Path::new(&loaded.name).file_stem()               // strip the extension
                .map(|s| s.to_string_lossy().into_owned())
        }
    }
}

fn resolve_thumbnail_path(source: &PackSource) -> Option<PathBuf> {
    let loaded = pack_loader::load_pack(source.path()).ok()?;
    let chosen = loaded.pack.images().iter()
        .find(|img| matches!(img.anchor, TimeAnchor::Solar { event: SolarEventKind::SolarNoon, .. }))
        .or_else(|| loaded.pack.images().first())?;
    loaded.image_paths.get(&chosen.id).cloned()
}
```

| Function | Input | Output |
|---|---|---|
| `resolve_pack_name` | A directory pack with `manifest.toml`'s `name = "Mountains"` | `Some("Mountains")` |
| `resolve_pack_name` | A static image pack at `.../sunrise.png` | `Some("sunrise")` (extension stripped, per the `/speckit-clarify` session) |
| `resolve_pack_name` / `resolve_thumbnail_path` | A registered source whose `load_pack` fails (status already `Unavailable`, spec 2) | `None` — callers show the FR-011/FR-020 placeholder |
| `resolve_thumbnail_path` | A pack with an image anchored `Solar { event: SolarNoon, .. }` | That image's resolved path (FR-019) |
| `resolve_thumbnail_path` | A pack with no solar-noon anchor (including a single-image static pack, which is always `Clock`-anchored) | Its first image's resolved path, in manifest/declaration order (FR-019) |

**Validation rules**: None beyond what `load_pack` (spec 2, unchanged) already enforces — both
functions add no new failure mode, only a display-formatting/selection step on an
already-successful load.

**Relationships**: `resolve_pack_name` is called from `pages::packs::rows_from_registry`
(replacing its current `entry.source.path().display().to_string()` name) and from
`pages::assignment::view` (replacing `source.path().display().to_string()`, and supplying every
dropdown option's label — data-model.md's Assignment page section, research.md R6) —
FR-010/FR-011/FR-012's single implementation. `resolve_thumbnail_path` is called only from
`pages::packs::rows_from_registry` (FR-018/FR-019/FR-020).

## Packs page state (extends `crates/wallpaper-settings/src/pages/packs.rs`)

| Field | Type | Notes |
|---|---|---|
| `rows` | `Vec<PackRow>` | Existing — `PackRow.name` now comes from `resolve_pack_name`, falling back to a placeholder string (`"(unnamed pack)"`) instead of a path. `PackRow` gains a `thumbnail: Option<PathBuf>` field from `resolve_thumbnail_path` (FR-018); `None` renders the FR-020 placeholder. |
| `pending_removal` | `Option<PackSource>` | NEW. Set when the user requests removing a pack; drives `App::dialog()`'s confirmation overlay (research.md R3). `None` = no dialog shown. |
| `add_error` | `Option<String>` | NEW. Set when a file-chooser add attempt fails (cancelled picker is not an error — only a load/registration failure is, FR-003's "clear, specific error"). Cleared on the next add attempt or successful add. |

### New messages

| Message | Trigger | Effect |
|---|---|---|
| `AddFolderRequested` | "Add pack folder…" button | `app.rs` issues a `cosmic::Task` running `file_chooser::open::Dialog::new().open_folder()` |
| `AddFileRequested` | "Add image file…" button | Same, via `.open_file()` |
| `AddResult(Result<PathBuf, String>)` | The file-chooser task resolves (or the user cancels, mapped to a no-op, not an error) | `Ok(path)`: `Registry::register(PackSource::resolve(path)?)`, refresh `rows`, clear `add_error`. `Err(reason)`: set `add_error`, `rows` unchanged (FR-003: no partial registration on failure). |
| `RemoveRequested(PackSource)` | A row's "Remove" button | Sets `pending_removal = Some(source)` — no removal yet, just opens the confirmation dialog (FR-002, research.md R3) |
| `RemoveConfirmed` | The confirmation dialog's primary action | `Registry::remove(&source)`, refresh `rows`, `pending_removal = None` |
| `RemoveCancelled` | The confirmation dialog's secondary action, or dismissing it | `pending_removal = None`, no registry change |

**State transitions** (removal only — addition has no multi-step state machine beyond the
in-flight `Task`):

```text
Idle --RemoveRequested(src)--> ConfirmingRemoval(src)
ConfirmingRemoval(src) --RemoveConfirmed--> Idle   (registry.remove(src) applied)
ConfirmingRemoval(src) --RemoveCancelled--> Idle   (no change)
```

## Assignment page (extends `crates/wallpaper-settings/src/pages/assignment.rs`)

No new `State` fields (research.md R6) — the toggle's checked state and each dropdown's selected
index are both derived at render time from `current_config`, the field that's already there.
`apply_assignment` (spec 7) is reused unchanged for the actual pack-selection write; one new pure
helper is added for the toggle's on/off transition:

```text
fn set_same_everywhere_enabled(config: &mut RendererConfig, enabled: bool, default_pack: Option<PackSource>) {
    if enabled {
        config.overrides.clear();                       // FR-015 — the 2026-08-14 clarification
        if config.same_pack_everywhere.is_none() {
            config.same_pack_everywhere = default_pack;   // pre-select rather than "on" with nothing chosen
        }
    } else {
        config.same_pack_everywhere = None;
    }
}
```

### New messages

| Message | Trigger | Effect |
|---|---|---|
| `ToggleSameEverywhere(bool)` | The toggle switch | `set_same_everywhere_enabled(&mut config, enabled, available_packs.first().cloned())` (FR-014, FR-015) |
| `SameEverywherePackSelected(usize)` | The single dropdown, shown when the toggle is on | `apply_assignment(&mut config, &AssignTarget::SameEverywhere, available_packs[index].clone())` (FR-013) |
| `OutputPackSelected(String, usize)` | A per-display dropdown, shown when the toggle is off | `apply_assignment(&mut config, &AssignTarget::Output(output_id), available_packs[index].clone())` (FR-013) |

### `view()` changes

- **Toggle on** (`current_config.same_pack_everywhere.is_some()`): one `widget::toggler` (checked)
  plus one `widget::dropdown` whose `selected` index is `available_packs.iter().position(|p|
  Some(p) == current_config.same_pack_everywhere.as_ref())`, options labeled via
  `resolve_pack_name` (FR-013, FR-014).
- **Toggle off**: the toggler (unchecked) plus one `widget::dropdown` per entry in
  `known_outputs`, each `selected` index computed the same way against
  `current_config.overrides.get(output)` (FR-013, FR-014).
- **No packs registered** (`available_packs.is_empty()`): dropdown(s) replaced with a message
  directing the user to register a pack first (FR-016), matching the Packs page's own
  already-established empty-state posture.
- Every place that previously rendered `source.path().display()` / `current.path().display()`
  is gone — dropdown option labels and the selected value's own closed-state display are both
  `resolve_pack_name` results, satisfying FR-010/FR-011 (User Story 4) as a side effect of User
  Story 5's own construction, not a separate code path (spec.md Assumptions).

## Location page state (extends `crates/wallpaper-settings/src/pages/location.rs`)

| Field | Type | Notes |
|---|---|---|
| `entry`, `latitude_input`, `longitude_input` | *(unchanged)* | |
| `show_ip_disclosure` | `bool` | NEW. Toggled by the info-icon button (research.md R4, FR-008); independent of `entry.mode`, and independent of hover state — the tooltip (FR-007) shows/hides itself via `widget::tooltip`'s own built-in hover behavior and needs no state field. |

### New message

| Message | Trigger | Effect |
|---|---|---|
| `ToggleIpDisclosure` | The info icon next to the IP-geolocation option | `show_ip_disclosure = !show_ip_disclosure` |

**Behavior change**: the disclosure (hover tooltip and info-icon-revealed text) is now shown
whenever the IP-geolocation option is present, not gated on `entry.mode == IpGeolocation` — this
is US3's entire point (discoverable *before* opting in, FR-007).

## `IP_GEOLOCATION_DISCLOSURE` (moves to `crates/wallpaper-ipc`)

| Before | After |
|---|---|
| Duplicated `pub const` in `crates/wallpaperctl/src/commands/location.rs` and `crates/wallpaper-settings/src/pages/location.rs` (identical text, no shared definition) | One `pub const IP_GEOLOCATION_DISCLOSURE: &str` in `crates/wallpaper-ipc/src/lib.rs` (or a new `disclosures.rs`), imported by both — research.md R4 |

**New text** (sentence case, FR-009; meaning unchanged per spec.md's Assumptions):

> IP-geolocation uses a bundled offline database for the location lookup, and briefly asks a STUN
> server for this machine's public IP address first, since that's not something the bundled
> database can determine on its own.

**Downstream adjustment**: `wallpaperctl`'s `location ip` command message (`commands/
location.rs:156`) is reworded from `"IP-geolocation enabled ({IP_GEOLOCATION_DISCLOSURE}) —
resolving…"` to avoid repeating "IP-geolocation" now that the constant itself leads with it —
e.g. `"Enabled — {IP_GEOLOCATION_DISCLOSURE} Resolving…"`. Cosmetic only; no change to
`wallpaperctl`'s exit codes, JSON shape, or any other contract.

## Confirmation dialog (transient, `App::dialog()`)

Not a data entity — a render-time decision. `App::dialog(&self) -> Option<Element<'_, Message>>`
returns `Some(widget::dialog::dialog()...)` exactly when `self.packs.pending_removal.is_some()`,
titled with the pack's resolved name (`resolve_pack_name`, falling back to the FR-011 placeholder
so the confirmation text itself never shows a raw path either) — e.g. *"Remove Mountains? This
cannot be undone."*
