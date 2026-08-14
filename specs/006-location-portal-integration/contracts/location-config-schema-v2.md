# Contract: `LocationConfig` `cosmic-config` schema, v2

Supersedes `specs/004-cli-control-surface/contracts/location-config-schema.md` (v1). That file
remains the historical record of v1's shape; this is the authoritative current schema. Full
field-level rationale lives in [../data-model.md](../data-model.md) — this file documents the
on-disk shape and the read/write contract between crates.

## Schema (RON, `cosmic-config`-managed)

```text
LocationConfigEntry(
    schema_version: 2,
    mode: Automatic,
    location: Some(Location(
        latitude: 45.5019,
        longitude: -73.5674,
    )),
    automatic_location: Some(Location(
        latitude: 45.4972,
        longitude: -73.6104,
    )),
    automatic_status: Resolved,
)
```

A freshly-degraded example (this project's own dev machine, per research.md R1/R2 — a real,
live-observed shape, not hypothetical):

```text
LocationConfigEntry(
    schema_version: 2,
    mode: Automatic,
    location: None,
    automatic_location: None,
    automatic_status: Unavailable(reason: "Location services disabled"),
)
```

## Migration from v1

v1's on-disk shape was `LocationConfig(schema_version: 1, location: Option<Location>)`
(historical file above). No hand-written migration function exists or is needed: `cosmic-config`
itself falls back to the previous version's directory, per-key, whenever a key is absent from
the current version (research.md R7, verified against the vendored `cosmic-config` source, not
assumed). Since `location` keeps its exact v1 name and type, it's carried over automatically;
`mode`, `automatic_location`, and `automatic_status` are new in v2 and simply take their
`Default` (`Manual`, `None`, `Unresolved` respectively). No user-visible behavior change until the
user explicitly enables automatic mode (data-model.md's Migration section has the full mapping
and the mechanism explained in full).

## Who writes this schema

- `wallpaperctl location set <lat> <lon>` — sets `location`, forces `mode: Manual` (research.md
  R7's documented default).
- `wallpaperctl location clear` — clears `location` only; `mode` and the `automatic_*` fields are
  untouched (research.md R7).
- `wallpaperctl location auto` — sets `mode: Automatic`. Does not touch `location`,
  `automatic_location`, or `automatic_status` (FR-001; if a prior automatic resolution is still
  cached from a previous session, it remains until the daemon re-resolves or degrades it).
- `wallpaperctl location manual` — sets `mode: Manual`. Does not touch `location` (FR-007/009 —
  this is precisely how "no re-entry required" is satisfied) or the `automatic_*` fields (they're
  simply not consulted while `mode == Manual`).
- **`wallpaperd`** — the *only* writer of `automatic_location` and `automatic_status`, updated
  whenever `portal_location.rs` receives a new resolution, failure, or timeout (research.md R5/
  R6). This is the one field pair in this schema a daemon writes rather than `wallpaperctl` —
  called out explicitly since every other `cosmic-config` entry in this project to date
  (`RendererConfig`, v1 `LocationConfig`) is `wallpaperctl`-write / `wallpaperd`-read only.

## Who must read this schema

- `wallpaperctl location get` — reads and displays `mode`, `location`, `automatic_location`, and
  `automatic_status` (contracts/wallpaperctl-location-cli.md). Daemon-optional, per FR-008/FR-012
  — reads the persisted entry directly, same posture as v1.
- `wallpaperd`'s `scheduler_bridge.rs` — reads the whole entry and calls `effective_location()`
  (data-model.md) to get the `Option<Location>` spec 1's `ValidatedPack::query` needs. **This is
  a required amendment to already-shipped spec 3 code** (plan.md's Cross-Spec Dependency) —
  today it reads v1's `location` field directly.
- `wallpaperd`'s config-watch (spec 3's existing `ConfigWatchSource`, unchanged mechanism) picks
  up any change to this entry — including the daemon's own writes to `automatic_location`/
  `automatic_status` — within the existing 2-second reaction bound (spec 3 FR-007). The daemon
  watching an entry it itself also writes is intentional: an external `wallpaperctl location
  auto` toggle and the daemon's own resolution updates flow through the identical code path,
  keeping there being exactly one way schedules react to a location change.

## Explicitly not in this contract

- The portal session/subscription mechanics themselves (research.md R5) — this is purely the
  persisted-shape contract, not the live D-Bus protocol.
- Any GUI (Future FR-22) — a hypothetical future GUI would be a second writer of `location`/
  `mode`, not a schema redesign, same posture v1 already established.
