# Contract Delta: `pack-loader` validation hardening

A **delta** against `specs/002-pack-format-loading/contracts/pack-loader-api.md`. The manifest
schema pack authors write is unchanged; `load_pack`'s signature is unchanged. Every change below
either rejects input that previously caused a panic or expensive unbounded work, or rejects input
that previously passed only by incidental containment-check ordering. A well-formed pack under the
existing schema loads identically to before.

## `load_pack(path: &Path) -> Result<LoadedPack, ManifestError>`

**New (US3/FR-011)**: if `path` is a directory, its `manifest.toml` is now rejected —
`ManifestError::ManifestTooLarge` — if it exceeds 512 KB, checked via a `stat` before the file is
read into memory. A real 64-anchor manifest is on the order of tens of KB; this cap leaves ~40x
headroom.

**Changed ordering, same outcome for valid input (US3/FR-010)**: the existing `MAX_ANCHORS` (64)
cap is now checked immediately after parsing, before any per-image filesystem work
(containment-check, canonicalize, header-read). A manifest declaring more than 64 images is
rejected exactly as before (same `ManifestError` variant, from `WallpaperPack::validate`), just
without first performing that many syscalls. **Observable difference**: rejection is now fast
regardless of declared image count; previously, rejection time scaled with the number of declared
(even if individually bogus) image entries.

**New (US5/FR-020)**: a manifest `[[images]]` entry whose `file` value is an absolute path is now
explicitly rejected with `ManifestError::PathEscapesPackDirectory` at the point the path is
resolved, rather than only incidentally caught by the existing containment `starts_with` check.
Same error variant and same practical outcome for every case that was already rejected; this
closes a gap where a future change to how pack directories are laid out could have silently
reopened an escape.

## `Color` (manifest schema: `fallback_color`, per-image color overrides if any)

**New (US1/FR-001)**: a hex color string containing non-ASCII bytes (e.g. `"#€AAA"`) now returns
`ManifestError::InvalidColor` instead of panicking the loading process. No change to what a valid
`#RRGGBB`/`#RRGGBBAA` string produces.

## `Registry`

**New (US6/FR-022)**: `Registry::persist()` (called internally by `register`/`remove`/
`reload_all`'s status-refresh) now acquires a cross-process advisory file lock for the duration of
the read-modify-write. **Observable difference**: two processes (e.g. `wallpaperctl register` and
the running daemon) writing at nearly the same moment now serialize rather than one silently
discarding the other's write; a lock-acquisition failure surfaces as a new
`RegistryError::LockFailed` rather than being possible to hit as an unsynchronized race today.

## Explicitly not in this contract

- Any change to the manifest TOML schema itself (`schema_version`, field names/types) — none of
  this feature's fixes bump the schema version (plan.md Constitution Check, Principle X: n/a).
- `image_check::check_readable`'s header-only readability check — unchanged; the new dimension/
  byte-ceiling check (US3/FR-012) lives in `renderer::texture`, not this crate, per the audit's own
  observation that this crate explicitly defers that limit downstream (and per research.md R9,
  confirmed still the right layer since only `renderer` decodes full pixel data).
- The pack-builder's write-side (`manifest::render`) — untouched by this feature except that
  `tools/generate-starter-pack` is changed to *use* it (US8/FR-041); the writer itself is
  unchanged.
