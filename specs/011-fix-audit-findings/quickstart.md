# Quickstart: Validating the Audit-Finding Fixes

This feature's real acceptance test is negative: every step below reproduces an input the
original audit used (several were originally run and confirmed against pre-fix code — "Verified —
reproduced" in the report). Each should now fail cleanly (a logged error, a rejected CLI
argument, a bounded queue) instead of panicking, corrupting state, or silently succeeding. Where
a step doesn't have a pre-existing automated test, add one that fails against pre-fix code and
passes after (spec.md Edge Cases, SC-001/SC-005) — the automated section below is the durable
form of this validation; the manual section is for the two findings (GUI path traversal, GUI
silent registry failure) that need a real COSMIC session.

## Prerequisites

- Stable Rust toolchain, same workspace as specs 1–10.
- A real COSMIC session, only for the two `wallpaper-settings` manual checks (US2, US6.3).
- `wallpaperctl`/`wallpaperd` built (`cargo build --workspace`).

## Run the automated test suite

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Expected new/updated coverage, one item per finding this quickstart doesn't require a manual
session for:

| Crate | Test | Confirms |
|---|---|---|
| `pack-loader` | `manifest::tests::color_parse_rejects_non_ascii_hex` | FR-001 — `Color::parse("#€AAA")` returns `Err`, not a panic |
| `pack-loader` | `load::tests::anchor_cap_rejected_before_per_image_io` | FR-010 — a 500-entry manifest is rejected without any per-image `canonicalize`/read call (assert via a fake/counting filesystem or a manifest referencing nonexistent files that would otherwise fail on the *first* one, not the cap) |
| `pack-loader` | `load::tests::oversized_manifest_rejected` | FR-011 — a >512 KB `manifest.toml` is rejected before being read |
| `pack-loader` | `path_safety::tests::rejects_absolute_path` | FR-020 — new fixture, `/etc/passwd`-shaped entry rejected explicitly |
| `pack-loader` | `registry::tests::concurrent_persist_serializes` | FR-022 — two `Registry` handles on the same store, both calling `persist()` around the same time, don't lose either write |
| `schedule-engine` | `pack::tests::solar_offset_out_of_range_rejected` | FR-004 — `TimeAnchor::solar(Sunrise, Some(TimeDelta::MAX))` is rejected by `validate`, and (property test) `query()` never panics for any offset that passed `validate` |
| `schedule-engine` | `query::tests::pole_latitude_returns_none_fast` | FR-038 — a ±90° location resolves via the fast path (assert on call count/timing bound, not just correctness) |
| `renderer` | *(no new dedicated test — see tasks.md T043)* `pack.rs`'s existing `check_solar_duplicate_instant` unit tests, now exercised from a real runtime call site (`surface.rs`'s `load_pack_for`) instead of only the pack-builder GUI | FR-040 — corrected during implementation, see research.md R34 |
| `renderer` | `surface::tests::clamp_reconfigure_size_rejects_zero_on_either_axis` (pure unit test) + `hotplug_mock::zero_size_reconfigure_does_not_panic` (integration, real-GPU-dependent) | FR-002 — reconfigure with `(0, height)` and `(width, 0)` both succeed with a clamped size |
| `renderer` | `surface::tests::crossfade_progress_rejects_non_finite` | FR-003 — `progress = f64::NAN`/`f64::INFINITY`/`-0.5` all handled without panicking |
| `renderer` | `texture::tests::oversized_image_rejected_before_decode` | FR-012 — a crafted header claiming e.g. 40000×40000 is rejected via header-only dimension read, never reaching `to_rgba8()` |
| `renderer` | `dbus_service::tests::reevaluate_all_coalesces` | FR-014 — 100 rapid `reevaluate_all()` calls leave exactly one `All` entry pending |
| `renderer` | `dbus_service::tests::pending_queue_bounded` | FR-014 — queue never exceeds 8 entries under sustained load |
| `renderer` | `dbus_service::tests::output_id_validated` | FR-017 — empty/oversized `output_id` rejected before the known-outputs lookup |
| `wallpaperctl` | `commands::list::tests::tab_newline_escaped_in_human_output` | FR-018 — a name containing `\t\n` renders as one row, not several; `--json` output still carries the raw value |
| `wallpaperctl` | `main::tests::output_flag_conflict_returns_usage_error` | FR-029 — no longer requires `process::exit`, now testable in-process |
| `wallpaperctl` | `error::tests::daemon_unreachable_exit_code_is_four` | FR-028 |
| `wallpaper-ipc` | `renderer_config::tests::output_id_validated_rejects_empty_and_oversized` | FR-017/FR-019 |
| `wallpaper-ipc` | `location_config::tests::corrupted_file_surfaces_warning_not_silent_default` | FR-023 |
| `wallpaper-ipc` | `location_config::tests::save_tightens_permissions` | FR-030 — Unix-only, `#[cfg(unix)]` |
| `wallpaper-settings` | `pages::pack_builder::tests::validate_destination_name_rejects_traversal` | FR-006 — `../../../.config/autostart`, `/home/user/.ssh`, and `""` all rejected |
| `wallpaper-settings` | `pages::pack_builder::tests::generate_handler_rechecks_all_assigned` | FR-026 |
| `wallpaper-settings` | `pages::pack_builder::tests::registration_failure_surfaces_to_move_error` | FR-024 |

