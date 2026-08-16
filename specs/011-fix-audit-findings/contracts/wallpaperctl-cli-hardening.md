# Contract Delta: `wallpaperctl` CLI hardening

A **delta** against `specs/004-cli-control-surface/contracts/wallpaperctl-cli.md` (the original
CLI contract). No command's argument shape, success output, or `--json` schema changes; every
change below either tightens validation of a previously-unvalidated input or fixes an exit-code
collision.

## Exit codes (supersedes the original contract's table, US7/FR-028)

| Code | Meaning | Changed? |
|---|---|---|
| 0 | Success | unchanged |
| 1 | Invalid input / validation failure (malformed location, unregistered pack, **invalid `--output` value — new, FR-019**) | extended |
| 2 | Usage error (clap parse errors, **and now also the `--output`/`--same-everywhere` conflict check — moved off `process::exit(1)`, FR-029**) | extended, now consistently means "usage error" for every case |
| 3 | Underlying pack-loader/config error surfaced verbatim (pack load/manifest failure, config I/O failure) | unchanged |
| 4 | **New** — daemon unreachable (`list outputs`, `query`, `reevaluate` only) | **moved from code 2** (FR-028) |

**Why this changes**: the original contract's code `2` meant two different, unrelated things —
"daemon unreachable" (this contract) and clap's own built-in usage-error exit code (not
previously documented here because it wasn't `CliError`-mediated) — verified by reproduction: a
plain argument typo and an actual daemon-down condition were indistinguishable by exit code alone.
Code `2` now consistently means "usage error" (clap's own errors, and the flag-conflict check that
previously bypassed `CliError` entirely); "daemon unreachable" moves to the previously-unused code
`4`. **Any script or supervisor gating on exit code `2` to mean "start the daemon" must be updated
to check for `4` instead** — this is a deliberate, documented breaking change to the exit-code
contract, called out here rather than silently shipped (spec.md Edge Cases: "rejecting
previously-accepted [behavior]... should ship with a clear error message explaining what
changed").

## `assign --output <id> | --same-everywhere`

**New (US5/FR-019)**: `--output <id>`'s value is now validated (non-empty, ≤256 bytes, ASCII
alphanumeric/`-`/`_` only, via the
same `OutputId::validated` the D-Bus boundary uses — see
`contracts/wallpaperd-dbus-hardening.md`) before being stored. An invalid value now exits `1`
with a specific message instead of silently writing an override key that can never match a real
output.

**Changed (US6/FR-029)**: specifying neither or both of `--output`/`--same-everywhere` now exits
`2` (via `CliError::UsageError`, routed through the normal error-printing path) instead of
`eprintln!` + a direct `process::exit(1)`. The *message* is unchanged; only the mechanism and
exit code (previously undocumented/inconsistent `1`, now documented `2`) change.

## `list packs` (default, non-`--json`)

**New (US5/FR-018)**: a pack `name` containing tab or newline characters is now escaped in the
human-readable (tab-delimited) output — those characters render as spaces rather than producing
extra fabricated rows. `--json` output is byte-for-byte unchanged (the raw `name` value is still
carried verbatim in the JSON string).

## `location get`

**New (US6/FR-023)**: if the on-disk location config is present but corrupted/unparseable, the
output text now distinguishes that case from "never configured" (e.g. "location config could not
be read, treating as unset" vs. "no location set"). **Exit code remains `0` in both cases** — this
is a visibility fix, not a new failure mode (constitution Principle VIII: a corrupt config entry
degrades to defaults, it does not become fatal).

## Explicitly not in this contract

- Any change to `register`/`remove`/`query`/`location set`'s argument shape.
- The D-Bus wire-level changes those commands depend on —
  see `contracts/wallpaperd-dbus-hardening.md`.
- `location set`'s multi-field write becoming atomic (US6/FR-025) — an internal implementation
  change (research.md R20) with no observable CLI-contract difference for a successful call; only
  observable if the process is killed mid-write, which this contract's success/failure shape
  doesn't otherwise describe.
