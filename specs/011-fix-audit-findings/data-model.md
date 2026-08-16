# Data Model: Fix Adversarial Audit Findings

This feature adds no new persisted entity and no `cosmic-config` schema changes (plan.md
Constitution Check, Principle X: n/a). "Data model" here means the new/changed in-memory types,
constants, and validation rules each fix introduces, grouped by crate. Every item below traces to
a research.md decision and a spec.md FR.

## `schedule-engine`

### New constants

| Constant | Value | Location | FR |
|---|---|---|---|
| `MAX_SOLAR_OFFSET_HOURS` | `24` | `pack.rs` (co-located with `MAX_ANCHORS`) | FR-004 |
| `POLE_LATITUDE_THRESHOLD` | `89.9999` (degrees) | `location.rs` | FR-038 |

### Changed types

- **`PackError`** (`pack.rs`) gains one variant:
  `SolarOffsetOutOfRange { event: SolarEventKind, offset: TimeDelta }` — returned by
  `WallpaperPack::validate` when a solar anchor's offset magnitude exceeds
  `MAX_SOLAR_OFFSET_HOURS`. Validation rule: `offset.num_hours().abs() <=
  MAX_SOLAR_OFFSET_HOURS` for every `TimeAnchor::Solar { offset: Some(_), .. }` in the pack
  (research.md R4).
- **`WallpaperPack::validate`** (`pack.rs`) gains one check (research.md R4): the offset-bound
  check above. (The duplicate-solar-instant check does **not** move into `validate` — see
  `renderer` below and research.md R34's corrected entry for why that turned out not to be
  possible.)
- **`MAX_SEARCH_RADIUS_DAYS`** (`query.rs`): value/doc comment corrected to state the true
  worst-case search radius (up to 512 days) reachable under the existing check-then-double
  loop ordering, rather than the previously-understated 370 (research.md R33). No behavior
  change — documentation-accuracy fix only.

## `pack-loader`

### New constants

| Constant | Value | Location | FR |
|---|---|---|---|
| `MAX_MANIFEST_BYTES` | `512 * 1024` | `load.rs` | FR-011 |

### Changed types

- **`ManifestError`** (`error.rs`) gains two variants:
  - `ManifestTooLarge { path: PathBuf, size: u64 }` — `load_directory_pack` rejects a manifest
    file whose `std::fs::metadata(...).len()` exceeds `MAX_MANIFEST_BYTES`, checked before
    `read_to_string` (research.md R8).
  - (No new variant needed for the absolute-path rejection — reuses the existing
    `PathEscapesPackDirectory { file: String }` variant, now also returned when
    `Path::new(file).is_absolute()`, research.md R16.)
- **`Color::parse`** (`manifest.rs`): validation rule added — `hex.is_ascii()` must hold before
  any byte-offset slicing; non-ASCII input now returns the existing `InvalidColor` variant
  instead of panicking (research.md R1). No new type.
- **`load_directory_pack`** (`load.rs`): validation-ordering rule — `parsed.images.len() >
  schedule_engine::MAX_ANCHORS` is now checked immediately after `manifest::parse` succeeds,
  before the per-image resolve/containment/readability loop runs (research.md R7). No new
  type; existing `ManifestError` variant for the anchor-count cap (already returned by
  `WallpaperPack::validate`) is now returned earlier, before per-image I/O.
- **`Registry`** (`registry.rs`) gains a private field holding the acquired `fd_lock::RwLock`
  guard's lock-file path (or equivalent — implementation detail, not part of the public API);
  `persist()`'s existing signature/behavior is unchanged from the caller's perspective, it now
  additionally acquires an exclusive cross-process file lock for the duration of the write
  (research.md R17). New `RegistryError` variant: `LockFailed { message: String }`.

## `renderer`

### New constants

| Constant | Value | Location | FR |
|---|---|---|---|
| `MAX_DECODED_IMAGE_BYTES` | `256 * 1024 * 1024` | `texture.rs` | FR-012 |
| `MAX_PENDING_DBUS_REQUESTS` | `8` | `dbus_service.rs` | FR-014 |
| `MAX_OUTPUT_ID_BYTES` | `256` | `dbus_service.rs` (reuses `wallpaper_ipc::OutputId::validated`, see below) | FR-017 |
| `GPU_REQUEST_TIMEOUT` | `Duration::from_secs(20)` | `gpu.rs` | FR-033 |
| `MAX_CACHED_TEXTURES_PER_OUTPUT` | `16` | `surface.rs` | FR-036 |
| `PORTAL_LOCATION_DEBOUNCE` | reuse existing 2s constant | `cosmic-wallpaperd.rs` | FR-032 |
| `MAX_PLAUSIBLE_LOCATION_JUMP_KM` | `2000` | `ip_geolocation.rs` | FR-031 |

### Changed types

- **`WallpaperDaemon::load_pack_for`** (`surface.rs`): gains a call to
  `loaded.pack.check_solar_duplicate_instant(location, Local::now().date_naive())` immediately
  after a successful load, when `self.location` is `Some` — logs `tracing::warn!` on a collision,
  never blocks the load (FR-040, research.md R34's corrected entry — this is the actual runtime
  call site the audit found missing, not a change to `WallpaperPack::validate` in
  `schedule-engine`, which is structurally unable to perform a date-scoped check).
