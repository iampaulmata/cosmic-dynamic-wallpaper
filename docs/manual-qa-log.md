# Manual QA Log

Consolidated record of manual-QA-only checks across specs 3, 5, and 6 — the checks
neither automated testing nor this dev environment's own limits could close on their
own (spec 7 US4, FR-019, tasks.md T035/T036). This file doesn't re-define what each
check is; see the cited spec's own `quickstart.md`/`tasks.md` for the full procedure.
Each entry records what was actually run, when, and against what — including entries
that are honestly still blocked, not just the ones that succeeded.

## Spec 5 — Session integration & packaging

**2026-08-14, live COSMIC session (this project's own dev machine)**:

- Autostart within ~2s of a fresh COSMIC session start: confirmed live.
- `wallpaperctl query` returns real answers against the autostarted daemon: confirmed
  live.
- Crash-restart backoff (`StartLimitIntervalSec`/`StartLimitBurst` in `[Unit]`, not
  `[Service]` — a real bug found and fixed during this same pass, see spec 5's own
  tasks.md): 8 consecutive SIGKILL-triggered restarts correctly land in `failed` with
  "Start request repeated too quickly" after the 5th attempt.
- Clean stop leaves no orphan process (verified via `pgrep -x`, not `pgrep -f`, after
  the first attempt self-matched its own command-line argument).
- `cosmic-bg` never double-renders and needs no restore on uninstall: its PID never
  changed across `wallpaperd` start/stop — process-continuity evidence, not a
  screenshot (see the honest gap below).

**Honest gap, T010's original check**: a pixel-level "no double-background flicker"
screenshot comparison wasn't obtainable — `cosmic-screenshot`'s non-interactive portal
call failed (`Portal request didn't succeed: Other`) in that session. Relying instead
on the structural evidence above (an opaque, exclusive `Layer::Background` surface)
plus this project's first real end-user pass (documented in project history), which did
visually confirm the wallpaper applying with no double-render reported. Not
independently re-attempted in this pass (spec 7) — recorded as still open.

**Still blocked, unchanged as of 2026-08-14 (spec 7 pass)**: T022 (a real
`sudo apt install ./target/debian/*.deb` / `sudo apt remove` cycle) and T025 (the final
smoke test, which depends on T022) — this agent's shell has no interactive `sudo`
session, the same constraint hit earlier in this project (`libxkbcommon-dev`'s install
step). **Ready to run whenever the user does it themselves** — quickstart.md step 5 has
the exact commands. Not a task this agent can close unilaterally.

## Spec 6 — Location portal integration

**2026-08-14, live COSMIC session (this project's own dev machine, during spec 6's own
implementation pass)**:

- `org.freedesktop.portal.Location`'s interface is genuinely implemented (not just
  declared) by `xdg-desktop-portal-cosmic`: a real `CreateSession` call reaches actual
  backend logic.
- The full FR-005 degrade path, end to end against a real backend response (not
  simulated): `wallpaperctl location auto` + a real portal `Start` attempt correctly
  produced `AutomaticStatus::Unavailable { reason: "Portal request failed:
  org.freedesktop.portal.Error.NotAllowed: Location services disabled" }`, persisted
  it, and `effective_location()` correctly fell back to the separately-stored manual
  location. Run via a throwaway `examples/portal_smoke.rs` harness (no Wayland/GPU,
  deleted after use) specifically to avoid taking over the live desktop background the
  way running the full `wallpaperd` binary would have.
- The v1→v2 schema migration, confirmed against genuinely pre-existing production
  config on disk (not just a tempdir test).

**Still blocked, unchanged as of 2026-08-14 (spec 7 pass, T036)**: the full
automatic-location *success* path (a real GeoClue2-backed resolution, `automatic_status:
Resolved` with real coordinates) — this dev environment has no GeoClue2 backend
installed (confirmed live: `systemctl status geoclue` reports "Unit geoclue.service
could not be found", `org.freedesktop.GeoClue2` is "not activatable" over D-Bus, no
`geoclue-2.0` package). **Attempted and confirmed blocked, not silently skipped**:
spec 7 T034 adds `Recommends: geoclue-2.0` to the Debian packaging metadata so a fresh
install *can* offer this out of the box on a distro that carries the package, but
validating the actual resolved-value success path needs a machine with GeoClue2
present — not this one. Every component up to the portal boundary is real and
live-verified (above); only the final "GeoClue2 actually answers with coordinates" hop
remains unconfirmed here.

## Spec 7 — V1 completion

Its own manual-QA items (the GUI's rendered appearance, the starter pack's zero-config
first run, IP-geolocation's live STUN/`maxminddb` happy path) are tracked in
`specs/007-v1-completion/quickstart.md`, not duplicated here — this file is the
cross-spec index T035/T036 asked for, not a replacement for each spec's own quickstart.
