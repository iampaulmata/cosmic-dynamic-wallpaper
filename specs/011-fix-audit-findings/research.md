# Research: Fix Adversarial Audit Findings

Every entry below was checked against the actual source at `main` @ `50ca3f6`, not just the
audit's prose description — several fixes below (R20, R33 in particular) turned out to have a
cleaner implementation than a literal reading of the finding would suggest. Grouped by user
story; each maps to one or more FR numbers from spec.md.

## User Story 1 — Crash-proofing

### R1: `Color::parse` non-ASCII guard (FR-001)

- **Decision**: Add `if !hex.is_ascii() { return Err(invalid()); }` as the first check inside
  `Color::parse`, before the `match hex.len()` that does byte slicing.
- **Rationale**: Confirmed the exact bug by reading `manifest.rs`: `hex.len()` counts bytes, and
  `hex[0..2]` etc. slice by byte offset. For ASCII input, byte offset and char boundary always
  coincide, so rejecting non-ASCII input up front makes every existing slice provably safe with
  zero change to the slicing logic itself — the fix the audit's own priority list recommends
  ("reject non-ASCII input before slicing") over char-boundary-aware indexing, because it's a
  one-line change to the one function versus rewriting four slice expressions.
- **Alternatives considered**: `s.get(0..2)` (char-boundary-safe slicing) — rejected as more
  invasive for no behavioral difference, since a manifest color is defined as hex digits (already
  ASCII-only by the format's own grammar) and non-ASCII is always an error either way.

### R2: Zero-size surface reconfigure guard (FR-002)

- **Decision**: In `reconfigure_output`, clamp `new_size` to `(new_size.0.max(1), new_size.1.max(1))`
  immediately after `self.outputs[index].size = Some(new_size)`, before it reaches
  `wgpu::SurfaceConfiguration`.
- **Rationale**: Confirmed `new_size.0`/`.1` flow directly into `SurfaceConfiguration.width`/
  `.height` with no guard. The layer-shell protocol's own semantics (0 on an axis means "you
  choose") make `.max(1)` the spec-correct interpretation, not a workaround — a 1×1 surface is a
  degenerate-but-valid render target that keeps the output alive until the next real configure
  event corrects it.
- **Alternatives considered**: Skip reconfiguration entirely on a zero axis (leave the previous
  surface config in place) — rejected because it leaves stale dimensions on first-ever configure
  (no previous config to fall back to), which `.max(1)` avoids by always producing *a* valid config.

### R3: Crossfade progress clamp (FR-003)

- **Decision**: At the point `result.transition` is destructured in `evaluate_output`, first check
  `!t.progress.is_finite()` and treat that case as "no visible transition progress" — skip this
  frame's transition update, the same as the existing `Ok(None)` arm already does — before
  `Duration::from_secs_f64` is ever called. For the remaining finite case, clamp to `0.0..=1.0`
  via `f64::clamp` before use.
- **Rationale**: Rust's `f64::clamp(min, max)` is defined as `if self < min { min } else if self >
  max { max } else { self }`; for `self = NaN`, both comparisons are `false` (NaN compares
  unordered against everything), so `clamp` returns `self` — i.e. `NaN.clamp(0.0, 1.0)` is still
  `NaN`, not a defined value. Clamping alone therefore does not neutralize the NaN/infinite case
  the audit reproduced; the explicit `is_finite` guard is required, with `clamp` only handling the
  finite-but-out-of-range case (a negative progress or a value slightly over `1.0`).
- **Alternatives considered**: Clamp only, relying on it to also handle NaN — rejected once
  `f64::clamp`'s actual NaN behavior was checked against its documented definition (above); it
  would leave the exact panic the audit reproduced unfixed.

### R4: Solar-anchor offset bound (FR-004)

- **Decision**: Add a bound check inside `WallpaperPack::validate` (`pack.rs`) — reject any
  `TimeAnchor::Solar { offset: Some(delta), .. }` whose `delta.num_hours().abs() > 24` (or
  equivalently `delta.abs() > TimeDelta::hours(24)`) with a new `PackError` variant, e.g.
  `SolarOffsetOutOfRange`.
- **Rationale**: Read `anchor.rs`: `TimeAnchor::solar()` is a bare, infallible constructor used by
  both the manifest parser and test code, so changing its signature to return `Result` would
  ripple across every call site for no benefit — the crate's own existing convention is that
  *construction* is free-form and *validation* happens once, centrally, in `WallpaperPack::
  validate` (the same function that already enforces `MAX_ANCHORS` and mixed-anchor-type
  rejection). A ±24h bound is double the pack-builder GUI's own existing ±12h clamp (spec
  010-custom-pack-builder), giving hand-authored manifests headroom the GUI doesn't need while
  still making the `DateTime + TimeDelta` overflow in `solar.rs:65` (`base + delta`) provably
  unreachable — no realistic date plus 24h can overflow `chrono`'s `DateTime` range.
- **Alternatives considered**: Bound at `TimeAnchor::solar()` construction (rejected — see above,
  breaks the crate's existing fallible-validation-is-centralized convention and every existing
  caller); bound only at the manifest-parsing layer in `pack-loader` (rejected — `schedule-engine`
  is the crate whose own "never panics" doc comment the audit quotes, so the fix belongs where
  that contract is stated and already enforced for other anchor properties).

## User Story 2 — Pack builder path traversal

### R5: Collision-rename validation (FR-006, FR-007)

- **Decision**: Add a new pure function `validate_destination_name(name: &str) -> Result<(), String>`
  local to `pack_builder.rs`, called at the point `CollisionNameChanged`/`CollisionConfirmed` is
  handled, before `move_pack` is ever invoked. Rejects: empty string, any `std::path::Component`
  other than `Normal` (i.e. rejects `..`, `.`, `/`, and Windows-style roots/prefixes via
  `Path::new(name).components()`), and `Path::new(name).is_absolute()`.
- **Rationale**: `pack-loader::path_safety::resolve_and_check` was considered and rejected as the
  reused implementation (see Alternatives) — its `candidate.exists()` requirement makes it the
  wrong tool for validating a *destination* name that must not yet exist. A local, dependency-free
  component-based check is both simpler and correct for this specific pre-existence use, and
  mirrors the same "no path separators, no `..`, no absolute path" shape the audit's own priority
  fix #1 specifies.
- **Alternatives considered**: Depend on `pack-loader::path_safety` from `wallpaper-settings` and
  adapt it — rejected because `resolve_and_check` fundamentally assumes the target exists
  (`candidate.exists()` is its first check) and is designed to validate a *read* path, not gate a
  future *write* path; forcing it to fit here would need a parallel code path anyway, at which
  point a small local function is more honest about what's actually being checked.

### R6: Empty-name-collapse and TOCTOU (FR-007, FR-009)

- **Decision**: Folded into R5 (`validate_destination_name` rejects the empty string, closing the
  `destination_root.join("")` collapse directly) plus keep `destination.exists()` and the
  subsequent `create_dir_all`/copy in `move_pack` as close together as they already are today (no
  intervening I/O or user interaction is currently inserted between them, and none is being added
  by any other fix in this feature) — no structural change beyond R5's input validation.
- **Rationale**: The audit itself calls the TOCTOU window "low severity on a single-user desktop
  app," and confirmed by reading `move_pack`: the existence check and the write already happen in
  the same synchronous function call with no yield point between them (this project has no async
  UI event loop that could interleave another operation there). The residual risk is the same
  general TOCTOU class every `if !path.exists() { create }` pattern has anywhere in Unix-like
  filesystems, and closing it completely would need `O_EXCL`-style atomic create-if-absent
  semantics on a whole-directory copy, which `std::fs` has no cross-platform primitive for — out of
  proportion to the audit's own severity rating for this specific note.
- **Alternatives considered**: `std::fs::create_dir` (not `create_dir_all`) on the destination
  itself as an atomic existence-and-create test (fails if it already exists) before
  `copy_dir_recursive` populates it — worth doing as a small strengthening since it's nearly free;
  adopted as part of R5's implementation rather than as a separate finding.

## User Story 3 — Resource caps on untrusted pack content

### R7: Anchor-count pre-check ordering (FR-010)

- **Decision**: In `load_directory_pack` (`load.rs`), add
  `if parsed.images.len() > schedule_engine::MAX_ANCHORS { return Err(...) }` immediately after
  `manifest::parse` succeeds, before the `for img in &parsed.images` loop that calls
  `path_safety::resolve_and_check`/`image_check::check_readable`.
- **Rationale**: Confirmed the exact ordering bug by reading `load.rs`: the loop performing
  per-image filesystem work runs unconditionally before `WallpaperPack::validate` (which enforces
  `MAX_ANCHORS`) is ever called. `MAX_ANCHORS` is already `pub` from `schedule-engine` (used
  by `wallpaper-settings`'s own `pack_builder.rs` scan-cap check already), so this is a same-crate-
  boundary reuse, not a new export.
- **Alternatives considered**: Move the whole per-image loop after `WallpaperPack::validate` —
  rejected because `validate` itself needs the resolved `(ImageId, TimeAnchor)` pairs the loop
  produces (a genuine dependency), whereas the *count* check needs nothing from the loop and can
  run first as a cheap, independent pre-check.

### R8: Manifest size cap (FR-011)

- **Decision**: Before `std::fs::read_to_string(&manifest_path)`, call
  `std::fs::metadata(&manifest_path)` and reject if `.len() > 512 * 1024` with a new
  `ManifestError::ManifestTooLarge { path, size }` variant.
- **Rationale**: A `metadata()` call is a single stat syscall, cheap even on the multi-gigabyte
  attack file the audit describes, and rejects before the expensive full read — exactly the
  "reject before the expensive work" pattern SC-003 requires. 512 KB (per clarification) leaves
  roughly 40x headroom over a realistic 64-anchor manifest's actual size.
- **Alternatives considered**: `std::io::Read::take(512 * 1024 + 1)` and check the byte count read
  — rejected as strictly worse here: it still performs the (bounded) read before rejecting, and
  `metadata()` avoids that read entirely for the common oversized case (TOCTOU between `metadata`
  and `read_to_string` is immaterial — a size that grows between the two only means the read
  itself may then also exceed the cap, which is fine since it's now bounded work either way, not a
  security-relevant race).

### R9: GPU image dimension/byte ceiling (FR-012)

- **Decision**: In `GpuTexture::load`, before calling `.to_rgba8()` (the actual decode), use
  `image::ImageReader::open(path)?.with_guessed_format()?.into_dimensions()` to read the image's
  dimensions from its header only. Reject if either dimension exceeds
  `device.limits().max_texture_dimension_2d`, or if `width as u64 * height as u64 * 4 >
  256 * 1024 * 1024` (the 256 MB decoded-byte ceiling from clarification). Only call
  `image::open(path)?.to_rgba8()` (the full decode) after both checks pass.
- **Rationale**: Confirmed `image::open(...).to_rgba8()` is a single call that fully decodes before
  any check can run today — the audit's "no gate before GPU upload" finding is really "no gate
  before *decode*," which is the more expensive and more dangerous half (a crafted image with
  legitimate-looking headers but a huge decoded size is the classic decompression-bomb shape).
  `image`'s `ImageReader::into_dimensions()` reads only the header for every format this crate
  already decodes (JPEG/PNG/GIF/WebP/BMP/TIFF), so this check is cheap even for a bomb.
- **Alternatives considered**: Check dimensions only against `max_texture_dimension_2d`, skip the
  byte ceiling — rejected because `max_texture_dimension_2d` alone (typically 8192–16384 on real
  GPUs) still permits a legal-by-that-check image whose RGBA8 decode is ~268–1074 MB; the separate
  256 MB byte ceiling (comfortably above a real 8K wallpaper's ~132 MB) closes that gap.

## User Story 4 — Local D-Bus trust boundary

### R10: Bounded, coalesced `ReevaluateAll` queue (FR-014)

- **Decision**: Two changes to `DbusState`/`DaemonInterface` in `dbus_service.rs`: (1)
  `reevaluate_all` becomes a no-op if `pending.back() == Some(&ReevaluateRequest::All)` *or* if
  `pending` already contains `ReevaluateRequest::All` anywhere (a linear scan over at most 8
  entries is negligible) — repeated `ReevaluateAll` calls collapse to one pending entry. (2) both
  `reevaluate` and `reevaluate_all` reject (return `zbus::fdo::Error::LimitsExceeded` for the
  former; silently drop-and-log for the latter, which returns `()` today) once
  `pending.len() >= 8`.
- **Rationale**: Confirmed `pending: VecDeque<ReevaluateRequest>` has unbounded `push_back` on both
  call paths. Coalescing dominates the actual attack the audit describes (spamming `ReevaluateAll`
  in a tight loop) — after the first queued `All`, every subsequent call in the loop becomes O(1)
  and produces zero additional queue growth, which is a stronger fix than a bound alone (a bound
  by itself still lets an attacker keep the queue pinned at 8 real pending re-evaluations
  indefinitely; coalescing means a spam loop costs the daemon nothing beyond the first request).
  The per-output `Reevaluate` bound still matters independently since `ReevaluateRequest::One`
  entries for *different* outputs don't coalesce with each other, and 8 comfortably covers any
  realistic multi-monitor setup.
- **Alternatives considered**: `ReevaluateRequest::One` deduplication by `OutputId` too — considered
  but not adopted: a physical desktop has a small, bounded number of outputs (rarely more than 4),
  so the 8-entry cap already bounds this case without needing per-output coalescing logic; adding
  it would be complexity without a corresponding attack surface (unlike `All`, which genuinely has
  no bound on how fast one client can call it).

### R11: `dbus-1` session-bus policy file (FR-015)

- **Decision**: Add `packaging/dbus-1/com.system76.CosmicDynamicWallpaper1.conf`, installed to
  `/usr/share/dbus-1/session.d/` by the Debian package, containing a standard
  `<policy user="...">`-scoped set of `<allow>`/`<deny>` rules: allow `own` of the well-known bus
  name only in the general default context (unchanged from today — the session bus already
  enforces this), and explicitly document in the file's own comments that per-uid isolation is the
  primary boundary this project relies on (constitution-documented trust model), with this policy
  file as defense-in-depth make-explicit-what-was-implicit rather than a new restriction.
- **Rationale**: Confirmed no `dbus-1` policy file exists anywhere in `packaging/` today (only
  `systemd/`, `desktop/`, `debian/`). The session bus already scopes all activity to one uid with
  no policy file at all, so the *practical* protection this file adds over doing nothing is small
  on a genuinely single-user machine — but shipping it closes the audit's specific, named gap
  ("no dbus-1 policy file exists anywhere in packaging"), gives future contributors a documented
  place to tighten further (e.g. if a `send_interface` allowlist is ever needed), and is the
  conventional, expected artifact for any project exposing a persistent session-bus service.
- **Alternatives considered**: Polkit-based per-method authorization — rejected as disproportionate
  for a single-user desktop wallpaper daemon with no privilege boundary to cross (polkit exists to
  gate privilege *escalation*; nothing here crosses a privilege boundary, only a same-uid process
  boundary that FR-014's rate limit already bounds the impact of).

### R12: `QueryAll` visibility (FR-016)

- **Decision**: Add a `tracing::debug!` log line in `DaemonInterface::query_all` recording that a
  query occurred (no caller identity is available from `zbus`'s sync `Interface` trait without
  adding `#[zbus(signal_context)]`/connection-credential lookups, which is a larger change than
  this finding's severity warrants) — this project's existing `packaging/systemd/` unit already
  directs `tracing` output to the systemd journal, so this makes the access observable via
  `journalctl` without new infrastructure.
- **Rationale**: The audit's own phrasing — "no way for the user to see... that access" — is
  satisfied by making the access appear in the daemon's existing log stream; the R11 policy file is
  the actual access-*scoping* half of this finding's fix. A full consent-prompt/allowlist UI is a
  materially larger feature (new persisted state, new GUI surface) disproportionate to a warning-
  severity finding on a single-user desktop and is explicitly out of scope for this hardening pass
  (flagged as a candidate follow-up, not silently dropped).
- **Alternatives considered**: `zbus::Connection::peer_credentials` to log the calling PID/uid —
  investigated; `zbus`'s sync interface handler doesn't have ergonomic access to the invoking
  message's connection without restructuring `DaemonInterface` to take `&zbus::message::Header`
  per method (a larger refactor than this finding justifies); left as a documented future
  enhancement rather than adopted now.

### R13: `output_id` validation (FR-017)

- **Decision**: Add `OutputId::validated(id: impl Into<String>) -> Result<Self, String>` to
  `wallpaper-ipc`'s existing `OutputId` (currently a bare infallible `new()` wrapper in
  `renderer_config.rs`) — rejects empty strings and anything over 256 bytes. Used at both untrusted
  boundaries: `dbus_service.rs`'s `reevaluate`/`query_output` handlers, and `wallpaperctl`'s
  `--output <id>` flag (R15, same function, one shared implementation). `OutputId::new` remains for
  trusted internal construction from real Wayland connector names.
