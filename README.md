# Dynamic Wallpaper

A from-scratch, COSMIC-native dynamic wallpaper daemon for the [COSMIC desktop
environment](https://github.com/pop-os/cosmic-epoch). It rotates wallpapers across the day
following real solar events (sunrise, sunset, civil/astronomical twilight, solar noon) for
your location — or a fully custom, location-free manual schedule — and crossfades smoothly
between images instead of hard-cutting.

## Why

Several desktops already do time-of-day wallpapers — macOS's Dynamic Desktop, Windows'
WinDynamicDesktop, GNOME's slideshow XML format, Cinnamon's **Dynamic Wallpaper** extension.
COSMIC doesn't have an equivalent yet: `cosmic-bg`'s slideshow feature only supports dumb
fixed-interval rotation through an unordered image list, with no concept of time-of-day or a
smooth transition between images.

This project differs from prior art in three ways:

- **Smooth GPU crossfade** between images at each transition, not a hard cut
- **Astronomically-anchored periods** (civil/astronomical twilight, solar noon), not just
  raw sunrise/sunset with evenly-spaced slots
- **Native libcosmic UI and `cosmic-config` persistence**, not a bolted-on toolkit

## Goals

- Wallpapers change automatically across the day, following either real solar events for
  your location or a fully custom manual schedule
- Transitions between images are visually smooth (crossfade), not jarring
- Works correctly across mixed multi-monitor, multi-scale-factor setups
- Feels like a native part of COSMIC — install, configure, and forget
- Idle cost (when no transition is happening) is effectively zero

## Non-goals (v1)

- Video, animated GIF, or GPU-shader wallpapers — timed *still-image* transitions only
- A curated wallpaper marketplace or image-hosting service
- Cross-desktop support (GNOME, KDE, etc.) — COSMIC only
- Weather-reactive imagery — solar/time-based only
- Parsing Apple's `.heic` dynamic-wallpaper metadata format directly

See [`docs/PRD.md`](docs/PRD.md) for the full requirements this project is scoped against.

## Status

**All seven planned v1 specs are implemented.** This project was built spec-driven using
[GitHub Spec Kit](https://github.com/github/spec-kit) — every feature is written up as a
spec, planned, and broken into tasks under [`specs/`](specs/) before implementation lands.
Every crate has been live-verified against a real COSMIC session (not just built): Wayland/GPU
rendering including multi-output, hotplug, and fractional-scale handling (spec 3); systemd
autostart, clean stop, and bounded crash-restart (spec 5); a genuine portal
(`org.freedesktop.portal.Location`) round trip and its graceful degrade path (spec 6); a
standalone `libcosmic` settings GUI opened against the live Vulkan/Wayland stack, a real STUN
round trip for IP-geolocation, and a mock-compositor-driven hotplug/disconnect/resize test
harness that closes a gap no physical hardware in this project's own dev environment could
exercise on demand (spec 7). A real image pack applies and crossfades on-screen via
`wallpaperctl`/`wallpaperd`/the GUI, end to end.

| # | Spec | Status |
|---|---|---|
| 1 | [Core scheduling engine](specs/001-core-scheduling-engine/) — pure solar/time logic, no rendering | **Implemented** — `crates/schedule-engine` |
| 2 | [Pack format & loading](specs/002-pack-format-loading/) — manifest schema, pack directory loading, `cosmic-config` registry | **Implemented** — `crates/pack-loader` |
| 3 | [Renderer](specs/003-wallpaper-renderer/) — Wayland layer-shell client, GPU crossfade, multi-output | **Implemented, live-verified** — `crates/renderer` (`wallpaperd` binary). Config is live-watched (no restart needed), the idle-wait timer is precise (schedule-driven, not a flat poll), the live D-Bus service backs `wallpaperctl query`/`reevaluate`/`list outputs`, all four scaling modes (Fill/Fit/Stretch/Center) are implemented and pixel-verified, and hotplug resize/rescale + fractional-scale are wired up and covered by a real mock-compositor test harness (spec 7). See [`crates/renderer/README.md`](crates/renderer/README.md) for the couple of caveats that remain unverified against real physical hotplug/disconnect events. |
| 4 | [CLI control surface](specs/004-cli-control-surface/) | **Implemented** — `crates/wallpaperctl` (binary: `wallpaperctl`) |
| 5 | [Session integration & packaging](specs/005-session-integration-packaging/) — systemd user unit, Debian package | **Implemented, live-verified** — [`packaging/`](packaging/). Autostart/clean-stop/bounded-crash-restart all demonstrated live; `cosmic-bg` confirmed to never double-render. The real `.deb` (`cargo deb -p renderer`) builds and its contents/maintainer-scripts are verified. |
| 6 | [Location portal integration](specs/006-location-portal-integration/) — automatic location via `org.freedesktop.portal.Location` | **Implemented, live-verified** — a real `CreateSession`/`Start` round trip against `xdg-desktop-portal-cosmic` confirmed the portal is genuinely implemented, and the graceful-degrade path (no GeoClue2 backend installed) was validated end to end. `wallpaperctl location auto\|manual` toggles the mode. |
| 7 | [V1 completion](specs/007-v1-completion/) — GUI, starter pack, IP-geolocation fallback, gap closure | **Implemented, live-verified** — `crates/wallpaper-settings` (standalone `libcosmic` GUI), a bundled zero-config starter pack, `crates/renderer/src/ip_geolocation.rs` (STUN + offline `.mmdb` database), and a mock Wayland compositor test harness closing spec 3's long-standing hotplug/disconnect/resize test gap. |

The project's governing principles — exclusive layer-shell ownership, Wayland-native
rendering with no X11 fallback, GPU-accelerated crossfade, `cosmic-config`-only persistence,
pure/deterministic/unit-tested solar math, and more — are ratified in
[`.specify/memory/constitution.md`](.specify/memory/constitution.md).

## Architecture at a glance

The daemon is a Cargo workspace of independent crates, each tracing back to one spec above:

- `crates/schedule-engine` — pure solar/clock scheduling logic (spec 1), no I/O
- `crates/pack-loader` — wallpaper pack manifest parsing, loading, and `cosmic-config`
  registry persistence (spec 2)
- `crates/wallpaper-ipc` — shared `cosmic-config` schema types and D-Bus client (spec 7) —
  the single source of truth `renderer`/`wallpaperctl`/`wallpaper-settings` all depend on,
  instead of each independently (and riskily) redefining the same shapes
- `crates/renderer` — the `wallpaperd` daemon: Wayland `wlr-layer-shell` background surfaces,
  GPU-accelerated crossfade via `wgpu`, per-output independent scheduling (spec 3), the
  location portal integration (spec 6), and IP-geolocation (spec 7)
- `crates/wallpaperctl` — the `wallpaperctl` CLI: register/assign packs, set location, query
  and control a running daemon (spec 4)
- `crates/wallpaper-settings` — the `wallpaper-settings` GUI: a standalone `libcosmic`
  settings app covering everything `wallpaperctl` does (spec 7) — the CLI remains fully
  supported alongside it, neither replaces the other
- `tools/generate-starter-pack` — maintainer-only, never shipped; a fallback generator for
  the bundled starter pack's images (spec 7)

Packs are user-authorable TOML manifests pointing at a directory of images, each tagged with
either a solar-event anchor (`sunrise`, `civil_dusk-30m`, etc.) or an absolute clock time —
see spec 2's [manifest schema](specs/002-pack-format-loading/contracts/pack-loader-api.md)
for the exact shape.

## Installing

### Download a release (recommended)

Pre-built `.deb` packages are published on this repository's
**[Releases page](https://github.com/iampaulmata/rust-dynamic-wallpaper/releases)**. To
install the latest one:

```sh
# Download the .deb from the Releases page above, then:
sudo apt install ./dynamic-wallpaper_*.deb
```

`wallpaperd` then autostarts with your COSMIC session via the bundled systemd user unit, and
a bundled starter pack is registered and actively scheduled automatically — nothing to
configure to see it working. Launch **wallpaper-settings** from your app launcher (or
`wallpaper-settings` from a terminal) to browse packs, change assignment/location/crossfade
settings, or use `wallpaperctl --help` for the CLI equivalent.

> No release has been published yet as of this writing — if the Releases page above is
> empty, build from source instead (below), or check back once one has been cut.

### Build from source

```sh
git clone https://github.com/iampaulmata/rust-dynamic-wallpaper.git
cd rust-dynamic-wallpaper
cargo build --release --workspace
cargo deb -p renderer          # produces target/debian/*.deb
sudo apt install ./target/debian/*.deb
```

See [`packaging/`](packaging/) and `specs/005-session-integration-packaging/quickstart.md`
for the full packaging walkthrough, and each crate's own README under `crates/` for its
individual build/toolchain requirements (e.g. `crates/renderer/README.md`'s
`libxkbcommon-dev` note).

### Running without installing (development)

1. Author a pack manifest (TOML) pointing at a directory of images, or use a zero-config
   directory of statically-named images.
2. `wallpaperctl register <path-to-pack>`, then `wallpaperctl assign --output <id> <path>`.
3. `wallpaperctl location set <lat> <lon>` (or `location auto`/`location ip` — specs 6/7) if
   the pack uses solar anchors.
4. Run `wallpaperd` — config changes take effect live (no restart needed). Optionally run
   `wallpaper-settings` instead of/alongside `wallpaperctl` for the same control surface as a
   GUI.

## Contributing

All seven planned v1 specs are implemented — see the table above and each spec's own
directory under [`specs/`](specs/) for what shipped and what's documented as a known,
honest gap. Build instructions and toolchain requirements live in each crate's own README
under `crates/`.

## License

GPL-3.0-or-later.
