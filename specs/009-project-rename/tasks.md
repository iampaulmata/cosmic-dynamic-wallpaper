---

description: "Task list for renaming the project to \"Cosmic Dynamic Wallpaper\""
---

# Tasks: Rename Project to "Cosmic Dynamic Wallpaper"

**Input**: Design documents from `/specs/009-project-rename/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/,
quickstart.md (all present)

**Tests**: Not explicitly requested as a TDD approach; test tasks below are included
only where `contracts/config-migration.md` explicitly requires them (the 4 migration
functions) and for build/CI verification (FR-007) — not as a blanket policy.

**Organization**: Tasks are grouped by the three user stories in `spec.md`, each
independently implementable and testable. Every task cites the exact
`contracts/identifier-rename-map.md` section it implements, so no task requires
re-deriving an old→new value from scratch.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: US1, US2, or US3 — omitted for Setup/Foundational/Polish

---

## Phase 1: Setup

- [X] T001 Create the implementation branch `009-project-rename` off `main` (this
      project's own established convention — never work directly on `main`)
- [X] T002 [P] Capture a pre-rename baseline: run `cargo build --release --workspace`,
      `cargo test --workspace`, and `cargo clippy --workspace --all-targets`; record the
      pass/fail outcome to diff against after the rename (FR-007's "no functional
      behavior change" is only checkable against a concrete baseline)

---

## Phase 2: Foundational

No foundational/blocking tasks. The three user stories below touch disjoint file sets
by design (plan.md's Structure Decision: US1 is docs/GitHub/folder, US2 is source-level
identifiers + packaging, US3 is code-comment/crate-README prose) — proceed directly to
the user story phases; none of them block another.

---

## Phase 3: User Story 1 - Consistent Public Branding (Priority: P1) 🎯 MVP

**Goal**: Every public-facing surface (GitHub repo, README, constitution, top-level
docs, local folder) reads "Cosmic Dynamic Wallpaper" consistently — no code touched.

**Independent Test**: Clone the repository fresh under its new URL, open the README,
and confirm the project name is consistent across the repo name, folder name, and
document titles/prose (spec.md US1's own Independent Test).

**Note**: README.md's own `# Cosmic Dynamic Wallpaper` title line was already changed
in an earlier, unrelated commit (`0792e98`) — confirmed via `git log -p`. Do not
re-touch line 1; only the remaining old-name references below are outstanding.

### Implementation for User Story 1

- [X] T003 [P] [US1] Update remaining old-name prose in `README.md` — the two
      `github.com/iampaulmata/rust-dynamic-wallpaper` URL references and the
      `dynamic-wallpaper_*.deb` install-command example (contracts/
      identifier-rename-map.md §1–2). Do NOT touch the "Cinnamon's **Dynamic
      Wallpaper** extension" or "Apple's `.heic` dynamic-wallpaper metadata format"
      references — those name other projects/formats, not this one (research.md R1)
- [X] T004 [P] [US1] Amend `.specify/memory/constitution.md` via `/speckit-constitution`:
      rename the document's title ("Dynamic Wallpaper Constitution" →
      "Cosmic Dynamic Wallpaper Constitution") and the Governance section's "the
      `dynamic-wallpaper` project" reference — a PATCH-level wording amendment
      (1.0.0 → 1.0.1) with an updated Sync Impact Report per the constitution's own
      amendment procedure, not a plain hand-edit
- [X] T005 [P] [US1] Update `docs/PRD.md` to use the new project name throughout
      (research.md R1 scope)
- [X] T006 [P] [US1] Update `docs/pack-manifest-schema.md` to use the new project name
      throughout (research.md R1 scope)
- [ ] T007 [US1] Rename the GitHub repository from `rust-dynamic-wallpaper` to
      `cosmic-dynamic-wallpaper` — manual step via GitHub's own Settings UI; this
      environment has no `gh`/API write access (research.md R6). **Requires explicit
      user confirmation immediately before doing it.**
- [ ] T008 [US1] Update the local `origin` git remote URL to the renamed repository
      (depends on T007)
