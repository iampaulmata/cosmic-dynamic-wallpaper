# Implementation Plan: Fix Adversarial Audit Findings

**Branch**: `011-fix-audit-findings` | **Date**: 2026-08-16 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/011-fix-audit-findings/spec.md`

**Note**: This template is filled in by the `/speckit-plan` command; its definition describes the execution workflow.

## Summary

Remediate all 52 findings (11 critical, 25 warning, 16 note) from the full-codebase adversarial
audit, ordered by the spec's 8 priority-ranked user stories. This is a hardening pass across all
six existing crates plus `packaging/` — **no new crate, no new top-level feature surface, no
`cosmic-config` schema changes**. Every fix tightens validation, bounds a resource, contains a
panic, or surfaces a previously-swallowed error at an existing boundary; none change what a
correctly-formed pack, config, or CLI invocation does. The three numeric thresholds fixed by
clarification (512 KB manifest cap, 256 MB decoded-image ceiling, 8-entry D-Bus queue bound) are
implemented as named constants co-located with the code they gate, matching every existing cap in
this codebase (`MAX_ANCHORS`, `MAX_SUPPORTED_SCHEMA_VERSION`, `MAX_SEARCH_RADIUS_DAYS`). Research
below confirms, against the real source (not just the audit's description), the exact fix shape
for every finding — several (FR-011/FR-025 in particular) turn out to have a cleaner fix available
than the audit's own suggested framing once the surrounding code is read (e.g. `cosmic-config`
already exposes a `transaction()` API that directly solves the multi-field-atomicity finding).

## Technical Context

**Language/Version**: Rust, stable toolchain (`rustc 1.97`), edition 2021 — unchanged, same
workspace as specs 1–10.

**Primary Dependencies**: No new dependency is required for the large majority of findings — every
fix reuses a type, pattern, or API already present in the crate it touches (`cosmic_config::
Config::transaction()`, `image`'s header-only dimension reader, `tracing`, `zbus::fdo::Error`,
`std::os::unix::fs::PermissionsExt`). Two findings need one small addition each, both scoped to a
single crate:
- **Registry locking** (US6/FR-022, `pack-loader`): a cross-process advisory file lock. `libc` is
  already resolved in `Cargo.lock` as a transitive dependency workspace-wide, but no crate depends
  on it directly today, and a raw `libc::flock` FFI call would be this crate's first `unsafe` block
  (constitution: "no unsafe code outside vetted, documented boundary shims... with a comment
  justifying each use"). Rather than open that boundary, add `fd-lock` (`~200 LOC`, pure safe Rust,
  no proc-macros, no transitive dependency growth beyond `libc` itself which is already resolved)
  as a direct dependency of `pack-loader` (research.md R17).
- **STUN reply sanity-bounding** (US7/FR-031, `renderer`): no new dependency — reuses
  `schedule_engine::Location`'s existing distance/validity logic (research.md R26); confirmed no
  crate for this needs adding.

**Storage**: No `cosmic-config` schema changes anywhere (constitution Principle X n/a — every
fix is a validation/bound tightening on an existing schema version, not a shape change). The one
storage-adjacent change is applying restrictive Unix file permissions (0600 file / 0700 directory)
to the on-disk location/renderer config after every write (research.md R25) and switching
`location set`'s multi-field write from sequential per-field commits to `cosmic_config::Config::
transaction()` (research.md R20) — both operational changes to *how* the existing format is
written, not the format itself.

**Testing**: `cargo test` per crate, following this workspace's existing convention of in-module
`#[cfg(test)] mod tests` (no separate `tests/` directory anywhere in this repo today). Every
finding marked "Verified — reproduced" in the audit gets a regression test that is confirmed to
fail against pre-fix code (spec Edge Cases, SC-001/SC-005) before the fix lands. `proptest`
(already a `schedule-engine` dev-dependency) is reused for the solar-anchor-offset bound
(research.md R4) and the pole-location fast path (research.md R33), matching that crate's existing
property-test style. No new test-harness or CI infrastructure.

**Target Platform**: Linux/Wayland (COSMIC desktop) — unchanged.

**Project Type**: Hardening pass across an existing Rust Cargo workspace (6 library/binary crates
+ 1 tool + `packaging/`). Not a new crate, not a new UI surface.

**Performance Goals**: Rejection of oversized/malformed input must happen before the expensive
work it currently precedes, not just eventually (SC-003) — e.g. the anchor-count cap check
(FR-010) must be a single `.len()` comparison before any per-image syscall, not a refactor that
merely makes the existing per-image loop faster. The pole-location fast path (FR-038) must turn a
~29ms/query worst case into an early return once latitude is recognized as a pole, not a general
speedup of the existing search.

