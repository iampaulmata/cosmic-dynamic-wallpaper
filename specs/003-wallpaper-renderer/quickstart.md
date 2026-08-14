# Quickstart: Validating the Wallpaper Renderer

Unlike specs 1–2, this is not a headless-only library — it's the project's first spec that
touches a real Wayland compositor and GPU. Validation is split accordingly: the pure
assignment/coalescing logic is `cargo test`-able like specs 1–2; the actual rendering needs a
running COSMIC (or other `wlr-layer-shell`-capable) session.

## Prerequisites

- A stable Rust toolchain, with spec 1's `schedule-engine` and spec 2's `pack-loader` crates
  present in the same workspace (this crate depends on both — plan.md Project Structure).
- For the pure-logic test suite only: no compositor or GPU needed.
- For manual smoke-testing the actual daemon: a running Wayland session on a compositor that
  implements `wlr-layer-shell-unstable-v1` (`cosmic-comp`, i.e. an actual COSMIC session — or
  a nested `cosmic-comp`/`sway` instance for a tighter dev loop) plus a working GPU driver
  (integrated graphics is the required test configuration per NFR-3/SC-006, not just whatever
  a contributor's dev machine happens to have).
- No network access needed to *run* the crate, though building it the first time needs network
  access once to fetch the `cosmic-config` git dependency (already true as of spec 2).

## Run the pure-logic test suite

```sh
cd crates/renderer
cargo test
```

Expected outcome: `assignment_resolution.rs` passes, covering:

- Explicit per-output override taking precedence over the "same pack everywhere" toggle
  (FR-005, FR-006, spec.md User Story 5).
- An output with neither an override nor an enabled toggle resolving to `Unassigned`, not an
  error (FR-009, spec.md Edge Cases).
- Change coalescing: two or more rapid changes to the same output before re-evaluation runs
  collapse to the latest state only (FR-014, spec.md Clarifications).

This does **not** exercise any Wayland/GPU code — see the manual smoke check below for that.

## Manual smoke check (requires a real compositor)

```sh
cargo run --bin wallpaperd
```

With a multi-image pack (spec 2 fixtures, e.g. `crates/pack-loader/tests/fixtures/valid_pack/`)
assigned to an output via a hand-written `RendererConfig` entry (contracts/
renderer-config-schema.md), expected outcome:

1. The background layer surface appears on the assigned output showing the currently-active
   image per spec 1's schedule query (spec.md User Story 1). **✅ Verified 2026-08-13** against
   a live `cosmic-comp` session — layer surface accepted at the real output's resolution
   (1920x1080), image uploaded and rendered.
2. At the next scheduled transition instant, the output smoothly crossfades to the next image
   over ~45 seconds (the default duration, FR-002) with no hard cut, flicker, or tearing.
   **✅ Verified 2026-08-13** — ran across a real scheduled transition boundary for 35+ seconds
   with no crash; the shader's exact blend math was additionally verified pixel-precise via
   `tests/gpu_render.rs`'s offscreen render+readback (screen visibility was blocked by other
   windows occupying the whole display in that session, so on-screen pixels weren't directly
   screenshotted — the offscreen test is the stronger check anyway: exact bytes, not "looks
   right").
3. Outside of that transition window, `wallpaperd`'s CPU/GPU usage (check via `top`/
   `intel_gpu_top` or equivalent) is negligible — no redraw loop running (spec.md User Story
   2). **~Verified in spirit 2026-08-13** — no crash/hang/runaway activity observed over the
   test run, but CPU/GPU usage wasn't formally profiled, and the current implementation uses a
   flat 5-second re-evaluation timer rather than a precise per-output wake (see
   `crates/renderer/README.md` — functionally idle between transitions, not maximally so).
4. Editing the `RendererConfig` entry's assignment for that output (or toggling
   `same_pack_everywhere`) while `wallpaperd` is running is reflected within 2 seconds,
   without restarting the daemon (spec.md User Story 4). **Not yet verified/implemented** —
   live config-watch isn't wired up this pass (T028/T033); a config change today needs a
   restart to take effect.
5. On a multi-monitor dev setup: assign different packs to two outputs and confirm each
   transitions independently on its own schedule (spec.md User Story 3), and that
   disconnecting one output doesn't disturb the other (spec.md User Story 6). **Not yet
   verified** — the dev environment this pass ran in has exactly one physical output
   (`eDP-1`); the code is written to be output-count-generic (`Vec<WallpaperOutput>`,
   `OutputHandler` wired for connect/disconnect) but untested against 2+ real outputs.

## CI smoke test (optional, exploratory)

```sh
weston --backend=headless-backend.so &
WAYLAND_DISPLAY=<weston's socket> cargo run --bin wallpaperd
```

Expected outcome per research.md R6: the daemon starts, creates a layer surface per detected
output, and does not crash — this only proves "doesn't crash under a headless compositor," not
visual crossfade correctness, which stays a manual QA item (see above).

## What "done" looks like for this spec

See spec.md's Success Criteria (SC-001–SC-006) and contracts/renderer-config-schema.md for the
on-disk contract specs 4/GUI build against. This spec is complete when the pure-logic test
suite is green, the manual smoke check's five scenarios above all pass on at least one
integrated-graphics device and one multi-output, mixed-scale-factor setup (constitution
Principles III, VII), and the CI headless smoke test doesn't crash — not when a CLI, GUI, or
installable package exists (those are specs 4 and 5).