- **Rationale**: Confirmed `OutputId` is already shared, `wallpaper-ipc`-crate-owned, and used by
  both `renderer` and `wallpaperctl` — the natural single place to add one validated constructor
  reused at every untrusted boundary rather than duplicating the same length/non-empty check in
  two crates.
- **Alternatives considered**: Validate inline at each call site instead of centralizing on
  `OutputId` — rejected as exactly the "same missing-validation shape, different files" pattern
  the audit's own cross-cutting-patterns section calls out; centralizing avoids repeating the
  mistake a third time.

## User Story 5 — Untrusted-string sinks

### R14: `wallpaperctl list` tab/newline escaping (FR-018)

- **Decision**: Add `fn sanitize_for_tsv(s: &str) -> Cow<'_, str>` in `wallpaperctl::output`,
  replacing `\t`, `\n`, and `\r` with a visible placeholder (`\t`→space, `\n`/`\r`→space,
  collapsing repeats), applied only in `commands/list.rs`'s human-readable closure (the
  `format!("{}\t{}\t{}", ...)` line) — never applied to the `--json` `Serialize` path, which must
  keep carrying the raw value.
- **Rationale**: Confirmed the exact sink: `PackListEntry.name` (sourced from a pack's untrusted
  manifest `name` field) is interpolated directly into a tab-delimited `format!` with no escaping.
  Scoping the fix to the rendering closure (not the `PackListEntry` struct itself) preserves the
  audit's own noted invariant that `--json` output is unaffected and should stay that way (its
  consumers expect the raw value, and JSON's own string escaping already makes tab/newline
  injection into the *document structure* impossible).
