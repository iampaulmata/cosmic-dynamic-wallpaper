# Contract: `wallpaperctl` command surface

This is the primary contract for this spec — the actual CLI a person or script drives. Unlike
specs 1–2's Rust-API contracts, this one is a command/argument/output schema.

## Commands

```text
wallpaperctl register <path>
wallpaperctl list packs
wallpaperctl list outputs
wallpaperctl remove <pack-source>
wallpaperctl assign --output <output-id> <pack-source>
wallpaperctl assign --same-everywhere <pack-source>
wallpaperctl location get
wallpaperctl location set <latitude> <longitude>
wallpaperctl location clear
wallpaperctl query [--output <output-id>]
wallpaperctl reevaluate [--output <output-id>]
```

- `register <path>` — FR-001. `<path>` is a directory (manifest pack) or a single image file
  (static pack). Idempotent on an already-registered source (FR-002). Config-only (FR-011) —
  no daemon required.
- `list packs` — FR-003. No arguments. Config-only (FR-011) — no daemon required.
- `list outputs` — FR-005. No arguments. **Requires a running `wallpaperd`** (FR-011,
  corrected from an earlier draft — see spec.md's Assumptions) — there is no persisted record
  of connected outputs anywhere; this reuses the same `QueryAll` D-Bus call as `query`
  (contracts/wallpaperd-dbus-interface.md), reporting just the output identifiers.
- `remove <pack-source>` — FR-004. Deletes the registry entry outright (spec 2 FR-012's
  distinction from automatic "unavailable"). Config-only — no daemon required.
- `assign` — FR-006. Exactly one of `--output <id>` or `--same-everywhere` is required.
  Config-only (FR-011) — no daemon required; assigning to a not-currently-connected output
  name is valid (a "configure ahead of time" case, FR-007) and is not itself checked against
  live output state, even if a daemon happens to be reachable (that case only produces a
  non-fatal warning, never a failure — FR-007).
- `location get|set|clear` — FR-008. `set` takes decimal latitude/longitude. Config-only — no
  daemon required.
- `query [--output <id>]` — FR-009. Omitting `--output` queries every managed output.
  **Requires a running `wallpaperd`.**
- `reevaluate [--output <id>]` — FR-010. Omitting `--output` re-evaluates every managed
  output. **Requires a running `wallpaperd`.**

## Global flags

- `--json` — machine-readable output mode (FR-013), available on every command that returns
  data (`list packs`, `list outputs`, `location get`, `query`). Commands with no return value
  beyond success/failure (`register`, `remove`, `assign`, `location set|clear`, `reevaluate`)
  print a short human-readable confirmation by default and nothing but the exit code under
  `--json` beyond a `{"ok": true}`-shaped acknowledgement.

## Exit codes (FR-012)

⚠️ **Superseded by spec 011's hardening delta**: the table below is this contract's original
exit-code scheme, kept here for history. `specs/011-fix-audit-findings/contracts/
wallpaperctl-cli-hardening.md` renumbers code `2` to mean "usage error" exclusively and moves
"daemon unreachable" to a new code `4` (US7/FR-028/FR-029) — read that file for the current,
authoritative table before relying on any exit code below.

| Code | Meaning |
|---|---|
| 0 | Success |
| 1 | Invalid input / validation failure (e.g. malformed location, unregistered pack) |
| 2 | Daemon unreachable (`list outputs`, `query`, `reevaluate` only, FR-011) — **moved to code 4, see above** |
| 3 | Underlying spec 2/3 error surfaced verbatim (pack load/manifest failure, config I/O failure) |

Every non-zero exit is paired with a specific, actionable message on stderr naming what
failed (data-model.md `CliError`) — never a silent failure (SC-002).

## Example: `--json` output shapes

```json
// wallpaperctl list packs --json
[
  { "name": "Seasons", "source": "/home/user/.local/share/wallpaper-packs/seasons", "status": "known" },
  { "name": "Office View", "source": "/home/user/Pictures/office-view.jpg", "status": "unavailable" }
]

// wallpaperctl query --output DP-3 --json
{ "output": "DP-3", "state": "assigned", "active_image": "dawn.jpg", "next_transition_at": "2026-08-14T06:12:00-04:00" }

// wallpaperctl location get --json
{ "location": { "latitude": 45.5019, "longitude": -73.5674 } }

// wallpaperctl list outputs --json
[ { "output": "eDP-1" }, { "output": "DP-3" } ]
```

A non-fatal warning (e.g. `assign` targeting a name `wallpaperd` doesn't currently manage,
FR-007) is written to stderr and does not affect the exit code (still 0) or the `--json`
acknowledgement shape.

## Explicitly not in this contract

- Any command that changes crossfade duration or other spec 3 rendering parameters — out of
  scope (spec.md Assumptions).
- Any GUI surface — this is the CLI-only interim control path (constitution Principle IX).
- The wire format of the D-Bus calls `query`/`reevaluate` make internally — that's
  contracts/wallpaperd-dbus-interface.md; this file only commits to the CLI's own
  command/output shape, which stays stable even if the underlying transport changes.
- Input validation added after this contract shipped (e.g. `assign --output`'s value now being
  checked for shape, and the `--output`/`--same-everywhere` conflict now routing through
  `CliError::UsageError` instead of a direct `process::exit`) — see
  `specs/011-fix-audit-findings/contracts/wallpaperctl-cli-hardening.md`, which supersedes the
  exit-code table above and is additive everywhere else (no argument shape or `--json` schema
  change).
