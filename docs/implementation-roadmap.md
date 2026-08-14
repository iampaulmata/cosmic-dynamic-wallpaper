# Implementation Roadmap: Specs 5, 6, 7

**Status**: Proposed, awaiting confirmation
**Created**: 2026-08-14
**Covers**: `specs/005-session-integration-packaging/`, `specs/006-location-portal-integration/`,
`specs/007-v1-completion/` — the three planned-and-tasked-but-unimplemented specs left after
specs 1–4 (fully implemented) and spec 3's 5 gaps (closed 2026-08-14).

This is a sequencing plan, not a re-plan — it takes each spec's already-generated `tasks.md` as
fixed and decides the *order* to implement them in, plus a regression-testing discipline applied
at every phase boundary so refactoring already-shipped code (spec 7 does this heavily) never
silently breaks specs 1–6.

## 1. Dependency graph

```
specs 1–4 (implemented)   spec 3's 5 gaps (closed)
        │                          │
        └──────────────┬───────────┘
                        │
              ┌─────────┴─────────┐
              │                   │
          Spec 5              Spec 6
      (packaging,          (location portal,
       systemd)             LocationConfig v1→v2)
              │                   │
              └─────────┬─────────┘
                        │
                     Spec 7
       (needs BOTH: extends spec 6's v2 schema to v3
        in wallpaper-ipc; amends spec 5's postinst
        for starter-pack registration; US4's manual QA
        directly re-verifies specs 5 and 6's own gaps)
```

**Hard constraints** (verified against each spec's actual `data-model.md`/`tasks.md`, not
assumed):

- Spec 7's Foundational phase (`tasks.md` T009/T011) literally cannot run until spec 6's
  `LocationConfigEntry` v2 (`LocationMode::{Manual, Automatic}`, `AutomaticStatus`) exists in
  `crates/renderer`/`crates/wallpaperctl` — spec 7 extends that exact type to v3, in place.
- Spec 7 US2's T044 (amend `packaging/debian/postinst` to register the starter pack) needs spec
  5's `postinst` script to exist first — there's nothing to amend otherwise.
- Spec 7 US4's T035/T036 (dated manual-QA evidence for spec 5's install lifecycle and spec 6's
  GeoClue happy path) are literally re-verifications of specs 5 and 6 — both must already be
  real, running features before those tasks mean anything.

**Soft/no constraint**: Specs 5 and 6 do not depend on each other at all — different crates
(spec 5 touches only `Cargo.toml` metadata + a new `packaging/` directory; spec 6 touches
`crates/renderer`/`crates/wallpaperctl` source). Either can go first.

## 2. Recommended order: Spec 5 → Spec 6 → Spec 7

**Spec 5 first.** Specs 1–4 are already fully implemented, so spec 5 alone — with zero new
application logic (its own plan.md: "this spec adds no new Rust application code") — turns
already-working code into a real, installable, autostarting package. Landing it first means
specs 6 and 7's own manual QA happens against a systemd-managed, actually-installed `wallpaperd`
instead of a manually-run `cargo run` / `wallpaperd &` process, the way every prior manual smoke
test in this project has had to work around. It also front-loads spec 5's one real external
dependency on the user: a `.deb` build/install pass needs real `sudo`, which this agent's shell
doesn't have (this project's own history — the `libxkbcommon-dev` gotcha was fixed by the user
running `sudo apt install` on their own terminal, not by this agent). Surfacing that requirement
first, rather than last, avoids it becoming a surprise blocker right before "done."

**Spec 6 second.** The location portal feature, on top of a now-installed package. Spec 6's own
plan/research already did the hard discovery work (live-spiked the portal, confirmed
`xdg-desktop-portal-cosmic` really implements it, confirmed GeoClue2 isn't installed on this dev
machine) — this phase is "build what was already scoped," lower discovery risk than spec 5's
first-ever real packaging pass or spec 7's first-ever GUI.

**Spec 7 last**, since it structurally needs both. Internally, spec 7 is sequenced MVP-first per
its own `tasks.md`: Foundational (the `wallpaper-ipc` refactor) → US1 (GUI) + US4 (gap closure,
independent of Foundational) → US2 (starter pack) → US3 (IP-geolocation).

If you'd rather see the location feature land before packaging (e.g. to validate the automatic-
location UX before investing in `.deb`/systemd work), swapping to **Spec 6 → Spec 5 → Spec 7** is
equally valid — nothing in spec 6 depends on spec 5. Flagging the choice rather than silently
picking one.

## 3. Phase-by-phase plan

### Phase 0 — Baseline (before touching anything)

- `cargo test --workspace && cargo clippy --workspace -- -D warnings` — confirm the current
  157-test, clippy-clean baseline (per project memory) is still true right now, before any new
  work. Any pre-existing failure gets fixed *before* Phase 1 starts, so later phases have a clean
  signal for "did my change break something."
- Record the current `cargo llvm-cov --workspace` percentage as the baseline to defend, not just
  a number to beat.

### Phase 1 — Spec 5: Session Integration & Packaging (25 tasks, 4 user stories)

`specs/005-session-integration-packaging/tasks.md`

1. Setup + Foundational (packaging scaffolding, `cargo-deb` tooling).
2. **US1 (P1, MVP)** — systemd unit: autostart on login, clean stop on logout, bounded
   crash-restart.