- **Alternatives considered**: Reject/truncate names containing control characters at manifest-
  parse time in `pack-loader` — rejected as overreach: a pack name legitimately containing a
  newline isn't itself invalid pack data, it's only dangerous at this one specific unescaped
  rendering sink; fixing the sink is more targeted and doesn't reject otherwise-valid packs.

### R15: `--output` flag validation (FR-019)

- **Decision**: Reuses R13's `OutputId::validated` — `wallpaperctl`'s `assign --output <id>`
  handler calls it and surfaces a `CliError::InvalidOutputId` (new variant, exit code 1, matching
  the existing usage-error class) on failure instead of storing the raw string.
- **Rationale**: Same validated-constructor reuse as R13; confirmed via `main.rs` that `--output`'s
  value currently flows straight into config storage with no check at all (reproduced by the audit
  with both an empty string and a string containing `;`).
- **Alternatives considered**: A `clap` custom `value_parser` directly on the `--output` argument —
  considered, but rejected in favor of routing through `OutputId::validated` so the *same* rule
  governs both this CLI boundary and the D-Bus boundary (R13), rather than two independently-
  maintained validation rules that could drift.

### R16: Absolute-path manifest entry rejection (FR-020, FR-021)

- **Decision**: In `path_safety::resolve_and_check`, add
  `if Path::new(file).is_absolute() { return Err(ManifestError::PathEscapesPackDirectory { file: file.to_string() }) }`
  as the first check, before `pack_dir.join(file)`. Add a new test fixture
  `fixtures/invalid/absolute_path` alongside the existing `path_traversal` fixture, plus a unit
  test in `path_safety.rs`'s own test module.
