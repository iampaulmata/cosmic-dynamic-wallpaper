# Contract: Debian Package Layout & Maintainer Scripts

The authoritative shape of this spec's Distribution Package (data-model.md) for the
Pop!_OS/Debian/Ubuntu target (research.md R4). Built via `cargo-deb` from
`[package.metadata.deb]` sections added to `crates/renderer/Cargo.toml` and
`crates/wallpaperctl/Cargo.toml` — no hand-written `debian/rules`/`debhelper` scaffolding
(research.md R4 Alternatives).

## Package contents (FR-008)

| Installed path | Source | Notes |
|---|---|---|
| `/usr/bin/wallpaperd` | `crates/renderer` release binary | Spec 3, unmodified |
| `/usr/bin/wallpaperctl` | `crates/wallpaperctl` release binary | Spec 4, unmodified |
| `/usr/lib/systemd/user/wallpaperd.service` | `packaging/systemd/wallpaperd.service` | `contracts/systemd-unit.md`'s exact contents |

## Maintainer scripts (FR-004–FR-007's actual scope, per research.md R3, plus FR-005/FR-009)

**`postinst`** (runs after files are unpacked, on both fresh install and upgrade):

```sh
#!/bin/sh
set -e
if [ "$1" = "configure" ]; then
    systemctl --user --global enable wallpaperd.service 2>/dev/null || true
fi
```

- `--global` enables the unit for all current and future users at once, matching how a system
  package installs a session component without needing to run inside each user's own session
  (Debian's conventional pattern for packaged systemd **user** units). The trailing
  `|| true` makes this step non-fatal to the overall package install (Technical Context:
  Constraints — a `postinst` failure can leave `dpkg` in a broken state, so this step degrading
  to "not yet enabled, user can `systemctl --user enable` by hand" is preferable to failing
  the whole package operation).
- **No `cosmic-bg`-related logic** (research.md R3) — this script's only job is enabling the
  Session Unit. FR-004's outcome follows automatically once `wallpaperd` is running (data-model.md).
- **Idempotent** (FR-005, SC-005): `systemctl --user --global enable` on an already-enabled unit
  is a systemd no-op, not an error — re-running `postinst` on an upgrade behaves identically to
  a fresh install.

**`prerm`** (runs before files are removed, on both uninstall and upgrade-in-progress):

```sh
#!/bin/sh
set -e
if [ "$1" = "remove" ]; then
    systemctl --user --global disable wallpaperd.service 2>/dev/null || true
fi
```

- Only on `remove` (real uninstall), not on `upgrade` — an in-place upgrade must not disable
  and lose the running instance; the new `postinst` above re-enabling is a no-op in that case
  anyway, matching FR-005/SC-005's idempotency requirement across the upgrade path too.
- Stopping the **already-running** instance in the user's live session is intentionally *not*
  attempted from a root-context maintainer script (`systemctl --user` from a `postinst`/`prerm`
  running as root cannot reliably reach a specific logged-in user's session bus without extra
  plumbing this spec doesn't need) — FR-006/FR-007's "no black screen" outcome doesn't actually
  require an instant stop; `cosmic-bg` is already running underneath regardless (research.md
  R3), so `wallpaperd` naturally stopping at the user's *next* logout/login is sufficient, and
  simpler and more robust than adding cross-user-session D-Bus plumbing for marginal benefit.

**`postrm`** (runs after files are removed, on purge):

```sh
#!/bin/sh
set -e
# No config to remove — this spec introduces no cosmic-config entries (data-model.md), and
# any user pack registry / assignment data (specs 2-3) is left alone on purge, matching
# Debian convention that a package purge removes the package's own files, not a user's data.
exit 0
```

## Verification (FR-009: removable through the same package manager)

`apt remove wallpaperdynamic` (or the chosen package name) MUST leave the system in the exact
state `prerm`/`postrm` above describe, with no separate manual cleanup script needed —
verified in quickstart.md.
