# Contract: `wallpaperctl location` — new/changed subcommands

Extends `specs/004-cli-control-surface/contracts/wallpaperctl-cli.md`'s `location` command
family (`get`/`set`/`clear`, unchanged in shape below except `get`'s output) with two new
subcommands. All five remain daemon-optional (FR-012) — every one reads/writes
`cosmic-config` only.

## `wallpaperctl location auto`

Enables automatic mode (FR-001, FR-002, FR-003). Idempotent — calling it while already in
automatic mode is a no-op success, not an error.

```console
$ wallpaperctl location auto
automatic location enabled (resolving…)
$ echo $?
0
```

Writes `mode: Automatic` only (contracts/location-config-schema-v2.md). Does **not** attempt to
resolve a location itself, and does **not** require a running `wallpaperd` to succeed — actual
resolution only happens once a live daemon picks up the config change (same daemon-optional
write / daemon-required-to-take-effect split spec 4 already established for `assign`).

## `wallpaperctl location manual`

Switches back to manual mode using whatever value is already stored in `location`, with no
re-entry (FR-007, FR-009).

```console
$ wallpaperctl location manual
manual location restored: 45.5019 -73.5674
$ wallpaperctl location manual   # no manual value was ever stored
manual mode set (no location stored — only clock-anchored packs usable)
$ echo $?
0
```

Writes `mode: Manual` only. Never fails (there is no invalid state to reject) — always exits 0.

## `wallpaperctl location get` (extended)

Existing command (spec 4 FR-008); output shape extended to include `mode` and `status` alongside
the existing `location` field, per data-model.md's `LocationConfigEntry`.

```console
$ wallpaperctl location get
mode: automatic
status: resolved
location: 45.4972 -73.6104  (from automatic resolution)

$ wallpaperctl location get
mode: automatic
status: unavailable (Location services disabled)
location: no location available

$ wallpaperctl location get --json
{"mode":"automatic","status":{"state":"unavailable","reason":"Location services disabled"},"location":null,"manual_location":{"latitude":45.5019,"longitude":-73.5674},"automatic_location":null}
```

Human-readable form shows the single **effective** location (data-model.md's
`effective_location()`) plus enough of `mode`/`status` to explain *why* — satisfying spec.md
FR-008 ("query... which mode... and coordinates currently used") and SC-004 in one command.
Machine-readable (`--json`, FR-013 precedent) surfaces the full underlying state (`manual_location`
separate from `automatic_location`) for scripted callers that need the distinction, not just the
effective value.

## `wallpaperctl location set <lat> <lon>` (unchanged behavior, one new side effect)

Unchanged validation/persistence (spec 4 FR-008), with one documented addition: also sets
`mode: Manual` (research.md R7) — setting a manual value while remaining in automatic mode would
have no observable effect, which is a worse default than switching modes explicitly.

## `wallpaperctl location clear` (unchanged)

Unchanged from spec 4 — clears `location` only. Does not affect `mode` or the automatic fields
(research.md R7); clearing while in automatic mode leaves automatic mode active with whatever its
current resolution state already is.