## Manual validation (real COSMIC session required)

### US2 — pack builder path traversal (the report's one direct arbitrary-write finding)

1. `cargo run -p wallpaper-settings`.
2. Packs page → "Add pack folder…" → pick a manifest-free scratch folder with a few images →
   configure and Generate a pack.
3. At the placement dialog, choose Move. When prompted for a destination name (simulate a
   collision by first creating a folder with the same name at the standard pack location, or by
   generating twice with the same folder name), type `../../../.config/autostart` and confirm.
4. **Expected (post-fix)**: the app rejects the value with a clear message; `~/.config/autostart`
   is untouched; the source folder is untouched. Repeat with an absolute path
   (e.g. `/tmp/should-not-exist`) and with an empty string — both rejected the same way.
5. **Pre-fix behavior this replaces**: the move silently succeeded outside the sandbox and deleted
   the original source folder (contracts/pack-loader-validation.md is not the relevant contract
   here — this fix is `wallpaper-settings`-local, see data-model.md).

### US6.3 — pack-builder registry-failure surfacing

1. Make the standard pack location temporarily unwritable
   (`chmod 000 ~/.local/share/cosmic-dynamic-wallpaper/packs` or point `XDG_DATA_HOME` somewhere
   read-only for the run).
2. Run through Generate → Move as above.
3. **Expected (post-fix)**: the wizard shows an explicit error (surfaced via `state.move_error`)
   and does not close as if it succeeded. Restore permissions afterward.

### US4 — D-Bus queue bound, observed live

```sh
cargo run -p renderer --bin cosmic-wallpaperd &
for i in $(seq 1 200); do
  busctl --user call com.system76.CosmicDynamicWallpaper1 \
    /com/system76/CosmicDynamicWallpaper1 \
    com.system76.CosmicDynamicWallpaper1.Daemon ReevaluateAll &
done
wait
```

**Expected (post-fix)**: the daemon's memory (`ps -o rss -p $(pgrep cosmic-wallpaperd)`) stays flat
across the burst rather than growing with call count; `journalctl --user -u cosmic-wallpaperd`
(or stdout, depending on how it's run) shows the coalescing/drop behavior in the trace-level logs,
not 200 individually-processed redraw cycles.

## Follow-up: re-run the adversarial audit

Once every user story above lands, the natural closing validation for this whole feature (spec.md
SC-007) is commissioning the same three-persona adversarial review against the fixed branch and
confirming a verdict better than BLOCK with zero remaining critical findings — out of scope for
this quickstart (it's an external review, not a `cargo`-runnable step), but worth flagging as the
actual definition of "done" for the feature as a whole, not just its individual FRs.