- [ ] T009 [US1] Rename the local project folder from `dynamic-wallpaper` to
      `cosmic-dynamic-wallpaper` (research.md R7). **Do this last, after every other
      task in this feature is committed** — it changes the working directory every
      subsequent command runs from, and requires explicit user confirmation
      immediately before doing it.

**Checkpoint**: US1 is independently complete and testable per its Acceptance
Scenarios — no dependency on US2 or US3.

---

## Phase 4: User Story 2 - Existing Installations Keep Working (Priority: P2)

**Goal**: Every system-facing identifier (binaries, D-Bus, `cosmic-config` app IDs,
`.desktop`, systemd unit, `.deb` package) is renamed; existing installations upgrade
with zero data loss and the old/new daemons are never both enabled at once.

**Independent Test**: Install the pre-rename package, configure a location and a pack
assignment, upgrade to the post-rename package, and confirm the configuration survives
untouched with no user action, and exactly one daemon package ends up installed and
enabled (spec.md US2's own Independent Test; `quickstart.md`'s SC-004/SC-005 sections).

### Implementation for User Story 2

**Binaries** (contracts/identifier-rename-map.md §3):

- [ ] T010 [P] [US2] Rename `crates/renderer/src/bin/wallpaperd.rs` →
      `crates/renderer/src/bin/cosmic-wallpaperd.rs`
- [ ] T011 [P] [US2] Rename the `wallpaperctl` binary to `cosmic-wallpaperctl`: update
      `[[bin]] name` in `crates/wallpaperctl/Cargo.toml` and the
      `#[command(name = ..., about = ...)]` attribute in
      `crates/wallpaperctl/src/main.rs` (contracts §3, §9)
- [ ] T012 [P] [US2] Rename the `wallpaper-settings` binary to
      `cosmic-wallpaper-settings` via `[[bin]] name` in
      `crates/wallpaper-settings/Cargo.toml` (contracts §3)

**D-Bus identifiers** (contracts §4 — the two files must stay byte-identical):

- [X] T013 [US2] Rename `BUS_NAME`/`OBJECT_PATH`/`INTERFACE` constants in
      `crates/wallpaper-ipc/src/dbus_client.rs`
- [X] T014 [US2] Rename the matching constants and the
      `#[zbus::interface(interface = "...")]` attribute in
      `crates/renderer/src/dbus_service.rs` (must match T013 exactly)

**`cosmic-config` application IDs + migrations** (data-model.md,
contracts/config-migration.md):

- [X] T015 [P] [US2] Rename `RENDERER_CONFIG_ID` in
      `crates/wallpaper-ipc/src/renderer_config.rs` and implement
      `migrate_from_old_app_id` for `RendererConfig` per
      contracts/config-migration.md's behavior contract, with its 4 required test cases
- [X] T016 [P] [US2] Rename `LOCATION_CONFIG_ID` in
      `crates/wallpaper-ipc/src/location_config.rs` and implement
      `migrate_from_old_app_id` for `LocationConfigEntry` per
      contracts/config-migration.md, with its 4 required test cases
- [X] T017 [P] [US2] Rename `REGISTRY_CONFIG_ID` in `crates/pack-loader/src/registry.rs`
      and implement `migrate_from_old_app_id` for `RegistryConfig` per
      contracts/config-migration.md, with its 4 required test cases
- [X] T018 [P] [US2] Rename `REMOVED_STARTER_PACKS_CONFIG_ID` in
      `crates/pack-loader/src/registry.rs` and implement `migrate_from_old_app_id` for
      `RemovedStarterPacksConfig` per contracts/config-migration.md, with its 4 required
      test cases
- [X] T019 [US2] Wire every existing load call site (`wallpaperd`'s and
      `wallpaper-settings`'s startup paths, and any other current `::load`/`::open`
      caller for these four stores) to call the new `migrate_from_old_app_id` functions
      from T015–T018 in place of a bare load (depends on T015–T018)

**`.desktop` entry** (contracts §6):

- [X] T020 [P] [US2] Rename
      `packaging/desktop/com.system76.CosmicWallpaperSettings.desktop` →
      `com.system76.CosmicDynamicWallpaperSettings.desktop` and update its `Name=`
      (`Cosmic Dynamic Wallpaper Settings`), `Comment=`, and `Exec=` (→
      `cosmic-wallpaper-settings`, matching T012) fields
