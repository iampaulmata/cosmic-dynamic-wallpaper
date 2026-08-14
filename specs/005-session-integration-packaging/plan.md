# Implementation Plan: Session Integration & Packaging

**Branch**: `005-session-integration-packaging` | **Date**: 2026-08-14 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/005-session-integration-packaging/spec.md`

**Note**: This template is filled in by the `/speckit-plan` command; its definition describes the execution workflow.

## Summary

Ship `wallpaperd` (spec 3) and `wallpaperctl` (spec 4) as a real, installable Pop!_OS/Debian
package with a systemd user service unit bound to `cosmic-session.target`, so the daemon
autostarts on login, stops cleanly on logout, and recovers from a crash within a bounded
5-attempt window — with zero manual launch step. **A significant real-world platform finding,
verified live against this project's own dev COSMIC session (not assumed), reshapes what
"install disables cosmic-bg" actually requires**: `cosmic-bg` is unconditionally spawned by
`cosmic-session` itself with no external toggle any package can use, so FR-004–FR-007's
outcomes (no double background, no black screen on uninstall) are satisfied structurally by
`wallpaperd`'s pre-existing exclusive layer-shell surface (spec 3) plus this spec's own
autostart/stop mechanism — not by new cosmic-bg-mutating code. See Real-World Platform Findings
below and research.md R1/R3 for the full evidence trail.

## Technical Context

**Language/Version**: Rust, stable toolchain, same workspace as specs 1–4 — but this spec adds
**no new Rust application code**. Its deliverables are packaging artifacts: a systemd unit file
(INI text), Debian maintainer scripts (POSIX shell), and `Cargo.toml` packaging metadata
consumed by `cargo-deb` (research.md R4/R5).

**Primary Dependencies**: [`cargo-deb`](https://crates.io/crates/cargo-deb) (build-time only,
not a runtime dependency of any crate) to produce the `.deb` from `[package.metadata.deb]`
sections in the workspace's binary crates (research.md R4). No new runtime crate dependencies —
`wallpaperd`/`wallpaperctl` themselves are unchanged by this spec.

**Storage**: N/A for new state — this spec reads no new config and writes no new
`cosmic-config` entry (research.md R3: the originally-anticipated cosmic-bg config mutation
turned out to be unnecessary for this spec's required outcomes).

**Testing**: No new `cargo test` surface (no new Rust logic). Validation is packaging/systemd
integration testing: build the `.deb`, install it in a way that doesn't disturb this dev
machine's real session (quickstart.md's guarded steps), verify the unit's `systemctl --user`
lifecycle (start on a simulated session start, `PartOf=` propagated stop, `Restart=`/
`StartLimitBurst=` behavior under a forced crash), and verify `wallpaperctl`/`wallpaperd`
function identically to the already-tested source-built binaries once packaged.

**Target Platform**: Linux, COSMIC desktop, Debian-family (Pop!_OS 24.04 LTS primary target,
matching this project's own dev environment and `cosmic-bg`'s own packaging precedent —
research.md R4). Flatpak explicitly out of scope for this spec (spec.md Assumptions,
research.md R4 Alternatives).

**Project Type**: Packaging/deployment spec — no new workspace crate. New non-Rust artifacts
under a new top-level `packaging/` directory (unit file, Debian maintainer scripts) plus
`[package.metadata.deb]` additions to `crates/renderer/Cargo.toml` and
`crates/wallpaperctl/Cargo.toml` (or a small shared packaging crate/workspace-level metadata,
per whichever `cargo-deb` convention proves cleanest during implementation — a task-level
detail, not a plan-level fork).

**Performance Goals**: SC-001's clarified ≤5-second bound from session start to `wallpaperd`
actively rendering (spec.md Clarifications) — already comfortably met by `wallpaperd`'s
real, live-verified sub-second Wayland/GPU startup cost (spec 3); this spec's unit (research.md
R2) adds no additional startup-ordering delay beyond normal systemd dependency resolution.

**Constraints**: No panics in any shipped shell script (maintainer scripts run as root during
package install/removal — a failing `postinst`/`prerm` can leave `dpkg` in a broken state);
every maintainer script MUST be idempotent (FR-005, SC-005) and MUST NOT fail the whole package
operation if `wallpaperd.service` happens to already be in the target state. The systemd unit
MUST NOT retry unboundedly on crash (FR-003, clarified: 5 attempts within a rolling window).

**Scale/Scope**: Single-user desktop install. No multi-user/multi-session coordination
(spec.md Assumptions) — `systemctl --user` units are inherently per-user already.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|---|---|---|
| I. Independent Renderer / Exclusive Output Ownership | **PASS** | This spec doesn't touch rendering, but directly implements Principle I's own second clause — "or fully superseding [cosmic-bg] as the session's background service" — via research.md R3's finding: `wallpaperd`'s pre-existing exclusive `Layer::Background` surface (spec 3) plus this spec's autostart already satisfies "never run concurrently [visibly]" without new code. |
| II. Wayland-Native, No X11 | N/A | No windowing/protocol code in this spec. |
| III. GPU-Accelerated Crossfade | N/A | No rendering in this spec. |
| IV. Settings Live in cosmic-config | N/A (verified, not violated) | This spec adds no new persisted config (Technical Context: Storage). Confirmed (research.md R3) that `cosmic-bg`'s own config already lives in `cosmic-config`, consistent with this principle — this spec doesn't need to touch it. |
| V. Solar/Time Logic Pure, Deterministic, Unit-Tested | N/A | No scheduling logic in this spec. |
| VI. Two Scheduling Modes | N/A | No render loop in this spec. Indirectly relevant: research.md R3's residual-optimization note (cosmic-bg redrawing unseen images wastes CPU/battery) is flagged as a future task, not a violation this spec introduces. |
| VII. Per-Output Correctness | N/A | No output-handling code in this spec. |
| VIII. Failures Are Contained, Never Fatal | **PASS** | FR-003's bounded restart (systemd `Restart=on-failure`/`StartLimitBurst=5`, research.md R2) is this principle's session-level analogue; maintainer scripts (Technical Context: Constraints) are required to be idempotent and non-fatal to the package operation, mirroring the same posture at the packaging layer. |
| IX. Native COSMIC UI | N/A | No UI in this spec. |
| X. Config Schema Versioned | N/A | No new config schema introduced (see Principle IV row). |
| XI. Session Integration, Including Cleanly Superseding cosmic-bg | **PASS** | This is the principle this entire spec exists to satisfy. FR-001–FR-003 (autostart/stop/bounded-restart via `wallpaperd.service`, research.md R1/R2) and FR-008/FR-009 (`.deb` packaging, research.md R4) directly implement it; FR-004–FR-007 (cosmic-bg handoff) satisfied per research.md R3 as described in Summary. |

**Gate result**: PASS. No Complexity Tracking entries required.

### ⚠️ Real-World Platform Findings (verified live, not assumed — research.md R1/R3)

Two findings materially changed this spec's actual implementation scope from what spec.md's
Assumptions originally left open for planning to decide. Both are documented in full in
research.md; summarized here per this project's established practice of surfacing real
findings prominently (e.g. spec 3's FR-16 addition, spec 4's Cross-Spec Dependencies) rather
than silently absorbing them:

1. **There is no external mechanism to "disable" `cosmic-bg`.** It is unconditionally spawned
   by `cosmic-session` itself (confirmed via this dev machine's live process tree *and*
   `cosmic-session`'s upstream source, fetched live) — no config file, environment variable, or
   CLI flag gates it. This was verified, not assumed, before concluding FR-004 needs no new
   code: `wallpaperd`'s already-implemented (spec 3) exclusive, opaque background layer-shell
   surface is what actually makes `cosmic-bg`'s rendering invisible, and this was true before
   this spec started — this spec's own contribution is making sure `wallpaperd` is *always
   running* (FR-001–FR-003), which is what makes that pre-existing occlusion continuous rather
   than dependent on a manual launch.
2. **Uninstall therefore needs no "restore cosmic-bg" step either.** Since `cosmic-bg` was
   never stopped, it's still running underneath `wallpaperd` the whole time; stopping
   `wallpaperd.service` (FR-002, already required) is sufficient for `cosmic-bg`'s
   already-live surface to become visible again, satisfying FR-006/FR-007 as a structural
   consequence rather than a separately-implemented action.

Neither finding changes spec.md's functional requirements (FR-004–FR-007's *outcomes* are
still required and still verified true) — only the previously-open *mechanism* question,
which is now resolved rather than left to task-level guesswork. spec.md's Assumptions section
has been updated in place to record this (not left stale).

**Post-Phase-1 re-check**: Design artifacts (data-model.md, contracts/, quickstart.md) build
directly on R1–R5's resolved decisions with no new gaps surfaced — the systemd unit shape
(R2) and packaging metadata (R4) are now concrete enough to task out directly. Gate result
unchanged: PASS.

## Project Structure

### Documentation (this feature)

```text
specs/005-session-integration-packaging/
├── plan.md              # This file (/speckit-plan command output)
├── research.md          # Phase 0 output (/speckit-plan command)
├── data-model.md         # Phase 1 output (/speckit-plan command)
├── quickstart.md         # Phase 1 output (/speckit-plan command)
├── contracts/             # Phase 1 output (/speckit-plan command)
│   ├── systemd-unit.md
│   └── debian-package.md
└── tasks.md               # Phase 2 output (/speckit-tasks command - NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
Cargo.toml                        # workspace root (spec 1); this spec adds [package.metadata.deb]
                                   # to member crates, not a new workspace member
