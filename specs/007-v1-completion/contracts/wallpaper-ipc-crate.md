# Contract: `wallpaper-ipc` crate — shared schema and D-Bus client

New workspace crate (research.md R2). This is the API contract `crates/renderer`,
`crates/wallpaperctl`, and `crates/wallpaper-settings` all depend on — the single source of truth
replacing three independently-defined copies.

## Dependencies (deliberately minimal)

`serde`, `cosmic-config` (git, `features = ["macro"]` — no `"calloop"`, that stays a `renderer`-
only feature since only the daemon watches these entries live), `zbus` (`features = ["async-io"]`
to match the workspace's existing choice), path dependencies on `schedule-engine` and
`pack-loader` only. **No** `wgpu`/`smithay-client-toolkit`/`wayland-client`/`calloop` — this is
the property spec 4 originally established for `wallpaperctl` and this crate exists specifically
to preserve it for the GUI too (plan.md Constitution Check finding 1).

## Exported types

- `RendererConfig`, `OutputAssignment`, `OutputId` — moved from `crates/renderer/src/output.rs`,
  unchanged shape (this is a *relocation*, not a redesign — spec 3's already-shipped on-disk
  format is untouched).
- `LocationConfigEntry`, `LocationMode`, `ResolutionStatus` — v3 (data-model.md), superseding both
  crates' independent v2 copies.
- `effective_location(&LocationConfigEntry) -> Option<Location>` — data-model.md, moved and
  extended from spec 6's `crates/renderer`-only version.
- `DbusClient` — moved from `crates/wallpaperctl/src/dbus_client.rs` unchanged (contracts/
  wallpaperd-dbus-interface.md, spec 4, remains the authoritative interface definition — this
  crate only relocates the client implementation, not the protocol).

## Who depends on this crate

- `crates/renderer` — as both a config **reader** (unchanged behavior) and, new in this spec, a
  config **writer** for `ip_location`/`ip_status` (mirroring how it already writes
  `automatic_location`/`automatic_status`, spec 6).
- `crates/wallpaperctl` — as both reader and writer, same as before the refactor; `dbus_client.rs`
  is deleted from this crate, replaced by a re-export from `wallpaper-ipc`.
- `crates/wallpaper-settings` (new) — as both reader and writer, and as the `DbusClient` consumer
  for live timeline/query views (spec.md FR-005).

## Compatibility guarantee this contract exists to enforce

Because all three consuming crates depend on one definition, the class of bug spec 6-era
`crates/renderer/src/config.rs` documents (two independently-typed "identical" shapes silently
failing to round-trip) is **structurally** prevented for every field this crate defines, not just
guarded by a regression test per field. New fields going forward should be added here first, not
independently in a consuming crate.