- [X] T021 [P] [US2] Update the `APP_ID` constant in
      `crates/wallpaper-settings/src/app.rs` to
      `com.system76.CosmicDynamicWallpaperSettings` (matches T020)

**systemd unit** (contracts §8):

- [X] T022 [P] [US2] Rename `packaging/systemd/wallpaperd.service` →
      `cosmic-wallpaperd.service` and update its `Description=`, `Documentation=`, and
      `ExecStart=` (→ `/usr/bin/cosmic-wallpaperd`, matching T010) fields
- [X] T023 [US2] Update the unit filename referenced in `packaging/debian/postinst` and
      `packaging/debian/prerm`'s `systemctl --user --global {enable,disable}
      wallpaperd.service` calls to `cosmic-wallpaperd.service` (depends on T022)

**Debian packaging** (contracts §7):

- [X] T024 [US2] In `crates/renderer/Cargo.toml`'s `[package.metadata.deb]`: rename
      `name` to `cosmic-dynamic-wallpaper`; add `replaces`, `conflicts`, and `breaks`
      all set to `"dynamic-wallpaper"`; update `extended-description`; and update every
      `assets` path to the renamed binaries/`.desktop`/systemd-unit filenames (depends
      on T010–T022 all being complete, since this task references their output paths)

**Checkpoint**: US2 is independently complete and testable per `quickstart.md`'s
SC-004/SC-005 sections — no dependency on US1 or US3.

---

## Phase 5: User Story 3 - Contributor-Facing Consistency (Priority: P3)

**Goal**: No contributor-visible reference to the old name remains anywhere in the
codebase outside frozen historical spec records (specs/001–008).

**Independent Test**: Full-text search the repository for the old project name outside
of historical content and confirm no unintentional matches remain (spec.md US3's own
Independent Test; `quickstart.md`'s SC-001 section).

### Implementation for User Story 3

- [X] T025 [P] [US3] Update the `description` field in all 6 crates' `Cargo.toml`
      (`crates/{renderer,wallpaperctl,wallpaper-settings,schedule-engine,pack-loader,
      wallpaper-ipc}/Cargo.toml`) to the new project name
- [X] T026 [P] [US3] Update `crates/{schedule-engine,pack-loader,wallpaper-ipc,
      renderer,wallpaperctl,wallpaper-settings}/README.md` to the new project name
- [X] T027 [P] [US3] Update the D-Bus name references in `crates/wallpaperctl/README.md`
      and `specs/004-cli-control-surface/contracts/wallpaperd-dbus-interface.md` to
      match T013/T014's renamed values (a living reference doc describing the current
      interface, not frozen history — research.md R1)
- [X] T028 [P] [US3] Update project-name comments in `crates/*/src/lib.rs` (all 6
      crates), `crates/renderer/src/{starter_pack,surface,ip_geolocation}.rs`,
      `crates/wallpaperctl/src/main.rs`, `crates/wallpaper-settings/src/main.rs`, and
      `tools/generate-starter-pack/src/main.rs`
- [X] T029 [US3] Full-repo audit: run the SC-001 grep from `quickstart.md` and fix any
      remaining unintentional match it turns up (depends on T003–T028 being complete)

**Checkpoint**: US3 is independently complete and testable — the SC-001 grep returns
empty. No dependency on US1 or US2.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [ ] T030 Re-run `cargo build --release --workspace`, `cargo test --workspace`, and
      `cargo clippy --workspace --all-targets`; diff against T002's baseline and confirm
      an identical pass/fail outcome (FR-007)
- [ ] T031 Run `cargo deb -p renderer --no-build` and validate the produced `.deb`
      against `quickstart.md`'s SC-003 section (correct filename, correct binary/
      desktop/unit paths inside it)
- [ ] T032 Execute `quickstart.md`'s SC-005 package-supersession scenario end to end on
      this dev machine (it already has the old package installed and enabled)
- [ ] T033 Execute `quickstart.md`'s SC-004 migration scenario end to end, confirming
      the location/packs/assignment configured under the old identifiers survive the
      upgrade untouched
- [ ] T034 [P] Prepare a new beta version/release notes for the renamed build, following
      this project's own established release convention (deb revision bump, tag, notes)

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — start immediately.
- **Foundational (Phase 2)**: Empty — nothing blocks the user stories.
- **User Stories (Phase 3–5)**: Each depends only on Setup, not on each other. They may
  be done in any order or in parallel; P1/P2/P3 in the phase numbering is priority
  order, not a hard dependency chain (see Implementation Strategy below for why P2
  shouldn't actually be deferred far behind P1 despite the label).
- **Polish (Phase 6)**: Depends on all three user stories being complete — it validates
  the renamed system end to end, which requires every identifier already renamed.

### Within Each User Story

- **US1**: T003–T006 are independent and parallel; T007 → T008 → T009 form a strict
  chain (repo rename, then remote update, then folder rename) and MUST be last.
- **US2**: T010–T012 (binaries), T015–T018 (migrations), T020–T021 (desktop), T022
  (systemd) are each internally parallel; T013 → T014 is a strict pair; T019 depends on
  T015–T018; T023 depends on T022; T024 depends on the entire rest of the story
  (references every renamed path).
- **US3**: T025–T028 are independent and parallel; T029 (the audit) depends on all of
  them plus US1/US2's file-level work actually having landed.

### Parallel Opportunities

- T003–T006 (US1's doc updates) together.
- T010–T012 (US2's three binary renames) together.
- T015–T018 (US2's four migration implementations) together — different files, same
  contract, no shared state.
- T020–T021 and T022 (US2's desktop and systemd work) together.
- T025–T028 (US3's prose sweep) together.
- Entire user story phases (US1, US2, US3) could be done by three different people in
  parallel, since none blocks another.

---

## Parallel Example: User Story 2's migration functions

```bash
Task: "Rename RENDERER_CONFIG_ID and implement migrate_from_old_app_id in crates/wallpaper-ipc/src/renderer_config.rs"
Task: "Rename LOCATION_CONFIG_ID and implement migrate_from_old_app_id in crates/wallpaper-ipc/src/location_config.rs"
Task: "Rename REGISTRY_CONFIG_ID and implement migrate_from_old_app_id in crates/pack-loader/src/registry.rs"
Task: "Rename REMOVED_STARTER_PACKS_CONFIG_ID and implement migrate_from_old_app_id in crates/pack-loader/src/registry.rs"
```

---

## Implementation Strategy

### MVP First — with a caveat this feature's priority labels don't fully capture

Structurally, US1 (P1) alone is a valid, demoable MVP: complete Setup, then US1, then
stop — the public-facing rename is done and independently correct.

**But**: unlike a typical feature, shipping *only* US1 for any length of time is worse
than shipping nothing, if it means a build goes out with renamed docs pointing at a
still-old-named package. In practice, **US2 should not be deferred far behind US1** even
though it's labeled P2 — it's the story carrying all the backward-compatibility risk
(FR-004a/FR-004b), and the whole point of this feature per spec.md's own User Story 2
priority note is that a half-applied rename is worse than none. Treat "US1 alone,
released" as a valid checkpoint to pause at, not a recommended shipping stop.

### Incremental Delivery

1. Setup → baseline captured.
2. US1 → public branding consistent (docs, README, constitution) — demoable, but hold
   off on the actual GitHub repo/folder rename (T007–T009) until US2 is also ready, so
   the newly-public repo name doesn't outpace an installable package that matches it.
3. US2 → system identifiers renamed, migration + supersession verified via `quickstart.md`.
4. Now do T007–T009 (repo/folder rename) — everything a visitor would find is now
   consistent, code and docs alike.
5. US3 → remaining contributor-facing prose cleanup, then the full SC-001 audit.
6. Polish → full verification pass, new build.

### Parallel Team Strategy

With multiple people: one on US1 (docs/branding), one on US2 (identifiers/migration/
packaging), one on US3 (crate READMEs/comments) — all three can proceed simultaneously
after Setup, converging at Polish.
