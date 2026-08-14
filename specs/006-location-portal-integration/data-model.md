# Data Model: Location Portal Integration

Types below extend spec 4's `LocationConfig` `cosmic-config` schema (v1 →v2) and are shared by
`crates/renderer` (read side, `config.rs`) and `crates/wallpaperctl` (write side, `config.rs`) —
same dual-crate-shared-shape pattern spec 3/4's `RendererConfig` already established. Spec 1's
`Location` (schedule-engine) is reused verbatim, not redefined.

## LocationMode

| Variant | Notes |
|---|---|
| `Manual` | Default. Scheduling uses `location` directly (spec 4's original, unchanged behavior). |
| `Automatic` | Scheduling uses `effective_location()`'s resolution below rather than `location` directly. |

## AutomaticStatus

Surfaces spec.md's **Location Availability Status** Key Entity without requiring a live daemon
query (FR-008 requires this to work "at any time," including with no daemon running — consistent
with the Clarification to persist the resolved value).

| Variant | Notes |
|---|---|
| `Unresolved` | Automatic mode was just enabled; no resolution attempt has completed yet. |
| `Resolved` | `automatic_location` holds a value from a successful portal resolution. |
| `Unavailable { reason: String }` | The most recent resolution attempt failed (portal absent, backend absent, permission declined, timeout, or a mid-session error) — `reason` is a short, specific string for display (e.g. `"Location services disabled"`, verbatim from research.md R1 where the portal supplies one), not a generic catch-all. |

Only meaningful when `mode == Automatic`; ignored (but still persisted as whatever it last was)
when `mode == Manual`, so re-enabling automatic mode later doesn't lose the last-known status.

## LocationConfigEntry (persisted via `cosmic-config`, v2 — supersedes spec 4's v1)

| Field | Type | Notes |
|---|---|---|
| `schema_version` | `u32` | `2` (was `1` in spec 4) — versioned independently of other specs' schemas (constitution Principle X) |
| `mode` | `LocationMode` | Default `Manual` (spec.md FR-002 — automatic is opt-in, never implicit) |
| `location` | `Option<Location>` | The manual value — spec 4's original field, **unchanged meaning**. Never cleared by switching to `Automatic` mode or by a successful automatic resolution (spec.md FR-007) |
| `automatic_location` | `Option<Location>` | The last successfully-resolved automatic value, persisted per spec.md's Clarifications so a restarted daemon has an immediate value (FR-010) |
| `automatic_status` | `AutomaticStatus` | Default `Unresolved` |

**Validation rule** (unchanged from v1): any value assigned to `location` or
`automatic_location` MUST pass spec 1's `Location::new(latitude, longitude)` before being
persisted — automatic resolutions that somehow fail this (malformed portal response) are treated
as a resolution failure (`AutomaticStatus::Unavailable`), never partially written.

### Migration (v1 → v2, constitution Principle X)

**No hand-written migration function is required** (research.md R7, verified against
`cosmic-config`'s actual source rather than assumed) — `cosmic-config`'s versioned `Config`
already falls back key-by-key to the previous version's directory when a key is missing from the
current one. Concretely, bumping `#[version = 2]` and giving `LocationConfigEntry` this
`Default`:

```text
LocationConfigEntry {
    schema_version: 2,
    mode: LocationMode::Manual,          // Default
    location: None,                       // Default — overwritten automatically by the
                                           // v1 value below if one exists
    automatic_location: None,             // Default — v2-only field, nothing to fall back to
    automatic_status: AutomaticStatus::Unresolved,  // Default
}
```

produces exactly the intended migration: `location` is read from the v2 store first and
**automatically** falls back to the existing v1 value (same field name and type in both
versions) via `cosmic-config`'s built-in `previous`-version chain; `mode`/`automatic_location`/
`automatic_status` don't exist in v1 at all, so they simply take their `Default` value. Every
existing user's config becomes "manual mode, same location value as before, automatic never
attempted" — behaviorally identical to today until a user explicitly opts into automatic mode
(FR-002) — satisfying constitution Principle X's "MUST NOT silently misinterpret an old-format
value" by construction, with `Self::load()`'s already-established
`get_entry(config).unwrap_or_else(|(_errors, default)| default)` pattern (unchanged from v1)
tolerating any per-key read error the same way it always has.

## `effective_location()` — the resolution rule (new pure function, unit-tested)

The single place scheduling code (spec 3's `scheduler_bridge.rs`, per plan.md's Cross-Spec
Dependency) asks "what location, if any, should solar-anchored packs use right now?" Pure
function of `LocationConfigEntry`, no I/O:

```text
fn effective_location(entry: &LocationConfigEntry) -> Option<Location> {
    match entry.mode {
        LocationMode::Manual => entry.location,
        LocationMode::Automatic => entry.automatic_location.or(entry.location),
    }
}
```

- `Manual` mode: identical to spec 4's original behavior — `location` or nothing.
- `Automatic` mode with a resolved value: use it (spec.md US1).
- `Automatic` mode, never resolved or currently unavailable: fall back to `location` if a manual
  value happens to also be stored (spec.md FR-005's "fall back to the existing manual location if
  one is stored"), else `None` — which spec 1/3's existing no-location degrade contract already
  handles (spec.md FR-005's second fallback tier). **No new failure mode**: everything downstream
  of this function is exactly spec 1/3's pre-existing `Option<Location>` handling.

This function is the entire scope of this spec's interaction with spec 1's scheduling — it
supplies an input, it never touches solar math itself (constitution Principle V untouched).

## PortalLocationReading (transient, `crates/renderer` only — not persisted directly)

The shape `portal_location.rs` receives from `ashpd`'s `LocationUpdated` signal before it's
validated and written into `automatic_location`.

| Field | Type | Notes |
|---|---|---|
| `latitude` | `f64` | From `ashpd::desktop::location::Location::latitude()` |
| `longitude` | `f64` | From `ashpd::desktop::location::Location::longitude()` |
| `accuracy` | `f64` | Radius in meters, per the portal's own reply shape — logged for diagnostics, not persisted (spec.md's schema has no accuracy field; not needed once validated into spec 1's `Location`) |

Converted to spec 1's `Location` via `Location::new(latitude, longitude)` immediately on receipt
— an out-of-range value from a misbehaving backend is treated as a resolution failure
(`AutomaticStatus::Unavailable`), never persisted, matching the same validate-before-write rule
applied to manual entry since spec 4.

## CliError additions (`crates/wallpaperctl`)

No new variants required — `location auto`/`location manual` (FR-001/FR-007/FR-009) are pure
`cosmic-config` writes with no new failure mode beyond the existing `ConfigError` spec 4 already
defines. `location get`'s extended output (mode + status) is a read, not a fallible write.
