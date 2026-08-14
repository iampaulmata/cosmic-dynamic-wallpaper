# Data Model: V1 Completion

## LocationConfigEntry v3 (extends spec 6's v2, `crates/wallpaper-ipc`)

Now the authoritative, single-source-of-truth definition (research.md R2) — `crates/renderer` and
`crates/wallpaperctl` both depend on this type rather than each defining their own.

| Field | Type | Notes |
|---|---|---|
| `schema_version` | `u32` | `3` (was `2` in spec 6) |
| `mode` | `LocationMode` | Gains a third variant: `Manual` \| `Automatic` \| `IpGeolocation` |
| `location` | `Option<Location>` | Unchanged — the manual value (spec 4) |
| `automatic_location` | `Option<Location>` | Unchanged — the portal-resolved value (spec 6) |
| `automatic_status` | `ResolutionStatus` | **Renamed** from spec 6's `AutomaticStatus` (research.md R9) — same shape, mode-agnostic name |
| `ip_location` | `Option<Location>` | NEW — the most recently resolved IP-geolocation value |
| `ip_status` | `ResolutionStatus` | NEW — reuses the renamed enum; default `Unresolved` |

### ResolutionStatus (renamed from spec 6's `AutomaticStatus`)

| Variant | Notes |
|---|---|
| `Unresolved` | No resolution attempt has completed yet for this mode. |
| `Resolved` | The corresponding `*_location` field holds a value from a successful resolution. |
| `Unavailable { reason: String }` | The most recent attempt failed — `reason` is specific (e.g. `"Location services disabled"` for the portal, or `"public IP discovery failed: STUN request timed out"` for IP-geolocation), never a generic catch-all. |

### Migration (v2 → v3)

No hand-written migration function, per the mechanism spec 6 research.md R7 already verified
against `cosmic-config`'s actual source: `mode`/`location`/`automatic_location`/
`automatic_status` carry over unchanged (same field names/types — `automatic_status`'s *type*
renamed but its on-disk key name and shape are unchanged, so existing v2 data reads correctly
into the renamed `ResolutionStatus` type); `ip_location`/`ip_status` are new in v3 and simply take
their `Default` (`None`, `Unresolved`).

### `effective_location()` (extends spec 6's version, `crates/wallpaper-ipc`)

```text
fn effective_location(entry: &LocationConfigEntry) -> Option<Location> {
    match entry.mode {
        LocationMode::Manual => entry.location,
        LocationMode::Automatic => entry.automatic_location.or(entry.location),
        LocationMode::IpGeolocation => entry.ip_location.or(entry.location),
    }
}
```

Same fallback posture as spec 6: an unresolved/unavailable non-manual mode falls back to a stored
manual value if present, else `None` — no new failure mode, consistent with spec.md FR-015.

## RendererConfig crossfade duration (extends spec 3, `crates/wallpaper-ipc`)

| Field | Type | Notes |
|---|---|---|
| `crossfade_duration_secs` | `u32` | NEW. Defaults to `45` — spec 3's existing `surface.rs::CROSSFADE_DURATION` constant value, so upgrading changes nothing until a user visits the GUI's Crossfade page (FR-006). |

**Real finding (plan.md Constitution Check finding 3)**: this field does not exist today —
`crates/renderer/src/surface.rs`'s `CROSSFADE_DURATION` is a plain Rust constant, not read from
any config, despite a stray doc comment in `crossfade.rs` claiming it's "configurable." This
spec's GUI work is what actually makes it true: `surface.rs`'s three call sites reading the
constant directly are amended to read `RendererConfig.crossfade_duration_secs` instead (same
live-config-watch mechanism spec 3 already uses for other fields — no new watch infrastructure).

**Migration**: same no-hand-written-migration pattern — new field, safe `Default`, carried
automatically by `cosmic-config`'s per-key fallback (research.md, spec 6 R7).

## PackRegistryEntry origin (extends spec 2, `crates/pack-loader`)

| Field | Type | Notes |
|---|---|---|
| `origin` | `PackOrigin` | NEW. `User` (default) \| `Package`. |

### PackOrigin

| Variant | Notes |
|---|---|
| `User` | Registered by a person via `wallpaperctl register` or the GUI (spec.md FR-011 — never overridden by a starter pack). |
| `Package` | Registered by `postinst` (spec 5) at install time — the starter pack (spec.md FR-008). |

### Removed-starter-pack tracking

A `Package`-origin entry's removal (via `wallpaperctl remove` or the GUI) writes a small marker —
reusing the registry's own persistence rather than a new store: the entry is deleted as normal
(spec 2's existing `Registry::remove`), and a new, separate small `RemovedStarterPacks` registry
entry (a `Vec<PackSource>`, `cosmic-config`, own tiny schema) records which starter-pack sources
were explicitly removed. `postinst` checks this list before re-registering on upgrade (spec.md
FR-010) — a persisted "don't reinstall this" marker, the same shape Debian's own conffile
handling uses conceptually (respecting a user's deliberate removal across upgrades).

**Migration**: brand-new schema, `schema_version: 1`, no migration concern.

## IP-Geolocation resolution (transient, `crates/renderer/src/ip_geolocation.rs` — not itself persisted)

| Field | Type | Notes |
|---|---|---|
| `public_ip` | `std::net::IpAddr` | Discovered via STUN (research.md R4), cached in-memory only with a 24-hour TTL — never written to `cosmic-config`; only the *result* of the subsequent database lookup (`ip_location`/`ip_status` above) is persisted. |
| `resolved_at` | `std::time::Instant` | Cache timestamp, in-memory only. |

The `.mmdb` database itself (research.md R3) ships as a static asset alongside the binary
(packaging detail, not a data-model concern) — read-only at runtime, never written.

## GUI (`crates/wallpaper-settings`) — no new persisted entity

Every field the GUI displays or edits is a read/write of one of: spec 2's pack registry, the
`LocationConfigEntry` above, or spec 3's `RendererConfig`/`OutputAssignment` (via
`wallpaper-ipc`) — the GUI introduces no persisted shape of its own, satisfying spec.md's Key
Entities note verbatim.

## Mock hotplug harness (`crates/renderer/tests/hotplug_mock.rs`) — test-only, not persisted

Not a data entity — a `wayland-server`-backed test double (research.md R7) driving the real SCTK
client code under test with synthetic global-add/remove/geometry events. No schema of its own;
documented here only so its existence is traceable from data-model.md's usual "what exists" scan.