- **Rationale**: Confirmed `pack_dir.join(file)` silently discards `pack_dir` when `file` is
  absolute (documented standard-library `Path::join` behavior) and containment currently only
  holds because the later `canonical_file.starts_with(&canonical_dir)` check happens to still
  catch it for paths outside the pack dir — but would *not* catch an absolute path that happens to
  point somewhere still nominally "inside" a canonicalized root a future refactor might introduce
  (e.g. if pack directories ever moved under a shared parent). Rejecting explicitly removes the
  reliance on that incidental ordering entirely, matching the audit's own framing.
- **Alternatives considered**: None — this is the audit's own specifically recommended fix with no
  meaningfully different approach available.

## User Story 6 — Surfacing failures instead of swallowing them

### R17: Registry cross-process lock (FR-022)

- **Decision**: Add `fd-lock = "4"` as a direct `pack-loader` dependency (research.md summary
  above). Wrap `Registry::persist()`'s write with an exclusive lock on a dedicated `.lock` file
  next to the registry's `cosmic-config`-managed storage (resolved the same way R25 resolves the
  location-config directory — see R25's documented convention). The lock is held only for the
  duration of the read-modify-write, matching `persist()`'s existing scope.
- **Rationale**: Confirmed both `wallpaperctl register` (fresh process) and `wallpaperd`'s in-
  process `Registry` independently load a snapshot once and unconditionally overwrite the whole
  entries list — a classic lost-update race with no synchronization today. `fd-lock` is a small,
  well-established, pure-safe-Rust crate (no unsafe in its public API) that wraps the platform
  advisory-lock primitive (`flock` on Unix) without this project taking on a raw FFI boundary
  itself.
- **Alternatives considered**: Raw `libc::flock` FFI directly in `pack-loader` — rejected per the
  Technical Context section's reasoning (would be this crate's first `unsafe` block for a solved
  problem); `cosmic-config`'s own `Config::transaction()` (used for R20) — investigated but its
  contract is single-writer-per-`Config`-handle style locking scoped to one transaction call and
  documented for atomic multi-field commits within one process's `Config`, not cross-process mutual
  exclusion across a completely separate `wallpaperctl` invocation and the daemon's long-lived
  handle — insufficient for this specific race.

### R18: Corrupted config visibility (FR-023) — **refined during implementation**

- **Decision**: `load`'s signature stays exactly `fn load(config: &Config) -> Self` — changing it
  to return `(Self, bool)` would have touched ~45 call sites across four crates (mostly test code),
  for no benefit to the ~40 of those that don't care about the distinction at all. Instead: `load`
  is now a thin wrapper around a new `load_reporting_corruption(config: &Config) -> (Self, bool)`,
  which every existing caller of `load` is unaffected by (`load` just discards the `bool` half
  internally), while the one caller that *does* need the distinction (`wallpaperctl location get`,
  T026) calls `load_reporting_corruption` directly. `load_reporting_corruption` also logs via
  `tracing::warn!` whenever it reports `true`, so every existing caller gets visibility into
  genuine corruption for free (in the daemon's/CLI's log output) even without touching their call
  site at all.
