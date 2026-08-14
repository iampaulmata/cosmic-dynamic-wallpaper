# Research: Session Integration & Packaging

All findings below were verified live against this dev machine's actual, running COSMIC
session (Pop!_OS 24.04 LTS, `cosmic-session` real binary + upstream source cross-checked via
network access), not assumed from general knowledge — matching this project's established
practice (e.g. spec 3's live Wayland/GPU verification, spec 1's live solar-accuracy check).

## R1: How does a COSMIC session actually autostart its components?

**Decision**: Ship a hand-authored systemd **user service unit** (`wallpaperd.service`) with
`WantedBy=cosmic-session.target` / `PartOf=cosmic-session.target`, rather than an XDG Desktop
Autostart `.desktop` entry.

**Evidence gathered**:
- `systemctl --user list-units` on this live session shows a real `cosmic-session.target`
  (`loaded active active "Cosmic Session Target"`), and `systemctl --user cat
  cosmic-session.target` shows it `Wants=`/`Before=` a real `xdg-desktop-autostart.target`,
  which is systemd's own standard generator-driven target for XDG Desktop Autostart
  (`~/.config/autostart/*.desktop` + `/etc/xdg/autostart/*.desktop`) — confirmed live: every
  non-first-party autostart app on this session (`nm-applet`, `print-applet`,
  `gnome-keyring-*`, `xdg-user-dirs`, etc.) appears as a generated `app-NAME@autostart.service`
  unit under that target.
- `cosmic-bg` itself, by contrast, is **not** one of those generated units —
  `systemctl --user status cosmic-bg` reports "Unit cosmic-bg.service could not be found";
  it only exists as a transient `cosmic-bg.scope`, and `ps -o ppid` confirms its parent process
  is `/usr/bin/cosmic-session` directly (PID 1831 → 1979), not systemd.
- Cross-checked against upstream `cosmic-session` source
  (`github.com/pop-os/cosmic-session/blob/master/src/main.rs`, fetched live): `cosmic-bg` and
  8 other first-party components (`cosmic-panel`, `cosmic-notifications`, `cosmic-app-library`,
  `cosmic-launcher`, `cosmic-workspaces`, `cosmic-osd`, `cosmic-greeter`,
  `cosmic-files-applet`, `cosmic-idle`) are started via a hardcoded `start_component(name, ...)`
  call each — there is no config file, environment variable, or CLI flag gating any of them.
  These are compiled into `cosmic-session` itself, not a pluggable/data-driven list a
  third-party package can add itself to.
- The same source also shows `cosmic-session` has its own **internal** XDG-autostart scanner
  (`#[cfg(feature = "autostart")] if !*is_systemd_used() { ... }`) that manually parses
  `.desktop` files when systemd is *not* managing the session — i.e. XDG Desktop Autostart is
  the deliberate, supported integration point for anything outside the 9 hardcoded components,
  on **both** systemd and non-systemd COSMIC sessions.

**Why a hand-written unit instead of a `.desktop` autostart entry anyway**: XDG Autostart's
traditional semantics are "launch once at session start" — the units systemd's own
`xdg-desktop-autostart` generator produces from a `.desktop` file carry no `Restart=` policy by
default (confirmed: none of the generated `app-*@autostart.service` units on this live session
have restart directives). FR-002/FR-003 need real "stop cleanly on logout" + "bounded
auto-restart on crash" semantics that a plain `.desktop` file doesn't express. A purpose-built
`wallpaperd.service` gets exactly that, and `cosmic-session.target` is a real, live, systemd
target any third-party unit can declare `WantedBy=`/`PartOf=` against — the same mechanism
`graphical-session.target` conventionally offers cross-desktop, just COSMIC-specific (correct
for a COSMIC-only project, since binding to the generic target would let the unit attempt to
start under a different, unsupported compositor).

**Alternatives considered**:
- *Hook into `cosmic-session`'s own component list*: rejected — would require patching
  `cosmic-session` itself, entirely out of scope for a third-party project.
- *Ship only a `.desktop` autostart entry*: rejected as the sole mechanism — works for
  "launch," not for FR-003's bounded-restart requirement, without a systemd unit drop-in
  override anyway (which is more indirection than just authoring the unit directly).
