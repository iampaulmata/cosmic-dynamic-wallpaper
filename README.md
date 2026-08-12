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
and broken into tasks under [`specs/`](specs/) before any implementation lands. There is no
buildable code yet.

| # | Spec | Status |
|---|---|---|
| 1 | [Core scheduling engine](specs/001-core-scheduling-engine/) — pure solar/time logic, no rendering | Planned, ready for implementation |
| 2 | [Pack format & loading](specs/002-pack-format-loading/) — manifest schema, pack directory loading | Planned, ready for implementation |
| 3 | Renderer — Wayland layer-shell client, GPU crossfade, multi-output | Not started |
| 4 | CLI control surface | Not started |
| 5 | Session integration & packaging | Not started |
| 6 | Location portal integration | Not started |

The project's governing principles — exclusive layer-shell ownership, Wayland-native
rendering with no X11 fallback, GPU-accelerated crossfade, `cosmic-config`-only persistence,
pure/deterministic/unit-tested solar math, and more — are ratified in
[`.specify/memory/constitution.md`](.specify/memory/constitution.md).

## Architecture at a glance

Once implemented, the daemon is planned as a Cargo workspace of independent crates, each
tracing back to one spec above:

- `crates/schedule-engine` — pure solar/clock scheduling logic (spec 1), no I/O
- `crates/pack-loader` — wallpaper pack manifest parsing and loading (spec 2)
- a Wayland layer-shell renderer crate (spec 3)
- a CLI control binary (spec 4)

Packs are user-authorable TOML manifests pointing at a directory of images, each tagged with
either a solar-event anchor (`sunrise`, `civil_dusk-30m`, etc.) or an absolute clock time —
see spec 2's [manifest schema](specs/002-pack-format-loading/contracts/pack-loader-api.md)
for the exact shape.

## Contributing

Not yet open for external contribution while the initial architecture is still being
specced out. Once spec 1 lands as real code, this section will cover build instructions,
toolchain requirements, and how to propose new specs.

## License

Not yet chosen.
