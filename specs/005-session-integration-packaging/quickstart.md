# Quickstart: Validating Session Integration & Packaging

Two tiers, matching this project's established caution around real, hard-to-reverse system
changes: a **safe, local dry run** (steps 1–4, touches nothing outside this user's own
`~/.config/systemd/user/` and a build output directory — fully reversible) that validates the
actual contract, and a **real package install** (step 5, registers global system state) that
should be run deliberately, not as a routine check.

## Prerequisites

- `cargo-deb` installed (`cargo install cargo-deb`) — see research.md R4.
- A real COSMIC session (this dev machine already has one — `cosmic-session.target` active).

## 1. Build the release binaries

```sh
cargo build --workspace --release
```

Expected: `target/release/wallpaperd` and `target/release/wallpaperctl` both exist and run
(already true today, per specs 3–4 — this step doesn't change).

## 2. Build the `.deb` and inspect it without installing

```sh
cargo deb -p renderer      # or whichever crate ends up owning [package.metadata.deb] — task-level detail
dpkg-deb --contents target/debian/*.deb
dpkg-deb --info target/debian/*.deb
```

Expected: the file listing matches `contracts/debian-package.md`'s table exactly
(`/usr/bin/wallpaperd`, `/usr/bin/wallpaperctl`, `/usr/lib/systemd/user/wallpaperd.service`),
and `--info` shows the maintainer scripts (`postinst`/`prerm`/`postrm`) present. This step
builds an artifact on disk only — no system state changes.

## 3. Local dry run of the systemd unit contract (safe — user-local, fully reversible)

```sh
mkdir -p ~/.config/systemd/user
cp packaging/systemd/wallpaperd.service ~/.config/systemd/user/
# Point ExecStart at the just-built binary for this dry run, rather than /usr/bin (not installed yet):
sed -i "s#/usr/bin/wallpaperd#$(pwd)/target/release/wallpaperd#" ~/.config/systemd/user/wallpaperd.service
systemctl --user daemon-reload
systemctl --user enable --now wallpaperd.service
```

**Verify FR-001/SC-001** (autostart shape, ≤5s to rendering):

```sh
systemctl --user status wallpaperd.service   # expect: active (running), started promptly
wallpaperctl query                            # expect: real answers, matching Gap 1's earlier live QA
```

**Verify FR-003 (bounded restart, contracts/systemd-unit.md)**:

```sh
systemctl --user kill --signal=SIGKILL wallpaperd.service
sleep 3
systemctl --user status wallpaperd.service   # expect: active (running) again, RestartSec=2 honored
```

**Verify FR-002 (clean stop, no orphan)**:

```sh
systemctl --user stop wallpaperd.service
pgrep -f target/release/wallpaperd            # expect: no output — no orphaned process
```

**Clean up the dry run** (this step is what makes 1–4 fully reversible):

```sh
systemctl --user disable --now wallpaperd.service
rm ~/.config/systemd/user/wallpaperd.service
systemctl --user daemon-reload
```

## 4. Verify FR-004/FR-006's structural claim (research.md R3) — no explicit action needed

With `wallpaperd` running (repeat step 3's enable), confirm `cosmic-bg` is still a live process
underneath (research.md R3's finding, not a new mechanism this spec adds):

```sh
pgrep -x cosmic-bg   # expect: still running — never stopped, per research.md R3
```

Stop `wallpaperd` (step 3's disable) and confirm the desktop background is whatever `cosmic-bg`
was already rendering — no black screen, no explicit "restore" command needed, since it was
never actually replaced at the process level.

## 5. Real package install (deliberate step — registers global system state)

```sh
sudo apt install ./target/debian/*.deb
```

Expected, per `contracts/debian-package.md`: `postinst` runs `systemctl --user --global enable
wallpaperd.service`; the *next* COSMIC session start (log out/in, or reboot) shows `wallpaperd`
already running with no manual step (User Story 1's actual end-to-end acceptance test — steps
1–4 above validate the mechanism, this step validates the real install path with it).

**To reverse this step**:

```sh
sudo apt remove wallpaperdynamic   # actual package name is a task-level decision
```

Per `contracts/debian-package.md`'s `prerm`/`postrm`, this disables the unit and leaves no
residual config (FR-009, SC-004).
