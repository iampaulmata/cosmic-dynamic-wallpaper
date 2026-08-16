---

description: "Task list for Fix Adversarial Audit Findings"
---

# Tasks: Fix Adversarial Audit Findings

**Input**: Design documents from `/specs/011-fix-audit-findings/`

**Prerequisites**: [plan.md](./plan.md) (required), [spec.md](./spec.md) (required for user stories), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/](./contracts/), [quickstart.md](./quickstart.md)

**Tests**: Explicitly required by spec.md (FR-005, FR-013, FR-021; Edge Cases: "every regression
test... must actually fail against the pre-fix code"). Following this codebase's existing
convention (every module colocates tests in its own `#[cfg(test)] mod tests` block), each
implementation task below folds in the regression test(s) it needs rather than splitting into a
separate red/green phase — quickstart.md's test table names each one explicitly for
cross-reference.

**Organization**: Tasks are grouped by spec.md's 8 user stories (US1–US8), in the same priority
order (P1 → P2 → P3 → P4).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: Which user story this task belongs to (US1–US8)
- Every task names its exact file path(s)

## Path Conventions

Existing Cargo workspace (plan.md's Project Structure) — no new crate. All paths are relative to
the repository root.

---

## Phase 1: Setup

**Purpose**: Dependencies and lint gates the later phases build on.

- [X] T001 [P] Add `fd-lock = "4"` as a direct dependency in `crates/pack-loader/Cargo.toml`
      (research.md R17 — needed by US6's registry-locking task)
- [X] T002 ~~Add `[lints.clippy]` to `crates/renderer/Cargo.toml`~~ — **no-op, already present**
      (verified directly against the file during implementation; plan.md corrected)
- [X] T003 ~~Add `[lints.clippy]` to `crates/wallpaper-settings/Cargo.toml`~~ — **no-op, already
      present** (verified directly against the file during implementation; plan.md corrected)

**Checkpoint**: `cargo build --workspace` still succeeds; all six crates already enforce
`unwrap_used`/`expect_used = "deny"`, so every later task in any crate must not trip that
pre-existing gate.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The one piece of shared logic two independent P2 stories (US4, US5) both depend on.

**⚠️ CRITICAL**: T004 must be complete before any task depending on it (T019, T021) can start.

- [X] T004 [P] Add `OutputIdError { reason: String }` and a new fallible constructor
      `OutputId::validated(id: impl Into<String>) -> Result<Self, OutputIdError>` (rejects an
      empty string or a string over 256 bytes) next to the existing `OutputId::new` in
      `crates/wallpaper-ipc/src/renderer_config.rs`; keep `OutputId::new` unchanged for trusted
      internal construction; add unit tests for an empty, an oversized, and a valid id
      (data-model.md `wallpaper-ipc`, research.md R13)

**Checkpoint**: `cargo test -p wallpaper-ipc` passes with the new constructor covered — US4 and
US5 can now both proceed.

---

## Phase 3: User Story 1 - Daemon never crashes on malformed or hostile pack data (Priority: P1) 🎯 MVP

**Goal**: The four documented reachable panics (color parse, zero-size surface reconfigure,
crossfade progress, solar-anchor overflow) become contained, logged errors instead.

**Independent Test**: Feed each of the four documented inputs directly to its function in
isolation and confirm a logged error, not a panic (quickstart.md's automated table, rows 1, 9, 10,
6).

### Implementation for User Story 1

- [X] T005 [P] [US1] In `crates/pack-loader/src/manifest.rs`, add `if !hex.is_ascii() { return
      Err(invalid()); }` as the first check in `Color::parse`, before the byte-offset slicing; add
      regression test `color_parse_rejects_non_ascii_hex` asserting `Color::parse` on a
      `"#€AAA"`-shaped value returns `Err`, not a panic (FR-001, research.md R1)
- [X] T006 [P] [US1] In `crates/renderer/src/surface.rs`'s `reconfigure_output`, clamp `new_size`
      to `(new_size.0.max(1), new_size.1.max(1))` immediately after
      `self.outputs[index].size = Some(new_size)`, before it reaches
      `wgpu::SurfaceConfiguration`; add regression test `zero_size_reconfigure_does_not_panic`
      covering `(0, h)` and `(w, 0)` (FR-002, research.md R2)
- [X] T007 [P] [US1] In `crates/renderer/src/surface.rs`'s `evaluate_output`, where
      `result.transition`/`t.progress` is destructured: skip the frame's transition update (as the
      existing `Ok(None)` arm already does) when `!t.progress.is_finite()`, and clamp the
      remaining finite value to `0.0..=1.0` via `f64::clamp` before `Duration::from_secs_f64` is
      called; add regression test `crossfade_progress_rejects_non_finite` covering `f64::NAN`,
      `f64::INFINITY`, and `-0.5` (FR-003, research.md R3)
- [X] T008 [US1] In `crates/schedule-engine/src/pack.rs`, add `pub const
      MAX_SOLAR_OFFSET_HOURS: i64 = 24;` and a check inside `WallpaperPack::validate` rejecting
      any `TimeAnchor::Solar { offset: Some(delta), .. }` whose `delta.num_hours().abs() >
      MAX_SOLAR_OFFSET_HOURS`, returning a new `PackError::SolarOffsetOutOfRange { event, offset
      }` variant; add regression test `solar_offset_out_of_range_rejected` for
      `TimeAnchor::solar(SolarEventKind::Sunrise, Some(TimeDelta::MAX))`, plus a `proptest`
      property asserting no offset accepted by `validate` can overflow
      `solar::resolve_solar_anchor`'s `base + delta` (FR-004, FR-005, research.md R4)

**Checkpoint**: User Story 1 is fully functional and independently testable — every input the
audit reproduced as a crash now returns a contained error. `cargo test -p pack-loader -p renderer
-p schedule-engine` passes.

---

## Phase 4: User Story 2 - Pack builder can never write outside its intended destination (Priority: P1) 🎯 MVP

**Goal**: The collision-rename field can no longer be used for path traversal or an empty-name
collapse.

**Independent Test**: Run the wizard's rename flow with `../../../.config/autostart`, an absolute
path, and an empty string — all three rejected with no filesystem change (quickstart.md's manual
US2 section).

### Implementation for User Story 2

- [X] T009 [US2] Add pure function `fn validate_destination_name(name: &str) -> Result<(), String>`
      in `crates/wallpaper-settings/src/pages/pack_builder.rs` — rejects an empty string, any
      `std::path::Component` other than `Normal` (via `Path::new(name).components()`), and
      `Path::new(name).is_absolute()`; call it from the `CollisionNameChanged`/`CollisionConfirmed`
      handling before `move_pack` is ever invoked; add unit tests
      `validate_destination_name_rejects_traversal` covering `../../../.config/autostart`, an
      absolute path (`/home/user/.ssh`), and `""` (FR-006, FR-007, research.md R5)
- [X] T010 [US2] Add a test in `crates/wallpaper-settings/src/pages/pack_builder.rs` asserting
      that a rejected rename value (from T009) leaves `generated_path` and the destination root's
      existing contents byte-for-byte unchanged (FR-008)
- [X] T011 [US2] In `crates/wallpaper-settings/src/pages/pack_builder.rs`'s `move_pack`, replace
      the `destination.exists()` check + `create_dir_all` with `std::fs::create_dir` on the final
      destination-name segment as an atomic exists-or-create check immediately before
      `copy_dir_recursive`, minimizing the window between check and write (FR-009, research.md R6)

**Checkpoint**: User Stories 1 AND 2 both work independently — `cargo test -p wallpaper-settings`
passes; the manual traversal reproduction in quickstart.md now fails cleanly.

---

## Phase 5: User Story 3 - Untrusted pack content cannot exhaust daemon or GPU resources (Priority: P2)

**Goal**: The 64-anchor cap, a manifest size cap, and an image dimension/byte ceiling all reject
before the expensive work they currently follow.

**Independent Test**: A 500-entry manifest, a >512 KB manifest, and an oversized/high-dimension
image are each rejected without the per-image filesystem work, full read, or GPU upload that
currently precedes rejection (quickstart.md automated table, rows 2, 3, 11).

### Implementation for User Story 3

- [X] T012 [P] [US3] In `crates/pack-loader/src/load.rs`'s `load_directory_pack`, add
      `if parsed.images.len() > schedule_engine::MAX_ANCHORS { return Err(...) }` immediately
      after `manifest::parse` succeeds, before the per-image resolve/containment/readability loop;
      add regression test `anchor_cap_rejected_before_per_image_io` confirming rejection happens
      without any per-image filesystem call (FR-010, research.md R7)
- [X] T013 [P] [US3] In `crates/pack-loader/src/load.rs`, add `pub const MAX_MANIFEST_BYTES: u64 =
      512 * 1024;` and, before `std::fs::read_to_string(&manifest_path)`, a
      `std::fs::metadata(&manifest_path)` size check rejecting anything larger with a new
      `ManifestError::ManifestTooLarge { path: PathBuf, size: u64 }` variant (add it in
      `crates/pack-loader/src/error.rs`); add regression test `oversized_manifest_rejected`
      (FR-011, research.md R8)
- [X] T014 [US3] In `crates/renderer/src/texture.rs`, add `pub const MAX_DECODED_IMAGE_BYTES: u64 =
      256 * 1024 * 1024;` and, in `GpuTexture::load` before `.to_rgba8()`, a header-only dimension
      check via `image::ImageReader::open(path)?.with_guessed_format()?.into_dimensions()`
      rejecting if either dimension exceeds `device.limits().max_texture_dimension_2d` or
      `width as u64 * height as u64 * 4 > MAX_DECODED_IMAGE_BYTES`; add new
      `RendererError::TextureTooLarge { path: PathBuf, width: u32, height: u32 }` in
      `crates/renderer/src/error.rs`; add regression test `oversized_image_rejected_before_decode`
      (FR-012, research.md R9)
- [X] T015 [US3] Add an end-to-end test (in `crates/renderer/src/texture.rs`'s test module,
      constructing a pack via `pack_loader::load_pack` against a fixture pack containing one
      oversized image) confirming the pack-loader → renderer boundary that `image_check.rs:14-24`
      documents is actually enforced downstream by T014 (FR-013)

**Checkpoint**: User Stories 1–3 all work independently — `cargo test -p pack-loader -p renderer`
passes; SC-003's three reject-before-expensive-work cases are all covered.

---

## Phase 6: User Story 4 - Local D-Bus interface can't be abused by another local process (Priority: P2)

**Goal**: `ReevaluateAll`/`Reevaluate` are bounded and coalesced, a `dbus-1` policy file ships,
`QueryAll` calls are logged, and `output_id` is validated.

**Independent Test**: A tight `ReevaluateAll` loop from a separate process leaves daemon
memory/CPU flat; an oversized/malformed `output_id` is rejected before use (quickstart.md
automated table rows 12–13, and the manual "D-Bus queue bound, observed live" section).

### Implementation for User Story 4

- [X] T016 [US4] In `crates/renderer/src/dbus_service.rs`, add `const
      MAX_PENDING_DBUS_REQUESTS: usize = 8;`; change `reevaluate_all` to a no-op if
      `ReevaluateRequest::All` is already anywhere in `pending`; make both `reevaluate` and
      `reevaluate_all` reject (`zbus::fdo::Error::LimitsExceeded` for the former; `tracing::warn!`
      + silent drop for the latter, whose signature returns `()`) once
      `pending.len() >= MAX_PENDING_DBUS_REQUESTS`; add regression tests
      `reevaluate_all_coalesces` and `pending_queue_bounded` (FR-014, research.md R10)
- [X] T017 [P] [US4] Create `packaging/dbus-1/com.system76.CosmicDynamicWallpaper1.conf` (standard
      `<busconfig><policy>` XML documenting the same-uid session-bus trust boundary) and reference
      it from `packaging/README.md` and the Debian packaging file list in
      `packaging/debian/postinst` (or wherever install paths are enumerated) (FR-015, research.md
      R11)
- [X] T018 [P] [US4] Add a `tracing::debug!` log line in `crates/renderer/src/dbus_service.rs`'s
      `DaemonInterface::query_all` recording that a query occurred (FR-016, research.md R12)
- [X] T019 [US4] In `crates/renderer/src/dbus_service.rs`'s `reevaluate` and `query_output`
      handlers, validate `output_id` via `wallpaper_ipc::OutputId::validated` (T004) before the
      existing known-outputs lookup, returning `zbus::fdo::Error::InvalidArgs` with a specific
      message on failure; add regression test `output_id_validated` (FR-017, depends on T004)

**Checkpoint**: User Stories 1–4 all work independently — `cargo test -p renderer` passes; the
manual D-Bus burst reproduction in quickstart.md shows flat memory.

---

## Phase 7: User Story 5 - User- and pack-supplied strings can't inject output or bypass validation (Priority: P2)

**Goal**: `wallpaperctl list`'s human-readable output can't be spoofed by a crafted pack name,
`--output` rejects empty/malformed values, and absolute-path manifest entries are explicitly
rejected.

**Independent Test**: A pack `name` containing tabs/newlines renders as one row, not several;
`assign --output ""` and `--output "DP-3;rm -rf /"` are both rejected; an absolute-path manifest
entry is explicitly rejected (quickstart.md automated table rows 14, 4).

### Implementation for User Story 5

- [X] T020 [P] [US5] Add `pub fn sanitize_for_tsv(s: &str) -> std::borrow::Cow<'_, str>` in
      `crates/wallpaperctl/src/output.rs` (replacing `\t`/`\n`/`\r` with a space, collapsing
      repeats), applied only in `crates/wallpaperctl/src/commands/list.rs`'s human-readable
      rendering closure — never the `--json` `Serialize` path; add regression test
      `tab_newline_escaped_in_human_output` confirming the `--json` output is byte-for-byte
      unaffected (FR-018, research.md R14)
- [X] T021 [US5] Add `CliError::InvalidOutputId { reason: String }` (exit code `1`) in
      `crates/wallpaperctl/src/error.rs`; validate `assign --output <id>`'s value via
      `wallpaper_ipc::OutputId::validated` (T004) in `crates/wallpaperctl/src/main.rs` before
      storing it, returning the new error on failure; add regression test covering an empty value
      and a value containing `;` (FR-019, research.md R13/R15, depends on T004). **Strengthened
      during implementation**: `OutputId::validated` (T004) originally only checked non-empty/
      length, which does not actually reject `"DP-3;rm -rf /"` (non-empty, well within the length
      limit) — spec.md's own Independent Test for US5 requires rejecting exactly that string.
      Added a character-class check (ASCII alphanumeric, `-`, `_` only — the shape every real
      Wayland connector name already has) to `OutputId::validated` itself, so both this call site
      and T019's D-Bus boundary now correctly reject it.
- [X] T022 [P] [US5] In `crates/pack-loader/src/path_safety.rs`'s `resolve_and_check`, add
      `if Path::new(file).is_absolute() { return Err(ManifestError::PathEscapesPackDirectory {
      file: file.to_string() }); }` as the first check, before `pack_dir.join(file)` (FR-020,
      research.md R16)
- [X] T023 [P] [US5] Add a new fixture directory (mirroring the existing
      `fixtures/invalid/path_traversal` layout) exercising an absolute-path manifest entry, and a
      unit test `rejects_absolute_path` in `crates/pack-loader/src/path_safety.rs`'s test module
      (FR-021)

**Checkpoint**: User Stories 1–5 all work independently — `cargo test -p wallpaperctl -p
pack-loader` passes.

---

## Phase 8: User Story 6 - Failures are surfaced, never silently swallowed (Priority: P3)

**Goal**: Registry writes serialize instead of racing, corrupted configs are distinguishable from
"never set," pack-builder registration failures are shown to the user, `location set` is atomic,
the generate handler re-checks its own gate, and the manifest write is deferred to the actual
Move/Keep choice.

**Independent Test**: Two concurrent `Registry` writers don't lose either write; a hand-corrupted
config file produces a visible warning; a simulated pack-builder registry failure is shown, not
silently discarded (quickstart.md automated table rows 5, 18–19, and manual US6.3 section).

### Implementation for User Story 6

- [X] T024 [US6] In `crates/pack-loader/src/registry.rs`, wrap `Registry::persist()`'s
      read-modify-write with an `fd-lock` (T001) exclusive lock on a dedicated `.lock` file next
      to the registry's `cosmic-config`-managed storage; add new `RegistryError::LockFailed {
      message: String }`; add regression test `concurrent_persist_serializes` using two `Registry`
      handles opened against the same custom path (FR-022, research.md R17, depends on T001)
- [X] T025 [P] [US6] **Revised during implementation** (research.md R18's corrected entry — a
      `load`-signature change would have touched ~45 call sites for no benefit to most of them).
      Added `load_reporting_corruption(config: &Config) -> (Self, bool)` to both
      `crates/wallpaper-ipc/src/renderer_config.rs`'s `RendererConfig` and
      `crates/wallpaper-ipc/src/location_config.rs`'s `LocationConfigEntry`; `load` is now a thin
      wrapper (signature unchanged, zero call-site ripple). Logs via `tracing::warn!` only when
      `cosmic_config::Error::is_err()` — the library's own never-configured-vs-genuinely-corrupted
      predicate — confirms real corruption, not the ordinary "key never written" case. Added
      regression tests `corrupted_file_surfaces_warning_not_silent_default` and
      `never_configured_is_not_reported_as_corruption` for each, writing genuinely invalid RON
      directly into the on-disk key file (`cosmic-config`'s real per-field layout, confirmed by
      direct inspection, not guessed) (FR-023, research.md R18)
- [X] T026 [US6] Update `crates/wallpaperctl/src/commands/location.rs`'s `location get` to print a
      distinguishing message when T025's corruption flag is set, while keeping exit code `0` in
      both the corrupted and never-set cases (FR-023, depends on T025)
- [X] T027 [US6] In `crates/wallpaper-settings/src/pages/pack_builder.rs`'s `register_and_close`,
      route `PackSource::resolve` and `registry.register` failures into `state.move_error` instead
      of `if let Ok(...)`/`let _ =` discarding them; only clear `pending_placement`/
      `pending_collision` on the success path; add regression test
      `registration_failure_surfaces_to_move_error` (FR-024, research.md R19)
- [X] T028 [P] [US6] **No code change — verified already the case, corrected during
      implementation** (research.md R20's corrected entry). Opened
      `crates/wallpaperctl/src/commands/location.rs` to make the planned change and found `set`/
      `clear`/`auto`/`ip` already call `state.save(config)` exactly once each (no sequential
      per-field write pattern exists in this project's code at all); `LocationConfigEntry::save`
      (`write_entry`, generated by `#[derive(CosmicConfigEntry)]`) already builds one
      `config.transaction()` covering every field before a single `tx.commit()`. The
      `//TODO: apply all changes at once` comment this task's research cited is real but lives
      inside `cosmic-config`'s own `ConfigTransaction::commit()` in the upstream `pop-os/libcosmic`
      dependency, not in this project — `commit()` still writes each field as its own separate
      `atomicwrites::AtomicFile` operation in a loop, so a residual (narrow, no-interleaved-logic)
      gap exists, but closing it fully requires an upstream fix this project doesn't control.
      Judged disproportionate to build a hand-rolled journaling workaround for a P3-priority,
      already-batched, dependency-internal gap (FR-025, research.md R20)
- [X] T029 [US6] In `crates/wallpaper-settings/src/pages/pack_builder.rs`, add a re-check of the
      existing `all_assigned(&state)` pure function at the top of the `Message::GenerateRequested`
      handler, populating `state.generate_error` and returning early if it's `false`; also change
      `build_draft` to return `Err` for unassigned rows instead of silently filtering them; add
      regression test `generate_handler_rechecks_all_assigned` (FR-026, research.md R21)
- [X] T030 [US6] **More involved than originally sketched** (research.md R22's corrected entry).
      In `crates/wallpaper-settings/src/pages/pack_builder.rs`: `generate()` no longer writes
      `manifest.toml` into `source_dir` — self-validates against a scratch directory (real file
      *copies*, not symlinks: an initial symlink-based version tripped `pack_loader::path_safety`'s
      own containment check, which deliberately rejects a symlink resolving outside the pack
      directory) and stores the rendered text in a new `state.pending_manifest_text` field. New
      `write_manifest_and_register` helper (used by `confirm_keep`/`cancel_collision_to_keep`)
      writes it into the source folder only when "Keep" is actually chosen; `move_pack` gained a
      `manifest_text: &str` parameter and writes it into the destination after copying, for "Move".
      Updated 5 existing tests whose assumptions no longer held (manifest pre-written to source);
      added `manifest_is_not_written_between_generate_and_the_placement_choice`, which drops
      `state` right after `generate()` (simulating a crash) and confirms `should_open_for` still
      re-opens the wizard rather than silently treating the folder as already placed (FR-027,
      research.md R22)

**Checkpoint**: User Stories 1–6 all work independently — `cargo test -p pack-loader -p
wallpaper-ipc -p wallpaperctl -p wallpaper-settings` passes.

---

## Phase 9: User Story 7 - Diagnostics, trust assumptions, and defensive gaps are accurate and hardened (Priority: P3)

**Goal**: Exit codes stop colliding, config files get tightened permissions, IP geolocation
sanity-bounds its result, portal updates are debounced, GPU requests time out, lost surfaces
recover, the unsafe block is documented, GPU textures are bounded, the D-Bus mutex invariant is
asserted, pole locations resolve fast, and duplicate-instant checking is unconditional.

**Independent Test**: A clap usage error and a genuine daemon-down condition now produce
different exit codes; a freshly-written location config file is not world-readable; a
pole-latitude query returns near-instantly (quickstart.md automated table rows 15–17, 6–8).

### Implementation for User Story 7

- [X] T031 [P] [US7] Change `CliError::DaemonUnreachable`'s `exit_code()` mapping from `2` to `4`
      in `crates/wallpaperctl/src/error.rs`; add regression test
      `daemon_unreachable_exit_code_is_four` (FR-028, research.md R23)
- [X] T032 [US7] Add `CliError::UsageError { message: String }` (exit code `2`) in
      `crates/wallpaperctl/src/error.rs`; change the `--output`/`--same-everywhere` conflict check
      in `crates/wallpaperctl/src/main.rs` to `return Err(CliError::UsageError { .. })` instead of
      `eprintln!` + `std::process::exit(1)` (or express the constraint via a `clap` `ArgGroup` if
      it fits the existing `Cli` derive layout without disruption — implementer's judgment call
      per research.md R24); add regression test `output_flag_conflict_returns_usage_error`
      (FR-029, research.md R24, depends on T031)
- [X] T033 [P] [US7] Add Unix-only (`#[cfg(unix)]`) permission-tightening in
      `crates/wallpaper-ipc/src/location_config.rs` and
      `crates/wallpaper-ipc/src/renderer_config.rs`: after a successful `save()`, resolve
      `dirs::config_dir().join("cosmic").join(app_id).join(format!("v{version}"))` (documented in
      a code comment as reconstructing `cosmic-config`'s internal-but-stable convention, per
      research.md R25) and `std::fs::set_permissions` that directory to mode `0o700`; add
      regression test `save_tightens_permissions` (FR-030, research.md R25)
- [X] T034 [US7] In `crates/renderer/src/ip_geolocation.rs`, add `const
      MAX_PLAUSIBLE_LOCATION_JUMP_KM: f64 = 2000.0;` and compare a newly-resolved location against
      the most recent trusted one, rejecting (log + skip) a jump exceeding that bound with no
      intervening manual location change; add a unit test using a synthetic forged reply (FR-031,
      research.md R26)
- [X] T035 [US7] In `crates/renderer/src/bin/cosmic-wallpaperd.rs`, wrap the `PortalEvent::Reading`
      handler insertion in the same 2s debounce primitive already used elsewhere in this file,
      replacing today's synchronous-per-event write (FR-032, research.md R27)
- [X] T036 [P] [US7] In `crates/renderer/src/gpu.rs`, add `const GPU_REQUEST_TIMEOUT: Duration =
      Duration::from_secs(20);` and wrap `request_adapter`/`request_device` with
      `futures_lite::future::or(actual_request, timeout_after(GPU_REQUEST_TIMEOUT))`; add new
      `RendererError::GpuRequestTimedOut` in `crates/renderer/src/error.rs` (FR-033, research.md
      R28)
- [X] T037 [US7] In `crates/renderer/src/surface.rs`'s draw path, on `SurfaceError::Lost`/
      `Outdated`, call `reconfigure_output` again using the output's last-known `size` instead of
      only logging and returning (FR-034, research.md R29, depends on T006)
- [X] T038 [P] [US7] Add a `// SAFETY:` comment above the `unsafe` raw-window-handle block in
      `crates/renderer/src/surface.rs` documenting that the borrowed Wayland `wl_surface`/
      `wl_display` handles are owned by structures that outlive every `wgpu::Surface` built from
      them (FR-035, research.md R30)
- [X] T039 [US7] In `crates/renderer/src/surface.rs`, replace the per-output
      `HashMap<ImageId, GpuTexture>` texture cache with a bounded `TextureCache` (LRU eviction,
      `const MAX_CACHED_TEXTURES_PER_OUTPUT: usize = 16;`), keeping `ensure_texture`'s existing
      signature unchanged (FR-036, research.md R31)
- [X] T040 [P] [US7] In `crates/renderer/src/dbus_service.rs`, add a `debug_assert!` capturing the
      main thread's `ThreadId` at daemon startup and checking every subsequent `DaemonInterface`
      call runs on it, plus a doc-comment cross-reference to the existing "never contended" module
      comment (FR-037, research.md R32)
- [X] T041 [US7] In `crates/schedule-engine/src/location.rs` (or `query.rs`, whichever owns the
      search entry point), add `const POLE_LATITUDE_THRESHOLD: f64 = 89.9999;` and an early
      `return None` when `location.latitude().abs() >= POLE_LATITUDE_THRESHOLD`, before the
      radius-doubling search runs; add regression test `pole_latitude_returns_none_fast` asserting
      a bounded cost (e.g. via a call-count or timing assertion) (FR-038, research.md R33)
- [X] T042 [P] [US7] Correct `MAX_SEARCH_RADIUS_DAYS`'s value/doc comment in
      `crates/schedule-engine/src/query.rs` to state the true worst-case search radius (up to 512
      days) under the existing check-then-double loop ordering (FR-039, research.md R33)
- [X] T043 [US7] **Corrected during implementation** (research.md R34's corrected entry —
      `WallpaperPack::validate` structurally cannot perform this check, it has no date/location
      argument). In `crates/renderer/src/surface.rs`'s `load_pack_for`, after a pack loads
      successfully, call `loaded.pack.check_solar_duplicate_instant(location, Local::now()
      .date_naive())` when `self.location` is `Some`, logging `tracing::warn!` on a collision
      without blocking the load — this is the actual runtime call site (every registered pack
      goes through it on assignment/reload/restart); the only prior caller anywhere in the
      workspace was the pack-builder GUI, at build time only. Covered by
      `check_solar_duplicate_instant`'s own existing unit tests in `pack.rs` (unchanged, still
      passing) plus manual code review — no new dedicated test added, since exercising the new
      call site's log-only side effect would need a full `WallpaperDaemon` (GPU/Wayland) harness
      disproportionate to a log-line-only fix (FR-040)

**Checkpoint**: User Stories 1–7 all work independently — `cargo test --workspace` passes; every
"Verified — reproduced" finding from the audit is now covered by a passing regression test.

---

## Phase 10: User Story 8 - Code-health and documentation gaps are cleaned up (Priority: P4)

**Goal**: The remaining note-level findings (refactors, naming, doc gaps) are resolved. Each is
self-contained with no design decision beyond "do what the finding says" (research.md R35–R36 and
summary).

**Independent Test**: Each item is verifiable by inspection/lint after the change — no behavioral
test beyond "existing tests still pass."

### Implementation for User Story 8

- [X] T044 [P] [US8] Route `tools/generate-starter-pack/src/main.rs`'s manifest generation through
      `pack_loader::manifest::render` instead of hand-built string interpolation (FR-041,
      research.md R35)
- [X] T045 [P] [US8] In `crates/renderer/src/surface.rs`'s `reconfigure_output`, guard
      `caps.formats[0]`/`caps.alpha_modes[0]` with `.first().ok_or(...)`, degrading that one
      output on `None` instead of panicking (FR-042, research.md R36)
- [X] T046 [P] [US8] Add a module-level index comment in `crates/renderer/src/surface.rs` listing
      every trait impl that can originate a resize/hotplug event (FR-043)
- [X] T047 [P] [US8] In `crates/wallpaper-ipc/src/dbus_client.rs`, preserve the real `InvalidArgs`
      error message instead of collapsing every case to "output not found" (FR-044)
- [X] T048 [P] [US8] Extract the backoff constants duplicated in
      `crates/renderer/src/portal_location.rs` and `crates/renderer/src/ip_geolocation.rs` into
      one shared module-level source (FR-045)
- [X] T049 [P] [US8] Factor the "spawn exactly once" pattern copy-pasted three times in
      `crates/renderer/src/bin/cosmic-wallpaperd.rs` into one shared helper (FR-046)
- [ ] T050 [P] [US8] Rename or doc-comment `crates/schedule-engine/src/query.rs`'s `active_before`
      field to clarify it holds the outgoing image during a transition, not the currently-active
      one (FR-047)
- [ ] T051 [P] [US8] Document or enforce a length bound on `ImageId`'s wrapped string in
      `crates/schedule-engine/src/pack.rs` (FR-048)
- [ ] T052 [P] [US8] Document the already-implemented `location ip` subcommand in
      `crates/wallpaperctl/README.md` (FR-049)
- [ ] T053 [P] [US8] Document the well-known-bus-name trust assumption in
      `crates/wallpaper-ipc/src/dbus_client.rs`, alongside the daemon-side authorization gap from
      US4 (FR-050)
- [ ] T054 [P] [US8] Update `crates/wallpaper-settings/README.md` to mention the pack builder
      wizard and reflect the current test count (FR-051)
- [ ] T055 [P] [US8] Add an in-flight guard on "Add pack folder…" in
      `crates/wallpaper-settings/src/app.rs`/`crates/wallpaper-settings/src/pages/packs.rs`
      preventing a rapid double-click from opening two concurrent file-chooser dialogs (FR-052)

**Checkpoint**: All 8 user stories are independently functional — every one of the 52 audit
findings has a corresponding completed task.

---

## Phase 11: Polish & Cross-Cutting Concerns

**Purpose**: Whole-workspace verification and documentation cross-references, after every user
story above is complete.

- [ ] T056 [P] Run `cargo clippy --workspace --all-targets -- -D warnings` and resolve anything
      the new lint gates (T002, T003) or any fix above surfaces
- [ ] T057 [P] Cross-reference `contracts/wallpaperd-dbus-hardening.md` and
      `contracts/wallpaperctl-cli-hardening.md`'s exit-code renumbering (T031) from
      `specs/004-cli-control-surface/contracts/wallpaperd-dbus-interface.md` and
      `specs/004-cli-control-surface/contracts/wallpaperctl-cli.md`, so the original contracts
      point forward to the hardening delta rather than silently going stale
- [ ] T058 Run the full `quickstart.md` validation: `cargo test --workspace` plus the three manual
      COSMIC-session reproductions (US2 path traversal, US6.3 registry failure, US4 D-Bus queue
      burst) — confirm every one now fails cleanly where the audit originally reproduced a crash
      or silent success

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — start immediately.
- **Foundational (Phase 2)**: Depends on Setup — BLOCKS US4 (T019) and US5 (T021) specifically,
  not the other stories.
- **User Stories (Phase 3–10)**: US1–US3 have no dependency on Phase 2 and can start as soon as
  Phase 1 completes; US4/US5 additionally need Phase 2 (T004). All 8 stories are otherwise
  independent of each other — see below for the handful of same-file orderings within a story.
- **Polish (Phase 11)**: Depends on every user story phase being complete.

### User Story Dependencies

- **US1, US2 (P1)**: No dependency on any other story — this is the MVP.
- **US3, US4, US5 (P2)**: No dependency on US1/US2/each other. US4 (T019) and US5 (T021) each
  depend on Phase 2's T004.
- **US6, US7 (P3)**: No dependency on US1–US5. Within US7, T032 depends on T031 (same file,
  sequential exit-code scheme); T037 depends on T006 (US1, same function `reconfigure_output`);
  T043 depends on T008 (US1, same function `WallpaperPack::validate`) — both cross-story
  dependencies are same-*function* edits, not story-level coupling, and are called out explicitly
  in each task above.
- **US8 (P4)**: No dependency on US1–US7 except T045 touching the same `surface.rs` function as
  T006/T037/T039 (US1/US7) — sequenced after those phases by priority order, not a hard blocker if
  worked out of order.

### Within Each User Story

- Tasks are listed in the order they should be implemented; `[P]`-marked tasks within a phase
  touch different files and can run in parallel with each other.
- Every regression test named in a task is written alongside that task's implementation (this
  codebase's existing convention — see quickstart.md).
- Each story's Checkpoint gates moving to the next priority tier, but does not block a *different*
  story at the same or lower priority from starting in parallel if staffed.

### Parallel Opportunities

- All Setup tasks (T001–T003) in parallel.
- T004 (Foundational) has no parallel peer but is small.
- US1's four tasks (T005–T008) are all `[P]` — different files/functions.
- US3's T012/T013 (`[P]`, both in `pack-loader`) and T014 (`renderer`) can run in parallel with
  each other; T015 depends on T014.
- US4's T017/T018 (`[P]`) can run in parallel with T016; T019 depends on T004.
- US5's T020/T022/T023 (`[P]`) can run in parallel with each other and with T021 (which depends on
  T004).
