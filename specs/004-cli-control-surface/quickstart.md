# Quickstart: Validating the CLI Control Surface

Most of this spec is headless-testable like specs 1–2 (config-only commands need no daemon or
compositor at all). The two D-Bus-dependent commands need a running `wallpaperd` — which,
as of this spec's authoring, requires spec 3 to be both implemented *and* amended per
plan.md's Cross-Spec Dependencies before they can be validated end-to-end.

## Prerequisites

- A stable Rust toolchain, with spec 1's `schedule-engine` and spec 2's `pack-loader` crates
  present in the same workspace (this crate depends on both — plan.md Project Structure).
- No compositor/GPU/Wayland session needed for register/list/remove/assign/location.
- For `query`/`reevaluate` only: a running `wallpaperd` implementing
  contracts/wallpaperd-dbus-interface.md, and a session D-Bus bus (present in any real desktop
  login session).

## Run the automated test suite

```sh
cd crates/wallpaperctl
cargo test
```

Expected outcome: `command_parsing.rs`, `register_list_remove.rs`, and `assign_location.rs`
all pass, covering:

- Argument parsing/validation for every subcommand (research.md R1), independent of any real
  pack, output, or config store.
- Registering a valid pack directory or static image and confirming it's subsequently listed
  (spec.md US1), against a `tempfile`-backed real `pack-loader` `Registry` (research.md R6,
  spec 2 R6 precedent).
- Idempotent re-registration of an already-known source (spec.md US1 Scenario 3).
- Removing a known pack and confirming it no longer lists (spec.md US7).
- Assigning a pack to an output (and enabling "same pack everywhere") and confirming the write
  lands correctly in a `tempfile`-backed `cosmic-config` instance matching spec 3's
  `RendererConfig` shape (spec.md US2).
- Setting, viewing, and clearing a location, including rejecting an invalid latitude/longitude
  using spec 1's own validation rule (spec.md US3).
- Every failure path (unregistered pack, unknown output, invalid location) exiting non-zero
  with a specific message (FR-012, SC-002).

This does **not** exercise the D-Bus-dependent commands — see the manual check below.

## Manual smoke check (requires a running `wallpaperd`)

Only possible once spec 3 is implemented and amended per plan.md's Cross-Spec Dependencies
(location config consumption, D-Bus service). Until then, this section documents the intended
flow rather than something currently runnable end-to-end.

```sh
# 1. Register a pack and set a location (no daemon needed — config-only, FR-011)
wallpaperctl register ./my-pack
wallpaperctl location set 45.5019 -73.5674

# 2. Start wallpaperd (spec 3), then discover outputs (this needs a running daemon — FR-005/FR-011)
wallpaperctl list outputs

# 3. Assign the pack (still works even if wallpaperd isn't running, but here it already is)
wallpaperctl assign --output DP-3 ./my-pack

# 4. Confirm live state
wallpaperctl query --output DP-3
wallpaperctl reevaluate --output DP-3
```

Expected outcome: step 1 succeeds with no daemon running at all (config-only, FR-011). Step 2
requires `wallpaperd` to already be running — it fails with a clear "daemon unreachable" error
otherwise (FR-005/FR-011, spec.md US5 Scenario 4). Step 3 (`assign`) works regardless of
whether the daemon happens to be running at that moment (FR-007) — it's shown here after step
2 only because that's the natural order for discovering a real output name to use. Step 4
reflects the same state a running `wallpaperd` is actually rendering (spec 3's own quickstart
scenarios) — the CLI's query result should match spec 3's manual smoke check output exactly.

## What "done" looks like for this spec

See spec.md's Success Criteria (SC-001–SC-005) and the three contracts (CLI surface, location
schema, D-Bus interface). This spec's own test suite is complete when `cargo test` is green
for every config-only command; **full SC-001 validation** ("a solar-anchored pack visibly
scheduled using only CLI commands") additionally requires spec 3's Cross-Spec Dependencies to
be resolved — that gap is spec 3's implementation work, not something this spec's own test
suite can close on its own.
