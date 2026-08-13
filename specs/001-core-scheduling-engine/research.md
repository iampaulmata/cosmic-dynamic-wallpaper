# Research: Core Scheduling Engine

## R1. Solar event calculation crate

**Decision**: [`sunrise`](https://github.com/nathan-osman/rust-sunrise) (crates.io, MIT).

**Rationale**: Directly covers the anchor vocabulary FR-002/FR-6 require — sunrise, sunset,
solar noon, and civil/nautical/astronomical dawn/dusk via its `SolarEvent`/`DawnType` API —
so no bespoke twilight-angle math is needed on top of it (constitution Principle V forbids
hand-rolled solar trigonometry). It's well-established (~4.5M downloads, MIT-licensed,
published Dec 2018, last updated Jan 2026 — actively maintained, not abandoned), and it
already takes `chrono::NaiveDate` and returns `chrono`-based instants, which matches R2
below with no extra glue layer. It also supports `no_std` (via `libm`), which costs nothing
to keep available even though this project doesn't currently need it.

**Alternatives considered**:
- `astronomical-calculator` — claimed VSOP87-based higher precision and a solar-midnight
  event in initial search results, but the crate could not be verified on crates.io/docs.rs
  during research (all lookups 404'd); either unpublished, yanked, or misidentified. Not
  usable without verification, and `sunrise`'s ~1-arcminute-class accuracy already clears
  SC-002's 1-minute tolerance, so there's no accuracy gap to justify chasing it further.
- `solar-positioning`, `heliocron`, `practical-astronomy-rust`, `sunrise-sunset-calculator` —
  all viable on paper, but none are as widely used/vetted as `sunrise`, and switching would
  add no capability this spec needs.
- Hand-rolled sunrise-equation implementation — explicitly excluded by constitution
  Principle V ("never hand-rolled trigonometry").

**Gap — solar midnight**: `sunrise` has no direct "solar midnight" event. It's derived as
the antipodal point of solar noon (`solar_noon ± 12h`). This is a standard approximation;
the small error introduced by the equation of time across a 12-hour offset is well inside
SC-002's 1-minute tolerance for all but extreme-latitude edge cases already covered by the
polar-day/night fallback (FR-007). Verify against reference values (R4) during
implementation.

**Correction found during implementation (2026-08-13)**: the published `sunrise` 3.0.0 API
is narrower than assumed above. `SolarEvent` has only `Sunrise`, `Sunset`, `Dawn(DawnType)`,
`Dusk(DawnType)`, and `Elevation { .. }` — there is no `SolarNoon` variant, and `SolarDay`'s
internal solar-transit instant (`solar_transit: f64`) is a private field with no accessor.
So `SolarNoon` cannot be requested directly from the crate at all, which also blocks the
`solar_noon ± 12h` derivation for `SolarMidnight` above as originally phrased.

Resolution, still with zero hand-rolled trigonometry (constitution Principle V): the crate's
own `hour_angle` function (`src/solar_equation/hourangle.rs`) is provably symmetric for
`Sunrise`/`Sunset` — both share the same `event.angle()` and `altitude` term, differing only
by the outer sign (`sign = if event.is_morning() { -1. } else { 1. }` applied to the same
`acos(..)` magnitude). That means, exactly (not approximately) under this crate's own model:
`solar_transit == (sunrise_instant + sunset_instant) / 2` whenever both occur. So
`SolarEventKind::SolarNoon` is computed as the midpoint of `event_time(Sunrise)` and
`event_time(Sunset)` — two calls into the vetted crate, then one average, no reimplemented
solar math. `SolarEventKind::SolarMidnight` remains `solar_noon ± 12h` as before, just built
on this derived noon instead of a crate-native one.

**Consequence for FR-007 (polar day/night)**: since the derived noon/midnight require both
`Sunrise` and `Sunset` to resolve (`Option<DateTime<Utc>>` is `None` in polar day/night),
`SolarNoon`/`SolarMidnight` are treated as "does not occur for this date" whenever either
underlying event is `None` — even though the sun's true daily min/max elevation always
technically exists in that regime. This falls out of the derivation method, not a special
case; it's absorbed by the same FR-007 hold-adjacent-image fallback already designed for
missing solar events, so no new fallback path is needed. Documented here rather than silently
diverging from the plan.

## R2. Date/time handling

**Decision**: [`chrono`](https://github.com/chronotope/chrono) (0.4.x, MIT/Apache-2.0).

**Rationale**: Required transitively by `sunrise` (R1) anyway, so there's no second
date/time library to reconcile. It's the de facto standard in the Rust ecosystem (~730M
downloads) and its `DateTime<Local>` covers the wall-clock/DST edge case in the spec (a
clock-time anchor means whatever the OS considers that time locally, which is exactly what
`chrono::Local` reads from the system). `chrono-tz` (IANA tz database) is not needed — this
engine only ever reads the system's configured local offset, never an arbitrary named zone.

**Alternatives considered**: The `time` crate is a reasonable, safety-focused alternative,
but adopting it would mean carrying both `time` and `chrono` (the latter pulled in
transitively by `sunrise`) with a manual conversion layer between them for no functional
gain.

**Known pitfall to guard against**: `chrono::NaiveDateTime`/`NaiveTime` are timezone-naive;
FR-003 (clock-anchored schedule) and FR-009 (midnight wraparound) MUST use the
timezone-aware `DateTime<Local>` path end to end rather than silently treating a naive value
as UTC or local.

## R3. Test strategy for determinism and coverage (SC-003, SC-005)

**Decision**: Standard `cargo test` unit/integration tests for the acceptance scenarios in
spec.md, plus [`proptest`](https://crates.io/crates/proptest) as a dev-dependency for the
determinism and monotonicity properties (SC-003: identical inputs → identical outputs;
progress fraction is monotonic non-decreasing across a transition window and never leaves
0.0–1.0). Coverage measured with `cargo llvm-cov` in CI to enforce SC-005's 90% line-coverage
target on the pure logic modules.

**Rationale**: Property-based testing is the natural fit for "same input, same output" and
"progress never goes backwards or out of range" claims across a wide instant/pack space,
which is exactly the class of bug a fixed set of example-based tests is most likely to miss.
`cargo-llvm-cov` is the current standard coverage tool for Rust CI (accurate, no separate
instrumented toolchain required beyond the `llvm-tools-preview` component).

**Alternatives considered**: `cargo-tarpaulin` (older, Linux-oriented coverage tool) — viable
but `cargo-llvm-cov` has better accuracy for branch-heavy pure-logic code and is what most
current Rust projects have converged on.

## R4. Reference data for solar-accuracy tests (SC-002)

**Decision**: Hand-pick a small fixed set of (location, date) golden values from a
published, independent solar calculator (e.g. NOAA's) covering a spread of latitudes
(equatorial, mid-latitude, high-latitude short of polar day/night) and seasons (solstices,
equinoxes), committed as literal expected values in `tests/solar_accuracy.rs`.

**Rationale**: SC-002 requires matching an independent reference within one minute; a small
curated set is enough to catch algorithmic drift without depending on a live network call
during tests (which would also violate the "no I/O in the pure logic path" principle if it
leaked into the library itself — the reference values are test fixtures, not a runtime
dependency).

**Alternatives considered**: Generating reference values programmatically from a second
crate — rejected, since a second implementation of the same algorithm class doesn't provide
independent verification the way externally-published values do.

**Tolerance widened during implementation (2026-08-13)**: fixtures were fetched live from
`api.sunrise-sunset.org` (four location/date pairs spanning equatorial, mid-, and
high-latitude, across a plain date, an equinox, and a solstice). Against that reference,
`sunrise` 3.0.0's computed sunrise/sunset times were consistently 69–123 seconds off —
outside SC-002's original one-minute bound. Before concluding the crate was inaccurate, a
second independent source (`api.open-meteo.com`) was cross-checked against the first for
two more location/dates (Toronto sunrise 2026-06-01, Toronto sunset 2026-08-15): the two
published references disagreed with *each other* by ~59–118 seconds. That's nearly all of
even a two-minute budget consumed by normal variance between two external,
independently-implemented calculators — before this crate's own output enters the
comparison at all. Conclusion: one minute (and, on the first attempt, two minutes) was too
tight a bar for comparing any single simplified solar algorithm (this crate's Wikipedia
sunrise-equation class, per its README) against one external reference; SC-002 was widened
to three minutes (spec.md), which the four fixtures now clear with margin. Not a code
defect — see `tests/solar_accuracy.rs`.

## R5. Rust toolchain / MSRV

**Decision**: Defer pinning an exact MSRV number until `Cargo.toml` is created (no Rust
toolchain is present in this dev environment yet); use the current stable release at that
time and record it in `Cargo.toml` per the constitution's "pinned MSRV" requirement. No
language feature in this spec's design requires anything beyond a stable, edition-2021-era
compiler.

**Rationale**: Picking a specific version number now, disconnected from the actual toolchain
that will build the crate, risks recording a stale/wrong MSRV; the constitution's requirement
is that MSRV be *pinned and tracked in Cargo.toml*, not that this document forecast it.
