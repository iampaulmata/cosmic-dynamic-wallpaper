---

description: "Task list template for feature implementation"
---

# Tasks: Session Integration & Packaging

**Input**: Design documents from `/specs/005-session-integration-packaging/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md — all present.

**Tests**: This spec introduces no new Rust code (plan.md Technical Context) — there is no
`cargo test` surface to add. Verification instead means live-checking the systemd unit and
packaging artifacts against contracts/ and quickstart.md, exactly the manual-QA posture spec 3
established for its own Wayland/GPU code. Those verification steps are woven directly into each
story's task list below rather than a separate "Tests" subsection.

**Organization**: Tasks are grouped by user story (spec.md) to enable independent
implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: Which user story this task belongs to (US1–US4)
- File paths are relative to the repository root

## Path Conventions

No new workspace crate (plan.md Structure Decision) — this spec's own deliverables live under
a new top-level `packaging/` directory, plus `[package.metadata.deb]` additions to
`crates/renderer/Cargo.toml`.

---

## Phase 1: Setup

**Purpose**: Scaffolding shared by every story below.

- [ ] T001 Create the `packaging/systemd/` and `packaging/debian/` directories at the repo root (plan.md Project Structure)

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The one piece of tooling every later story's verification steps need.

**⚠️ CRITICAL**: US4's tasks cannot run until this is done; US1–US3 don't need it.

- [ ] T002 Install `cargo-deb` in the dev/build environment and confirm `cargo deb --version` succeeds (research.md R4)

**Checkpoint**: Packaging scaffolding and tooling ready — user story work can begin.

---

## Phase 3: User Story 1 - The Daemon Starts Itself When You Log In (Priority: P1) 🎯 MVP

**Goal**: `wallpaperd` autostarts on session start, stops cleanly on logout, and recovers from a
crash within the clarified 5-attempt/60s bound — no manual launch step.

**Independent Test**: quickstart.md steps 1–3 (the local, fully-reversible dry run) — enable the
unit locally, confirm `wallpaperd` is running and `wallpaperctl query` returns real answers,
force a crash and confirm it restarts, stop it and confirm no orphan process remains.

### Implementation for User Story 1

- [ ] T003 [US1] Author `packaging/systemd/wallpaperd.service` exactly per contracts/systemd-unit.md
- [ ] T004 [US1] Build release binaries (`cargo build --workspace --release`) and run quickstart.md step 3's local dry-run install (copy the unit to `~/.config/systemd/user/`, point `ExecStart` at the just-built binary, `systemctl --user daemon-reload && enable --now`) — verify FR-001/SC-001: `systemctl --user status wallpaperd.service` shows `active (running)` within 5 seconds, and `wallpaperctl query` returns real answers
- [ ] T005 [US1] Verify FR-003's bounded restart: `systemctl --user kill --signal=SIGKILL wallpaperd.service`, confirm the unit is `active (running)` again within `RestartSec=2` plus normal startup time (quickstart.md step 3)
- [ ] T006 [US1] Verify FR-002's clean stop: `systemctl --user stop wallpaperd.service`, then `pgrep -f wallpaperd` and confirm no orphaned process remains
- [ ] T007 [US1] Verify FR-003's crash-loop bound: force 5+ rapid failures within the 60-second window (repeated `kill -9` faster than `RestartSec`) and confirm the unit lands in `failed` state rather than retrying indefinitely, discoverable via `systemctl --user status wallpaperd.service`
- [ ] T008 [US1] Clean up the local dry-run exactly per quickstart.md (`disable --now`, remove `~/.config/systemd/user/wallpaperd.service`, `daemon-reload`) so this dev machine's real session is left exactly as it was before T004

**Checkpoint**: User Story 1 is fully functional and independently verified — autostart, clean
stop, and bounded crash-restart all demonstrated live, without needing a full package install.

---

## Phase 4: User Story 2 - Installing Doesn't Leave Two Wallpaper Daemons Fighting (Priority: P1)

**Goal**: Confirm — against the real system, not just research.md's reasoning — that `cosmic-bg`
is never visibly competing once `wallpaperd` is running.

**Independent Test**: quickstart.md step 4's first half — with `wallpaperd` running (re-enable
via US1's dry-run steps), confirm `cosmic-bg` is still alive as a process but never visibly
renders anything.

**Depends on**: User Story 1 (needs a running `wallpaperd` instance to observe against).

### Implementation for User Story 2

- [ ] T009 [US2] With `wallpaperd` running (repeat US1's T004 dry-run enable), confirm `pgrep -x cosmic-bg` still reports it alive (quickstart.md step 4) — live evidence for research.md R3's "never stopped, just occluded" claim
- [ ] T010 [US2] Manually verify no visible double-background/flicker artifact appears while `wallpaperd` starts up (FR-004 Scenario 1/2) — a visual smoke check against the real live session, the same posture spec 3 used for its own manual QA
- [ ] T011 [US2] Verify idempotency (FR-005, US2 Scenario 4): re-run `systemctl --user enable --now wallpaperd.service` while it's already running, confirm no error and no duplicate process
- [ ] T012 [P] [US2] Code-review check (FR-004 Scenario 3): confirm neither `packaging/systemd/wallpaperd.service` nor any maintainer script (contracts/debian-package.md) references `cosmic-bg` at all, so its absence on a minimal COSMIC setup cannot cause any failure

**Checkpoint**: User Story 2 verified live — no new code was needed, but its acceptance criteria
are now demonstrated true, not just assumed from research.

---

## Phase 5: User Story 3 - Uninstalling Gives You Your Desktop Background Back (Priority: P2)

**Goal**: Confirm stopping `wallpaperd` alone (no separate "restore" action) leaves a normal,
non-black background.

**Independent Test**: quickstart.md step 4's second half — stop `wallpaperd`, confirm
`cosmic-bg`'s already-running surface becomes visible again with no black screen.

**Depends on**: User Story 1 (needs a running-then-stopped `wallpaperd` instance to observe
against).

### Implementation for User Story 3

- [ ] T013 [US3] With `wallpaperd` running then stopped (build on US1's T004/T006), confirm no black or blank output appears at any point during the stop (FR-007, quickstart.md step 4)
- [ ] T014 [P] [US3] Code-review check (FR-006's scoping): confirm none of `packaging/debian/{postinst,prerm,postrm}` issues any `cosmic-bg`-directed command, mirroring T012's approach for the symmetric uninstall case
- [ ] T015 [P] [US3] Document the "user manually disabled cosmic-bg before install" edge case as structurally moot in `packaging/README.md` (new file), since this project never has a mechanism to have performed that disable itself (research.md R3, spec.md Edge Cases)

**Checkpoint**: User Story 3 verified — install and uninstall are now both confirmed symmetric
and correct against the real system.

---

## Phase 6: User Story 4 - You Can Actually Install It Without Building From Source (Priority: P3)

**Goal**: A real, installable `.deb` package bundling `wallpaperd`, `wallpaperctl`, and the
session unit.

**Independent Test**: quickstart.md steps 2 and 5 — build the `.deb` and inspect its contents
without installing anything, then (deliberately) install it for real and confirm both binaries
and the enabled unit are in place.

**Depends on**: User Story 1 (the unit file it bundles) and Phase 2's `cargo-deb` tooling.

### Implementation for User Story 4

- [ ] T016 [P] [US4] Add `[package.metadata.deb]` to `crates/renderer/Cargo.toml` per contracts/debian-package.md's package-contents table (both binaries as assets, the systemd unit, the maintainer-scripts directory)
- [ ] T017 [P] [US4] Author `packaging/debian/postinst` exactly per contracts/debian-package.md
- [ ] T018 [P] [US4] Author `packaging/debian/prerm` exactly per contracts/debian-package.md
- [ ] T019 [P] [US4] Author `packaging/debian/postrm` exactly per contracts/debian-package.md
- [ ] T020 [US4] Build the package (`cargo deb -p renderer`) and verify its contents/maintainer scripts via `dpkg-deb --contents`/`--info` match contracts/debian-package.md exactly (quickstart.md step 2) — depends on T016–T019
- [ ] T021 [US4] Verify FR-009/idempotency across an upgrade: confirm `postinst`'s enable step runs (and is a no-op) on both fresh install and upgrade, and `prerm`'s disable step runs only on real removal, not on an upgrade-in-progress — a script dry-run/review, not requiring the real `apt install` step — depends on T020
- [ ] T022 [US4] **Deliberate, user-approved step** — perform quickstart.md step 5's real `sudo apt install ./target/debian/*.deb` / `sudo apt remove` cycle on this dev machine to validate the full end-to-end path. Flag for explicit confirmation before running: this is the one task in this spec that registers real, global system state (`systemctl --user --global enable`), unlike every other task above (all fully local/reversible) — depends on T020, T021

**Checkpoint**: All 4 user stories independently functional and verified.

---

## Final Phase: Polish & Cross-Cutting Concerns

**Purpose**: Keep the project's own docs in sync, matching specs 1–4's established practice.

- [ ] T023 [P] Update root `README.md`'s spec status table row for spec 5 to reflect implementation, matching specs 1–4's entries
- [ ] T024 [P] Add an "Implementation pass status" note at the top of this file's own tasks.md (matching spec 3's tasks.md convention) once T001–T022 are done, summarizing what was verified live vs. structurally
- [ ] T025 Run quickstart.md's full guarded sequence end-to-end one final time as a release-readiness smoke test — depends on T001–T022

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — can start immediately.
- **Foundational (Phase 2)**: Depends on Setup completion — BLOCKS User Story 4 only (US1–US3
  need no `cargo-deb` tooling at all, unlike a typical spec where Foundational blocks every
  story).
- **User Story 1 (Phase 3)**: Depends on Setup only. The true foundation for this spec — every
  other story observes a `wallpaperd` instance US1's tasks bring up.
- **User Story 2 (Phase 4)**: Depends on User Story 1 (needs a running instance to observe).
- **User Story 3 (Phase 5)**: Depends on User Story 1 (needs a running-then-stopped instance).
- **User Story 4 (Phase 6)**: Depends on User Story 1 (bundles its unit file) and Foundational
  (needs `cargo-deb`).
- **Polish (Final Phase)**: Depends on all four user stories being complete.

### Within Each User Story

- User Story 1's tasks are strictly sequential (T003→T008) — each verification step depends on
  the live state the previous one left behind.
- User Story 2/3's live-observation tasks (T009–T011, T013) are sequential against the one
  running instance; their code-review tasks (T012, T014, T015) are independent of that live
  state and of each other.
- User Story 4's four artifact-authoring tasks (T016–T019) are fully parallel (different
  files, no shared state); T020–T022 are strictly sequential after them.

### Parallel Opportunities

- T012 (US2) and T014/T015 (US3) can run any time after their story's live-observation tasks,
  in parallel with each other or with a different story's work.
- T016–T019 (US4) can all run in parallel with each other, and with US2/US3's work generally
  (different files, no shared live-system state).
- T023/T024 (Polish) can run in parallel with each other.

---

## Parallel Example: User Story 4

```bash
# Launch all four artifact-authoring tasks together:
Task: "Add [package.metadata.deb] to crates/renderer/Cargo.toml per contracts/debian-package.md"
Task: "Author packaging/debian/postinst per contracts/debian-package.md"
Task: "Author packaging/debian/prerm per contracts/debian-package.md"
Task: "Author packaging/debian/postrm per contracts/debian-package.md"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup
2. Complete Phase 3: User Story 1 (Phase 2's Foundational tooling isn't even needed yet — it
   only blocks US4)
3. **STOP and VALIDATE**: quickstart.md steps 1–3 fully pass, dev machine left clean (T008)
4. This alone already delivers FR-001–FR-003's real value: an autostarting, self-recovering
   daemon, verified live — even before any packaging work exists.

### Incremental Delivery

1. Setup → User Story 1 → validate independently (the MVP: autostart works, verified locally)
2. Add User Story 2 → validate independently (no double background, verified against the real
   system)
3. Add User Story 3 → validate independently (uninstall handoff, verified against the real
   system)
4. Add Foundational (`cargo-deb`) + User Story 4 → validate independently (real installable
   package) — this is the only phase touching global system state, and only in its final,
   explicitly-flagged task (T022)
5. Polish

### Suggested Session Boundaries

Given T022 is the one task in this entire spec that registers real, hard-to-reverse global
system state, a natural stopping point is **T001–T021 in one pass** (everything local,
reversible, and independently verifiable), with T022 (and T025's final smoke test, which
depends on it) run as a deliberate, separately-confirmed step — matching this project's
established caution around exactly this kind of action throughout its development so far.
