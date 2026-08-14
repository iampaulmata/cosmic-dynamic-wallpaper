# Data Model: Session Integration & Packaging

This spec introduces no new Rust types and no new persisted `cosmic-config` schema
(plan.md Technical Context: Storage) — its "entities" are packaging/systemd artifacts, not
application data. Documented here in the same entity-shape spec 4's data-model.md used, adapted
to what actually exists for this spec.

## Session Unit (`wallpaperd.service`)

A systemd user unit file (research.md R2) — not a Rust struct, but its directive set is the
closest analogue to this spec's "data model," since it's the one artifact with real
fields/states/transitions:

| Directive (section) | Value | Maps to |
|---|---|---|
| `PartOf=` (`[Unit]`) | `cosmic-session.target` | FR-002 — propagates a session-end stop |
| `After=` (`[Unit]`) | `cosmic-session.target` | Ordering only; doesn't by itself cause a start |
| `Restart=` (`[Service]`) | `on-failure` | FR-003 — restart on crash, not on a clean exit |
| `RestartSec=` (`[Service]`) | `2` | Brief pause between attempts, avoids a tight spin |
| `StartLimitIntervalSec=` (`[Unit]`) | `60` | The "rolling window" from spec.md Clarifications |
| `StartLimitBurst=` (`[Unit]`) | `5` | The clarified attempt count before giving up |
| `WantedBy=` (`[Install]`) | `cosmic-session.target` | What `systemctl --user enable` actually binds |

**State transitions** (systemd's own unit states, not a new state machine this spec invents):

```
inactive --(session start, WantedBy pulls it in)--> activating --> active
active --(wallpaperd exits unexpectedly)--> failed --(Restart=on-failure)--> activating --> active
   ... repeats, counted against StartLimitBurst within StartLimitIntervalSec ...
active/failed --(burst exceeded)--> failed, stays stopped (Edge Cases: discoverable via `systemctl --user status`)
active --(session end, PartOf= propagates)--> deactivating --> inactive
```

No field of this unit is persisted/mutated by `wallpaperd` or `wallpaperctl` themselves at
runtime — it's a static file shipped by the Distribution Package below, only ever touched by
`systemctl --user enable|disable|start|stop`, invoked from the maintainer scripts (Debian
package, `contracts/debian-package.md`).

## Distribution Package

| Field | Notes |
|---|---|
| `wallpaperd` binary | Spec 3's already-built release binary — unmodified by this spec |
| `wallpaperctl` binary | Spec 4's already-built release binary — unmodified by this spec |
| Session Unit | `wallpaperd.service`, installed to the package's systemd user-unit directory |
| Maintainer scripts | `postinst`/`prerm`/`postrm` — enable/disable the Session Unit at the right lifecycle point (`contracts/debian-package.md`) |

No "`cosmic-bg` Handoff State" field exists here (spec.md's Key Entities section was corrected
during planning — see that section's note) — there is nothing install records and nothing
uninstall reads back, per research.md R3.

## Non-entities worth naming explicitly (things this spec deliberately does NOT introduce)

- **No new `cosmic-config` schema** — confirmed in Constitution Check (Principle IV/X rows):
  this spec neither reads nor writes any persisted config of its own.
- **No IPC/D-Bus surface** — unlike spec 4, this spec has no live-daemon-facing contract; its
  only "interface" is the Session Unit (a systemd contract) and the Distribution Package (a
  `dpkg`/`apt` contract), both documented in `contracts/`.
- **No cosmic-bg-facing data shape** — research.md R3's finding means this spec never
  constructs, reads, or writes anything shaped like `cosmic-bg`'s own
  `com.system76.CosmicBackground` config entry.
