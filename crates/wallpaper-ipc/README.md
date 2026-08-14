# wallpaper-ipc

Shared `cosmic-config` schema types and D-Bus client for the dynamic wallpaper project
(spec 7 research.md R2, contracts/wallpaper-ipc-crate.md) — the single source of truth
`crates/renderer`, `crates/wallpaperctl`, and `crates/wallpaper-settings` all depend on,
replacing three independently-defined copies of the same shapes.

## Why this crate exists

This project has already been bitten once by exactly the bug class a shared crate
prevents: `crates/renderer/src/config.rs`'s own history records `RendererConfig.
overrides` being independently typed in two crates (`HashMap<OutputId, PackSource>` vs.
`wallpaperctl`'s `HashMap<String, PackSource>`) — RON doesn't treat `OutputId`'s
newtype-struct form as transparently equivalent to a bare string key, so this silently
produced an **empty map** at runtime, caught only by a live round-trip test against a
real `wallpaperctl`-written config. Introducing a third control surface
(`wallpaper-settings`, spec 7) was the natural moment to fix this at the root — every
field defined here is now structurally guaranteed wire-compatible across every reader
and writer, not guarded by a per-field regression test alone (though those still exist
too, for the historical record).

## What's in here

- **`renderer_config.rs`** — `RendererConfig`, `OutputAssignment`, `OutputId`,
  `resolve_assignment`, `effective_pack` (spec 3's own schema, moved from
  `crates/renderer/src/output.rs`, unchanged shape). Spec 7 adds
  `crossfade_duration_secs`.
- **`location_config.rs`** — `LocationConfigEntry` (v3), `LocationMode`,
  `ResolutionStatus` (renamed from spec 6's `AutomaticStatus`), `effective_location()`.
- **`dbus_client.rs`** — `DbusClient`, `QueryEntry`, `DbusError` (moved from
  `crates/wallpaperctl/src/dbus_client.rs`, unchanged protocol — contracts/
  wallpaperd-dbus-interface.md remains authoritative).

Deliberately dependency-light: `serde`, `cosmic-config` (macro feature only, no
`calloop` — only the daemon watches these entries live), `zbus`, path dependencies on
`schedule-engine`/`pack-loader`. **No** `wgpu`/`smithay-client-toolkit`/
`wayland-client`/`calloop` — preserving the property spec 4 originally established for
`wallpaperctl` (never linking spec 3's heavy Wayland/GPU dependencies), now for the GUI
too.

## A real finding from this crate's own migration tests

`cosmic-config`'s `previous`-version fallback chain (used for schema migration, spec 6
research.md R7) is **one version hop deep only** — its recursive lookup passes
`look_for_previous: false`, so `previous.previous` is always `None` no matter how many
versions back the chain nominally spans. A direct v1 → v3 jump (skipping v2 entirely —
i.e. a machine that never ran a spec-6-era build even once) does **not** carry
`location` forward; see `location_config.rs`'s
`v1_location_entry_does_not_migrate_directly_to_v3_skipping_v2` test for the verified
behavior and full rationale. Not a live regression for any known deployment of this
still-unreleased project (every machine that ran a v2-era build already has a `v2/`
directory on disk, which bridges correctly one hop at a time) — flagged here as a
documented, verified limitation of the mechanism itself, not silently absorbed.

## Testing

```sh
cargo test --package wallpaper-ipc
```

23 tests — schema round-trips (including the v1→v2, v2→v3, and the one-hop-limit
migration tests above), `effective_location()`'s full three-way match, and the D-Bus
client's real fail-fast behavior against whatever session bus the test host has (no
mocking — no service is ever registered under its bus name in a test environment).
