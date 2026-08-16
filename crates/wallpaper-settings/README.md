# wallpaper-settings

Standalone `libcosmic` settings GUI for the Cosmic Dynamic Wallpaper daemon (spec 7 US1,
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

**Spec 008 re-verification (same date, after implementing US1–US6)**: re-launched
against the same live session — a real registered pack ("Mountains", with a genuine
solar-noon-anchored image) and two real connected displays (`eDP-1`, `HDMI-A-1`, via
`wallpaperctl list outputs`) were present, so the default Packs page rendered a real
`widget::image` thumbnail on startup, not a placeholder. Ran stably for 8+ seconds
with no panic, and exited cleanly. **Not verified in this non-interactive pass**:
actually clicking the file-chooser buttons, the removal confirmation dialog, the
Assignment toggle/dropdowns, or the Location hover tooltip/info icon — none of those
are scriptable without a real pointer/keyboard driving the window, so they remain
manual QA for the user to run via quickstart.md's six smoke checks, same posture this
project uses for every other interactive Wayland-adjacent surface.

## Pages

Five pages (`src/pages/`), sidebar navigation (`src/app.rs`) — each holds its own pure
view-state/mapping logic (unit-tested, independent of rendering) plus a `view()`
function building the real widgets:

- **Packs** (FR-001–FR-004, FR-012, FR-018–FR-020, spec 008) — browse, add, and remove
  registered packs via `pack_loader::Registry`, each shown by its resolved name and a
  thumbnail preview (the solar-noon-anchored image, or the first image, spec 008
  research.md R7) rather than a raw file path. Adding uses the native file/folder
  picker (`cosmic::dialog::file_chooser`); removing requires confirming in a dialog
  (`Application::dialog()`). Supersedes spec 7's original "browse registered packs,
  not register new ones" scope note — `wallpaperctl register`/`remove` remain fully
  supported alongside the GUI, neither replaces the other.
- **Assignment** (FR-003, FR-010–FR-011, FR-013–FR-017, spec 008) — a "same pack
  everywhere" toggle (on by default) plus, when off, an independent per-display
  dropdown, writing the identical `wallpaper_ipc::RendererConfig` shape
  `wallpaperctl assign` does. Every dropdown option is labeled by the pack's resolved
  name, not its path. Switching the toggle on clears any existing per-display
  assignments so it applies unconditionally — a deliberate GUI-specific behavior;
  `wallpaperctl assign --same-everywhere` itself is unchanged (spec 008 research.md
  R6). Supersedes spec 7's "assigns the first registered pack" simplification.
- **Location** (FR-004, FR-007–FR-009, spec 008) — manual/automatic/IP-geolocation
  mode switch, writing the identical `wallpaper_ipc::LocationConfigEntry` shape
  `wallpaperctl location`'s subcommands do. The STUN-disclosure copy is discoverable
  by hovering the IP-geolocation option (`widget::tooltip`) or tapping its persistent
  info icon, *before* that option is selected, not only after (spec 008 US3 —
  supersedes spec 7's post-selection-only placement). The disclosure text itself now
  lives in `wallpaper_ipc::IP_GEOLOCATION_DISCLOSURE` — a real, single shared constant
  (spec 008 research.md R4), not the two independently-duplicated copies this crate
  and `wallpaperctl` each carried before despite a doc comment here claiming they were
  kept in sync.
- **Timeline** (FR-005) — today's schedule via `wallpaper_ipc::DbusClient`, read-only,
  same "daemon unreachable" fallback UX `wallpaperctl query` uses.
- **Crossfade** (FR-006) — `RendererConfig.crossfade_duration_secs`, the field this
  spec's own Foundational phase actually created (plan.md Constitution Check finding
  3 — it didn't exist before spec 7 despite an old doc comment claiming otherwise).

### Pack builder wizard (spec 010-custom-pack-builder)

Not a sidebar page — a modal wizard flow (`src/pages/pack_builder.rs`) launched from
the Packs page's "Add pack folder…" when the chosen folder has no `manifest.toml` of
its own yet. Walks the user through naming each image and picking its time anchor,
self-validates the generated draft against `pack_loader::load_pack` before offering to
place it (catching a bad manifest before it's ever written), and only writes
`manifest.toml` into the source folder — or copies the whole pack into the registry's
managed pack directory first, on a name collision — once the user actually confirms
Move/Keep, not at generation time (spec 011 US6 FR-027). Collision-rename input is
validated against path traversal, an absolute path, and an empty string before it's
ever used to construct a destination path (spec 011 US2 FR-006/FR-007).

FR-007 (GUI/CLI interchangeability) is enforced structurally: every page links
`wallpaper_ipc`'s shared types directly, the same crate `wallpaperctl` depends on — not
by convention, and not by an independent second definition (plan.md Constitution Check
finding 1, the same bug class `wallpaper-ipc`'s own extraction already fixed once).

## What's simplified

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

81 tests (spec 011 US8 FR-051 — previously documented as 29, stale since spec
010-custom-pack-builder's `pack_builder` wizard landed) — each page's pure
view-state/mapping logic, including the pack builder wizard's validation/self-check/
placement-flow coverage, plus `pack_display`'s name/thumbnail resolution, independent
of `libcosmic` rendering. The actual rendered
window (dialogs, tooltips, dropdowns, scrolling) is manual QA against a real COSMIC
session — see `specs/007-v1-completion/quickstart.md`'s "Manual smoke check 1" and
`specs/008-gui-usability-improvements/quickstart.md`'s six smoke checks, same posture
as this project's other Wayland-adjacent crates (`renderer`).
