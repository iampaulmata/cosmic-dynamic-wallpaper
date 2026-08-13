# pack-loader

Turns a directory (manifest + images) or a single image file into a fully validated
`LoadedPack` — the second crate in the COSMIC dynamic-wallpaper daemon's workspace,
depending on [`schedule-engine`](../schedule-engine) (spec 1) for the time-anchor and
pack-validation contract every loaded pack must satisfy.

## Scope

- Load a multi-image, time-anchored pack from a directory containing a `manifest.toml`
  plus image files (FR-001–FR-003, FR-006, FR-006a, FR-008).
- Load a single image file with no manifest as a zero-config static pack — full parity
  with "just set a normal wallpaper" (FR-004).
- Resolve pack-level and per-image scaling/fit mode (Fill, Fit, Stretch, Center) plus a
  fallback fill color (FR-005).
- Reject, with a specific and actionable error, every malformed-input case: bad TOML,
  missing/unreadable image, invalid scaling mode or color, unsupported schema version,
  or a manifest entry that resolves outside the pack directory (FR-006, FR-006a).

## Explicitly not in scope

- **Persisting the set of known pack locations** (FR-010–FR-012, User Story 4). This is
  designed for (see `PackSource`, `data-model.md`'s `PackRegistryEntry`) but not yet
  implemented in this pass — it depends on `cosmic-config`, an unpublished git
  dependency on `pop-os/libcosmic`, deferred to keep this crate's build fast and
  reliable while the P1 (US1+US2) and P2 (US3) stories — the actual MVP-blocking scope —
  landed first. Tracked as the remaining work in
  `specs/002-pack-format-loading/tasks.md` Phase 6.
- **Assigning a loaded pack to a specific output, or rendering it** — spec 3.
- **Full pixel decode** — this crate only header-validates that an image is readable
  (`image::ImageReader::into_dimensions`); actual decode is the renderer's job.
- **Network-sourced packs** — local disk only (PRD Non-Goal NG2).

## Manifest schema

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

`anchor` accepts:
- A solar event name (`sunrise`, `sunset`, `solar_noon`, `solar_midnight`, `civil_dawn`,
  `civil_dusk`, `astronomical_dawn`, `astronomical_dusk`), optionally offset —
  `"civil_dawn-30m"`, `"sunset+1h"` (any [`humantime`](https://docs.rs/humantime)
  duration after the `+`/`-`).
- An absolute clock time, `"HH:MM"` or `"HH:MM:SS"`.

A pack's images must use one anchor kind consistently (spec 1's `WallpaperPack::validate`
rejects a mix). The loader looks for a manifest file literally named `manifest.toml`
inside the pack directory (`MANIFEST_FILE_NAME`) — not itself spec'd by name anywhere in
spec.md, an implementation choice.

## Testing

```sh
cargo test --package pack-loader
cargo llvm-cov --package pack-loader --summary-only
```

`tests/load_pack.rs` runs spec.md's acceptance scenarios (US1–US3) against committed
fixture directories under `tests/fixtures/` — a valid multi-image pack, a zero-config
static image, pack/per-image scaling overrides, and one fixture per FR-006/FR-006a
rejection case (malformed TOML, missing image, unsupported schema version, path
traversal, unreadable image, invalid scaling mode, malformed color).