crates/
├── schedule-engine/               # spec 1 (dependency, not modified here)
├── pack-loader/                   # spec 2 (dependency, not modified here)
├── renderer/                      # spec 3 — gains [package.metadata.deb] asset entries only
│   └── Cargo.toml                 # (binary: wallpaperd, already built; packaging metadata added)
└── wallpaperctl/                  # spec 4 — gains [package.metadata.deb] asset entries only
    └── Cargo.toml                 # (binary: wallpaperctl, already built; packaging metadata added)

packaging/                        # NEW — this spec's actual deliverables
├── systemd/
│   └── wallpaperd.service          # research.md R2 — the autostart/stop/restart unit
└── debian/
    ├── postinst                    # systemctl --user enable (+ start if a session is active)
    ├── prerm                       # systemctl --user disable + stop
    └── postrm                      # cleanup on purge (no config deletion — cosmic-config
                                     # entries are user data, left alone per Debian convention)
```

**Structure Decision**: A new top-level `packaging/` directory (not a workspace crate) holds
this spec's actual deliverables — the systemd unit and Debian maintainer scripts — since none
of it is Rust code. `wallpaperd`/`wallpaperctl` themselves gain only `Cargo.toml` metadata
(`[package.metadata.deb]`), not code changes; this keeps specs 3–4's existing crates,
tests, and behavior completely untouched by this spec, matching Technical Context's "no new
Rust application code" framing.

## Complexity Tracking

*No entries — Constitution Check passed with no unjustified violations. The Real-World
Platform Findings above are a resolved planning discovery, not a constitution deviation this
spec needs to justify.*