- **Correction found while implementing**: the original plan (both here and data-model.md) said
  "logs the discarded `_errors`" as if any non-empty `errors` from `get_entry`'s `Err` variant
  meant genuine corruption. Reading `cosmic_config`'s derive macro output directly showed this is
  wrong: `errors` also fills up for the completely ordinary "this key was simply never written"
  case (`cosmic_config::Error::NotFound`), which ordinarily happens on every single first run.
  Treating *any* non-empty `errors` as "corrupted" would have falsely flagged that ordinary case.
  `cosmic_config::Error` itself already ships the right predicate for this —
  `Error::is_err(&self) -> bool`, whose own doc comment is literally "whether the reason for the
  missing config is caused by an error... useful for determining if it is appropriate to log as an
  error," excluding exactly `NotFound`/`NoConfigDirectory`. `corrupted = errors.iter().any(Error::
  is_err)` is the correct check, verified against the library's own source rather than assumed.
- **Rationale**: Confirmed `get_entry` already *produces* the errors in its `Err` variant — this
  fix only stops discarding information the API already provides, the smallest possible change
  that closes the "corrupted file is indistinguishable from never-set" gap the audit reproduced,
  now with zero call-site ripple and the correct never-configured/genuinely-corrupted distinction.
  `wallpaperctl location get` keeps exit code 0 in both cases (neither is a fatal error per
  constitution Principle VIII).
- **Alternatives considered**: Fail loudly (non-zero exit / hard error) on a corrupted config file
  — rejected as inconsistent with constitution Principle VIII ("a corrupt config entry MUST
  degrade only that one... set and MUST NOT crash or hang") and with this crate's own existing
  fallback-to-default posture for every other config read; the fix here is about *visibility*, not
  changing the degrade-to-default behavior itself.

### R19: Pack-builder registry-failure surfacing (FR-024)

- **Decision**: Change `register_and_close` to route `PackSource::resolve` and `registry.register`
  failures into `state.move_error` (a field that already exists on `pack_builder::State` for
  exactly this class of user-visible error, per `move_error: Option<String>`'s existing doc
  comment "FR-017's error surface for a failed move") instead of `if let Ok(...)`/`let _ =`
  discarding them, and only clear `pending_placement`/`pending_collision` (closing the wizard) on
  the success path.
- **Rationale**: Confirmed the exact swallow via `if let Ok(source) = PackSource::resolve(path) {
  let _ = registry.register(source); }` with unconditional state-clearing after. The fix reuses an
  error-surface field the wizard already has and already renders (no new UI plumbing needed) — the
  bug was purely that this specific failure path didn't route into it.
- **Alternatives considered**: A new dedicated `registration_error` field distinct from
  `move_error` — rejected as unnecessary; `move_error`'s existing semantics ("something went wrong
  after Generate succeeded, during placement") already cover registration failure as a sub-case,
  and the wizard's view layer already has a render path for that field.

### R20: Multi-field config write atomicity (FR-025) — **no code change; verified already the case, corrected during implementation**

- **Original decision (incorrect as stated, corrected below)**: this entry originally proposed
  replacing "`commands/location.rs`'s sequential per-field `Config::set`/`write_entry` calls" with
  `cosmic_config::Config::transaction()`, and claimed the `//TODO: apply all changes at once`
  marker sat "at exactly this call site" in this project's own code. Both claims were wrong,
  caught while actually opening `commands/location.rs` to make the change:
  1. `commands/location.rs`'s `set`/`clear`/`auto`/`ip` each already mutate a `LocationConfigEntry`
     in memory and call `state.save(config)` **exactly once** — there is no sequential per-field
     write pattern in this project's code to begin with.
  2. `LocationConfigEntry::save` is `write_entry`, generated by the `#[derive(CosmicConfigEntry)]`
     macro — and that generated code **already** builds one `config.transaction()` and writes
     every field of the struct through it before a single `tx.commit()`. `location set` was
     already going through `Config::transaction()`, for every field, before this feature existed.
  3. The `//TODO: apply all changes at once` comment is real, but it lives inside
     `cosmic-config`'s own `ConfigTransaction::commit()` in the **upstream `pop-os/libcosmic`
     dependency**, not in this project's `commands/location.rs`. Reading `commit()`'s actual body
     confirms why the TODO exists: even inside one transaction, it writes each field as its own
     separate `atomicwrites::AtomicFile` operation, in a sequential loop — the *batching* (no
     application logic between writes) is real, but true single-operation atomicity across the
     whole struct is not, and that gap is upstream's to close, not reachable from this project's
     own source.
- **Actual decision**: no code change in this project. The finding's underlying concern (a crash
  mid-write could leave fields out of sync) has a real, narrow residual window inside
  `cosmic-config`'s `commit()` loop, upstream of anything this codebase controls; `location set`
  already gets the strongest atomicity this dependency's public API offers (one transaction, no
  interleaved application code, all field writes back-to-back). Documented here, in
  `contracts/wallpaperctl-cli-hardening.md`, and in tasks.md's T028 rather than silently marking a
  no-op task complete with no explanation — the same posture as T002/T003 (setup) and R34 (US7)
  once each turned out not to need the originally-planned change.
- **Alternatives considered**: A hand-rolled write-ahead journal/marker file in `wallpaper-ipc`
  that a subsequent `load` could detect and treat as "known-interrupted, discard rather than trust
  partial state" — genuinely closes the gap, but is a materially larger, novel mechanism for a
  P3-priority, narrow-window, upstream-dependency-shaped issue; judged disproportionate rather than
  built speculatively. Forking/patching `cosmic-config` itself — out of scope (external repository,
  no mechanism in this project to vendor a patched fork without a much larger workflow change).

### R21: Generate-handler re-check (FR-026)

- **Decision**: At the top of the `Message::GenerateRequested` handler (`app.rs`/`pack_builder.rs`'s
  update logic), call the existing `all_assigned(&state)` pure function (already used to gate the
  UI button's `enabled` state per the audit's own citation of `app.rs:356-360`) and return early
  with a `state.generate_error` message (the field already exists, per `pack_builder.rs`'s own doc
  comment "FR-017's error surface for a failed Generate") instead of proceeding to `build_draft` if
  it returns `false`.
- **Rationale**: `all_assigned` already exists as a pure, already-tested function reused by the
  button-enable check — this fix is purely about calling it a second time, in the handler, rather
  than trusting the view layer's gate was the only path to `GenerateRequested`. Zero new logic,
  zero new state.
- **Alternatives considered**: Make `build_draft` itself return `Err` for unassigned rows instead of
  silently filtering them (its current behavior) — adopted *in addition* to the handler re-check,
  since both are cheap and each closes a slightly different gap (the re-check stops the call before
  it starts; `build_draft` erroring instead of filtering stops a *different* future call site from
  producing a silently-incomplete pack even if it forgets the re-check too).

### R22: Defer manifest write until Move/Keep is chosen (FR-027) — **implementation more involved than originally sketched**

