# Quickstart: Validating Location Portal Integration

Split the same way spec 4 was: the config-mode-toggle logic (`LocationConfigEntry` v2, the v1→v2
migration, `effective_location()`, `wallpaperctl`'s new subcommands) is fully headless-testable.
The live portal subscription itself is manual-QA-verified against a real COSMIC session — the
same split spec 3 already established for Wayland/GPU code.

## Prerequisites

- A stable Rust toolchain, same workspace as specs 1–5.
- No compositor/GPU/portal needed for the automated suite below.
- For the manual portal check: a real COSMIC session (`xdg-desktop-portal` +
  `xdg-desktop-portal-cosmic` running — confirmed present on Pop!_OS 24.04, research.md R1). A
  GeoClue2 backend is *not* required to validate the degrade path (FR-005) — this project's own
  dev machine has none, and that's exactly the scenario worth validating first (research.md R2).

## Run the automated test suite

```sh
cd crates/renderer && cargo test
cd crates/wallpaperctl && cargo test
```

Expected outcome, covering:

- v1 → v2 migration: a hand-written v1 RON entry loads as `mode: Manual`, same `location`,
  `automatic_location: None`, `automatic_status: Unresolved` (data-model.md Migration).
- `effective_location()`'s three branches: `Manual` mode returns `location`; `Automatic` mode
  with a resolved value returns `automatic_location`; `Automatic` mode unresolved/unavailable
  falls back to `location`, then to `None` (data-model.md).
- `wallpaperctl location auto`/`manual` round-trip correctly against a `tempfile`-backed
  `cosmic-config` instance, matching spec 2/4's existing test precedent — including idempotency
  (`auto` called twice) and no-value-stored (`manual` with nothing in `location`) cases
  (contracts/wallpaperctl-location-cli.md).
- `wallpaperctl location set` continues to validate via spec 1's `Location::new` and now also
  asserts it flips `mode` to `Manual` (research.md R7).
- `wallpaperctl location clear` continues to leave `mode`/`automatic_*` untouched (research.md
  R7 — a regression test for the "don't expand `clear`'s scope" decision).
- `wallpaperctl location get --json` output shape matches contracts/wallpaperctl-location-cli.md
  exactly (`mode`, `status`, effective `location`, plus `manual_location`/`automatic_location`).

## Manual smoke check (requires a real COSMIC session, `wallpaperd` running)

Unlike spec 4's quickstart (which was blocked entirely until spec 3 existed), **this check is
already partially runnable today**, before any of this spec's code exists — steps 1–3 below were
run live against this project's own dev machine during planning (research.md R1/R2) and are
reproduced here as the expected baseline.

```sh
# 1. Confirm the portal interface is real, not just declared (research.md R1)
busctl --user introspect org.freedesktop.portal.Desktop /org/freedesktop/portal/desktop \
    org.freedesktop.portal.Location

# 2. Confirm what a live CreateSession call actually returns on this machine (research.md R1/R2)
busctl --user call org.freedesktop.portal.Desktop /org/freedesktop/portal/desktop \
    org.freedesktop.portal.Location CreateSession a{sv} 2 \
    session_handle_token s smoketest distinct_name_token s smoketest

# --- once this spec's code exists ---

# 3. Enable automatic mode (no daemon required to set the toggle, FR-001)
wallpaperctl location auto

# 4. Start wallpaperd and watch it attempt resolution
wallpaperd &
wallpaperctl location get     # expect: mode automatic, status unavailable/resolved depending on backend

# 5. Confirm a solar-anchored pack degrades cleanly if unresolved (FR-005), or schedules
#    correctly if a GeoClue backend is actually present and location services are enabled (US1)
wallpaperctl query --output <your-output>
```

**Expected outcome on this project's own dev machine** (research.md R1/R2, steps 1–2 already
verified live during planning): step 1 lists `CreateSession`/`Start`/`LocationUpdated` — the
interface is real. Step 2 returns `"Location services disabled"`, not a D-Bus
unknown-interface error. Once step 3–5's code exists: `wallpaperctl location get` should report
`status: unavailable (Location services disabled)` and `location: no location available` (or
whatever manual value is separately stored, per `effective_location()`'s fallback) — this is the
FR-005 degrade path validated against a real backend response, not a simulated one.

**Expected outcome on a machine with GeoClue2 installed and location services enabled**: step 4's
`wallpaperctl location get` should report `status: resolved` with real coordinates, and step 5's
query should reflect a solar-anchored pack scheduled against them (US1) — this project's own dev
machine cannot validate this half without installing GeoClue2 first (research.md R2's cross-spec
packaging note), so it remains a documented expectation, not yet independently confirmed live,
same honest-caveat posture spec 3's README already uses for its own untested branches.

## What "done" looks like for this spec

See spec.md's Success Criteria (SC-001–SC-005). The automated suite closes out the config/
migration/CLI logic (headless). Full SC-001 (auto-resolve a working schedule) additionally
requires a machine with a working GeoClue2 backend and location services enabled to observe
end-to-end — not available in this project's own dev environment as of this planning pass (R2).
SC-002 (graceful degrade) is the one criterion this project's own dev environment can validate
completely and has already partially demonstrated live, before implementation even starts.
