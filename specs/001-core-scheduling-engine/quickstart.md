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
cargo llvm-cov --html
```

Open the generated report and confirm `src/solar.rs`, `src/pack.rs`, `src/location.rs`, and
`src/query.rs` each meet the 90% threshold.

## Manual smoke check

```rust
use schedule_engine::{Location, WallpaperPack, PackImage, TimeAnchor, SolarEventKind};
use chrono::{Local, Duration};

let loc = Location::new(51.5072, -0.1276)?;               // London
let pack = WallpaperPack::validate(vec![
    PackImage::new("dawn.jpg", TimeAnchor::solar(SolarEventKind::Sunrise, None)),
    PackImage::new("noon.jpg", TimeAnchor::solar(SolarEventKind::SolarNoon, None)),
    PackImage::new("dusk.jpg", TimeAnchor::solar(SolarEventKind::Sunset, None)),
])?;

let result = pack.query(Some(&loc), Local::now(), Duration::seconds(60));
println!("{result:?}");
```

Expected outcome: prints a `ScheduleQueryResult` naming whichever of the three images is
active right now in London, with a `transition` field populated only if the query happens to
land inside a 60-second window around sunrise, solar noon, or sunset.

## What "done" looks like for this spec

See spec.md's Success Criteria (SC-001–SC-005) and contracts/schedule-engine-api.md for the
committed API surface that specs 2–4 will build against. This spec is complete when the test
suite above is green and the coverage/accuracy/determinism criteria all hold — not when a
daemon or UI exists to consume it (those are later specs).
