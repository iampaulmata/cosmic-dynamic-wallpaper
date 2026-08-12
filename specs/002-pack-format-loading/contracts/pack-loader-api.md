# Contract: `pack-loader` public API

Library crate — the "contract" is the public Rust API surface spec 3 (Renderer) and spec 4
(CLI) build against, plus the on-disk manifest schema pack *authors* write to.

## Loading

```text
load_pack(path: &Path) -> Result<LoadedPack, ManifestError>
```

- If `path` is a directory: look for a manifest file, parse it (R1), resolve and
  containment-check every image path (R3), header-validate each image is readable (R2),
  and hand the resolved `(id, TimeAnchor)` list to spec 1's
  `WallpaperPack::validate` (contracts/schedule-engine-api.md, spec 1) — FR-001–FR-003,
  FR-005, FR-006, FR-006a.
- If `path` is a single image file: produce the static, manifest-free `LoadedPack` (FR-004).
- Never panics; every failure mode in data-model.md's `ManifestError` is returned, not
  thrown (constitution Principle VIII).

## Manifest schema (what pack authors write)

```toml
schema_version = 1
name = "Example Pack"
author = "Jane Author <jane@example.com> — CC-BY-4.0"
default_scaling = "Fill"
fallback_color = "#000000"

[[images]]
file = "dawn.jpg"
anchor = "sunrise"

[[images]]
file = "noon.jpg"
anchor = "solar_noon"
scaling = "Fit"
```

This is the committed, documentation-facing shape (data-model.md `PackManifest`/
`ManifestImage`) — the schema pack authors target directly, distinct from the Rust-internal
`LoadedPack` type consumers of this crate see.

## Registry

```text
Registry::register(source: PackSource) -> Result<(), RegistryError>
Registry::remove(source: &PackSource) -> Result<(), RegistryError>
Registry::known_packs() -> Vec<PackRegistryEntry>
Registry::reload_all() -> Vec<(PackSource, Result<LoadedPack, ManifestError>)>
```

- `register` persists a new known pack location via `cosmic-config` (FR-010, research.md R4).
- `remove` deletes a registry entry outright (FR-012) — distinct from the automatic
  `Unavailable` marking `reload_all` produces when a source is no longer reachable (FR-011).
- `reload_all` attempts every known pack independently — one failing pack (marked
  `Unavailable`) MUST NOT prevent the others from loading (FR-011).

## Error surface

`ManifestError` and `RegistryError` (data-model.md) are `std::error::Error` + `Debug` +
`Display`, naming the specific file/field/value at fault, so the CLI spec can surface them
directly to a user without re-wrapping (same posture as spec 1's contract).

## Explicitly not in this contract

- Assigning a loaded pack to a specific output (spec 3, FR-17/FR-18).
- Rendering or decoding full image pixel data (spec 3) — this crate only header-validates
  readability (research.md R2).
- Anything solar/clock/time-anchor-correctness related — fully owned by spec 1; this crate
  only calls into it (FR-003).