- **Decision**: `generate()` no longer writes `manifest.toml` into `state.source_dir` at all. It
  still self-validates (FR-012's requirement, preserved) — but against a throwaway scratch
  directory instead: every referenced image is copied (see correction below) into a fresh temp
  directory alongside the rendered manifest text, run through the real `pack_loader::load_pack`,
  then the scratch directory is deleted regardless of outcome. On success, the rendered text is
  held in a new `state.pending_manifest_text: Option<String>` field (not written anywhere real
  yet) alongside the existing `pending_placement`. A new shared `write_manifest_and_register`
  helper — used by both `confirm_keep` and `cancel_collision_to_keep` (the two "stay in place"
  paths) — writes `pending_manifest_text` into the source folder, self-validates again in place,
  then registers; `move_pack` (the "relocate" path) gained a `manifest_text: &str` parameter and
  now writes it into the *destination* right after copying the source's images there, before its
  own existing self-validation call. Either way, nothing is ever written to `state.source_dir`
  until the user's actual Move/Keep choice runs.
- **Correction found while implementing**: the first version of the scratch-directory
  self-validation used symlinks (cheap, no copying) pointing back into `state.source_dir`. Every
  test failed with `PathEscapesPackDirectory` — `pack_loader::path_safety::resolve_and_check`
  canonicalizes every entry specifically to catch a symlink whose *real* target resolves outside
  the pack directory (its own doc comment states this is deliberate, to catch exactly this shape).
  A symlink from the scratch dir back to the real source directory is precisely that shape, so
  pack-loader's own containment check — working exactly as intended — rejected it every time.
  Switched to real file copies for the scratch validation; bounded and one-time per Generate click
  (at most `MAX_ANCHORS` = 64 images), not a hot path.
- **Rationale**: Confirmed via `pack_builder.rs`: the manifest was previously written during
  `GenerateRequested` handling, before `pending_placement`'s dialog was even shown — so a crash
  between those two points left a written-but-unconfirmed manifest that `should_open_for`'s
  `ManifestNotFound`-only check treated as "already has a manifest" on next launch, silently
  skipping the wizard. Moving the write to the point of actual user choice removes the window
  entirely rather than trying to make it recoverable — verified directly with a test that drops
  `state` after `generate()` (simulating a crash) and confirms `should_open_for` still correctly
  re-opens the wizard, not silently treating the folder as already placed.
- **Alternatives considered**: Write the manifest early (as today) but into a temp/staging location
  and only move it into the source folder once a choice is made — rejected in favor of not writing
  anywhere on disk at all until the choice, which closes the window completely rather than moving
  it. Skipping the scratch-dir self-validation entirely (defer *all* validation to Move/Keep time)
  — rejected: would regress FR-012's requirement that Generate itself reports a self-validation
  failure immediately, not only once the user clicks Move or Keep.

## User Story 7 — Diagnostics and defensive hardening

### R23: `DaemonUnreachable` exit-code renumbering (FR-028)

- **Decision**: Change `CliError::DaemonUnreachable`'s exit code from `2` to `4` (the next unused
  code in `error.rs`'s existing scheme: `1` = usage-shaped errors, `2` = currently daemon-
  unreachable/soon-to-be freed, `3` = pack-load/config errors). Update
  `contracts/wallpaperctl-cli-hardening.md` (this feature) and cross-reference the original
  `specs/004-cli-control-surface/contracts/wallpaperd-dbus-interface.md` exit-code table for
  consistency.
- **Rationale**: Confirmed `clap`'s own usage-error exit code is `2` (its documented default,
  reproduced by the audit via a plain typo'd argument) and `CliError::DaemonUnreachable => 2`
  collides with it exactly. Moving to an unused code (`4`) is the minimal change; renumbering the
  *other* variants was considered and rejected to avoid an unrelated breaking change to
  already-shipped, already-documented exit codes for `PackNotFound`/`OutputNotFound`/
  `InvalidLocation` (all `1`) and `PackLoadFailed`/`ConfigError` (both `3`).
- **Alternatives considered**: Renumber so usage errors and `DaemonUnreachable` share no code by
  moving `DaemonUnreachable` to `1` and shifting others — rejected as a larger-than-necessary
  breaking change to already-documented exit codes for no additional benefit over simply freeing
  `2`.

### R24: Flag-conflict via `CliError` (FR-029)

- **Decision**: Add `CliError::UsageError { message: String }` (exit code `2`, now unambiguous per
  R23) and change `main.rs`'s `--output`/`--same-everywhere` conflict branch to
  `return Err(CliError::UsageError { message: "specify exactly one of --output <id> or --same-everywhere".to_string() })`
  instead of `eprintln!` + `std::process::exit(1)`.
- **Rationale**: Confirmed the exact bypass in `main.rs`'s `Command::Assign` match arm. Routing
  through `CliError` makes this path testable (the audit notes it's currently untested precisely
  *because* `process::exit` would abort a multi-threaded test binary) and composable with `run()`'s
  existing `Result` return, and its new exit code (`2`, freed by R23) now correctly identifies it
  as the same *class* of error `clap` itself reports with that code.
- **Alternatives considered**: Express the constraint via `clap`'s `ArgGroup`
  (`#[group(required = true, multiple = false)]`) so `clap` itself rejects the invalid combination
  before `run()` is ever called — investigated as the more idiomatic `clap` solution; deferred to
  an implementation-time judgment call in tasks.md (both are correct; `ArgGroup` is preferable if
  it doesn't fight the existing `Cli` struct's derive layout, otherwise the `CliError::UsageError`
  path above is the fallback).

### R25: Location config file permissions (FR-030)

- **Decision**: After every successful `LocationConfigEntry::save`/`RendererConfig::save`, resolve
  the on-disk directory via the same construction `cosmic_config::Config::new_inner` uses
  internally (confirmed by reading the pinned `cosmic-config` source: `dirs::config_dir()/"cosmic"/
  {sanitize_name(app_id)}/v{version}/`) and call `std::fs::set_permissions` with mode `0o700` on
  that directory (covering every key file inside it, present or future, rather than guessing
  individual file names) — Unix-only (`#[cfg(unix)]`), a no-op on other platforms.
- **Rationale**: `cosmic_config::Config` does not expose its resolved path publicly (confirmed:
  `user_path`/`system_path` are private fields, `key_path` is a private method) — this is the one
  fix in this feature that must reconstruct an external crate's internal-but-stable directory
  convention rather than calling a public accessor. Tightening the whole per-app config directory
  (not just the `location` file the audit specifically named) also closes the same gap for
  `renderer_config.rs`'s directory, which the audit didn't call out by name but shares the exact
  same root cause (default `cosmic_config` permissions).
