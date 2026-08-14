# Quickstart: Validating V1 Completion

Four areas, each with a different mix of automated vs. manual validation — split the same way
this project's prior specs have been.

## Prerequisites

- A stable Rust toolchain, same workspace as specs 1–6.
- A real COSMIC session for the GUI (any COSMIC app's baseline requirement) and for the mock
  hotplug harness's `wayland-server` double (no physical extra display needed — that's the point).
- For IP-geolocation: outbound UDP reachability for STUN (research.md R4) — this dev machine has
  network access, confirmed elsewhere in this project's session history.
- For the starter pack: no prerequisites — it's static, checked-in content.

## Run the automated test suite

```sh
cargo test --workspace
```

Expected coverage, by area:

- **Shared schema (`wallpaper-ipc`)**: v2→v3 `LocationConfig` migration round-trip
  (data-model.md), `effective_location()`'s now-three-way match, `RendererConfig`'s new
  `crossfade_duration_secs` default/round-trip, `PackRegistryEntry.origin` default/round-trip.
- **IP-geolocation (`crates/renderer/src/ip_geolocation.rs`)**: `maxminddb` lookup against a
  small fixture `.mmdb` (a handful of known test IP-to-location mappings, not the full bundled
  database) — fully offline, no real STUN/network call in `cargo test`.
- **Mock hotplug harness (`crates/renderer/tests/hotplug_mock.rs`)**: output connect, disconnect,
  and resize events driven through the real SCTK client code path via the `wayland-server` double
  (research.md R7) — closes spec 3 tasks.md T043 for real, not simulated in prose.
- **Starter pack registry (`pack-loader`)**: a `Package`-origin entry's removal is recorded in
  `RemovedStarterPacks`, and a simulated `postinst` re-run correctly skips re-registering it.

## Manual smoke check 1: GUI (requires a real COSMIC session)

```sh
cargo run -p wallpaper-settings
```

Expected outcome: the app opens as a standalone libcosmic window (not embedded in
`cosmic-settings`). With at least one pack registered (via `wallpaperctl register` beforehand),
the Packs page shows it with a preview; the Assignment page's write is visible via a concurrent
`wallpaperctl list` call; the Location page's mode switch is visible via `wallpaperctl location
get`; the Timeline page matches `wallpaperctl query`'s output for the same output; the Crossfade
page's change is picked up by a running `wallpaperd` without a restart (spec 3's existing live-
config-watch).

## Manual smoke check 2: Starter pack zero-config first run

```sh
# Simulates what postinst does, without a real package install:
wallpaperctl register assets/starter-pack --origin package   # or the real postinst script, once spec 5 lands
wallpaperd &
wallpaperctl query --output <your-output>
```

Expected outcome: the starter pack is active with no other configuration. Removing it
(`wallpaperctl remove assets/starter-pack`) and re-running the registration step again should
**not** re-add it — confirming `RemovedStarterPacks` is respected (FR-010).

## Manual smoke check 3: IP-geolocation happy path and degrade path

```sh
wallpaperctl location ip   # or the GUI's equivalent toggle
wallpaperd &
wallpaperctl location get
```

Expected outcome on this project's own dev machine (network-reachable): `mode: ip_geolocation`,
`ip_status: resolved`, with an approximate (city-level, per the bundled database's own precision)
location. Confirm the one disclosed external touchpoint (STUN) is visible in the GUI/CLI's opt-in
copy per FR-014 before this step is considered validated — not just that resolution succeeds.
Disconnecting the network before enabling should instead show `ip_status: unavailable`, with
`location get` falling back to any stored manual value or the existing no-location state, exactly
as it does for spec 6's portal mode.

## What "done" looks like for this spec

See spec.md's Success Criteria (SC-001–SC-006). `cargo test --workspace` closes SC-004 (hotplug/
resize/disconnect automated coverage) completely — the one criterion this project's own dev
environment can fully validate today. SC-001/SC-002/SC-003 need the manual smoke checks above
run at least once against this real session (not just "should work" from the automated suite
alone). SC-005 (recording spec 5/6's remaining manual-QA-only gaps) is tracked outside this
spec's own automated suite — see specs 5 and 6's own quickstart.md files for what those specific
checks are; this spec doesn't re-define them, only requires they've been run and recorded.
