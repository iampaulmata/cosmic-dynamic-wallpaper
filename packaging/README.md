# Packaging

This directory holds spec 5 (session integration & packaging)'s deliverables — no Rust code lives
here (see `specs/005-session-integration-packaging/plan.md`).

```text
packaging/
├── systemd/
│   └── wallpaperd.service   # systemd user unit (contracts/systemd-unit.md)
├── dbus-1/
│   └── com.system76.CosmicDynamicWallpaper1.conf  # session-bus policy (spec 011 US4 FR-015)
└── debian/
    ├── postinst              # enables the unit on install/upgrade
    ├── prerm                 # disables the unit on real removal
    └── postrm                # no-op (no cosmic-config entries of this spec's own to clean up)
```

Built via `cargo deb -p renderer`, which reads `[package.metadata.deb]` from
`crates/renderer/Cargo.toml` (research.md R4, `contracts/debian-package.md`).

## Edge case: what if a user already disabled `cosmic-bg` before installing this package?

Documented explicitly because spec.md's own Edge Cases section raises it. **This scenario is
structurally moot for this project**: research.md R3 (live-verified against this project's own
dev machine, not assumed) found there is no mechanism — no config file, environment variable, or
CLI flag — for *any* external party, including this package itself, to disable `cosmic-bg`.
`cosmic-session` spawns it unconditionally, hardcoded, every session. Since this package never
disables `cosmic-bg` in the first place (`postinst` only enables `wallpaperd.service` — see
`debian/postinst`), there's no "was it already disabled beforehand" state to reason about, and
no interaction between this package's install and a `cosmic-bg` disable state that doesn't and
can't exist. If `cosmic-session` itself ever grows a real way to disable `cosmic-bg` in the
future, this note — and research.md R3's residual-optimization suggestion — would need revisiting.
