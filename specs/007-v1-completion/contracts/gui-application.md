# Contract: `wallpaper-settings` GUI application

Standalone libcosmic application (spec.md Clarifications, research.md R1). Not a `cosmic-settings`
panel. This contract documents the app's page structure and its read/write relationship to
already-defined state — not visual design, which is an implementation-phase concern.

## Page structure (`cosmic::Application`, single window, sidebar navigation)

| Page | FR | Reads | Writes |
|---|---|---|---|
| Packs | FR-002 | `pack-loader::Registry` (spec 2) via `wallpaper-ipc` | Registration is out of this spec's GUI scope unless a task adds it — spec.md doesn't require the GUI to *register* new packs, only browse already-registered ones (FR-002's own text: "browse registered packs"). `wallpaperctl register` remains the way a pack becomes known. |
| Assignment | FR-003 | `RendererConfig` (via `wallpaper-ipc`) | `RendererConfig.overrides` / `same_pack_everywhere` — identical write spec 4's `assign` command already makes. |
| Location | FR-004 | `LocationConfigEntry` v3 (via `wallpaper-ipc`) | `mode`, `location` — identical writes to `wallpaperctl location set/auto/manual` plus a new IP-geolocation toggle (contracts/location-config-schema-v3.md). |
| Timeline | FR-005 | `wallpaper-ipc::DbusClient` `QueryOutput`/`QueryAll` (spec 4's existing D-Bus interface, unchanged) | Read-only — same "daemon unreachable" fallback UX as `wallpaperctl query` when no daemon is running, not a new failure mode. |
| Crossfade | FR-006 | `RendererConfig` (extended with a duration field — **new field this spec adds**, since spec 3 fixed this at a 45s constant with no config surface) | `RendererConfig.crossfade_duration_secs` (new, defaults to spec 3's existing 45s constant so upgrading doesn't change behavior until a user touches this page) |

## FR-007: GUI/CLI interchangeability

Enforced structurally (plan.md Constitution Check finding 1), not by convention: both the GUI and
`wallpaperctl` link the same `wallpaper-ipc` types for every schema above. A value written by one
is immediately readable by the other via the same `cosmic-config` entries — this contract makes no
additional promise beyond what `wallpaper-ipc`'s own contract (contracts/wallpaper-ipc-crate.md)
already guarantees.

## Daemon-optional pages

Packs, Assignment, and Location pages work with no `wallpaperd` running (same daemon-optional
posture as their CLI equivalents, spec 4 FR-011) — they read/write `cosmic-config` directly. The
Timeline page requires a running daemon (same as `wallpaperctl query`) and shows a clear
"daemon unreachable" state otherwise, not a hang or a blank page.

## Explicitly out of scope for this contract

- Pack *registration* through the GUI (spec.md FR-002 only requires browsing already-registered
  packs) — may be added in a future spec/task, not required here.
- A GUI-native crossfade *preview* (seeing the actual GPU blend inside the settings app) — FR-006
  only requires adjusting the duration value, not rendering a live preview of it.
