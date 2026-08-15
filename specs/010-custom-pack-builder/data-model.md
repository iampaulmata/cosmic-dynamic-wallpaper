# Data Model: Custom Pack Builder

No `cosmic-config` schema changes anywhere in this spec (constitution Principle X doesn't
apply — nothing persisted gains or loses a field). Two kinds of new shapes: (1) a small,
symmetric write-side addition to `pack-loader`'s existing manifest module, and (2) transient,
never-persisted GUI state in `wallpaper-settings`, in the same style as `pages::packs::State`/
`pages::assignment::State`.

## 1. `pack-loader` additions (`crates/pack-loader/src/manifest.rs`)

### `ManifestDraft` (new)

The write-side counterpart to the existing (read-only) `PackManifest` — what a caller hands in
to produce `manifest.toml` text, rather than what parsing produces.

```text
pub struct ManifestDraft {
    pub name: String,
    pub author: Option<String>,          // None → omit the `author` line entirely
    pub default_scaling: ScalingMode,    // reused as-is from the read side
    pub fallback_color: Color,           // reused as-is from the read side
    pub images: Vec<ManifestDraftImage>,
}

pub struct ManifestDraftImage {
    pub file: String,                    // file name, relative to the pack directory
    pub anchor: schedule_engine::TimeAnchor,
    // no `scaling` field — the wizard never sets a per-image override (R10)
}
```

**Validation rules** (enforced by the wizard *before* constructing a `ManifestDraft`, reusing
`schedule_engine::WallpaperPack::validate` — research.md R4 — rather than duplicated here):

1. `images` is non-empty and at most `schedule_engine::pack::MAX_ANCHORS` (64) long.
2. Every `anchor` is the same kind (`TimeAnchor::is_solar()`/`is_clock()`) across all images.
3. No two images share a `file`.
4. No two images resolve to the same instant (FR-008; research.md R4's two layers for solar
   mode, `PackError::DuplicateInstant` directly for clock mode).

### `render(draft: &ManifestDraft) -> String` (new)

Pure function producing `manifest.toml` text via `toml`'s `Serialize` support (research.md R3).
Always writes `schema_version = 1`. Omits the `author` key when `draft.author` is `None`
(matches the format's own "no default" documented behavior — the wizard itself always supplies
`Some("Artist Unknown")` when the user leaves the field blank, per FR-010, so `None` here is a
theoretical/library-level case, not one the wizard's own UI ever produces).

### `format_anchor(anchor: &TimeAnchor) -> String` (new)

The exact inverse of the existing private `parse_anchor` (same module) — `TimeAnchor::Solar {
event, offset }` → `"sunset-30m"`/`"sunrise"`-style strings, `TimeAnchor::Clock(t)` →
`"HH:MM"`. Round-trip property (`parse_anchor(format_anchor(a)) == a` for every constructible
`a`) is the unit-test contract this function ships with.

## 2. Wizard state (`crates/wallpaper-settings/src/pages/pack_builder.rs`, new)

### `AssignmentMode`

```text
enum AssignmentMode { SolarPeriod, SpecificTime }
```

Chosen once, up front (FR-004); switching later discards every row's current assignment
(Edge Case) since a pack cannot mix anchor kinds.

### `ImageRow`

One scanned image (research.md R2) plus its current, possibly-still-empty assignment.

```text
struct ImageRow {
    file_name: String,           // relative to the source folder; also the manifest `file` value
    thumbnail_path: PathBuf,     // absolute path, handed to widget::image(...) directly
    solar: Option<SolarAssignment>,   // Some only in SolarPeriod mode
    time: Option<NaiveTime>,          // Some only in SpecificTime mode
}

struct SolarAssignment {
    event: schedule_engine::SolarEventKind,
    offset_hours: i32,     // -12..=12 (research.md R6)
    offset_minutes: u32,   // 0..=59, clamped to 0 when |offset_hours| == 12
}
```

Exactly one of `solar`/`time` is meaningful at a time, gated by `AssignmentMode` — kept as two
`Option` fields rather than an enum-of-two-variants so switching modes is "clear the other
field," a one-line pure operation, not a row rebuild.

### `State`

```text
struct State {
    source_dir: PathBuf,
    mode: Option<AssignmentMode>,       // None until the up-front choice is made (FR-004)
    rows: Vec<ImageRow>,
    author: String,                     // free text; blank means "Artist Unknown" at generate time
    conflict: Option<String>,           // Some(message) when FR-008's check currently fails
    pending_collision: Option<PendingCollision>,  // Some while the destination-name prompt is open
    pending_placement: Option<GeneratedPlacement>, // Some between a successful Generate and the move/keep choice
    scan_error: Option<String>,         // FR-018: zero usable images / too many images
}

struct PendingCollision {
    generated_path: PathBuf,     // where the manifest was actually written (source_dir)
    suggested_name: String,      // pre-filled input, e.g. source folder's name
}

struct GeneratedPlacement {
    generated_path: PathBuf,     // == source_dir; the just-generated, self-validated pack
}
```

**State machine** (mirrors `packs::State`'s `pending_removal` shape):

| State | Meaning |
|---|---|
| `mode: None` | Mode-choice screen showing (FR-004) |
| `mode: Some(_)`, `pending_placement: None` | Configuration screen showing; rows editable; Generate enabled iff every row is assigned (research.md R5) and `conflict` is `None` |
| `pending_collision: Some(_)` | Generate succeeded at writing+self-validating the manifest, but the move-to-standard-location step hit a same-name folder; asking for a new destination name (FR-014a) |
| `pending_placement: Some(_)` | Generate succeeded; asking move-vs-keep (FR-013) |
| (all `None`, wizard closed) | Back to the previously active nav page (research.md R9) |

### Pure functions

```text
fn all_assigned(rows: &[ImageRow], mode: AssignmentMode) -> bool
fn detect_conflict(rows: &[ImageRow], mode: AssignmentMode, location: Option<Location>) -> Option<String>
fn build_draft(rows: &[ImageRow], mode: AssignmentMode, folder_name: &str, author: &str) -> ManifestDraft
fn combine_offset(hours: i32, minutes: u32) -> chrono::TimeDelta      // research.md R6, clamps
fn effective_author(input: &str) -> String                           // blank → "Artist Unknown" (FR-010)
```

All five are pure (no I/O), independent of `libcosmic` rendering, and unit-tested directly —
matching the existing `apply_assignment`/`rows_from_registry`-style split between pure logic and
`view()` in every other page in this crate.

## Relationships

```text
ImageRow*  ──assembled by──▶  build_draft  ──▶  ManifestDraft  ──render()──▶  manifest.toml text
                                                                         │
                                                            pack_loader::load_pack (self-validate)
                                                                         │
                                                              PackSource::resolve + Registry::register
                                                                         (R1 — identical to "Add pack folder…")
```