**Constraints**: All six crates already carry `[lints.clippy] unwrap_used = "deny"` /
`expect_used = "deny"` (re-confirmed directly against each `Cargo.toml` during implementation —
an earlier truncated read of `renderer`'s and `wallpaper-settings`' manifests had missed their
`[lints.clippy]` sections near the bottom of each file and incorrectly reported them as missing;
corrected here rather than left standing). No new lint-gate task is needed; every fix below keeps
`cargo clippy --workspace --all-targets` clean at the level already enforced everywhere.

**Scale/Scope**: Touches all six crates (`pack-loader`, `renderer`, `schedule-engine`,
`wallpaperctl`, `wallpaper-ipc`, `wallpaper-settings`), `tools/generate-starter-pack`, and
`packaging/` (new `dbus-1` policy file). No crate is untouched; no crate is added or removed.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Applies? | Assessment |
|---|---|---|
| I. Independent Renderer, Exclusive Ownership | No | No change to layer-shell ownership model or `cosmic-bg` interplay. |
| II. Wayland-Native, No X11 | No | The zero-size-surface fix (FR-002) stays entirely within the existing `smithay-client-toolkit`/`wgpu` surface-configure path; no protocol or pacing model change. |
| III. GPU-Accelerated Crossfade | **Yes — pass** | The crossfade-progress clamp (FR-003) and the GPU-texture eviction policy (FR-036) both touch the crossfade path, but neither changes *where* blending happens (still GPU-side) or the idle/active-transition split — they add input validation and a memory bound around the existing GPU compositing, matching this principle rather than working around it. |
| IV. Settings Live in `cosmic-config` | **Yes — pass** | No new persistence format is introduced anywhere (see Storage above); the transaction-based atomic write (FR-025) and the file-permission tightening (FR-030) both operate through/around the existing `cosmic-config` store, not alongside it. |
| V. Solar/Time Logic Is Pure, Deterministic, Unit-Tested | **Yes — pass** | The solar-anchor-offset bound (FR-004), pole fast-path (FR-038), `MAX_SEARCH_RADIUS_DAYS` correction (FR-039), and duplicate-instant reachability fix (FR-040) all land in `schedule-engine`'s existing pure functions with no I/O added, and each ships with a unit/property test per Testing above — strengthening this principle's coverage, not working around it. |
| VI. Two Scheduling Modes (daemon) | No | No change to the idle-wait/active-transition state machine itself; the adapter/device timeout (FR-033) bounds how long *entering* active-transition can block, without blurring the two states. |
| VII. Per-Output Correctness Under Hotplug and Scaling | **Yes — pass** | The zero-size reconfigure guard (FR-002) and surface-loss recovery (FR-034) are direct fixes to this principle's own "failure contained to one output" contract, which the audit found violated (a zero-size axis currently panics *every* output, not just one). |
| VIII. Failures Are Contained, Never Fatal | **Yes — pass, this is the principle most of this feature exists to restore.** All four crash-proofing fixes (US1), the resource caps (US3), and every "surface instead of swallow" fix (US6) are direct instances of this principle; the new `unwrap_used`/`expect_used` lint gates on `renderer` and `wallpaper-settings` (Constraints above) enforce it going forward, not just for this feature's own diffs. |
| IX. Native COSMIC Look and Feel | **Yes — pass** | The pack-builder fixes (US2, US6) add validation/error-surfacing to existing `wallpaper-settings` wizard state and dialogs — no new widget kind, no non-`libcosmic` UI. |
| X. Config Schema Is Versioned With a Migration Path | No | No schema version bump anywhere in this feature (see Storage above) — every fix tightens validation or write mechanics within the current schema version. |
| XI. Session Integration, Including Cleanly Superseding cosmic-bg | **Yes — pass** | The new `dbus-1` policy file (FR-015) is a `packaging/` addition alongside the existing systemd unit and desktop file — session-integration surface area, added rather than modified, with no change to autostart/install/uninstall behavior. |

No violations. No entries needed in Complexity Tracking — every fix is additive validation/bounds/
error-surfacing within existing architecture, not a new architectural element.

**Post-design re-check** (after Phase 0/1, research.md + data-model.md + contracts/): confirmed
still no violations. The two closest calls researched in detail — `fd-lock` as a new direct
dependency (research.md R17) and reconstructing `cosmic-config`'s private directory-naming
convention for permission-tightening (research.md R25) — were both evaluated specifically against
constitution Principle VIII ("no unsafe code outside vetted, documented boundary shims") and
Principle IV ("`cosmic-config` is the only runtime-read persistence layer"), respectively, and
found not to violate either: `fd-lock` avoids introducing a new unsafe boundary rather than adding
one, and the permission-tightening operates *on* `cosmic-config`'s own on-disk files without
introducing a second read/write path for the same data.

