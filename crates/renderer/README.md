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

**Follow-up gap-closure pass (2026-08-14)**: all 5 gaps documented below as of the
initial pass are now closed — precise idle-wait timer, live config/location watch, a
live D-Bus service, all four scaling modes, and hotplug resize/rescale + fractional
scale. Live-verified against a real two-output `cosmic-comp` session (this dev
environment now has `eDP-1` + `HDMI-A-1` connected). See each section below for the
specifics and the couple of caveats that remain honestly unverified (a real
disconnect/resize event can't be triggered on this dev machine).

## What's implemented and tested

- **`output.rs`** — `OutputId`, `OutputAssignment`, `RendererConfig` (the
  `cosmic-config` schema `wallpaperctl assign` writes to), and the FR-005/006
  resolution rule (explicit override > toggle > unassigned).
- **`crossfade.rs`** — `CrossfadeTransition`'s progress math (FR-001/002/004/011) *and*
  `CrossfadePipeline`, the real two-texture WGSL blend (`shaders/crossfade.wgsl`), all
  four `ScalingMode`s — Fill/Fit/Stretch/Center (FR-005), each texture scaled
  independently per its own pack's `image_scaling`/`fallback_color`. Pixel-verified on
  real hardware, including Fit/Center's letterboxing (`sample_or_fallback` in the
  shader, substituting `fallback_color` where the transformed UV falls outside `[0,
  1]`). **A real bug found by the GPU pixel test, not the pure-math unit tests**: an
  initial version of `fit_uv_transform`/`center_uv_transform` reused `fill_uv_transform`'s
  crop-direction formula (self-consistent enough that hand-derived-the-same-wrong-way
  unit tests still passed), which can structurally never produce an out-of-bounds UV —
  so no letterboxing ever actually happened. Only the offscreen GPU test (expecting
  `fallback_color` at a known letterboxed pixel) caught it. Fixed with the correct
  inverse relationship — see `crossfade.rs`'s `letterbox_scale_offset` doc comment for
  the full derivation.
- **`gpu.rs`** — `wgpu` instance/adapter/device setup, automatic Vulkan/GL backend
  selection.
- **`texture.rs`** — full-resolution image decode (`image` crate) + GPU texture upload.
- **`surface.rs`** — the real thing: SCTK registry/output/compositor/layer-shell/
  viewporter wiring, per-output `wlr-layer-shell` background surface creation, the
  raw-window-handle bridge to `wgpu::Surface`, the `WallpaperDaemon` application state
  that loads each output's assigned pack, evaluates its schedule, uploads textures on
  demand, and draws/presents frames — including the frame-callback-paced draw loop
  during an active crossfade (subscribes only while animating, per FR-003/FR-004).
