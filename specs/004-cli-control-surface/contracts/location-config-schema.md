# Contract: `LocationConfig` `cosmic-config` schema

⚠️ **Cross-spec dependency**: this schema is written by this spec's `wallpaperctl location`
commands, but must be *read* by spec 3's `scheduler_bridge.rs` to supply `location` to spec
1's `ValidatedPack::query` for solar-anchored packs. Spec 3, as currently tasked, does not read
this entry — see plan.md's Cross-Spec Dependencies section. This file documents the shape spec
3 needs to consume, independent of when that reading side gets implemented.

## Schema (RON, `cosmic-config`-managed)

```text
LocationConfig(
    schema_version: 1,
    location: Some(Location(
        latitude: 45.5019,
        longitude: -73.5674,
    )),
)
```

- `location: None` means no manual location has been set — only clock-anchored packs (spec 1
  FR-11) are usable; a solar-anchored pack assigned to an output degrades per spec 1/3's
  existing failure-containment posture (spec.md US3 Scenario 4), not a new error this schema
  introduces.
- `latitude`/`longitude` are validated against spec 1's `Location::new` rule *before* being
  written — an invalid value is never persisted (spec.md US3 Scenario 3, data-model.md
  Validation rule).

## Who writes this schema

`wallpaperctl location set|clear` (this spec, FR-008) — the only writer. No daemon involvement
required to write it (constitution Principle IV; config-only commands work without a running
`wallpaperd`, spec.md FR-011).

## Who must read this schema

Spec 3's `scheduler_bridge.rs`, watched the same way it already watches `RendererConfig`
(spec 3 research.md R4) — a location change should be picked up within spec 3's existing
2-second reaction bound (spec 3 FR-007), the same as any other schedule-relevant setting
change, per spec.md's Edge Cases. **This is not implemented as of this spec's authoring** —
flagged explicitly in plan.md rather than assumed.

## Explicitly not in this contract

- Automatic, portal-based location (PRD FR-10) — a distinct, separate concern owned by spec 6,
  which may eventually write a *different* source of truth (or the same schema — an open
  question for spec 6 to resolve, not this one).
- Any UI for entering a location beyond the CLI (`wallpaperctl location set`) — a future GUI
  (FR-22) would be a second writer of this same schema, not a redesign of it.