- Once Phase 2 completes, **US1, US2, US3, US4, and US5 (all of P1+P2) can be worked fully in
  parallel** by different contributors — none of them depends on another.
- US6 and US7 (P3) can likewise proceed in parallel with each other once P1/P2 are done (or even
  concurrently with them, aside from the two same-function edges noted above).
- US8's twelve tasks (T044–T055) are all `[P]` — each touches a distinct file/finding with no
  cross-task dependency.

---

## Parallel Example: User Story 1 (the MVP's crash-proofing half)

```bash
# All four US1 tasks touch different files/functions — launch together:
Task: "Fix Color::parse ASCII guard in crates/pack-loader/src/manifest.rs (T005)"
Task: "Guard zero-size surface reconfigure in crates/renderer/src/surface.rs (T006)"
Task: "Clamp crossfade progress in crates/renderer/src/surface.rs's evaluate_output (T007)"
Task: "Bound solar-anchor offset in crates/schedule-engine/src/pack.rs (T008)"
```

Note T006 and T007 are both in `surface.rs` but touch different functions (`reconfigure_output`
vs. `evaluate_output`) with no shared state — safe to parallelize as separate edits, though a
single contributor doing both back-to-back is equally reasonable.

---

## Implementation Strategy

### MVP First (User Stories 1 + 2 — both P1)

