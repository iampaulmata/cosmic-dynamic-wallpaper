<!--
Sync Impact Report
==================
Version change: 1.0.0 → 1.0.1 (PATCH — wording only)
Rationale for PATCH: Project rename to "Cosmic Dynamic Wallpaper" (spec
009-project-rename, FR-001/FR-005). No principle is added, removed, or redefined, and
no governance rule changes in substance — only the document title and the Governance
section's project-slug reference are updated to match the renamed repository/package
(`cosmic-dynamic-wallpaper`, per specs/009-project-rename/contracts/
identifier-rename-map.md).

Modified principles: none (wording-only change, no principle text touched)

Added sections: none

Removed sections: none

Templates requiring follow-up review: none — this amendment doesn't touch anything a
template's Constitution Check gate references (principle names/numbers are unchanged).

Deferred placeholders: none.
-->

# Cosmic Dynamic Wallpaper Constitution

## Core Principles

### I. Independent Renderer, Exclusive Ownership of Managed Outputs

The project ships its own Wayland layer-shell renderer. It does NOT drive stock `cosmic-bg`
via config, and does NOT depend on `cosmic-bg`'s slideshow/rotation feature. This is decided,
not left open: `cosmic-bg` has no crossfade/blend capability between images, only hard swaps,
and smooth time-of-day transitions are a non-negotiable feature of this project, not a
stretch goal.

The daemon MUST take exclusive ownership of the background layer-shell surface on every
output it manages, and MUST NOT run concurrently with `cosmic-bg` rendering a background on
the same output — two clients racing to draw the same layer surface is a bug, not a
supported configuration. The daemon's install/first-run path MUST handle disabling
`cosmic-bg`'s background role for managed outputs (or fully superseding it as the session's
background service) rather than leaving that step to the user's discovery.

**Rationale**: This is the one architectural fork every other principle depends on, and it is
now settled by the crossfade requirement. Getting the ownership question wrong means two
background renderers silently stomping on each other's layer surface, which is a uniquely
confusing bug for a user to diagnose.

### II. Wayland-Native, No X11 Fallbacks

