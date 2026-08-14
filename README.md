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

This project is under active, spec-driven development using [GitHub Spec
Kit](https://github.com/github/spec-kit) — every feature is written up as a spec, planned,
and broken into tasks under [`specs/`](specs/) before implementation lands. Specs 1, 2, and 4
are fully implemented and tested; spec 3 has real, live-verified Wayland/GPU rendering with a
few documented gaps remaining. This has run end-to-end on a real COSMIC session: a real image
pack applied and crossfaded on-screen via `wallpaperctl` + `wallpaperd`.

| # | Spec | Status |
|---|---|---|
| 1 | [Core scheduling engine](specs/001-core-scheduling-engine/) — pure solar/time logic, no rendering | **Implemented** — `crates/schedule-engine` |
| 2 | [Pack format & loading](specs/002-pack-format-loading/) — manifest schema, pack directory loading, `cosmic-config` registry | **Implemented** — `crates/pack-loader` |
| 3 | [Renderer](specs/003-wallpaper-renderer/) — Wayland layer-shell client, GPU crossfade, multi-output | **Mostly implemented, live-verified** — `crates/renderer` (`wallpaperd` binary). Config is live-watched (no restart needed), the idle-wait timer is precise (schedule-driven, not a flat poll), the live D-Bus service backs `wallpaperctl query`/`reevaluate`/`list outputs`, and all four scaling modes (Fill/Fit/Stretch/Center) are implemented and pixel-verified. Remaining gap: no hotplug resize/rescale. See [`crates/renderer/README.md`](crates/renderer/README.md) for the full, current list. |
| 4 | [CLI control surface](specs/004-cli-control-surface/) | **Implemented** — `crates/wallpaperctl` (binary: `wallpaperctl`) |
| 5 | Session integration & packaging | Not started |
| 6 | Location portal integration | Not started |

The project's governing principles — exclusive layer-shell ownership, Wayland-native
rendering with no X11 fallback, GPU-accelerated crossfade, `cosmic-config`-only persistence,
pure/deterministic/unit-tested solar math, and more — are ratified in
[`.specify/memory/constitution.md`](.specify/memory/constitution.md).

## Architecture at a glance

The daemon is a Cargo workspace of independent crates, each tracing back to one spec above:

- `crates/schedule-engine` — pure solar/clock scheduling logic (spec 1), no I/O
- `crates/pack-loader` — wallpaper pack manifest parsing, loading, and `cosmic-config`
  registry persistence (spec 2)
- `crates/renderer` — the `wallpaperd` daemon: Wayland `wlr-layer-shell` background surfaces,
  GPU-accelerated crossfade via `wgpu`, per-output independent scheduling (spec 3)
- `crates/wallpaperctl` — the `wallpaperctl` CLI: register/assign packs, set location, query
  and control a running daemon (spec 4)

Packs are user-authorable TOML manifests pointing at a directory of images, each tagged with
either a solar-event anchor (`sunrise`, `civil_dusk-30m`, etc.) or an absolute clock time —
see spec 2's [manifest schema](specs/002-pack-format-loading/contracts/pack-loader-api.md)
for the exact shape.

### Running it today

1. Author a pack manifest (TOML) pointing at a directory of images, or use a zero-config
   directory of statically-named images.
2. `wallpaperctl register <path-to-pack>`, then `wallpaperctl assign --output <id> <path>`.
3. `wallpaperctl location set <lat> <lon>` if the pack uses solar anchors.
4. Run `wallpaperd` (it reads config once at startup — restart it after further config
   changes, since live-reload isn't wired up yet).

## Contributing

Not yet open for external contribution while spec 3's remaining gaps and specs 5-6 are still
being worked through. Build instructions and toolchain requirements live in each crate's own
README under `crates/`.

## License

GPL-3.0-or-later.
