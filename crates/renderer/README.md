# renderer

The wallpaper renderer daemon (`wallpaperd`) for the dynamic wallpaper project —
**this crate currently implements the pure-logic subset only.**

## What's implemented (and tested — 93% line coverage, no `cargo-llvm-cov` gaps of note)

- **`output.rs`** — `OutputId`, `OutputAssignment`, `RendererConfig` (the
  `cosmic-config` schema spec 4's `wallpaperctl assign` writes to), and the resolution
  rule: an explicit `overrides` entry always wins; else `same_pack_everywhere` if set;
  else `Unassigned` (FR-005, FR-006).
- **`crossfade.rs`** — `CrossfadeTransition`'s progress math: monotonic, clamped to
  `[0.0, 1.0]`, deterministic given `started_at`/`duration`/now, immediately complete
  for a zero-duration transition (FR-001, FR-002, FR-004).
- **`config.rs`** — reading `RendererConfig` and spec 4's `LocationSource` via
  `cosmic-config`, and `Coalescer`: the FR-014 debounce logic (a repeated change to the
  same output before its 2-second deadline replaces the pending one wholesale, never
  queued).
- **`scheduler_bridge.rs`** — ties assignment resolution, a loaded pack, and a location
  together into spec 1's `ScheduleQueryResult` for one output. **Found and fixed a real
  cross-spec bug here**: spec 1's `ValidatedPack::query` *panics* if called with
  `location: None` on a solar-anchored pack (a documented caller-contract violation in
  spec 1's own contract) — but this daemon can legitimately reach exactly that state at
  runtime (a solar pack assigned before any location is configured). Naively calling
  `query()` there would crash the whole daemon, violating FR-013's per-output
  containment. This module checks the pack's anchor kind first and returns
  `RendererError::LocationRequired` (degrading just that one output) instead.
- **`dbus_types.rs`** — `QueryResponse`, the data shape a live D-Bus query would answer
  with (FR-016) — pure mapping from a `ScheduleQueryResult` to the response shape, no
  actual D-Bus connection.
- **`error.rs`** — the full `RendererError` enum matching data-model.md, including
  variants this pass never constructs (see below).

## What's explicitly NOT implemented

The actual Wayland/GPU rendering: `gpu.rs` (`wgpu` device/adapter setup), `surface.rs`
(`wlr-layer-shell` background surfaces, `wp_viewporter`), `texture.rs` (image decode +
GPU upload), the frame-callback-paced draw loop, SCTK's `OutputHandler` for hotplug
events, the live `zbus` D-Bus *server* (as opposed to the client `wallpaperctl` already
has), and the `wallpaperd` binary entry point tying it all together. None of
`smithay-client-toolkit`, `wgpu`, `calloop`, `calloop-wayland-source`,
`raw-window-handle`, or `zbus` are dependencies of this crate as a result.

**Why**: this pass was implemented in a sandboxed dev environment with no Wayland
compositor and no GPU rendering target. Every other spec in this project (1, 2, 4) is
pure logic or filesystem/D-Bus-client I/O, fully verifiable by `cargo test` alone —
this is the one spec where the actual core deliverable (a smooth GPU crossfade on a
real screen) cannot be *observed* to work correctly without a real compositor, no
matter how much code gets written. Writing that code without any way to verify it
renders correctly would mean shipping untested guesses in exactly the highest-risk part
of the whole project (spec.md itself calls this "the highest-risk spec per the PRD's own
breakdown"). The four `RendererError` variants tied to that unimplemented code
(`SurfaceCreationFailed`, `GpuDeviceUnavailable`, `TextureUploadFailed`,
`OutputProtocolError`) are still defined, matching the full data model, so the type is
ready for that code to construct them later without an API break — they're just never
constructed by anything in this crate today (see `error.rs`'s own module doc).

**Resuming this**: `specs/003-wallpaper-renderer/tasks.md`'s Phase 3+ implementation
tasks (T011–T018, T020–T022, T024–T026 [multi-output wiring], T030, T033,
T036–T042, T044, T053–T054) are the remaining work, all needing a real Wayland session
and (for the crossfade/integrated-graphics success criteria, SC-001/SC-002/SC-006) a
real GPU to verify against — see `quickstart.md`'s manual smoke check. Everything this
README lists as implemented is a stable foundation that code can build directly on
without rework.

## Testing

```sh
cargo test --package renderer
cargo llvm-cov --package renderer --summary-only
```

No Wayland/GPU/D-Bus connection needed for any test here — everything is pure logic,
`cosmic-config` I/O against a `tempfile`-backed scratch directory (same `open_at`
pattern as `pack-loader`'s `Registry` and `wallpaperctl`'s config types), or plain data
mapping.