- **`config.rs`** — reading `RendererConfig` + `LocationSource` (now spec 6's v2 schema,
  `mode`/`location`/`automatic_location`/`automatic_status`) via `cosmic-config`, plus
  `effective_location()` (spec 6's pure resolution rule) and `Coalescer` (FR-014's
  debounce), including `earliest_pending` (peeked, not drained — feeds the idle-wait
  timer's wake computation).
- **`scheduler_bridge.rs`** — ties assignment + a loaded pack + location into spec 1's
  `ScheduleQueryResult`, with the location-required-panic fix described below.
- **`portal_location.rs`** (spec 6, US1–US3) — automatic location via
  `org.freedesktop.portal.Location` (`ashpd`), driven inside `wallpaperd`'s existing
  single `calloop` loop (no dedicated OS thread): session creation at `Accuracy::City`,
  a 5s resolution timeout, an ongoing `LocationUpdated` subscription for as long as
  automatic mode is active, and exponential backoff (30s–5min) on any failure. Every
  outcome is validated through spec 1's `Location::new` and written back via
  `apply_reading`/`apply_failure` — the pure, fully-unit-tested half of this module.
  **Live-verified against this project's own real COSMIC session** (not just planned):
  a genuine `CreateSession`/`Start` round trip against `xdg-desktop-portal-cosmic`
  correctly produced `AutomaticStatus::Unavailable { reason: "...Location services
  disabled" }`, persisted, and correctly fell back to the stored manual location via
  `effective_location()` — end-to-end, not simulated. See "What's simplified or not
  implemented" below for the one honest gap (the resolved-value success path needs a
  GeoClue2-backed machine this dev environment doesn't have) and the deliberate
  "spawned once, not cancelled on mode toggle" simplification documented in the
  module's own doc comment.
- **`dbus_types.rs`** — `QueryResponse`, the pure data-mapping half of spec 4's D-Bus
  interface.
- **`dbus_service.rs`** — the live `zbus` server (T049/T053/T054, FR-016):
  `QueryOutput`/`QueryAll`/`Reevaluate`/`ReevaluateAll` exactly matching
  `specs/004-cli-control-surface/contracts/wallpaperd-dbus-interface.md`, integrated
  into `wallpaperd.rs`'s `calloop` loop via `internal_executor(false)` + `EventLoop::
  block_on` (no extra thread for this daemon's own code — see the module doc for the
  full integration story, including why `DbusState` is `Arc<Mutex<_>>` rather than
  `Rc<RefCell<_>>`). Live-verified against `crates/wallpaperctl/src/dbus_client.rs`
  (unchanged) — `wallpaperctl list outputs`/`query`/`reevaluate` all get real answers
  now, including the `InvalidArgs`→`CliError::OutputNotFound` mapping round-tripping
  for an unmanaged output name. **One honest caveat**: `zbus`'s `async-io` backend
  keeps one lazy background OS thread alive for its own reactor regardless of
  `internal_executor(false)` (a property of the `async-io` crate itself) — inert w.r.t.
  Wayland/wgpu state (never touches it), confirmed live: instantaneous CPU usage settles
  to 0% once idle (checked via `/proc/[pid]/stat` deltas, not just `ps`'s
  lifetime-averaged `%CPU` column, which is misleading right after the GPU/Vulkan
  startup burst).
- **`src/bin/wallpaperd.rs`** — the actual daemon binary: connects to Wayland, loads
  config, runs the `calloop` event loop, wires up the two live `cosmic-config`
  watches below, and serves the D-Bus service.
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
- **Hotplug resize/rescale + fractional scale** (T040) — `OutputHandler::update_output`
  now compares an output's current logical size against `LayerShellHandler::configure`'s
  own cached size and calls a shared `reconfigure_output` helper (the two handlers'
  previously-separate logic factored into one) when they differ, so a `wl_output`-level
  metadata change that doesn't also trigger a fresh layer-surface `configure` (e.g. a
  scale-only change) isn't silently dropped. `wp_fractional_scale_manager_v1` is now
  bound (optional/soft — degrades to a log line, doesn't fail the daemon, if a
  compositor doesn't advertise it) and a real `Dispatch<WpFractionalScaleV1, _>` handles
  its `preferred_scale` event (unlike `wp_viewporter`, which is purely imperative and
  safely `delegate_noop!`'d). **Live-verified further than expected**: this dev
  environment's real `cosmic-comp` *does* implement the protocol — binding succeeded and
  real `preferred_scale` events (120 = 1×) arrived for both managed outputs, confirmed
  via logs, with no regression to the existing single/multi-output behavior. **What's
  still not live-verified**: an actual logical-size change firing `update_output`'s
  reconfigure branch — this dev environment has no way to trigger a real resolution/scale
  change on its physical displays, so that branch is structurally correct and
  code-reviewed (same FR-013 per-output containment pattern as `configure`) but not
  exercised against a real event, the same honest caveat `T039`/`T043` already carry for
  hotplug disconnect.

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

- **Overlapping-transition GPU resource cleanup** (T030): a new `CrossfadeTransition`
  value cleanly *replaces* the old one in this crate's data model (verified, see
  `crossfade.rs`'s tests) and old textures simply stay in the per-output cache for
  potential reuse (no explicit `wgpu::Texture` destruction) — `wgpu`'s reference
  counting handles actual GPU memory reclamation once a texture is no longer
  referenced, so there's no dangling-resource bug, but there's also no
  explicit "cancel the in-progress blend" step to point to as T030's deliverable.
- **Automatic location's resolved-value success path** (spec 6 US1/US3): the degrade
  path (FR-005) is fully live-verified against this project's own real COSMIC session —
  see `portal_location.rs`'s entry above. The *successful*-resolution half needs a
  machine with GeoClue2 installed and location services enabled, which this dev
  environment doesn't have (spec 6 research.md R2) — every component up to the portal
  boundary is real and live-spiked (a genuine `CreateSession`/`Start` round trip), the
  final resolved-value hop is a documented, honest gap, not a task this crate can close
  on its own without that hardware/software dependency present.
- **Automatic-mode task lifecycle** (spec 6): the portal-driving resolution/subscribe/
  retry task is spawned once automatic mode is (or becomes) active and then runs for
  the remainder of the daemon's lifetime — it is not cancelled if the user later
  switches back to manual mode. Documented as harmless (not a correctness gap) in
  `portal_location.rs`'s module doc: `effective_location()` ignores `automatic_location`
  entirely while `mode == Manual`, so a background retry loop simply has no observable
  effect until automatic mode is re-enabled.

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
cargo test --package renderer            # 50 tests: pure logic + real offscreen GPU renders
cargo llvm-cov --package renderer --summary-only
```

`tests/gpu_render.rs` needs a real `wgpu`-compatible GPU adapter (skips gracefully, not
fails, if none is found — e.g. a CI runner with no GPU). Everything else needs no
Wayland/GPU access at all. Running the actual `wallpaperd` binary against a live
compositor is manual QA — see `quickstart.md`'s smoke-check steps; this was done
during implementation, not just described.
