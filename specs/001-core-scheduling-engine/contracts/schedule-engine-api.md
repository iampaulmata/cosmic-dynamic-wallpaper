# Contract: `schedule-engine` public API

This crate is a library, not a network service, so its "contract" is the public Rust API
surface that specs 3 (Renderer) and 4 (CLI) will build against. Signatures below are
conceptual (final types/names are an implementation detail) but the shape — inputs, outputs,
error modes, and purity — is the committed contract for this spec.

## Construction & validation

```text
Location::new(latitude: f64, longitude: f64) -> Result<Location, LocationError>

WallpaperPack::validate(images: Vec<PackImage>) -> Result<ValidatedPack, PackError>
```

- Pure, synchronous, no I/O. Never panics on malformed input (constitution Principle VIII).
- `ValidatedPack` is the only type the query function below accepts — callers cannot query
  an unvalidated `Vec<PackImage>`, so an invalid pack cannot reach the scheduling logic.

## Querying schedule state

```text
ValidatedPack::query(
    location: Option<&Location>,   // None for Clock-anchored packs (FR-003); required for Solar-anchored packs
    at: DateTime<Local>,            // the query instant
    crossfade_duration: Duration,   // external parameter, FR-010
) -> ScheduleQueryResult
```

- Deterministic: identical `(pack, location, at, crossfade_duration)` always returns an
  identical `ScheduleQueryResult` (FR-004, SC-003). No caching, no hidden state, no clock
  reads inside the function — `at` is the only source of "now."
- Panics only on a contract violation that validation should have already prevented (e.g.
  `location: None` passed for a `Solar`-anchored pack) — such misuse is a caller bug, not a
  runtime/user-input condition, and is documented as such rather than silently guessed at.

## Next wake-up

```text
ValidatedPack::next_transition_after(
    location: Option<&Location>,
    at: DateTime<Local>,
) -> Option<DateTime<Local>>
```

- Supports the daemon's future idle-wait sleep (constitution Principle VI) without this
  spec implementing the sleep/timer itself. `None` only for the degenerate single-image pack
  (Edge Cases in spec.md), which never transitions.

## Error surface

Both `LocationError` and `PackError` (data-model.md) are `std::error::Error` +
`Debug` + `Display`, with enough context to log without a debugger (constitution Principle
VIII). Downstream specs (CLI, pack loader) are expected to surface these directly to the
user rather than re-wrapping them into a vaguer error.

## Explicitly not in this contract

- Reading/writing `cosmic-config` (spec 2, FR-20).
- Anything Wayland/GPU/render-loop related (specs 3).
- Resolving location automatically via the geolocation portal (spec 6, FR-10) — this
  contract only ever accepts a `Location` the caller already has.
