# schedule-engine

A pure, deterministic Rust library that answers one question: **given a wallpaper pack and
an instant in time, which image is active, and how far through a crossfade are we?**

It is the foundation crate of the Cosmic Dynamic Wallpaper daemon (specs 2–4 in
`docs/PRD.md`'s breakdown build on top of it) and is fully unit-testable standalone — no
Wayland session, GPU, or network access required.

## Scope

- Resolve a **solar-anchored** pack (images tied to sunrise, sunset, solar noon, solar
  midnight, civil/astronomical dawn or dusk, each with an optional signed offset) against
  a manually-entered latitude/longitude, using the vetted [`sunrise`](https://crates.io/crates/sunrise)
  crate — never hand-rolled solar trigonometry (project constitution Principle V).
- Resolve a **clock-anchored** pack (images tied to an absolute `HH:MM`) using only
  wall-clock time, with zero location input required, requested, or read anywhere on that
  path (FR-003, FR-11).
- Report the active image, any in-progress crossfade (outgoing/incoming image ids plus a
  `0.0..1.0` progress fraction), and the next transition instant — deterministically, as a
  pure function of its arguments (FR-004, FR-005, FR-008, SC-003).
- Validate packs at construction time: anchor-count cap, uniform anchor type, no
  duplicate image ids, no two anchors resolving to the exact same instant (FR-001,
  FR-006, FR-006a).

## Explicitly not in scope

- **Persistence.** Packs, location, and any other configuration are constructed
  in-memory by the caller; reading/writing `cosmic-config` is spec 2's job (FR-20).
- **Rendering.** No Wayland, GPU, or layer-shell code anywhere in this crate; it only
  produces the data (active/outgoing/incoming image ids, a progress fraction) that a
  renderer would consume — that's spec 3.
- **Automatic location.** Only the manually-entered latitude/longitude path (FR-9) is
  implemented here. Resolving location via the `org.freedesktop.portal.Location` portal
  (FR-10) is a separate, later spec.
- **The idle-wait sleep/timer itself.** `next_transition_after` supplies the *duration* a
  daemon's idle-wait state needs to sleep for (constitution Principle VI); the calloop
  timer/state machine that actually sleeps is the daemon's (spec 3's) responsibility.

## Public API

See `contracts/schedule-engine-api.md` in `specs/001-core-scheduling-engine/` for the
committed contract. In short:

```rust,ignore
let location = Location::new(latitude, longitude)?;               // FR-002a
let pack = WallpaperPack::validate(images)?;                       // FR-001/006/006a

let result = pack.query(Some(&location), at, crossfade_duration);  // FR-004
let next_wakeup = pack.next_transition_after(Some(&location), at); // FR-005
```

`location` is `None` for clock-anchored packs (FR-003); passing `None` for a
solar-anchored pack is a caller contract violation (the anchor kind is already known from
`validate`), not a runtime/user-input condition, so it panics rather than returning a
`Result` — documented in `contracts/schedule-engine-api.md`.

Solar packs also expose `ValidatedPack::check_solar_duplicate_instant(location, date)`,
a separate fallible check callers invoke per-date (FR-006a) — solar event instants shift
day to day, so unlike clock packs this can't be a one-time check at `validate` time, and
`query`/`next_transition_after` stay intentionally infallible.

## Testing

```sh
cargo test --package schedule-engine   # unit + acceptance-scenario + property tests
cargo llvm-cov --package schedule-engine --summary-only   # SC-005: >=90% line coverage
```

- `tests/solar_accuracy.rs` — golden-reference accuracy tests (SC-002) against an
  independently-published solar calculator, four location/date pairs spanning
  equatorial, mid-, and high-latitude.
- `tests/schedule_resolution.rs` — spec.md's acceptance scenarios (US1–US3) as
  integration tests, plus the documented edge cases (polar day/night, DST, overlapping
  crossfade windows, duplicate-instant rejection).
- `tests/determinism.rs` — `proptest`-based determinism and progress-monotonicity
  properties (SC-003).

See `quickstart.md` in `specs/001-core-scheduling-engine/` for a runnable end-to-end
smoke-test walkthrough.