- *`graphical-session.target` instead of `cosmic-session.target`*: rejected for this
  COSMIC-only project — `cosmic-session.target` is the more precise binding and avoids ever
  attempting to start under an unsupported compositor.

## R2: Concrete unit shape for FR-001–FR-003 (autostart, clean stop, bounded restart)

**Decision**:

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

**Rationale**: `PartOf=cosmic-session.target` (not just `After=`) is what gives FR-002's
"stops cleanly when the session ends" for free — systemd propagates a stop of the target to
every unit bound to it via `PartOf=`. `Restart=on-failure` + `StartLimitBurst=5` +
`StartLimitIntervalSec=60` implements the clarified 5-attempts-within-a-rolling-window bound
(spec.md Clarifications) directly in the `[Unit]` section fields systemd defines for exactly
this purpose — no custom retry logic needed in `wallpaperd` itself. Past the burst limit,
systemd leaves the unit in a `failed` state, itself visible via `systemctl --user status
wallpaperd.service` (satisfies "discoverable rather than silent," Edge Cases).

**Alternatives considered**: A `Type=notify` unit with `wallpaperd` itself signaling readiness
was considered for a tighter SC-001 bound, but rejected as unnecessary complexity — `wallpaperd`
already completes real Wayland/GPU startup in well under a second (live-verified during spec 3's
own work), so `Type=simple`'s immediate "started" signal already leaves ample headroom under
the clarified 5-second SC-001 target without needing a readiness protocol.

## R3: What does "disable cosmic-bg's role" (FR-004) actually mean, given R1's findings?

**Decision**: FR-004–FR-007's *observable outcomes* (no visible double background on install;
no black screen on uninstall) are already satisfied by mechanisms that already exist, once
FR-001–FR-003's autostart unit is in place — **no new cosmic-bg-toggling code is required for
this spec's acceptance criteria.**

**Evidence and reasoning**:
- R1 already established `cosmic-bg` cannot be stopped or gated by any external package —
  it is unconditionally spawned by `cosmic-session` every session, with no config/env/CLI
  lever found in the live binary, the live process tree, or the upstream source.
- Constitution Principle I itself anticipates exactly this, offering two equally-valid framings:
  "disabling `cosmic-bg`'s background role... **or fully superseding it as the session's
  background service**." Given R1's finding that the first framing (disabling) has no real
  mechanism available to a third-party package, this spec leans on the second: `wallpaperd`
  (spec 3, already implemented) already takes exclusive ownership of the `Layer::Background`
  layer-shell surface on every output it manages and renders a fully opaque image — confirmed
  by reading `crates/renderer/src/surface.rs`'s `add_output`/`draw`. Wayland's layer-shell
  protocol permits multiple clients to hold `Background`-layer surfaces on the same output
  simultaneously; there is no protocol-level "exclusive lock" to fight over. What FR-004
  actually needs — `cosmic-bg`'s output never being *visible* — is a direct consequence of
  `wallpaperd`'s surface being present, opaque, and (per this spec's FR-001) started
  automatically before the user ever sees the desktop.
