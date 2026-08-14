# renderer

The wallpaper renderer daemon (`wallpaperd`) for the dynamic wallpaper project —
Wayland `wlr-layer-shell` background surfaces, GPU-accelerated crossfade via `wgpu`,
per-output independent scheduling.

**Verified working against a real system** (2026-08-13): built and run against a live
`cosmic-comp` session on real hardware (Intel HD Graphics 630, Vulkan backend) —
registered a background layer surface, bridged it to `wgpu`, loaded real images,
uploaded GPU textures, and rendered/animated a real crossfade transition, running
stably across the full 45-second window with no crashes. The exact blend math was also
verified precisely via an offscreen GPU render + pixel readback test
(`tests/gpu_render.rs`) — not just "looked right" but exact byte values at
progress 0.0/0.5/1.0.

## What's implemented and tested

- **`output.rs`** — `OutputId`, `OutputAssignment`, `RendererConfig` (the
  `cosmic-config` schema `wallpaperctl assign` writes to), and the FR-005/006
  resolution rule (explicit override > toggle > unassigned).
- **`crossfade.rs`** — `CrossfadeTransition`'s progress math (FR-001/002/004/011) *and*
  `CrossfadePipeline`, the real two-texture WGSL blend (`shaders/crossfade.wgsl`),
  "Fill" (cover) scaling per texture. Pixel-verified on real hardware.
- **`gpu.rs`** — `wgpu` instance/adapter/device setup, automatic Vulkan/GL backend
  selection.
- **`texture.rs`** — full-resolution image decode (`image` crate) + GPU texture upload.
- **`surface.rs`** — the real thing: SCTK registry/output/compositor/layer-shell/
  viewporter wiring, per-output `wlr-layer-shell` background surface creation, the
  raw-window-handle bridge to `wgpu::Surface`, the `WallpaperDaemon` application state
  that loads each output's assigned pack, evaluates its schedule, uploads textures on
  demand, and draws/presents frames — including the frame-callback-paced draw loop
  during an active crossfade (subscribes only while animating, per FR-003/FR-004).