All output/rendering code MUST target Wayland layer-shell protocols via
`smithay-client-toolkit` (or libcosmic's wrapper over it) — no X11 root-window pixmap
tricks, no xwinwrap-style hacks, even as a compatibility shim. Redraws MUST be paced by the
compositor's `wl_surface.frame` callback, not a free-running fixed-rate timer, so the
renderer never draws faster than the compositor presents and never fights variable refresh
rate / power-saving behavior.

**Rationale**: COSMIC is Wayland-only end to end; an X11 fallback is dead code that still has
to be maintained and tested. Frame-callback pacing is what keeps "smooth crossfade" from
turning into "burns a full CPU core for 30 seconds twice a day."

### III. GPU-Accelerated Crossfade, Not Per-Frame CPU Blending

Crossfade transitions MUST be composited on the GPU (e.g. via wgpu or GL, blending two
textures) rather than by decoding and alpha-blending full-resolution frames on the CPU every
tick. The renderer MUST sit idle (no active render loop, no frame-callback subscription)
whenever no transition is in progress — the frame-paced loop from Principle II activates
only for the duration of an actual crossfade, then stops.

This does NOT raise the project's minimum hardware bar: any device running `cosmic-comp`
already has a working GL/EGL path, and a two-texture crossfade blend is trivial load on even
older integrated graphics (Intel iGPUs included). CI/manual QA MUST include at least one
integrated-graphics device — not just whatever dGPU happens to be in a contributor's dev
machine — since that is the common case for real users, not the edge case.

**Rationale**: This is the principle that makes owning the renderer (Principle I) not a
regression on Principle VI's efficiency goals. A multi-monitor, multi-second CPU-side blend
twice a day is a noticeable, measurable battery cost; the same blend on the GPU, paced to
actual transition windows, is not.

### IV. Settings Live in cosmic-config, Not a Bespoke Format

Persistent state (wallpaper sets, per-output assignment, timing rules) MUST be stored
through `cosmic-config`'s RON-based store so it participates in the same watch/reload model
`cosmic-bg` and `cosmic-settings` use. Import support for existing dynamic-wallpaper formats
(GNOME's time/sun-position background XML, macOS `.heic` solar metadata) is allowed and
encouraged, but only as importers/converters into the native schema — never as the format
the daemon reads at runtime.

**Rationale**: A second live-reloaded config format alongside `cosmic-config` invites drift
and double-watching the filesystem for the same intent.

### V. Solar/Time Logic Is Pure, Deterministic, and Unit-Tested

The core "which image for right now" logic (sunrise/sunset calculation, time-of-day
interpolation, transition scheduling) MUST be implemented as pure functions with no I/O or
rendering dependencies, callable and testable in isolation. Solar position math MUST use a
vetted algorithm/crate rather than a hand-rolled approximation — timing errors are the single
most visible bug class in this category of app (wrong wallpaper at noon is immediately
obvious to every user, every day).

**Rationale**: This is the feature that differentiates the project from a plain slideshow; it
needs the highest test coverage in the codebase, and it cannot get that coverage if it is
entangled with the render loop.

### VI. Two Scheduling Modes: Idle-Wait and Active-Transition

The daemon has exactly two states, and MUST NOT blur them.

- **Idle-wait**: computes the next transition instant and sleeps on a single calloop timer
  for that duration — no polling, no active render loop, no frame-callback subscription.
- **Active-transition**: triggered by the timer firing, or by config/output-hotplug changes;
  subscribes to `wl_surface.frame` (Principle II) and runs the GPU crossfade (Principle III)
  until the blend completes, then drops immediately back to idle-wait.

Battery/CPU impact is a first-class review criterion for both states, not an afterthought.

**Rationale**: Owning the renderer means owning this tradeoff explicitly. It is easy to
accidentally leave a frame-callback subscription or a redraw timer running after a
transition finishes; that turns "occasional GPU blend" into "background CPU/GPU drain,"
which defeats the entire point of Principle III.

### VII. Per-Output Correctness Under Hotplug and Scaling — Fully Owned, Not Inherited

Because this project no longer delegates rendering to `cosmic-bg`, multi-monitor handling is
entirely its own responsibility: independent wallpaper sets and crossfade state per output,
correct behavior when an output is added/removed/resized at runtime, and correct rendering
under fractional scaling — all without `cosmic-bg`'s existing implementation to fall back on.
This MUST be tested with at least one multi-output, mixed-scale-factor scenario in CI or
documented manual QA before any release, not just single-monitor development.

**Rationale**: This used to be free (inherited from `cosmic-bg` under a config-driven
approach); under an independent renderer it is the largest area of new surface area and the
easiest place for regressions to hide, since a single-monitor dev setup will not exercise it.

### VIII. Failures Are Contained, Never Fatal

A malformed wallpaper pack, a missing image file, an unreachable/invalid location for solar
calculation, or a corrupt config entry MUST degrade only that one wallpaper set (fall back to
a static image or skip it) and MUST NOT crash or hang the daemon. `unwrap()` / `expect()` are
prohibited outside of tests and provably unreachable states; all fallible paths return
`Result` and are logged with enough context to debug without attaching a debugger.

**Rationale**: This runs unattended in a session; a panic here means the user's desktop
background silently reverts or the process needs manual restart, with no feedback loop
telling them why.

### IX. Native COSMIC Look and Feel for Any UI Surface

Any settings GUI MUST be built with libcosmic widgets and the shared COSMIC theme tokens —
not GTK, Qt, or a raw web view — so it is visually and behaviorally consistent with
`cosmic-settings` and other applets. A CLI-only control path (config file plus an optional
`*-ctl` binary) is an acceptable substitute for a full GUI in early milestones, but a
partial/foreign-toolkit GUI is not.

**Rationale**: A bolted-on GTK panel inside a COSMIC-native environment is the most common
way these community extensions look unfinished, even when the underlying logic is solid.

### X. Config Schema Is Versioned With a Migration Path

Any change to the on-disk config schema MUST bump a schema version and ship a migration
function; the daemon MUST refuse to silently misinterpret an old-format value rather than
guess. Breaking schema changes require a documented migration note in the release.

**Rationale**: `cosmic-config`'s live-reload model means a bad migration does not just fail
on next launch — it can corrupt state while the daemon is running.

### XI. Session Integration, Including Cleanly Superseding cosmic-bg

The project MUST ship as a proper autostart/session component (systemd user unit or
cosmic-session integration) rather than requiring users to manually launch and keep a
terminal open. Per Principle I, install/uninstall MUST also handle `cosmic-bg`'s role on
managed outputs cleanly — disabling it on install, and restoring it on uninstall — so the
user is never left with neither renderer active, or both fighting for the same surface.
Packaging (distro package and/or Flatpak) is a release requirement before calling any version
"usable," not a nice-to-have.

**Rationale**: A background daemon that is not actually backgrounded — i.e., does not survive
logout/login and does not autostart — fails the basic use case the whole project exists for.

## Technology Stack Constraints

- **Language**: Rust, on a pinned MSRV tracked in `Cargo.toml`; no unsafe code outside
  vetted, documented boundary shims (e.g. GPU/FFI interop) with a comment justifying each use.
- **Windowing/protocol layer**: `smithay-client-toolkit` (or libcosmic's wrapper) for
  Wayland layer-shell surfaces; `calloop` as the event loop, per Principle VI's two-state
  model.
- **Rendering**: `wgpu` or raw GL for GPU-side crossfade compositing (Principle III); no
  software rasterization of the crossfade blend on the CPU hot path.
- **Configuration**: `cosmic-config` (RON-based) is the only runtime-read persistence layer
  (Principle IV); other formats are import-time only.
- **Solar/time math**: a vetted, published solar-position crate (e.g. an established sunrise/
  sunset or solar-position-algorithm implementation) — not a hand-rolled approximation
  (Principle V).
- **UI**: libcosmic for any graphical settings surface (Principle IX); CLI/config-file control
  is an acceptable interim surface.
- **Packaging/session**: systemd user unit and/or cosmic-session integration, plus at least
  one of a distro package or Flatpak manifest, before any release is called "usable"
  (Principle XI).

## Development Workflow & Quality Gates

- Every plan and PR touching rendering, scheduling, or config code MUST state explicitly how
  it complies with Principles I–VIII and XI, or document a justified, time-boxed exception.
- `unwrap()`/`expect()` outside of `#[cfg(test)]` code is a review-blocking finding
  (Principle VIII); prefer `clippy::unwrap_used` / `clippy::expect_used` lints enabled at
  least on non-test code, enforced in CI.
- CI (or documented manual QA, if CI cannot yet run compositor-backed tests) MUST exercise,
  before any release:
  - at least one integrated-graphics device for the crossfade path (Principle III), and
  - at least one multi-output, mixed-scale-factor scenario (Principle VII).
- Any change to the on-disk config schema MUST include a migration function and a release
  note describing the migration (Principle X); PRs changing the schema without one are
  rejected.
- Idle-state CPU/GPU usage (no transition in progress) and active-transition duration are
  first-class review criteria (Principle VI) — a PR that leaves a frame-callback subscription
  or timer running after a transition completes is a bug, not a style nit.
- Solar/time logic changes MUST ship with or extend unit tests covering the affected pure
  functions (Principle V); rendering or I/O code MUST NOT be required to exercise that logic
  in tests.

## Governance

This constitution supersedes all other project practices, conventions, and prior informal
agreements for the `cosmic-dynamic-wallpaper` project. Where a plan, PR, or design doc
conflicts with a principle here, the principle wins unless the constitution itself is
amended first.

**Amendment procedure**: Amendments are proposed as a change to this file (via
`/speckit-constitution` or an equivalent reviewed PR), must include an updated Sync Impact
Report describing what changed and why, and require the same review rigor as any other
project-governance change before merging. Silent or undocumented edits to this file are not
permitted.

**Versioning policy**: This document follows semantic versioning:
- **MAJOR** — backward-incompatible governance/principle removals or redefinitions (e.g.
  reversing the exclusive-ownership decision in Principle I).
- **MINOR** — a new principle or section added, or materially expanded guidance on an
  existing principle.
- **PATCH** — clarifications, wording, typo fixes, and other non-semantic refinements.

**Compliance review**: Every `/speckit-plan` and `/speckit-implement` pass MUST re-check its
output against the Core Principles above (the plan template's Constitution Check gate is the
mechanical enforcement point). Complexity or deviation from a principle MUST be justified in
the plan's Complexity Tracking section, not silently absorbed. Reviewers treat unresolved
conflicts with this constitution as blocking, not advisory.

**Version**: 1.0.1 | **Ratified**: 2026-08-11 | **Last Amended**: 2026-08-15