## Project Structure

### Documentation (this feature)

```text
specs/011-fix-audit-findings/
├── plan.md              # This file (/speckit-plan command output)
├── research.md          # Phase 0 output (/speckit-plan command)
├── data-model.md         # Phase 1 output (/speckit-plan command)
├── quickstart.md        # Phase 1 output (/speckit-plan command)
├── contracts/            # Phase 1 output (/speckit-plan command)
│   ├── wallpaperd-dbus-hardening.md
│   ├── wallpaperctl-cli-hardening.md
│   └── pack-loader-validation.md
├── checklists/
│   └── requirements.md
└── tasks.md              # Phase 2 output (/speckit-tasks command - NOT created by /speckit-plan)
```

### Source Code (repository root)

This is an existing Rust Cargo workspace (`Cargo.toml` `[workspace] members`), not a fresh
project — the structure below lists only what this feature touches inside it, grouped by user
story.

```text
crates/
├── pack-loader/src/
│   ├── manifest.rs        # US1 FR-001: Color::parse ASCII guard
│   ├── load.rs             # US3 FR-010/FR-011: anchor-cap + manifest-size pre-checks
│   ├── path_safety.rs      # US5 FR-020: explicit absolute-path rejection
│   ├── registry.rs         # US6 FR-022: cross-process lock around persist()
│   └── Cargo.toml          # + fd-lock (research.md R17)
│
├── schedule-engine/src/
│   ├── pack.rs              # US1 FR-004, US7 FR-040: offset bound + duplicate-instant reachability
│   ├── solar.rs             # US1 FR-004: bound enforced before the overflow site
│   ├── location.rs          # US7 FR-038: pole fast path
│   └── query.rs             # US7 FR-038/FR-039: fast path wiring + constant correction
│
├── renderer/src/
│   ├── surface.rs           # US1 FR-002/FR-003, US7 FR-034/FR-035/FR-036: zero-size guard,
│   │                        #   progress clamp, surface-loss recovery, unsafe doc, texture eviction
│   ├── texture.rs           # US3 FR-012: header-dimension pre-check + byte ceiling
│   ├── gpu.rs                # US7 FR-033: adapter/device request timeout
│   ├── dbus_service.rs      # US4 FR-014/FR-016/FR-017: bounded+coalesced queue, QueryAll logging,
│   │                        #   output_id validation
│   ├── ip_geolocation.rs    # US7 FR-031: sanity-bound resolved location
│   ├── config.rs            # US7 FR-032: portal-location debounce reuse
│   ├── Cargo.toml           # + [lints.clippy] unwrap_used/expect_used = "deny" (new gate)
│   └── bin/cosmic-wallpaperd.rs  # US7 FR-032: wire debounce into PortalEvent::Reading
│
├── wallpaperctl/src/
│   ├── commands/list.rs     # US5 FR-018: tab/newline escaping in human-readable output
│   ├── main.rs               # US6 FR-029: flag-conflict via CliError, not process::exit
│   └── error.rs              # US7 FR-028: DaemonUnreachable exit-code renumbering
│
├── wallpaper-ipc/src/
│   ├── renderer_config.rs   # US5 FR-019, US4 FR-017: shared OutputId::validated()
│   ├── location_config.rs   # US6 FR-023, US7 FR-025: surfaced load errors + 0600 permissions
│   └── location.rs (n/a — validation lives on OutputId here, not a new module)
│
└── wallpaper-settings/src/pages/
    ├── pack_builder.rs      # US2 FR-006/FR-007/FR-008, US6 FR-024/FR-026/FR-027: rename
    │                        #   validation, error surfacing, generate re-check, deferred write
    └── Cargo.toml           # + [lints.clippy] unwrap_used/expect_used = "deny" (new gate)

tools/generate-starter-pack/src/
└── main.rs                  # US8 FR-041: route through toml::Serialize

packaging/
└── dbus-1/
    └── com.system76.CosmicDynamicWallpaper1.conf  # US4 FR-015: new session-bus policy file

tests/ (in-crate, existing convention — `#[cfg(test)] mod tests` per module, no separate tests/ dir)
```

**Structure Decision**: Extends all six existing crates and `tools/generate-starter-pack` in
place; adds one new packaging artifact (`packaging/dbus-1/*.conf`). No new crate, no new workspace
member, no new top-level module in any crate — every change lands inside a file (or, for the two
`unwrap_used`/`expect_used` lint additions and one `Cargo.toml` dependency, a manifest) that
already exists.

## Complexity Tracking

*No entries — Constitution Check above found no violations to justify.*
