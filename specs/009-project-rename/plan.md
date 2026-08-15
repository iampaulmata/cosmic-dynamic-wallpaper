# Implementation Plan: Rename Project to "Cosmic Dynamic Wallpaper"

**Branch**: `009-project-rename` | **Date**: 2026-08-15 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/009-project-rename/spec.md`

## Summary

Rename the project from "dynamic-wallpaper" / "rust-dynamic-wallpaper" to "Cosmic
Dynamic Wallpaper" everywhere: prose/documentation, the local folder, the GitHub
repository, and every system-facing identifier (binary names get the COSMIC-ecosystem
`cosmic-` prefix; the D-Bus bus/interface name, every `cosmic-config` application ID,
and the `.desktop` application ID all become `com.system76.CosmicDynamicWallpaper*`;
the Debian package becomes `cosmic-dynamic-wallpaper`). The renamed `.deb` declares
`Replaces`/`Conflicts`/`Breaks` against the old package so a normal `apt install`
supersedes it cleanly, and each renamed `cosmic-config` store gets a one-time,
idempotent migration so no existing installation loses its configured location, pack
registry, or renderer assignment. Purely a naming/identifier change — no rendering,
scheduling, or solar-math logic is touched (FR-007).

## Technical Context

**Language/Version**: Rust, workspace MSRV 1.97 (edition 2021, per every crate's
`Cargo.toml`) — unchanged by this feature.

**Primary Dependencies**: No new external dependency. Uses only what's already in the
workspace: `cosmic-config` (the migration reuses the same `Config`/`CosmicConfigEntry`
APIs `LocationConfigEntry`'s existing v2→v3 migration already uses), `zbus` (D-Bus name
constants), `cargo-deb` 3.7.0 (confirmed to support the `replaces`/`conflicts`/`breaks`/
`provides` `[package.metadata.deb]` fields FR-004b needs — verified directly against
the pinned version's source, not assumed).

**Storage**: `cosmic-config` RON stores — four existing application IDs are renamed and
each needs a one-time copy-forward migration: `com.system76.CosmicWallpaper.Renderer`,
`.Location`, `.Registry`, `.RemovedStarterPacks` → their `CosmicDynamicWallpaper`
equivalents.

**Testing**: `cargo test --workspace` (existing convention, unchanged). New unit tests
per migration function (mirroring `location_config.rs`'s existing migration test
style). The `.deb` supersession behavior (FR-004b) can't be meaningfully unit-tested in
Rust — validated via the documented manual QA scenario in `quickstart.md` instead
(install old package → install new package → assert exactly one is present/enabled).

**Target Platform**: Linux, COSMIC desktop (Wayland), Debian/apt-based packaging —
unchanged.

**Project Type**: Existing Rust workspace (daemon + CLI + GUI, 6 crates + 1 tool) plus
non-code surfaces this feature also touches: repository-wide docs, packaging metadata,
the local folder name, and GitHub repository settings. No new crate, module, or project
type is introduced.

**Performance Goals**: N/A — FR-007 requires zero functional/behavioral change; nothing
runtime-performance-sensitive is touched.

**Constraints**:
- FR-007: no functional behavior change — the existing test suite's assertions do not
  change, only names/comments may.
- FR-004a: zero data loss migrating any existing installation's settings.
- FR-004b: a single `apt install`/upgrade supersedes the old package; the two daemons
  (old and new identifiers) must never both be enabled at once (constitution Principle
  I).
- Repository/folder rename (FR-002, FR-003) requires manual user action — this
  environment has no `gh`/GitHub API write access, and moving the live working
  directory out from under an active session/editor/build cache is a deliberate,
  user-coordinated step, not something to script unattended.

**Scale/Scope**: 6 workspace crates + 1 tool (`tools/generate-starter-pack`); ~30 files
with an in-scope old-name reference in prose (research.md R1 defines exactly which);
4 `cosmic-config` application IDs; 1 D-Bus bus/interface name; 1 systemd unit; 1
`.desktop` entry; 1 Debian package name; 1 GitHub repository.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|---|---|---|
| I. Exclusive Ownership | **PASS** | FR-004b's `Replaces`/`Conflicts`/`Breaks` is the mechanism that guarantees the old daemon is disabled (via its own existing `prerm`) before the new one can be enabled — the two never run concurrently. |
| II. Wayland-Native | N/A | No rendering/protocol code touched. |
| III. GPU Crossfade | N/A | No rendering code touched. |
| IV. cosmic-config, Not Bespoke | **PASS** | Storage mechanism is unchanged — only the application-ID string each store is keyed by changes. The migration itself is implemented with `cosmic-config`'s own APIs, not a new format. |
| V. Solar/Time Logic Purity | N/A | No solar/scheduling logic touched (FR-007). |
| VI. Two Scheduling Modes | N/A | Unaffected. |
| VII. Per-Output Correctness | N/A | Unaffected. |
| VIII. Failures Contained | **PASS** | Migration is a no-op (not an error) when there's nothing to migrate (spec Edge Cases); it's copy-forward, never destructive to the old store, so a failed/partial migration can't corrupt data — worst case it just doesn't complete and is retried idempotently on the next run. |
| IX. Native COSMIC Look | **PASS** | Unaffected — only display strings (window title, `.desktop` `Name`) change, not toolkit usage. |
| X. Versioned Config Migration Path | **PASS, extended** | This isn't an in-place schema-version bump like the existing v2→v3 precedent, but the same rigor applies: automatic, no silent misinterpretation of old-format data, documented in release notes (FR-008). Data-model.md documents each migration explicitly. |
| XI. Session Integration & Packaging | **PASS** | Systemd unit and `.desktop` entry are renamed consistently; FR-004b's package supersession is precisely this principle's existing "cleanly superseding" pattern (already required for `cosmic-bg`), applied to superseding this project's own prior package. |

No unjustified violations — Complexity Tracking is empty.

**Post-Design re-check** (after Phase 1): `data-model.md`'s migration concept and
`contracts/config-migration.md`'s never-mutate-the-old-store rule are direct
implementations of the Principle VIII/X rows above, not new surface area — nothing in
Phase 1 design introduced a gate that wasn't already accounted for. Table unchanged.

## Project Structure

### Documentation (this feature)

```text
specs/009-project-rename/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md         # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/            # Phase 1 output
│   ├── identifier-rename-map.md
│   └── config-migration.md
└── tasks.md              # Phase 2 output (/speckit-tasks — not this command)
```

### Source Code (repository root)

No new crates, modules, or directories are introduced. Every existing crate keeps its
current role and internal layout; this feature changes *names*, not structure. The
concrete old→new identifier mapping (binary names, crate package names, app IDs,
package name, folder name) is the single most load-bearing artifact for `/speckit-tasks`
and lives in `contracts/identifier-rename-map.md` rather than being duplicated here.

```text
dynamic-wallpaper/                     → cosmic-dynamic-wallpaper/   (FR-002, manual step)
├── crates/
│   ├── schedule-engine/               (package name unchanged — internal-only, never
│   │                                    published, no user/system-facing surface)
│   ├── pack-loader/                   (package name unchanged — same reasoning; owns
│   │                                    the Registry/RemovedStarterPacks config IDs
│   │                                    that DO rename, per contracts/)
│   ├── wallpaper-ipc/                 (package name unchanged — same reasoning; owns
│   │                                    the Renderer/Location config IDs and the D-Bus
│   │                                    name constants that DO rename)
│   ├── renderer/                      (package name unchanged; produces the
│   │   └── src/bin/wallpaperd.rs      →  src/bin/cosmic-wallpaperd.rs)
│   ├── wallpaperctl/                  (package name unchanged; binary renamed)
│   │   └── [[bin]] name = "wallpaperctl" → "cosmic-wallpaperctl"
│   └── wallpaper-settings/            (package name unchanged; binary renamed)
│       └── [[bin]] name = "wallpaper-settings" → "cosmic-wallpaper-settings"
├── packaging/
│   ├── desktop/com.system76.CosmicWallpaperSettings.desktop
│   │   → com.system76.CosmicDynamicWallpaperSettings.desktop
│   └── systemd/wallpaperd.service → cosmic-wallpaperd.service
└── README.md, .specify/memory/constitution.md, crate READMEs  (prose only)
```

**Structure Decision**: This is a pure rename across the existing structure — no crate
is added, removed, merged, or relocated. Internal Cargo *package* names
(`schedule-engine`, `pack-loader`, `wallpaper-ipc`, `renderer`, `wallpaperctl`,
`wallpaper-settings`) are deliberately left unchanged: they're workspace-internal only
(never published to crates.io — confirmed in spec.md's Assumptions), never seen by an
end user, and renaming them would be pure churn across every `Cargo.toml`
`[dependencies]` block with zero user-visible benefit. Only the artifacts a user or
another process actually *sees* — compiled binary names, D-Bus/`cosmic-config`/
`.desktop` identifiers, the `.deb` package name, the folder, and the GitHub repo — are
renamed. This keeps FR-001–FR-004's intent ("every aspect...throughout") satisfied at
every externally-visible surface while keeping the internal diff (and regression risk
against FR-007) as small as it can be.

## Complexity Tracking

*No entries — no constitution violations requiring justification.*