- **Alternatives considered**: Chmod only the specific `location` file, guessed by name — rejected
  as narrower than necessary and more fragile (depends on knowing the exact key filename
  `cosmic-config` uses, whereas chmod'ing the directory covers every key without needing to name
  them); upstreaming a `Config::path()` accessor to `libcosmic` — noted as the ideal long-term fix
  in a code comment at the reconstruction site, but out of scope for this feature (an external
  dependency's API surface, not this project's own code).

### R26: STUN reply sanity-bounding (FR-031)

- **Decision**: After IP-geolocation resolves a `Location`, compare it against the most recent
  previously-*trusted* location for that resolution mode (persisted alongside the existing
  location-config entry) and reject the new reading if it implies an implausible jump (e.g. more
  than ~2000 km between consecutive successful resolutions with no manual location change in
  between) rather than applying it outright; log the rejection.
- **Rationale**: The audit's own suggested mitigations include "sanity bounds on the resolved
  location" as an explicit alternative to DNS pinning/DNSSEC — chosen over those because this
  project already stores the previous location (no new persisted state needed beyond what
  `LocationConfigEntry` already tracks) and a plausibility bound is effective against exactly the
  threat model described (a single forged UDP reply causing one bad jump) without taking on
  DNS-transport-security work orthogonal to this project's actual scope.
- **Alternatives considered**: DNS pinning/DNSSEC validation for the STUN server hostname —
  rejected as materially larger scope (this project has no existing DNS-security-aware resolution
  path anywhere) for a threat (on-path DNS spoofing) that a plausibility bound on the *result*
  already mitigates without touching DNS resolution at all.

### R27: Portal-location debounce reuse (FR-032)

- **Decision**: Confirmed `cosmic-wallpaperd.rs` already implements a 2s coalescer for other
  config-driven re-evaluations elsewhere in the same file; wrap the `PortalEvent::Reading` handler
  insertion in the same debounce primitive (a `calloop` timer source that resets on each new event
  and only applies the *latest* reading once the window elapses) rather than applying every event
  synchronously as it arrives today.
- **Rationale**: Reuses an existing, already-correct pattern in the same file rather than
  introducing a second debouncing mechanism — the audit's own framing ("unlike every other config
  write") makes clear the fix is consistency with what's already proven to work elsewhere in this
  exact daemon.
- **Alternatives considered**: A longer or shorter debounce window than the existing 2s — rejected;
  matching the existing constant keeps behavior uniform across every debounced write in the daemon
  rather than introducing a second, differently-tuned window a future maintainer has to reconcile.

### R28: GPU adapter/device request timeout (FR-033)

- **Decision**: Wrap the `request_adapter`/`request_device` calls in `gpu.rs` with
  `futures_lite::future::or(actual_request, timeout_after(Duration::from_secs(20)))` (both
  `futures-lite` and `async-io` are already direct `renderer` dependencies per its `Cargo.toml`),
  returning a new `RendererError::GpuRequestTimedOut` on the timeout branch, mirroring the 20s
  bound the crate's own test suite already applies to itself.
- **Rationale**: The audit explicitly notes the test suite already has this exact protection and
  "acknowledges the risk in its module doc" — this is a case of production code needing to catch up
  to a pattern already proven correct in its own tests, using dependencies already present, not a
  new technique.
- **Alternatives considered**: A configurable timeout (env var / config value) — rejected as
  unnecessary complexity; 20s (matching the test suite's own existing constant) is a reasonable
  fixed bound for a one-time-per-output startup/hotplug operation, not a hot path needing tuning.

### R29: Surface-loss active recovery (FR-034)

- **Decision**: On `SurfaceError::Lost`/`Outdated` in the draw path, call `reconfigure_output` again
  using the output's last-known `size` (already stored on `self.outputs[index].size`) instead of
  only logging and returning.
- **Rationale**: `reconfigure_output` is already idempotent (its own doc comment states this,
  confirmed by reading `surface.rs`) and already contains R2's zero-size guard — calling it again
  with the last-known size is a direct, minimal recovery path using a function that already exists
  and is already safe to call redundantly.
- **Alternatives considered**: Wait for the compositor's next configure event as today — this *is*
  the bug (a surface loss with no accompanying event never recovers); actively re-configuring is
  the only fix that doesn't depend on an event that might not come.

### R30: Unsafe raw-window-handle safety comment (FR-035)

- **Decision**: Add a `// SAFETY:` comment directly above the `unsafe` block in `surface.rs`
  explaining the actual invariant this code relies on: the Wayland `wl_surface`/`wl_display`
  handles borrowed here are owned by `self.outputs[index]`/the daemon's `Connection` respectively,
  both of which outlive the `wgpu::Surface` constructed from them for the surface's entire
  lifetime (the daemon never drops an output's Wayland objects while a `wgpu::Surface` referencing
  them is still alive) — matching constitution's "no unsafe code... without a comment justifying
  each use."
- **Rationale**: Documentation-only change; no behavior change. The audit's finding is specifically
  that the invariant is unstated, not that it's wrong — confirmed by reading the surrounding
  ownership structure that the invariant does in fact hold today.
- **Alternatives considered**: None — this is a pure documentation fix with no implementation
  alternative.

### R31: GPU texture eviction (FR-036)

- **Decision**: Change the per-output texture cache (currently an unbounded map keyed by
  `ImageId`) to evict least-recently-used entries once the cache exceeds a bound of 16 resident
  textures per output (comfortably above the largest realistic simultaneously-visible set — at
  most 2 during any single crossfade, per `ensure_texture`'s own call sites — while still allowing
  headroom for a pack with many distinct anchors that get visited across a day without needing to
  re-decode every time the same image recurs).
- **Rationale**: Confirmed the cache today is only cleared on a full pack reload. An LRU bound
  reclaims memory from packs with many images over the daemon's long lifetime while still avoiding
  needless re-decode churn for the handful of images actually in rotation on any given day.
- **Alternatives considered**: No caching beyond the current transition's two images (evict
  immediately after each transition completes) — rejected as a regression for packs that revisit
  the same few images across a day (e.g. a 4-anchor pack cycling sunrise/noon/sunset/night daily
  would re-decode from disk on every single transition instead of only once).

### R32: `Arc<Mutex<DbusState>>` invariant enforcement (FR-037)