- Live-verified (carefully, non-destructively — the exact original file content was backed up
  and restored, and `cosmic-bg`'s live process was unaffected by the whole test):
  `cosmic-bg`'s own config lives at a real, versioned `cosmic-config` entry
  (`com.system76.CosmicBackground`, schema `v1`, fields `output`/`source`/`scaling_mode`/etc.)
  — confirming Principle IV's own framing that this project and `cosmic-bg` already share the
  same live-reloaded config store. Removing that entry's file for ~8 seconds did not crash
  `cosmic-bg`, did not cause it to regenerate a default, and produced no error in its journal —
  consistent with `cosmic-config`'s watch-based (not poll-based) reload model.
- **Consequence for FR-006/FR-007 (uninstall)**: since `cosmic-bg` was never actually stopped
  by install, uninstall does not need a separate "restore" action either — stopping
  `wallpaperd.service` (FR-002's existing stop path) is sufficient; `cosmic-bg`'s
  already-running, already-rendering surface becomes visible again the instant `wallpaperd`'s
  surface is gone, with no explicit restore step, no risk of a black screen, and no dependency
  on remembering "did install actually disable it."

**Residual optimization (explicitly out of this spec's required scope, flagged for a future
task)**: `cosmic-bg` continuing to decode and redraw images no one can see is wasted CPU/
battery, in tension with constitution Principle VI's efficiency stance. A future enhancement
could have install best-effort-write an inert value to `com.system76.CosmicBackground`'s own
config to reduce that waste — but doing so safely requires understanding `cosmic-bg`'s own
tolerance for edge-case config values (not fully characterized by the config-removal test
above, which only proved *absence* is tolerated, not what value causes it to stop rendering
while still running), which is genuinely out of reach without reading `cosmic-bg`'s own source
in more depth than this spec's scope justifies. Not required for FR-004–FR-007's acceptance
criteria, which this decision already satisfies structurally.

**Alternatives considered**:
- *Write a value to `com.system76.CosmicBackground` on every install, unconditionally*:
  rejected as a required mechanism — unverified safety (risk of a malformed value causing
  `cosmic-bg` to error or fall back unpredictably) for an outcome (hidden waste, not visible
  correctness) that isn't actually required by any FR in this spec. Left as an optional future
  task instead.
- *Attempt to prevent `cosmic-session` from spawning `cosmic-bg` at all* (e.g. shadowing the
  binary name in `PATH`, patching `cosmic-session`): rejected — fragile, undocumented,
  out of scope, and unnecessary now that R1/R3 show the outcome is achievable without it.

## R4: Packaging mechanism (FR-008/FR-009)

**Decision**: A native Debian package (`.deb`) via `cargo-deb`, targeting Pop!_OS/Debian/
Ubuntu-family systems — the same distribution family this project's own dev environment runs,
and the same format `cosmic-bg` itself ships as (`dpkg -s cosmic-bg` on this live system:
`Version: 0.1.0~...~24.04~...`, maintained by System76, `.deb`, no bundled systemd/`.desktop`
files of its own — consistent with R1/R3's findings). Constitution Principle XI requires "at
least one of a distro package or Flatpak manifest" — this spec picks the one directly
verifiable on this project's own real dev/test machine, leaving Flatpak as an explicitly
out-of-scope future addition (Assumptions).

**Rationale**: `cargo-deb` (a standard, widely-used `cargo` subcommand for Rust projects) reads
packaging metadata from a `[package.metadata.deb]` section in `Cargo.toml` — including
`assets` (binaries, the systemd unit from R2, docs) and `maintainer-scripts` (a directory of
Debian `postinst`/`prerm`/`postrm` shell scripts) — and produces a real `.deb` without
hand-writing `debian/rules`/`debhelper` boilerplate. `dpkg-deb` is already present on this dev
machine; `cargo-deb` itself is a one-line `cargo install` away, not a new heavyweight
toolchain.

**`postinst`/`prerm` scope, given R3's finding**: since FR-004–FR-007 need no cosmic-bg
mutation, the maintainer scripts' job is narrow and low-risk — `postinst` runs
`systemctl --user enable wallpaperd.service` scoped correctly for a user-service package
(Debian's `dh_systemd_user`/`deb-systemd-helper` conventions apply to user units the same way
as system ones); `prerm`/`postrm` runs the matching `disable`/`stop`. No config mutation,
no cosmic-bg-specific logic, per R3.

**Alternatives considered**:
- *Flatpak*: rejected as the primary target — Flatpak's app-container sandboxing model is
  designed around windowed applications with a `.desktop` launcher, not always-running,
  session-level background services with `wlr-layer-shell` access; making that combination
  work well is a materially harder, separately-scoped problem than this spec's own MVP, and
  the constitution only requires "at least one" mechanism. Left as a documented future option
  (Assumptions), not blocking this spec.
- *Hand-rolled `debian/` directory with `debhelper`*: rejected — `cargo-deb` is the
  established, lower-maintenance convention for a Rust-only project with no other language
  toolchain already in the packaging picture, and this project has no existing `debian/`
  scaffolding to build on.

## R5: What binaries does the package need to ship?

**Decision**: `wallpaperd` (spec 3) and `wallpaperctl` (spec 4) — both already build as
workspace binaries (`cargo build --workspace --release` already produces both, verified
against the current `Cargo.toml`). No new Rust code is required to produce the binaries
themselves; this spec's own deliverables are the unit file (R2), the `.deb` packaging metadata
and maintainer scripts (R4), and (optionally) a small install-time README/first-run notice —
not new application logic in `crates/`.
