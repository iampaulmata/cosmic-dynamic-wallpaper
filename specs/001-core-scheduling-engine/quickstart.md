# Quickstart: Validating the Core Scheduling Engine

This is a pure Rust library crate with no daemon, no Wayland session, and no GPU required to
validate — everything below runs from a terminal with just a Rust toolchain.

## Prerequisites

- A stable Rust toolchain (MSRV pinned in `crates/schedule-engine/Cargo.toml` once created —
  see research.md R5).
- No location services, no display server, no network access.

## Setup

```sh
cd crates/schedule-engine
cargo build
```

## Run the test suite

```sh
cargo test
```

Expected outcome: all tests pass, covering (at minimum) the acceptance scenarios from
spec.md's three user stories:

- Solar-anchored resolution against golden reference values (research.md R4) — User Story 1.
- Clock-anchored, location-free resolution — User Story 2.
- Determinism and next-transition queries — User Story 3.
- Validation rejections: mixed anchor types, exact-instant ties, out-of-range location,
  >64 anchors (FR-001, FR-002a, FR-006, FR-006a).

## Check coverage (SC-005: ≥90% line coverage on pure logic)

```sh
cargo install cargo-llvm-cov --locked   # one-time
rustup component add llvm-tools-preview # one-time
cargo llvm-cov --package schedule-engine --summary-only
```

Confirm the `TOTAL` line coverage is at or above 90%. (As of this writing it's ~97.7%
aggregate; the two source files with any gaps at all — `pack.rs`, `query.rs` — are both
individually above 96%, and the handful of uncovered lines are defensive branches that
are unreachable given `WallpaperPack::validate`'s own invariants, e.g. a `Clock` anchor
inside an already-validated all-`Solar` pack.)

## Manual smoke check

```rust
use schedule_engine::{Location, WallpaperPack, PackImage, TimeAnchor, SolarEventKind};
use chrono::{Local, TimeDelta};

let loc = Location::new(51.5072, -0.1276)?;               // London
let pack = WallpaperPack::validate(vec![
    PackImage::new("dawn.jpg", TimeAnchor::solar(SolarEventKind::Sunrise, None)),
    PackImage::new("noon.jpg", TimeAnchor::solar(SolarEventKind::SolarNoon, None)),
    PackImage::new("dusk.jpg", TimeAnchor::solar(SolarEventKind::Sunset, None)),
])?;

let result = pack.query(Some(&loc), Local::now(), TimeDelta::seconds(60));
println!("{result:?}");
```

Expected outcome: prints a `ScheduleQueryResult` naming whichever of the three images is
active right now in London, with a `transition` field populated only if the query happens to
land inside a 60-second window around sunrise, solar noon, or sunset.

This exact snippet now lives as a runnable doctest in `crates/schedule-engine/src/lib.rs`
(`cargo test --package schedule-engine --doc`) — verified passing 2026-08-13 (T028). Note
it uses `TimeAnchor::solar(..)`/`TimeAnchor::clock(..)` convenience constructors and
`chrono::TimeDelta` (not the originally-drafted `Duration` alias name), added/corrected
during implementation to match this doc rather than the other way around.

## What "done" looks like for this spec

See spec.md's Success Criteria (SC-001–SC-005) and contracts/schedule-engine-api.md for the
committed API surface that specs 2–4 will build against. This spec is complete when the test
suite above is green and the coverage/accuracy/determinism criteria all hold — not when a
daemon or UI exists to consume it (those are later specs).
