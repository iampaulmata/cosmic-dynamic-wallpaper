# Contract: `wallpaperd.service` (systemd user unit)

The authoritative shape of this spec's Session Unit (data-model.md), installed to
`/usr/lib/systemd/user/wallpaperd.service` by the Debian package
(`contracts/debian-package.md`). Any implementation task producing this file MUST match this
exactly — verified against real, live `systemctl --user` behavior on this project's own dev
COSMIC session during research (research.md R1/R2), not a template guess.

```ini
[Unit]
Description=Dynamic wallpaper renderer
Documentation=https://github.com/iampaulmata/rust-dynamic-wallpaper
PartOf=cosmic-session.target
After=cosmic-session.target

[Service]
Type=simple
ExecStart=/usr/bin/wallpaperd
Restart=on-failure
RestartSec=2
StartLimitIntervalSec=60
StartLimitBurst=5

[Install]
WantedBy=cosmic-session.target
```

## Guarantees this unit MUST provide (traced to spec.md's FRs)

- **FR-001** (autostart, no manual launch): `WantedBy=cosmic-session.target` — once
  `systemctl --user enable wallpaperd.service` has run (the package's `postinst`,
  `contracts/debian-package.md`), every subsequent COSMIC session start pulls this unit in
  automatically.
- **FR-002** (clean stop on logout, no orphan): `PartOf=cosmic-session.target` — systemd
  propagates a stop of `cosmic-session.target` to this unit. `Type=simple` means systemd tracks
  the exact `wallpaperd` process (no double-fork/orphaning risk).
- **FR-003** (bounded auto-restart): `Restart=on-failure` (not `always` — a clean, intentional
  exit, e.g. via a future `wallpaperctl` "stop the daemon" command, is never treated as a
  crash) + `StartLimitBurst=5` within `StartLimitIntervalSec=60`, per spec.md's Clarifications.
  Once the burst is exceeded, systemd leaves the unit `failed` and does not retry again —
  satisfying "discoverable rather than silent" (Edge Cases) via ordinary
  `systemctl --user status wallpaperd.service` / `journalctl --user -u wallpaperd.service`,
  no bespoke logging/alerting this spec needs to build.
- **SC-001** (≤5s to rendering): no unit-level delay is introduced — `After=` is ordering-only
  and `cosmic-session.target` is already active well before a user would notice, so the bound
  is dominated entirely by `wallpaperd`'s own real startup cost (sub-second, spec 3).

## What this unit deliberately does NOT do

- It does not reference, start, stop, or configure `cosmic-bg` in any way (research.md R3 —
  there is no mechanism to do so, and none is needed for this spec's FRs).
- It carries no `ExecStartPre=`/`ExecStop=` hooks — `wallpaperd` itself already handles its own
  Wayland connection lifecycle and clean shutdown; this unit only supervises the process.
- It does not set `WatchdogSec=`/require `Type=notify` — rejected in research.md R2 as
  unnecessary complexity given `wallpaperd`'s already-fast, already-verified startup.

## Verification

`systemctl --user status wallpaperd.service` after a session start MUST show `active (running)`
within the SC-001 bound. `systemctl --user kill --signal=SIGKILL wallpaperd.service` followed by
`systemctl --user status wallpaperd.service` MUST show the unit back in `active (running)`
within `RestartSec=2` plus normal startup time, demonstrating FR-003 without needing to actually
corrupt `wallpaperd`'s own state to trigger a "real" crash (quickstart.md's validation steps use
exactly this).
