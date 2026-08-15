# wallpaperctl

The CLI control surface for the Cosmic Dynamic Wallpaper daemon (contracts/wallpaperctl-cli.md)
— the only settings surface until a future GUI exists (constitution Principle IX).

```sh
cosmic-wallpaperctl register <path>                          # directory (manifest pack) or single image file
cosmic-wallpaperctl list packs
cosmic-wallpaperctl list outputs
cosmic-wallpaperctl remove <pack-source>
cosmic-wallpaperctl assign --output <output-id> <pack-source>
cosmic-wallpaperctl assign --same-everywhere <pack-source>
cosmic-wallpaperctl location get|set <lat> <lon>|clear|auto|manual
cosmic-wallpaperctl query [--output <output-id>]
cosmic-wallpaperctl reevaluate [--output <output-id>]
```

Add `--json` to any command for machine-readable output (FR-013).

## Config-only vs. daemon-required commands

Most commands act directly on persisted state (spec 2's pack registry, or `cosmic-config`
schemas this crate owns/writes) and work whether or not `wallpaperd` happens to be
running:

| Config-only (no daemon needed) | Daemon-required |
|---|---|
| `register`, `list packs`, `remove`, `assign`, `location get\|set\|clear\|auto\|manual` | `list outputs`, `query`, `reevaluate` |

The daemon-required column has no persisted record to fall back on — "which outputs
exist" and "what's currently active" are live daemon state, not config. Those three
commands connect to `wallpaperd` over D-Bus (`com.system76.CosmicDynamicWallpaper1`,
contracts/wallpaperd-dbus-interface.md) and fail immediately with a clear
"daemon unreachable" error (exit code 2) if it isn't running — never a hang.

**`assign` is deliberately config-only even for a not-yet-connected output name** — e.g.
pre-configuring a docking-station monitor before plugging it in. It only checks the
pack is registered (spec 2's local registry); if the daemon happens to be reachable, it
additionally prints a non-fatal warning when the output name isn't currently connected,
but never fails because of it (FR-007).

## Cross-spec dependencies this crate writes to or calls, not implemented yet

Spec 3 (the renderer/`wallpaperd` daemon) doesn't exist as of this crate's
implementation. Three things here are the *writer*/*caller* side of contracts spec 3
must eventually satisfy on its side:

- **`RendererConfig`** (`src/config.rs`) — `assign` writes spec 3's own
  `cosmic-config` schema (contracts/renderer-config-schema.md) exactly as documented
  there; spec 3's daemon is the intended reader, whenever implemented. The
  `cosmic-config` application id (`RENDERER_CONFIG_ID`) is this crate's own choice
  (not fixed by that contract) — spec 3 must match it.
- **`wallpaperd`'s D-Bus interface** (`src/dbus_client.rs`) — `query`/`reevaluate`/
  `list outputs` call `QueryOutput`/`QueryAll`/`Reevaluate`/`ReevaluateAll`
  (contracts/wallpaperd-dbus-interface.md) exactly as specified; nothing currently
  implements the daemon side, so these three commands reliably (and correctly, per
  FR-011) report "daemon unreachable" until spec 3 does.
- **`LocationConfig`** (`src/config.rs`'s `LocationConfigEntry`, now spec 6's v2 schema:
  `mode`, `location`, `automatic_location`, `automatic_status`) — this crate writes
  `mode`/`location` (`location set|clear|auto|manual`, FR-008/spec 6 FR-001/002/003/007/
  009); `wallpaperd` (`crates/renderer`, spec 6) is the *only* writer of
  `automatic_location`/`automatic_status` and the reader for actual scheduling, via
  `effective_location()` (spec 6 data-model.md). `location get` reads and displays the
  full entry — `mode`, `status`, and the effective location — daemon-optional, same
  posture as every other command in the left-hand column above.

None of this blocks this crate's own correctness or tests — every command here is fully
implemented and tested against the *client*/*writer* side of these contracts. It just
means the full end-to-end loop (`wallpaperctl assign` → `wallpaperd` actually changing
what's on screen) can't be manually verified until spec 3 lands.

## Explicitly not in scope

- Any command to change crossfade duration or other spec 3 rendering parameters (spec.md
  Assumptions — spec 3 already sets a sane fixed default).
- Any GUI — this is the CLI-only interim control path (constitution Principle IX).
- Multi-user access control — single-user desktop session trust boundary, same as every
  other daemon in this project.

## Testing

```sh
cargo test --package wallpaperctl
cargo llvm-cov --package wallpaperctl --summary-only
```

Config-only commands get real `tempfile`-backed integration-style tests (via each
config type's doc-hidden `open_at` test hook, matching `pack-loader`'s `Registry`
pattern) — no mocking needed, since they never touch a daemon at all. Daemon-required
commands (`list outputs`, `query`, `reevaluate`) are tested for real too, not mocked:
every test environment has no `wallpaperd` registered on the session bus, so the
"daemon unreachable, fail fast, don't hang" path (FR-011) is exercised as its actual,
real behavior rather than a simulated one.
