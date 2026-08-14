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

**Confirmed live during implementation (2026-08-14)**: `cargo run -p wallpaper-settings` opened a
real window against this dev machine's live Vulkan/Wayland stack (`wgpu` selected "Intel(R) HD
Graphics 630", the same GPU adapter `crates/renderer` itself uses), ran stably for 6+ seconds with
zero panics, and shut down cleanly on `SIGTERM`. A pixel-level screenshot comparison wasn't
obtainable non-interactively — the desktop portal's `Screenshot` call needs interactive user
consent, the same limitation spec 5's own README already documents (its T010 note). The
page-by-page read/write behavior above is exercised by each page's own pure-logic unit tests
(11 total, `crates/wallpaper-settings/src/pages/`), not re-verified click-by-click in this pass.

## Manual smoke check 2: Starter pack zero-config first run

**Drift fixed during implementation**: `wallpaperctl register` has no `--origin` flag — a real
design correction, not this doc catching up to a missed feature (see
`crates/renderer/src/starter_pack.rs`'s module doc for the full rationale: `postinst` runs once,
as root, with no access to any user's per-user `cosmic-config` store, so it can't correctly
register anything there). The actual mechanism is `wallpaperd`'s own first-run self-registration,
checked on every startup:

```sh
sudo mkdir -p /usr/share/dynamic-wallpaper
sudo cp -r assets/starter-pack /usr/share/dynamic-wallpaper/starter-pack   # simulates the .deb's own asset install
wallpaperd &
wallpaperctl query --output <your-output>
```

Expected outcome: on first run against an empty registry, `wallpaperd` registers
`/usr/share/dynamic-wallpaper/starter-pack` as `Package`-origin and assigns it via
`same_pack_everywhere` (only if nothing was already configured — FR-011), then the starter pack is
active with no other configuration. Removing it (`wallpaperctl remove
/usr/share/dynamic-wallpaper/starter-pack`) and restarting `wallpaperd` again should **not**
re-add it — confirming `RemovedStarterPacks` is respected (FR-010).

**Not run live in this dev environment**: placing a file under `/usr/share/` needs `sudo`, which
this implementation session's shell doesn't have interactively (the same constraint spec 5's own
quickstart.md already documents for its own install/uninstall cycle, T022). The logic itself —
every branch of `maybe_register()` (fresh install, missing path, already-removed, existing
assignment untouched, idempotent across restarts) — is fully covered by `starter_pack.rs`'s own
6 unit tests instead (99.5% region coverage). Ready to run for real whenever the user does the
`sudo` steps above themselves.

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

**Confirmed live during implementation (2026-08-14), the STUN half specifically**: a genuine
`discover_public_ip_blocking()` round trip against the real default STUN server
(`stun.l.google.com:19302`) correctly returned this dev machine's real public IP — via a
throwaway `crates/renderer/examples/stun_smoke.rs` harness, deleted after use. **A real bug was
found and fixed doing this**: this machine's DNS resolves the STUN server to an IPv6 address
first; the original implementation bound an IPv4-only wildcard socket, so a naive
`.to_socket_addrs().next()` silently picked the IPv6 result and failed with an opaque "UDP socket
error" (an address-family mismatch, not a network problem, and not the kind of thing a fixture
test would catch — this specific failure mode only exists against real DNS). Fixed to prefer an
IPv4 result when one exists and bind accordingly; re-verified live after the fix. The database
half (a real bundled DB-IP Lite lookup) remains unverified in this dev environment — no `.mmdb`
is present locally (it's a release-process download, `crates/renderer/README.md`'s
"IP-geolocation database" section, not something available during implementation) — the fixture
test suite (`ip_geolocation.rs`) is what closes SC coverage for the lookup logic itself.

## What "done" looks like for this spec

See spec.md's Success Criteria (SC-001–SC-006). `cargo test --workspace` closes SC-004 (hotplug/
resize/disconnect automated coverage) completely — the one criterion this project's own dev
environment can fully validate today. SC-001/SC-002/SC-003 need the manual smoke checks above
run at least once against this real session (not just "should work" from the automated suite
alone). SC-005 (recording spec 5/6's remaining manual-QA-only gaps) is tracked outside this
spec's own automated suite — see specs 5 and 6's own quickstart.md files for what those specific
checks are; this spec doesn't re-define them, only requires they've been run and recorded.