3. **US2 (P1)** — live-verify (not just trust research.md's reasoning) that `cosmic-bg` really
   can't double-render alongside an autostarted `wallpaperd`.
4. **US3 (P2)** — uninstall leaves a normal desktop background, not a black screen.
5. **US4 (P3)** — the actual `.deb` build + install pass. **Requires the user**: this agent's
   shell has no passwordless `sudo`; building/installing/testing the real package needs to happen
   on the user's own terminal, the same pattern as this project's first real end-user validation
   pass (2026-08-13/14, documented in project memory).

**Regression gate before moving on**: `cargo test --workspace` still green (specs 1–4's ~157
tests, unchanged); re-run specs 1–4's own manual smoke-test flow (register → assign → location
set → observe on screen) but now through the *installed* package/systemd unit instead of manual
binaries, confirming packaging didn't change runtime behavior.

### Phase 2 — Spec 6: Location Portal Integration (34 tasks, 4 user stories)

`specs/006-location-portal-integration/tasks.md`

1. Foundational — `LocationConfigEntry` v1→v2, `effective_location()`.
2. **US1 (P1, MVP)** — portal resolution success path (`ashpd`, `portal_location.rs`, calloop
   wiring, `scheduler_bridge.rs` amendment).
3. **US2 (P1, MVP)** — graceful degrade (this dev machine's own live-observed
   `"Location services disabled"` path — real, not simulated).
4. **US3 (P2)** — live location updates via the ongoing `LocationUpdated` subscription.
5. **US4 (P2)** — CLI mode/status visibility (`location auto`/`manual`/extended `get`) — fully
   parallel-safe with US1–US3 per that spec's own tasks.md.

**Regression gate before moving on**: full workspace test suite green; re-run spec 5's install/
autostart smoke test to confirm the now-installed `wallpaperd` still starts cleanly with the new
`LocationConfig` entry present (an empty/default one, since nothing's configured it yet) —
confirms the v1→v2 migration path is exercised for real, not just in a unit test.

### Phase 3 — Spec 7: V1 Completion (59 tasks, 4 user stories)

`specs/007-v1-completion/tasks.md`

This phase is the highest-risk of the three — two new crates, a refactor of already-shipped
code, and this project's first GUI. Treat its own internal phase boundaries as real checkpoints,
not just task-list bookkeeping:

1. **Setup + Foundational** — the `wallpaper-ipc` extraction (T008–T018). This is a refactor of
   already-tested spec 3/4 code with one explicit acceptance bar stated in spec 7's own
   `tasks.md`: **every existing `renderer`/`wallpaperctl` test still passes, unchanged, with zero
   behavior difference.** Don't treat "new tests pass" as sufficient here — the old ones not
   regressing is the actual bar. Also lands the `LocationConfig` v2→v3 bump and the
   previously-fictional "configurable" crossfade duration (plan.md finding 3) becoming real.
2. **US1 + US4 (both P1, MVP)** — the GUI and the hardening/gap-closure work, which its own
   tasks.md confirms can run in parallel with each other and (for US4) even in parallel with
   Foundational.
3. **US2 (P2)** — starter pack; also independent of Foundational per spec 7's own dependency
   analysis, could realistically land earlier than listed here if you want to parallelize.
4. **US3 (P3)** — IP-geolocation, needs Foundational's v3 schema.

**Regression gate at the end**: this is the big one. Full workspace test suite green; full
`RUSTFLAGS="-W missing_docs" cargo build --workspace` (this project's own history: narrower
per-crate checks have missed real gaps before — spec 2's `pack-loader` had undocumented items a
crate-local check didn't catch); re-run **every prior spec's** manual smoke-test flow end to end
one more time — spec 4's original CLI flow, spec 5's install/autostart, spec 6's location
modes — now via both the CLI *and* the new GUI, confirming FR-007's "GUI and CLI never disagree"
holds against real `cosmic-config` state, not just the unit tests that assert it structurally.

## 4. Regression-testing discipline (applies at every phase boundary above, not just the end)

1. **`cargo test --workspace` after every phase, not just every spec.** A phase that only
   touches one crate can still break another via a shared schema (exactly the bug class spec 7's
   `wallpaper-ipc` extraction exists to prevent structurally — but until that extraction lands in
   Phase 3, specs 5/6 still carry the old duplicated-type risk spec 7's research.md R2
   documents).
2. **Never accept "new tests pass" alone as a phase's exit criterion once prior specs exist to
   regress.** Explicitly diff the test count and check for skipped/ignored tests, not just green
   CI — a shrinking test count with green CI is a silent regression.
3. **Consolidate each spec's manual `quickstart.md` smoke checks into one running checklist**
   (`docs/regression-checklist.md`, new — not created by this roadmap, but proposed here) — after
   each phase, append that spec's manual steps, and re-run the *entire accumulated list* at the
   next phase's regression gate, not just the newest spec's steps. This is what actually catches
   "spec 7's refactor broke spec 4's `location set`" instead of only catching "spec 7's own new
   features work."
4. **`cargo clippy --workspace -- -D warnings` and the full-workspace `missing_docs` build stay
   gates, not suggestions**, at every phase boundary — matching the CI posture already
   established for specs 1–4.
5. **Coverage is a floor, not a target**: re-run `cargo llvm-cov --workspace` after each phase and
   treat any drop from the Phase 0 baseline as a finding to explain, not just a number to note.

## 5. Open items before starting

- **Order confirmation**: Spec 5 → 6 → 7 (recommended above) vs. Spec 6 → 5 → 7 — either is
  dependency-safe; this is a value-sequencing choice, not a technical one.
- **Spec 5 US4's real `.deb` install pass** will need you (not this agent) to run the actual
  `sudo`-requiring steps on your own terminal, same as this project's first end-user validation
  pass — flagging now so it's expected, not a surprise mid-phase.
- **Spec 7's finding 2 (STUN for public-IP discovery)**: already confirmed acceptable by you —
  no further action needed, just noting it's specifically Phase 3 (US3) work, several sessions
  away from Phase 1.