- **Decision**: Add a `debug_assert!` at the top of every `DaemonInterface` method (or, more
  cheaply, once in the daemon's main-loop setup, capturing `std::thread::current().id()` and
  asserting every subsequent D-Bus-driven call happens on that same thread id) plus a doc comment
  cross-referencing the module-level comment that already states the "never contended" assumption.
- **Rationale**: Rust cannot statically prove single-threaded execution here without removing
  `Send`/`Sync` from the type (which `zbus`'s `Interface` trait requires it keep) — a `debug_assert`
  is the pragmatic middle ground: free in release builds, catches a regression immediately in any
  debug build or test run (this workspace's own `cargo test` runs in debug mode by default) rather
  than only manifesting as a rare, hard-to-reproduce production race.
- **Alternatives considered**: Switch to `Rc<RefCell<_>>` with a custom `unsafe impl Send`/`Sync`
  wrapper documented as sound because of the single-thread invariant — rejected as a larger, riskier
  change (introduces a new `unsafe impl`, which is a stronger claim than a `debug_assert` and would
  need its own safety-invariant documentation) for a problem the assert already catches in practice.

### R33: Pole-location fast path (FR-038, FR-039)

- **Decision**: In `location.rs`/`query.rs`, add an early check: if `location.latitude().abs() >=
  89.9999` (effectively the pole, accounting for floating-point representation), return `None`
  immediately from the solar-event search entry point rather than running the radius-doubling
  search to its full extent. Separately, correct `MAX_SEARCH_RADIUS_DAYS`'s doc comment/value to
  reflect the true worst case (up to 512 days) given the existing check-then-double ordering,
  rather than changing that ordering itself.
- **Rationale**: Confirmed via `location.rs`/`query.rs`: no solar event ever resolves at exactly
  lat = ±90°, so the search runs to its full, expensive extent every single call for a location
  that can *never* succeed — an early, exact recognition of that case turns a ~29ms/query cost into
  a sub-microsecond early return, which is a stronger fix than merely making the existing search
  faster. The `MAX_SEARCH_RADIUS_DAYS` fix is corrected independently since it's a documentation-
  accuracy issue (the constant's *name* undersells the real worst case), not a behavior bug — changing
  the check-then-double ordering itself was considered and rejected as unnecessary scope once the
  pole fast-path removes the case that was hitting the true worst case in practice.
- **Alternatives considered**: Cap the search radius itself lower (e.g. enforce the *documented*
  370-day figure as a hard stop) — rejected because that would change behavior for genuinely
  slow-but-valid high-latitude (not exactly polar) locations that legitimately need a wide search,
  which is a correctness regression the audit doesn't ask for; the pole-specific fast path targets
  only the case that can never succeed at all.

### R34: Duplicate-instant check reachability (FR-040) — **corrected during implementation**

- **Original decision (wrong, corrected below)**: this entry originally proposed folding
  `check_solar_duplicate_instant` into `WallpaperPack::validate`. That is not actually possible:
  `validate(images: Vec<PackImage>)` takes no `Location`/date argument at all, and
  `check_solar_duplicate_instant`'s own doc comment in `pack.rs` explains why — solar event
  instants shift day to day, so a collision on one date doesn't imply one on another, which is
  exactly why the crate's own author made this a separate, date-scoped method in the first place.
  This was missed during planning by not reading `pack.rs`'s full doc comments closely enough
  before writing the research decision; caught and fixed while implementing T043, not left
  standing.
- **Actual decision**: confirmed via `grep` that the *only* real call site of
  `check_solar_duplicate_instant` anywhere in the workspace was
  `wallpaper-settings::pack_builder.rs` (the custom-pack-builder wizard, checking only at build
  time) — `pack-loader`'s/`renderer`'s actual runtime pack-loading path
  (`renderer::surface::WallpaperDaemon::load_pack_for`, the function every registered pack goes
  through on assignment, reload, and daemon restart) never called it at all. Added a call there
  instead: after a pack loads successfully, if a location is currently available, call
  `loaded.pack.check_solar_duplicate_instant(location, Local::now().date_naive())` and log a
  `tracing::warn!` on a collision — never block the load (constitution Principle VIII: a collision
  degrades to "one anchor's transition is skipped today," not a load failure). This naturally
  re-runs on every reassignment/reload/restart and revisits the check for whatever "today" is at
  that moment, which is the closest a load-time check can get to this method's inherently
  date-scoped nature without turning `query()` itself into a fallible per-call API (see original
  Alternatives below, still valid).
- **Alternatives considered**: Have `query()` call it internally — rejected (unchanged from the
  original research): `query()` is documented as intentionally infallible
  (`ScheduleQueryResult`, not `Result`) and runs on every schedule evaluation; making it fallible
  would ripple to every caller (`scheduler_bridge::evaluate` and beyond) for a check that's
  fundamentally a load-time/day-boundary concern, not a per-query one. Threading a `Location`
  parameter through `pack_loader::load_pack` itself — rejected: `pack-loader` has no location
  concept at all today (by design, per its own crate scope) and location is a `renderer`-owned
  runtime concern; the fix belongs where both a freshly-loaded pack and the current location are
  already both in scope, which is `load_pack_for`.

## User Story 8 — Code health and documentation (lighter treatment; each is a self-contained, low-risk change with no design decision beyond "do what the finding says")

- **R35 (FR-041)**: `generate-starter-pack`'s `main.rs` already has `pack-loader`'s `manifest::render`
  available as a workspace-internal dependency path (`pack-loader` is already a path dependency
  pattern used elsewhere) — route its manifest generation through it instead of hand-built string
  interpolation. Confirmed `pack-loader::manifest::render` exists (added by spec 010) and is exactly
  the symmetric writer this tool should have been using.
- **R36 (FR-042)**: Guard `caps.formats[0]`/`caps.alpha_modes[0]` in `surface.rs` with
  `.first().ok_or(...)` and degrade that one output on `None`, matching every other per-output
  failure path in this file.
- **R37–R46 (FR-043–FR-052)**: Each is a documentation, naming, or refactor-only change (module-
  level index comment, error-message preservation, constant deduplication, helper extraction,
  field rename, README updates, in-flight-guard flag) with no architecturally-relevant decision —
  deferred to tasks.md for direct, self-explanatory implementation without a dedicated research
  entry each.
