# Data Model: GUI Usability Improvements

No `cosmic-config` schema changes anywhere in this spec — every entity below is either transient
GUI-only state (never persisted) or a pure display-derivation function computed from data already
on disk. Constitution Principle X (versioned schema + migration path) doesn't apply; there is
nothing to migrate.

## Pack name resolution (`crates/wallpaper-settings`, new)

A pure function, not a stored entity — the single source both `pages/packs.rs` and
`pages/assignment.rs` call (research.md R2):

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
```

| Input | Output |
|---|---|
| A directory pack with `manifest.toml`'s `name = "Mountains"` | `Some("Mountains")` |
| A static image pack at `.../sunrise.png` | `Some("sunrise")` (extension stripped, per the `/speckit-clarify` session) |
| A registered source whose `load_pack` fails (status already `Unavailable`, spec 2) | `None` — callers show the FR-011 placeholder |

**Validation rules**: None beyond what `load_pack` (spec 2, unchanged) already enforces — this
function adds no new failure mode, only a display-formatting step on an already-successful load.

**Relationships**: Called from `pages::packs::rows_from_registry` (replacing its current
`entry.source.path().display().to_string()` name) and from `pages::assignment::view` (replacing
`source.path().display().to_string()`) — FR-010/FR-011/FR-012's single implementation.

## Packs page state (extends `crates/wallpaper-settings/src/pages/packs.rs`)

| Field | Type | Notes |
|---|---|---|
| `rows` | `Vec<PackRow>` | Existing — `PackRow.name` now comes from `resolve_pack_name`, falling back to a placeholder string (`"(unnamed pack)"`) instead of a path. |
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

No new fields or messages — `apply_assignment`'s write shape is unchanged (FR's already covered
by spec 7). Only `view()` changes: every place it currently renders `source.path().display()` or
`current.path().display()` renders `resolve_pack_name(source).unwrap_or_else(|| "(unnamed
pack)".to_string())` instead (FR-010/FR-011).

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