1. Complete Phase 1: Setup.
2. Complete Phase 2: Foundational (only strictly needed before US4/US5, but cheap enough to do
   first regardless).
3. Complete Phase 3 (US1) and Phase 4 (US2) — together, these close every crash and the one
   arbitrary-write vector the audit found.
4. **STOP and VALIDATE**: run `cargo test -p pack-loader -p renderer -p schedule-engine -p
   wallpaper-settings` and the quickstart.md US1/US2 checks independently.
5. This is the point at which the audit's **BLOCK** verdict's most severe root causes (crashes,
   arbitrary write) are resolved, even if P2–P4 items remain.

### Incremental Delivery

1. Setup + Foundational → foundation ready.
2. US1 + US2 (P1) → validate independently → this is the MVP.
3. US3 + US4 + US5 (P2) → validate independently → closes resource-exhaustion and trust-boundary
   gaps.
4. US6 + US7 (P3) → validate independently → closes silent-failure and diagnostic gaps.
5. US8 (P4) → code-health cleanup, safe to defer past a release if time-boxed (spec.md
   Assumptions).
6. Phase 11 (Polish) → whole-workspace verification, then the external follow-up adversarial
   review (spec.md SC-007) as the feature's true closing validation.

### Parallel Team Strategy

With multiple contributors, once Phase 1+2 are done: assign US1/US2 to whoever owns the MVP
timeline, and US3/US4/US5 to others in parallel — all five are independent of each other. US6/US7
can start any time after Phase 2 as well; US8 is the natural "pick up whatever's left" bucket
since every one of its 12 tasks is independent and low-risk.

---

## Notes

- `[P]` tasks = different files or different functions within a shared file, no dependency on an
  incomplete task.
- `[Story]` label maps every implementation task to its spec.md user story for traceability back
  to the original audit finding (every task cites its FR number and research.md decision).
- Every regression test named above is required to fail against pre-fix code before the fix lands
  (spec.md Edge Cases) — this is the acceptance bar for each task, not an optional nice-to-have.
- Commit after each task or logical group; stop at any Checkpoint to validate a story
  independently before continuing.
- Avoid: combining two unrelated FRs into one commit, skipping a task's named regression test,
  reordering a same-file dependency called out above (T031→T032, T006→T037, T008→T043, T004→T019/
  T021).