- **`config.rs`** — reading `RendererConfig` + spec 4's `LocationSource` via
  `cosmic-config`, and `Coalescer` (FR-014's debounce), including `earliest_pending`
  (peeked, not drained — feeds the idle-wait timer's wake computation).
- **`scheduler_bridge.rs`** — ties assignment + a loaded pack + location into spec 1's
  `ScheduleQueryResult`, with the location-required-panic fix described below.
- **`dbus_types.rs`** — `QueryResponse`, the pure data-mapping half of spec 4's D-Bus
  interface.
- **`src/bin/wallpaperd.rs`** — the actual daemon binary: connects to Wayland, loads
  config, runs the `calloop` event loop, and wires up the two live `cosmic-config`
  watches below.
- **Precise idle-wait timer** (T021) — `WallpaperDaemon::reschedule_idle_timer`
  replaces every managed output on a flat 5s poll with a single `calloop`
  `Timer::from_deadline` computed from `next_wake()` (the real next-transition
  instant) and `Coalescer::earliest_pending()` (so a pending config change is never
  serviced later than its own 2s deadline), rescheduled after every timer fire, every
  live config/location change, and every output's first `configure`. Live-verified:
  logged deadlines track real solar-schedule instants tens of minutes out, not a flat
  5s/60s cadence.
- **Live config-watch** (T028/T033/T050) — `cosmic_config::calloop::ConfigWatchSource`
  (wired in `wallpaperd.rs`) watches both `RendererConfig` and `LocationSource` for
  changes, feeding `Coalescer` via `WallpaperDaemon::on_renderer_config_changed`/
  `on_location_changed`. A `wallpaperctl assign`/`location set` while `wallpaperd` is
  running now takes effect within ~2s with **no restart** — live-verified against a
  real two-output session (assigning a pack to a previously-unassigned output, and
  changing location, both observed taking effect without restarting the daemon).

**Real cross-spec bug found and fixed** in `scheduler_bridge.rs`: spec 1's
`ValidatedPack::query` panics if called with `location: None` on a solar-anchored pack
(a documented caller-contract violation there) — but this daemon can legitimately reach
exactly that state at runtime (a solar pack assigned before any location is
configured). Checking the pack's anchor kind before ever calling `query()` turns a
would-be whole-daemon crash into a per-output `RendererError::LocationRequired`
degrade, matching FR-013.

**Second real bug found and fixed**, this one only surfaces when actually running
against a live config written by `wallpaperctl`: `RendererConfig.overrides` was
originally typed `HashMap<OutputId, PackSource>`. RON (the `cosmic-config` wire format)
does *not* treat a single-field tuple struct as transparently equivalent to its inner
`String` when used as a map key — it expects `OutputId`'s own textual form. Since
`wallpaperctl`'s independently-defined `RendererConfig` uses `HashMap<String,
PackSource>` (plain strings), this crate was silently reading an **empty** overrides
map on every real config `wallpaperctl` had actually written — the RON parse error was
swallowed by `RendererConfig::load`'s `unwrap_or_else` fallback to `Default`, with no
crash and no log line pointing at the cause. Fixed by matching `wallpaperctl`'s exact
on-disk shape (`HashMap<String, PackSource>`); a permanent regression test
(`config.rs`'s `overrides_parses_the_exact_shape_wallpaperctl_writes`) hand-constructs
the literal RON text `wallpaperctl` writes and confirms it parses correctly, rather
than only round-tripping through this crate's own (single) definition of the type. See
this crate's `RendererConfig` doc comment for the full story — a good example of why
"structurally identical Rust types in two crates" isn't automatically "wire compatible"
once a specific serialization format's own semantics enter the picture.

## What's simplified or not implemented

- **Hotplug resize/rescale**: `OutputHandler::update_output` is a no-op (T040) — a
  runtime resolution/scale change isn't reconfigured without a restart. `new_output`/
  `output_destroyed` (connect/disconnect) *are* wired (T037-T039's core), verified
  structurally but not exercised against a real hotplug event in this pass.
  `wp_fractional_scale_v1` isn't wired either — only `wp_viewporter`'s destination-size
  path, which is enough for integer-scale correctness but not fractional scaling.
- **The live D-Bus service** (`dbus_service.rs`, T049/T053/T054) — not implemented.
  `wallpaperctl query`/`reevaluate`/`list outputs` (spec 4) correctly report "daemon
  unreachable" until this lands. `dbus_types::QueryResponse`'s data mapping is done and
  tested, ready for a `zbus` server to use.
- **Per-image scaling overrides**: only "Fill" (cover) scaling is implemented in the
  shader; `pack-loader`'s `ScalingMode::Fit`/`Stretch`/`Center` aren't wired into
  `crossfade.rs`'s UV transform yet (`fill_uv_transform` would need siblings for the
  other three modes).
- **Overlapping-transition GPU resource cleanup** (T030): a new `CrossfadeTransition`
  value cleanly *replaces* the old one in this crate's data model (verified, see
  `crossfade.rs`'s tests) and old textures simply stay in the per-output cache for
  potential reuse (no explicit `wgpu::Texture` destruction) — `wgpu`'s reference
  counting handles actual GPU memory reclamation once a texture is no longer
  referenced, so there's no dangling-resource bug, but there's also no
  explicit "cancel the in-progress blend" step to point to as T030's deliverable.

## Building on a real system

This crate needs `libxkbcommon`'s development headers/`.pc` file
(`smithay-client-toolkit`'s build script requires them even though this daemon sets
`KeyboardInteractivity::None` and never actually processes keyboard input) — install
`libxkbcommon-dev` (Debian/Ubuntu) or your distribution's equivalent. On a system where
only the runtime library is present (no dev package, as in the sandbox this was
developed in), a `pkg-config` shim pointing at any available `xkbcommon.h` +
`libxkbcommon.so` (even from an unrelated SDK/Flatpak runtime, since only the ABI
matters, not where the files happen to live) works too — set `PKG_CONFIG_PATH` at build
time. A normal desktop dev machine with COSMIC's own build dependencies installed
should already have the real `-dev` package and need no workaround.

## Testing

```sh
cargo test --package renderer            # 27 tests: pure logic + a real offscreen GPU render
cargo llvm-cov --package renderer --summary-only
```

`tests/gpu_render.rs` needs a real `wgpu`-compatible GPU adapter (skips gracefully, not
fails, if none is found — e.g. a CI runner with no GPU). Everything else needs no
Wayland/GPU access at all. Running the actual `wallpaperd` binary against a live
compositor is manual QA — see `quickstart.md`'s smoke-check steps; this was done
during implementation, not just described.
