# Contract: `LocationConfig` `cosmic-config` schema, v3

Supersedes `specs/006-location-portal-integration/contracts/location-config-schema-v2.md` (v2).
Now defined once in `crates/wallpaper-ipc` (contracts/wallpaper-ipc-crate.md) rather than
independently in `crates/renderer` and `crates/wallpaperctl`.

## Schema (RON, `cosmic-config`-managed)

```text
LocationConfigEntry(
    schema_version: 3,
    mode: IpGeolocation,
    location: None,
    automatic_location: None,
    automatic_status: Unresolved,
    ip_location: Some(Location(
        latitude: 45.5,
        longitude: -73.6,
    )),
    ip_status: Resolved,
)
```

A degraded example (STUN unreachable — an offline laptop):

```text
LocationConfigEntry(
    schema_version: 3,
    mode: IpGeolocation,
    location: None,
    automatic_location: None,
    automatic_status: Unresolved,
    ip_location: None,
    ip_status: Unavailable(reason: "public IP discovery failed: STUN request timed out"),
)
```

## Migration from v2

No hand-written migration function — data-model.md's Migration section has the full mechanism
(reusing spec 6 research.md R7's verified `cosmic-config` per-key fallback). `automatic_status`'s
on-disk key name is unchanged despite the Rust type rename (`AutomaticStatus` → `ResolutionStatus`,
research.md R9) — the rename is source-level only, not a schema change in its own right.

## Who writes this schema

- `wallpaperctl location auto|manual|set|clear` (spec 6, unchanged) and the GUI's location page
  (spec.md FR-004) — both via `wallpaper-ipc`'s shared `LocationConfigEntry`, never independently.
- `wallpaperctl location ip` / the GUI's IP-geolocation toggle (**new**, this spec) — sets
  `mode: IpGeolocation` only, same posture as spec 6's `location auto`.
- **`wallpaperd`** — writes `automatic_location`/`automatic_status` (unchanged, spec 6) and now
  also `ip_location`/`ip_status` (new, `ip_geolocation.rs`), the only daemon-written fields, same
  posture spec 6 already established for the portal fields.

## Who must read this schema

- `wallpaperctl location get` / the GUI's location page — display `mode`, all three status/value
  pairs, and the single effective value (`effective_location()`).
- `wallpaperd`'s `scheduler_bridge.rs` — via `effective_location()` (data-model.md), unchanged
  call site from spec 6, now covering the third mode automatically since the function itself
  gained the new match arm.

## Explicitly not in this contract

- The STUN/`.mmdb` resolution mechanics themselves (research.md R3/R4) — purely the persisted-
  shape contract.
- Any change to spec 6's portal-mode behavior — `Automatic` mode's fields and semantics are
  completely unchanged by this spec.
