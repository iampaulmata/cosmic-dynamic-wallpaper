# wallpaper-settings

Standalone `libcosmic` settings GUI for the dynamic wallpaper daemon (spec 7 US1,
contracts/gui-application.md) — the CLI (`wallpaperctl`) remains fully supported
alongside it, not replaced.

```sh
cargo run -p wallpaper-settings
```

**Not a `cosmic-settings` panel** — a standalone app (spec 7 spec.md Clarifications,
research.md R1: COSMIC has no general third-party settings-panel extension mechanism).

## Live-verified (2026-08-14, this project's own dev COSMIC session)

Run for real, not just built: opened a real window against the live Vulkan/Wayland
stack (`wgpu` selected "Intel(R) HD Graphics 630", the same adapter `crates/renderer`
itself uses), ran stably for 6+ seconds with no panic, and shut down cleanly on
`SIGTERM`. A screenshot comparison wasn't obtainable in this session — the desktop
portal's `Screenshot` call needs interactive user consent, the same non-interactive
limitation spec 5's own README already documents for this project.

## Pages

Five pages (`src/pages/`), sidebar navigation (`src/app.rs`) — each holds its own pure
view-state/mapping logic (unit-tested, independent of rendering) plus a `view()`
function building the real widgets:

- **Packs** (FR-002) — browse already-registered packs via `pack_loader::Registry`.
  Registration itself remains `wallpaperctl register`'s job (contracts/
  gui-application.md's own explicit non-scope: "browse registered packs", not
  "register new ones").
- **Assignment** (FR-003) — per-output / same-pack-everywhere, writing the identical
  `wallpaper_ipc::RendererConfig` shape `wallpaperctl assign` does.
- **Location** (FR-004) — manual/automatic/IP-geolocation mode switch, writing the
  identical `wallpaper_ipc::LocationConfigEntry` shape `wallpaperctl location`'s
  subcommands do. Shows the STUN-disclosure copy (FR-014) when IP-geolocation mode is
  selected — the identical wording `wallpaperctl location ip` itself surfaces
  (duplicated as a literal string constant in each binary, not a shared dependency —
  this is UI copy, not a schema, so the cross-crate-drift risk `wallpaper-ipc`
  specifically exists to prevent doesn't apply here; cross-referenced via doc comments
  in both places instead).
- **Timeline** (FR-005) — today's schedule via `wallpaper_ipc::DbusClient`, read-only,
  same "daemon unreachable" fallback UX `wallpaperctl query` uses.
- **Crossfade** (FR-006) — `RendererConfig.crossfade_duration_secs`, the field this
  spec's own Foundational phase actually created (plan.md Constitution Check finding
  3 — it didn't exist before spec 7 despite an old doc comment claiming otherwise).

FR-007 (GUI/CLI interchangeability) is enforced structurally: every page links
`wallpaper_ipc`'s shared types directly, the same crate `wallpaperctl` depends on — not
by convention, and not by an independent second definition (plan.md Constitution Check
finding 1, the same bug class `wallpaper-ipc`'s own extraction already fixed once).

## What's simplified

- **Packs page preview**: shown as a file path (`widget::text::caption`), not a
  rendered `<image>` thumbnail — contracts/gui-application.md's own text only
  requires "browse... with preview", not a specific rendering; a real thumbnail is a
  reasonable follow-up, not required here.
- **Assignment page pack picker**: assigns the *first* registered pack
  (`available_packs[0]`) rather than a full dropdown/picker widget — this page's
  actual contract requirement is writing the identical `RendererConfig` shape
  `wallpaperctl assign` does (verified, `pages/assignment.rs`'s own unit tests), not a
  complete picker UX.
- **No live-refresh subscription**: pages reload their state on navigation and via an
  explicit "Refresh" button (Packs, Timeline) rather than subscribing to
  `cosmic_config`'s live-watch mechanism or polling on a timer — a config change made
  externally (e.g. via `wallpaperctl` in another terminal) is picked up the next time
  the page is visited or refreshed, not instantly. `wallpaperd` itself still reacts
  within its own existing 2s bound regardless of whether this GUI happens to be open.

## Testing

```sh
cargo test --package wallpaper-settings
```

11 tests — each page's pure view-state/mapping logic (T019–T023), independent of
`libcosmic` rendering. The actual rendered window is manual QA against a real COSMIC
session (see `specs/007-v1-completion/quickstart.md`'s "Manual smoke check 1"), same
posture as this project's other Wayland-adjacent crates (`renderer`).
