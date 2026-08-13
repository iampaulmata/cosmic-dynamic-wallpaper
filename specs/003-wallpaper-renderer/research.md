# Research: Wallpaper Renderer

## R1. Wayland layer-shell + output/scaling protocol stack

**Decision**: [`smithay-client-toolkit`](https://github.com/Smithay/client-toolkit) (SCTK,
0.20.x on crates.io) for `wlr-layer-shell-unstable-v1` surfaces (the background layer),
`xdg-output`/`wl_output` tracking for hotplug (FR-008–FR-010), and `wp_viewporter` +
`fractional-scale-v1` for correct rendering under fractional scale factors (FR-008's scaling
requirement).

**Rationale**: This is not a novel integration — `cosmic-bg` itself is built on SCTK (`sctk`)
for exactly this same protocol set: a `CosmicBgLayer` per connected display containing a
Wayland layer surface and a viewport for scaling, confirmed directly from `cosmic-bg`'s own
architecture during research. Using the same toolkit and the same viewport-based scaling
approach means this project isn't inventing a second, unproven way to solve a problem
`cosmic-bg` already solves correctly — the actual differentiator (constitution Principle I) is
owning the surface exclusively and GPU-compositing the blend (R3), not the output/protocol
plumbing underneath it. SCTK is also the toolkit the constitution names directly ("or
libcosmic's wrapper over it") as the mandated windowing layer.

**Alternatives considered**: A hand-rolled `wayland-client` protocol implementation without
SCTK — rejected; SCTK's `OutputHandler`/`LayerShellHandler` traits already solve hotplug event
delivery and layer-surface configure/lifecycle correctly, and reimplementing that is pure risk
with no benefit given `cosmic-bg` already validates the same toolkit choice in production on
this exact desktop.

## R2. Event loop integration

**Decision**: [`calloop`](https://github.com/Smithay/calloop) as the event loop (constitution-
mandated), with [`calloop-wayland-source`](https://github.com/Smithay/calloop-wayland-source)
(0.3.x — actively maintained, ~2.4M downloads/month, used in 1,000+ crates as of this
research) as the adapter feeding `wayland-client`'s `EventQueue` into calloop's poll loop, plus
a per-output calloop timer source for the idle-wait next-transition sleep (constitution
Principle VI, FR-003).

**Rationale**: `calloop-wayland-source` is the exact adapter `cosmic-bg` itself uses to bridge
its Wayland event queue into calloop — same reasoning as R1: proven integration pattern on
this desktop, not a novel one. A single calloop instance driving both the Wayland event source
and N per-output timer sources (up to 8, per spec.md's Clarifications) keeps the "one process,
one event loop, no polling" property (Principle VI) intact without a second thread or async
runtime.

**Alternatives considered**: `tokio` with a Wayland-async bridge crate — rejected; pulling in
an async runtime for what is fundamentally a small, bounded set of fd-driven event sources
(one Wayland connection, ≤8 timers) is unjustified complexity, and calloop is what the
constitution already commits the project to.

## R3. GPU crossfade backend and Wayland surface bridging

**Decision**: [`wgpu`](https://wgpu.rs) (0.2x-series current stable as of this research, May
2026) for the two-texture crossfade blend, using its Vulkan backend where available (falling
back to GL) — bridged to SCTK's `wl_surface`/`wl_display` objects via
[`raw-window-handle`](https://github.com/rust-windowing/raw-window-handle) (0.6.x,
`RawWindowHandle::Wayland`/`RawDisplayHandle::Wayland`).

**Rationale**: The constitution explicitly permits either `wgpu` or raw GL (Technology Stack
Constraints). `wgpu` is chosen because: (1) it's backend-portable (Vulkan on modern integrated
GPUs, GL fallback on older ones) without this project hand-writing two code paths, directly
serving NFR-3/SC-006's integrated-graphics requirement; (2) it's the same rendering stack
`libcosmic`/`iced` (and so `cosmic-comp`'s own client-side rendering story) already commits
the COSMIC ecosystem to, so this project isn't introducing a second GPU API story; (3) its
safe Rust API keeps `unsafe` usage confined to the documented surface-creation boundary shim
the constitution explicitly allows ("vetted, documented boundary shims... with a comment
justifying each use"), rather than spreading raw GL calls through the crossfade logic itself.

**Known integration friction (flagged, not blocking)**: community reports during research
show past rough edges combining SCTK's raw Wayland objects with `wgpu`'s surface-creation API
(stale `HasRawDisplayHandle` availability across `raw-window-handle` versions, and at least one
report of developers getting stuck rendering a layer-shell surface with `wgpu`). Neither is a
correctness blocker — both crates are on their current stable major versions at this research
date — but it means the `gpu.rs`/`surface.rs` bridging code (Project Structure) should be
built and manually smoke-tested early, not assumed trivial, before the crossfade pipeline
itself is built on top of it.

**Alternatives considered**: Raw GL via `wayland-egl` + `glow` — a valid constitution-
compliant alternative, but would require this project to hand-manage EGL context/surface
lifecycle itself with no cross-backend fallback if a target device lacks a working GL driver;
`wgpu`'s automatic backend selection gets that fallback for free.

## R4. Output-assignment & "same pack everywhere" toggle persistence

**Decision**: A new `cosmic-config` schema, owned by this crate, for per-output pack
assignments and the "same pack on all outputs" toggle state (FR-005–FR-007) — versioned with
its own `schema_version` (constitution Principle X), watched via `cosmic-config`'s own
change-notification mechanism to satisfy the 2-second reaction bound (FR-007, spec.md
Clarifications) without this crate hand-rolling filesystem watching.

**Rationale**: Constitution Principle IV mandates `cosmic-config` as the only runtime-read
persistence layer for daemon state, and this is new state spec 2's pack-registry schema
doesn't cover (that schema tracks known pack *locations*, not which output shows which pack).
Keeping it a separate schema — rather than overloading spec 2's registry — mirrors spec 2's
own precedent of giving each distinct piece of state its own explicit schema/version rather
than assuming one covers the other.

**Scope note**: The exact on-disk shape (RON structure, key names) is this spec's own design
(contracts/renderer-config-schema.md), not spec 2's — spec 2's `PackSource`/registry types are
reused *by reference* (an assignment points at a `PackSource` spec 2 already validates), not
redefined.

**Alternatives considered**: A D-Bus method call from a future CLI/GUI directly into a running
daemon instance for "assign pack to output" — rejected as this spec's persistence mechanism
(though a live-query D-Bus surface *for spec 4's CLI* remains an open, spec-4-owned question,
per spec.md Assumptions); writing directly to `cosmic-config` and having every daemon instance
watch for changes is simpler, consistent with how `cosmic-bg`/`cosmic-settings` already behave,
and means this daemon has exactly one way state changes reach it, not two.

## R5. Full-resolution image decode & GPU texture upload

**Decision**: Reuse [`image`](https://crates.io/crates/image) (0.25.x) — already a spec 2
dependency for header-only readability validation — for the full pixel decode this spec needs
to actually build a GPU texture, uploaded via `wgpu`'s standard `Queue::write_texture` path.

**Rationale**: Spec 2's research (research.md R2 there) already picked `image` specifically
*because* it's the same crate the renderer would eventually need for full decode, avoiding a
second image-decode dependency entering the tree. Nothing in this spec's requirements needs a
different or additional decode library — decoding happens once per image the first time it
becomes relevant to an active or imminent transition, not on every frame of a crossfade (the
GPU holds the already-decoded texture and blends it every frame via R3's pipeline).

**Alternatives considered**: None seriously — this is a direct continuation of spec 2's own
already-settled decision, not a new evaluation.

## R6. Testing/CI strategy for Wayland+GPU code

**Decision**: Split testing into two tiers. (1) The pure `assignment.rs` module (output-
assignment resolution, override-vs-toggle precedence, change coalescing — FR-005–FR-007,
FR-014) has zero Wayland/GPU dependency and is fully `cargo test`-able like specs 1–2's crates.
(2) Everything Wayland/GPU-touching (`output.rs`, `surface.rs`, `gpu.rs`, `crossfade.rs`) is
validated primarily via a documented manual QA checklist — the constitution's own explicit
allowance ("CI or documented manual QA, if CI cannot yet run compositor-backed tests") — with
an exploratory CI smoke test running under Weston's headless backend (which exists specifically
to test Wayland clients "without any windowing system or any need for drm access") paired with
a software Vulkan/GL implementation (e.g. `lavapipe`/`llvmpipe`) to assert only "the daemon
starts, creates a layer surface per output, and doesn't crash on a simulated hotplug event" —
not pixel-level crossfade correctness, which stays a manual QA item.

**Rationale**: This spec is the project's first to touch real compositor/GPU state, and the
constitution anticipates exactly this gap rather than pretending full CI coverage is available
on day one. Distinguishing "the pure logic is fully unit-tested" from "the
Wayland/GPU integration is smoke-tested plus manually verified" keeps the spec honest about
what's actually machine-checked versus what a release still needs a human to confirm
(constitution's own CI/manual-QA requirement for Principle III's integrated-graphics
requirement and Principle VII's multi-output/mixed-scale requirement).

**Alternatives considered**: Standing up a full `cosmic-comp` instance in CI for true
end-to-end testing — appealing in principle, but out of proportion for this spec; `cosmic-comp`
isn't packaged for easy headless CI use today, and Weston's headless backend already exists
for exactly this "test a Wayland client without hardware" purpose, at far lower setup cost.
Revisit if CI gaps around the multi-output/hotplug paths (FR-008–FR-010) prove to be a real
source of regressions once the crate exists.

## R7. Consuming a manual location for solar-anchored packs (added, Amendment 2026-08-13)

**Decision**: `config.rs` watches spec 4's `LocationConfig` `cosmic-config` entry (a schema
this crate reads but does not own or write) the same way it already watches its own
`RendererConfig`; `scheduler_bridge.rs` reads the current value and passes it as the
`location` argument to spec 1's `ValidatedPack::query` whenever the pack being scheduled is
solar-anchored.

**Rationale**: This gap surfaced directly from planning spec 4 — its own Success Criteria (a
solar-anchored pack visibly scheduled using only CLI commands) is unreachable if nothing in
this daemon ever reads the location spec 4's CLI persists. Reusing the exact same watch
mechanism `RendererConfig` already uses (rather than inventing a second config-reading
pathway) keeps this addition small and consistent with R4's existing pattern.

**Alternatives considered**: Requiring the location to be passed as a CLI/environment argument
to `wallpaperd` at startup instead of read from `cosmic-config` — rejected; it would mean
changing the location requires restarting the daemon, directly contradicting this spec's own
FR-007 (config changes take effect within 2 seconds, without a restart).

## R8. The D-Bus service this daemon exposes (added, Amendment 2026-08-13)

**Decision**: `dbus_service.rs` runs a [`zbus`](https://crates.io/crates/zbus) (5.x) server on
the session bus, registered under the bus name and interface documented in spec 4's
`contracts/wallpaperd-dbus-interface.md`, integrated into the same `calloop` event loop (R2)
via `zbus`'s async-compatible connection primitives rather than a second, competing event loop.

**Rationale**: Spec 4's CLI (its own research.md R3/R5) already committed to `zbus` as the
client for this interface — using the same crate on the server side means no protocol
mismatch and no second D-Bus implementation entering the dependency tree. Folding the D-Bus
connection into the existing `calloop` loop (rather than spawning a separate thread/runtime
for it) preserves this spec's "one process, one event loop" property (constitution Principle
VI) that R2 already established for the Wayland connection and per-output timers.

**Alternatives considered**: A separate thread running its own async runtime for the D-Bus
server — rejected; `zbus` supports integration with external event loops precisely to avoid
this, and a second thread would complicate the "exactly two states" reasoning (idle-wait /
active-transition) constitution Principle VI asks this spec to keep simple.
