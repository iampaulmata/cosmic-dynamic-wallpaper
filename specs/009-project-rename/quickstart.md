# Quickstart: Validating the Rename

Runnable checks proving each Success Criterion in `spec.md` end to end. Run from the
repository root, after implementation is complete, in the order below — several steps
assume the previous one succeeded. Identifier values referenced below are the ones
defined in `contracts/identifier-rename-map.md`; nothing here duplicates that table.

## Prerequisites

- A COSMIC desktop session (this project's normal dev/test environment).
- The old `dynamic-wallpaper` package installed and its systemd unit enabled/running —
  the realistic starting point for validating FR-004b/FR-004a. If not already the case:
  ```sh
  sudo apt install ./dynamic-wallpaper_0.1.0-3_amd64.deb   # or whatever was last built
  systemctl --user status wallpaperd.service                # confirm it's running
  ```
- At least one setting actually configured under the old identifiers, so the migration
  has something real to prove it carried forward — e.g. set a manual location:
  ```sh
  wallpaperctl location set 43.6532 -79.3832
  ```

## SC-001: No lingering old-name references in documentation

```sh
grep -rIn "dynamic.wallpaper" --include=*.md --include=*.toml \
  --exclude-dir=target --exclude-dir=.git \
  . | grep -viE "specs/00[1-8]|cinnamon|\.heic dynamic-wallpaper metadata"
```

Expected: no output (empty). Any match is either a real leftover reference (fix it) or a
gap in this exclusion pattern worth double-checking against `research.md` R1's scope
decision before assuming it's fine.

## SC-003: Clean build and package under the new names

```sh
cargo build --release --workspace
cargo deb -p renderer --no-build
ls target/debian/cosmic-dynamic-wallpaper_*.deb   # filename itself proves the rename
dpkg-deb -c target/debian/cosmic-dynamic-wallpaper_*.deb | \
  grep -E "cosmic-wallpaperd|cosmic-wallpaperctl|cosmic-wallpaper-settings|CosmicDynamicWallpaperSettings\.desktop|cosmic-wallpaperd\.service"
```

Expected: build and packaging both succeed with zero errors, and the `dpkg-deb -c`
listing shows every renamed binary/desktop-entry/systemd-unit path from
`contracts/identifier-rename-map.md` present under its new name.

```sh
cargo test --workspace
cargo clippy --workspace --all-targets
```

Expected: same pass/fail outcome as before the rename (FR-007) — no new failures caused
by the rename itself. (This dev machine has a pre-existing, environmental "daemon
unreachable" test flakiness unrelated to this feature whenever the daemon is genuinely
running — see prior session notes; that's not a regression to chase here.)

## SC-005 / FR-004b: Package supersession, no dual-daemon state

```sh
dpkg -l dynamic-wallpaper                                   # confirm still installed
sudo apt install ./target/debian/cosmic-dynamic-wallpaper_*.deb
dpkg -l dynamic-wallpaper cosmic-dynamic-wallpaper           # expect: old absent, new installed
systemctl --user list-unit-files | grep -i wallpaper         # expect: only the new unit
systemctl --user is-active wallpaperd.service                # expect: "inactive" or unit-not-found
systemctl --user is-enabled cosmic-wallpaperd.service         # expect: "enabled"
```

Expected: exactly one package, one unit, one daemon — never both installed/enabled at
once, matching Constitution Principle I.

## SC-004 / FR-004a: Config migration carries settings forward

```sh
# Log out and back in (or reboot) so cosmic-wallpaperd.service starts fresh under the
# new session, per the existing README caveat about first-session-after-install.
wallpaperctl location get
```

Expected: reports the manual location set in Prerequisites (`43.6532, -79.3832`) —
proving `LocationConfigEntry`'s migration ran. Repeat the equivalent check for registered
packs (`wallpaperctl list packs`) and any per-display assignment configured before the
upgrade — each should read back unchanged. See `contracts/config-migration.md` for the
exact guarantee being validated (in particular: this MUST work with zero manual steps
between installing the new package and these commands succeeding).

## SC-002: GitHub-facing branding (manual, external to this repo)

After `FR-003`'s repository rename (a manual step — see `research.md` R6):

1. Open `https://github.com/iampaulmata/cosmic-dynamic-wallpaper` in a browser.
2. Confirm the repository name and the rendered README title both read "Cosmic Dynamic
   Wallpaper" above the fold, with no visible old-name text.
3. Confirm the old URL (`.../rust-dynamic-wallpaper`) redirects to the same page.
