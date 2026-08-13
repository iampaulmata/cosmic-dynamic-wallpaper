# Quickstart: Validating Pack Format & Loading

Like spec 1, this is a library crate — no daemon, Wayland session, or GPU needed to validate
it. It does touch the real filesystem and (for the registry) `cosmic-config`'s on-disk store.

## Prerequisites

- A stable Rust toolchain, with spec 1's `schedule-engine` crate present in the same
  workspace (this crate depends on it — plan.md Project Structure).
- No network access needed to *run* the crate, though building it the first time needs
  network access once to fetch the `cosmic-config` git dependency (research.md R4).

## Setup

```sh
cd crates/pack-loader
cargo build
```

## Run the test suite

```sh
cargo test
```

Expected outcome: all tests pass, covering:

- Loading a valid multi-image manifest pack (User Story 1) against fixtures in
  `tests/fixtures/valid_pack/`.
- The zero-config static single-image path (User Story 2).
- Pack-level and per-image scaling resolution (User Story 3).
- Registry persistence and reload across a simulated restart (User Story 4), using a
  `tempfile`-backed `cosmic-config` instance (research.md R6).
- Every rejection case in FR-006/FR-006a: malformed TOML, missing image, path-traversal
  attempt, unreadable image, invalid scaling mode, malformed color, unsupported schema
  version — each against its own fixture under `tests/fixtures/invalid/`.

## Manual smoke check

```rust
use pack_loader::{load_pack, Registry};
use std::path::Path;

// Multi-image pack
let loaded = load_pack(Path::new("./my-pack"))?;
println!("Loaded {:?} with {} images", loaded.name, loaded.pack.len());

// Static single-image pack
let static_pack = load_pack(Path::new("./sunset.jpg"))?;
assert!(static_pack.pack.is_static());

// Registry round-trip — Registry::open() uses the real cosmic-config XDG location;
// see Registry::open_at(path) for a scratch-directory variant used by tests/doctests.
let mut registry = Registry::open()?;
registry.register(loaded.source.clone())?;
assert!(registry.known_packs().iter().any(|e| e.source == loaded.source));
```

Expected outcome: the multi-image pack loads with its declared images; the static pack loads
as a one-image, no-anchor pack; the registered pack shows up in `known_packs()`.

This snippet's shape (adapted to use a scratch directory instead of real paths/config so
it can run unattended) is a real, passing doctest in `crates/pack-loader/src/lib.rs` —
verified passing 2026-08-13 (T037). No drift found between this doc and the actual API
this time (contrast spec 1's quickstart, which did have drift caught at this same step).

## What "done" looks like for this spec

See spec.md's Success Criteria (SC-001–SC-005), research.md R7's 500ms/64-image load bound,
and contracts/pack-loader-api.md for the API surface spec 3 (Renderer) and spec 4 (CLI) will
build against. This spec is complete when the test suite is green and every FR-006/FR-006a
rejection case is covered — not when a daemon or UI exists to point at real packs.