- **`ReevaluateRequest`** (`dbus_service.rs`): no new variant, but `DbusState::pending`'s
  invariant changes — at most one `ReevaluateRequest::All` may be present at a time
  (subsequent `ReevaluateAll` calls are no-ops while one is already queued), and
  `pending.len()` never exceeds `MAX_PENDING_DBUS_REQUESTS` (research.md R10). New
  `RendererError`/`zbus::fdo::Error` surfacing: `reevaluate` returns
  `zbus::fdo::Error::LimitsExceeded` once the queue is full; `reevaluate_all` logs and drops
  silently (its D-Bus signature returns `()`, unchanged).
- **`RendererError`** (`error.rs`) gains:
  - `TextureTooLarge { path: PathBuf, width: u32, height: u32 }` (FR-012).
  - `GpuRequestTimedOut` (FR-033).
- **Per-output texture cache** (`surface.rs`, `Output.textures: HashMap<ImageId, GpuTexture>`
  today) becomes bounded with LRU eviction, tracked by a new small
  `struct TextureCache { map: HashMap<ImageId, GpuTexture>, order: VecDeque<ImageId> }`
  wrapper replacing the bare `HashMap` (research.md R31). Public behavior (`ensure_texture`'s
  signature) is unchanged; only the internal storage shape changes.
- **`GpuTexture::load`** (`texture.rs`): validation rule added — decoded dimensions must not
  exceed `device.limits().max_texture_dimension_2d` on either axis, and
  `width as u64 * height as u64 * 4` must not exceed `MAX_DECODED_IMAGE_BYTES`, both checked
  from the image's header (via `image::ImageReader::into_dimensions`) before the full
  `to_rgba8()` decode runs (research.md R9).

## `wallpaper-ipc`

### Changed types

- **`OutputId`** (`renderer_config.rs`) gains a new fallible constructor:
  `OutputId::validated(id: impl Into<String>) -> Result<Self, OutputIdError>` — rejects an
  empty string or a string longer than `MAX_OUTPUT_ID_BYTES` (256). The existing
  `OutputId::new` remains, for trusted internal construction from real Wayland connector names
  (research.md R13). New small error type `OutputIdError { reason: String }`, `Display`-only
  (mirrors this crate's existing error-type conventions).
- **`LocationConfigEntry::load`** and **`RendererConfig::load`** (`location_config.rs`,
  `renderer_config.rs`): return type changes from `Self` to a small
  `LoadOutcome<T> { value: T, corrupted: bool }` (or equivalent — a tuple `(Self, bool)` is an
  acceptable simpler alternative at implementation time) so callers can distinguish "read
  successfully" from "fell back to defaults after a read/parse error," which is now also
  logged via `tracing::warn!` with the discarded error detail (research.md R18). Every existing
  caller that only needs `.value`/`.0` is updated to destructure accordingly.
- **`LocationConfigEntry::save`** and **`RendererConfig::save`**: unchanged signature; now also
  tightens the owning config directory's Unix permissions to `0700` after a successful write
  (research.md R25). Unix-only; no-op on other platforms.

## `wallpaperctl`

### Changed types

- **`CliError`** (`error.rs`):
  - `DaemonUnreachable`'s `exit_code()` mapping changes from `2` to `4` (research.md R23).
  - New variant `UsageError { message: String }`, exit code `2` (research.md R24) — used by the
    `--output`/`--same-everywhere` conflict check instead of a direct `process::exit(1)`.
  - New variant `InvalidOutputId { reason: String }`, exit code `1` (matches the existing
    usage-error-shaped class alongside `PackNotFound`/`OutputNotFound`) — returned when
    `OutputId::validated` rejects the `--output` flag's value (research.md R15).
- **`commands/list.rs`**: `packs()`'s human-readable rendering closure now passes `e.name`
  through a new `output::sanitize_for_tsv(&str) -> Cow<str>` helper before formatting; the
  `--json` path and the `PackListEntry` struct itself are unchanged (research.md R14).

## `wallpaper-settings`

### Changed types

- **`pack_builder::State`**: no new fields — `move_error`, `generate_error`, and
  `pending_collision`/`pending_placement` (all already present) are reused, now populated by
  code paths that previously discarded the same errors (research.md R19, R21).
- **New pure function** `validate_destination_name(name: &str) -> Result<(), String>`
  (`pack_builder.rs`) — rejects an empty string, any non-`Normal` `Path::Component` (rejects
  `..`, `.`, root/prefix components), and an absolute path (research.md R5). Called from the
  `CollisionNameChanged`/`CollisionConfirmed` handler before `move_pack` runs.
- **`Message::GenerateRequested` handling**: gains a re-check of the existing `all_assigned`
  pure function before calling `build_draft`, populating `state.generate_error` on failure
  instead of proceeding (research.md R21).
- **Manifest-write ordering**: the point at which `manifest.toml` is written moves from
  `GenerateRequested` handling to a new shared `finalize(state, choice)` step invoked by both
  `MoveRequested` and `KeepRequested` (research.md R22) — no new state field, just a
  reordering of when the existing write call happens.

## `packaging/`

### New file

- **`packaging/dbus-1/com.system76.CosmicDynamicWallpaper1.conf`**: standard `dbus-1` policy
  XML (`<busconfig><policy>...</policy></busconfig>`), installed to
  `/usr/share/dbus-1/session.d/` by the Debian package (research.md R11). Not a Rust type —
  listed here for completeness of "what this feature adds."

## Traceability

Every constant, type, and validation rule above is referenced by exactly one research.md
decision (R1–R36) and traces back to one or more spec.md functional requirements — see
research.md for the full Decision/Rationale/Alternatives reasoning behind each.
