# Data Model: Pack Format & Loading

All types below are owned by this spec's `pack-loader` crate. Time-anchor and pack-validation
types (`TimeAnchor`, `SolarEventKind`, `WallpaperPack::validate`, `PackError`) are **not**
redefined here — they're consumed from spec 1's `schedule-engine` crate per FR-003, so this
loader never re-implements anchor-correctness rules.

## PackManifest (on-disk TOML shape)

The `#[derive(Deserialize)]` shape read directly from a manifest file, before any validation.

| Field | Type | Notes |
|---|---|---|
| `schema_version` | `u32` | Checked against supported versions at parse time (FR-007, research.md R5) |
| `name` | `String` | Pack display name (FR-001) |
| `author` | `Option<String>` | License/author note (FR-001) |
| `default_scaling` | `ScalingMode` | Pack-level default (FR-005) |
| `fallback_color` | `Color` | For letterboxed edges under Fit/Center (FR-005) |
| `images` | `Vec<ManifestImage>` | Ordered; becomes the pack's image list |

## ManifestImage (on-disk TOML shape, one per entry)

| Field | Type | Notes |
|---|---|---|
| `file` | `String` (relative path) | Resolved against the pack directory; MUST stay inside it (FR-006a) |
| `anchor` | spec 1's `TimeAnchor` shape | Solar or clock — reused verbatim, not redefined here |
| `scaling` | `Option<ScalingMode>` | Per-image override of the pack default (FR-005) |

## ScalingMode

Enum: `Fill`, `Fit`, `Stretch`, `Center` (FR-005) — matches `cosmic-bg`'s existing scaling
vocabulary per spec.md's User Story 3.

## Color

An RGB(A) fallback fill color for letterboxed edges (FR-005). Validated as a well-formed
color value at load time (FR-006) — exact wire representation (hex string vs. struct) is an
implementation detail, not a product decision.

## LoadedPack (validated, in-memory form)

The output of a successful load — what specs 1 (scheduling) and 3 (renderer) actually consume.

| Field | Type | Notes |
|---|---|---|
| `source` | `PackSource` | Directory path (manifest pack) or single file path (static pack) — the identity key (FR-009) |
| `name` | `String` | From the manifest, or derived from the filename for a static pack |
| `author` | `Option<String>` | |
| `default_scaling` | `ScalingMode` | |
| `fallback_color` | `Color` | |
| `pack` | spec 1's validated `WallpaperPack` | Built by handing every resolved `(image id, TimeAnchor)` pair to `WallpaperPack::validate` (FR-003) |
| `image_paths` | `Map<image id, PathBuf>` | Resolved, containment-checked absolute paths (FR-006a) — the loader's own bookkeeping, not part of spec 1's contract |
| `image_scaling` | `Map<image id, ScalingMode>` | Resolved per-image scaling (override or pack default applied, FR-005) |

## PackSource

A tagged union — exactly one of:

- `Directory(PathBuf)` — a manifest-based pack; identity is the canonicalized directory path.
- `StaticFile(PathBuf)` — a static, manifest-free pack (FR-004); identity is the
  canonicalized file path.

## StaticWallpaper

Not a distinct type — represented as a `LoadedPack` whose `pack` field is spec 1's
single-image degenerate `WallpaperPack` (spec 1's data-model.md already defines this case:
one always-active image, no transitions). `source` is `PackSource::StaticFile`.

## PackRegistryEntry (persisted via `cosmic-config`, FR-010)

| Field | Type | Notes |
|---|---|---|
| `source` | `PackSource` | The identity key (FR-009); also what's re-loaded on daemon restart |
| `status` | `RegistryStatus` | `Known` or `Unavailable` (FR-011) — see state notes below |

**State notes** (resolves the FR-011 vs. FR-012 distinction from `/speckit-clarify`):

- A registry entry starts and normally stays `Known`.
- It becomes `Unavailable` automatically when a reload attempt finds its `source` missing,
  moved, or unreadable (FR-011) — the entry is *retained*, just flagged, so the user can see
  what's missing rather than have it silently vanish.
- It is *deleted outright* (not just flagged) only via explicit user removal (FR-012) — this
  is a distinct registry operation, `RegistryEntry::remove(source)`, not a transition of
  `RegistryStatus`.

## Error types

- `ManifestError` — `ParseFailure`, `UnsupportedSchemaVersion`, `MissingImageFile`,
  `PathEscapesPackDirectory` (FR-006a), `UnreadableImage`, `InvalidScalingMode`,
  `InvalidColor` (FR-006).
- `RegistryError` — wraps `cosmic-config` I/O failures; contained per constitution
  Principle VIII, never panics.

All error types implement `std::error::Error`, `Debug`, and `Display`, and name the specific
file/field/value at fault — per constitution Principle VIII, no `unwrap()`/`expect()` outside
`#[cfg(test)]` code on any of these paths.
