# Data Model: Core Scheduling Engine

All types below are pure data plus validation logic (constitution Principle V / FR-008) —
no I/O, no rendering, no persistence. Persistence of these shapes (`cosmic-config`, FR-20)
is spec 2's responsibility; this engine only accepts already-loaded values as arguments.

## Location

Manually-entered coordinates (FR-9 baseline path).

| Field | Type | Constraint |
|---|---|---|
| `latitude` | `f64` | Must be in `[-90.0, 90.0]`, finite (not NaN/±inf) — FR-002a |
| `longitude` | `f64` | Must be in `[-180.0, 180.0]`, finite — FR-002a |

**Validation**: `Location::new(lat, lon) -> Result<Location, LocationError>`. Constructing an
out-of-range or non-finite value returns `LocationError::OutOfRange` /
`LocationError::NotFinite` rather than panicking (FR-002a, constitution Principle VIII).

## SolarEventKind

Enum of the eight named solar events FR-6 recognizes: `Sunrise`, `Sunset`, `SolarNoon`,
`SolarMidnight`, `CivilDawn`, `CivilDusk`, `AstronomicalDawn`, `AstronomicalDusk`.

`SolarMidnight` is derived (research.md R1: `solar_noon ± 12h`); the other seven map
directly onto the `sunrise` crate's `SolarEvent`/`DawnType` variants.

## TimeAnchor

A tagged union — exactly one of:

- `Solar { event: SolarEventKind, offset: Option<SignedDuration> }` — e.g. `sunset - 30m`.
- `Clock(NaiveTime)` — an absolute `HH:MM` (FR-11).

A single `WallpaperPack` MUST use only one variant across all its anchors (FR-6, FR-001).

## PackImage

| Field | Type | Constraint |
|---|---|---|
| `id` | opaque image identifier (deferred to spec 2's pack-loading data model — this spec treats it as an opaque, `Eq`-comparable handle, not a filesystem path) | Unique within the pack |
| `anchor` | `TimeAnchor` | See above |

## WallpaperPack (validated form)

An ordered `Vec<PackImage>` plus the derived anchor-type it was validated against.

**Validation rules** (`WallpaperPack::validate(images) -> Result<ValidatedPack, PackError>`):

1. At least one image (FR-1 / static-mode note in Assumptions: exactly one image with no
   anchor is the degenerate static-mode case, represented as a one-element pack whose single
   anchor is treated as always-active rather than a distinct type).
2. At most 64 images/anchors (FR-001) — `PackError::TooManyAnchors`.
3. All anchors are the same `TimeAnchor` variant (FR-6, FR-006) — `PackError::MixedAnchorTypes`.
4. No two anchors resolve to the exact same instant (FR-006a) — `PackError::DuplicateInstant`.
   - For `Clock` packs this is a static, one-time check at validation (clock times don't
     move).
   - For `Solar` packs, exact-instant equality can only be checked against a resolved date
     (solar times shift day to day), so this check re-runs whenever the engine resolves a
     pack for a given date, not only once at pack-load time. A pack that happens to collide
     only on rare dates (e.g. a specific offset landing on an equinox) is still caught the
     day it would matter, not silently mis-scheduled.
5. Image identifiers are unique within the pack — `PackError::DuplicateImageId`.

## ScheduleQueryResult

The answer to "what's active right now" (FR-004, FR-013, User Story 3).

| Field | Type | Notes |
|---|---|---|
| `active_before` | image id | The most-recently-passed anchor's image (always present) |
| `transition` | `Option<TransitionState>` | `Some` only when the query instant falls inside a crossfade window |
| `next_transition_at` | `Option<DateTime<Local>>` | `None` only for the degenerate single-image/static pack (Edge Cases) |

## TransitionState

| Field | Type | Constraint |
|---|---|---|
| `outgoing` | image id | The image fading out |
| `incoming` | image id | The image fading in |
| `progress` | `f64` | `0.0 <= progress < 1.0`, strictly increasing over the window (FR-004) |

## Error types

- `LocationError` — `OutOfRange`, `NotFinite` (FR-002a).
- `PackError` — `Empty`, `TooManyAnchors`, `MixedAnchorTypes`, `DuplicateInstant`,
  `DuplicateImageId` (FR-001, FR-006, FR-006a).

All error types implement `std::error::Error` and carry enough context (which field, which
value) to log without a debugger, per constitution Principle VIII. None of this module's
public functions may `unwrap()`/`expect()` outside `#[cfg(test)]` code.
