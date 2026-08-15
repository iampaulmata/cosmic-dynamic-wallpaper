# Research: Custom Pack Builder

Ten decisions, each closing a concrete unknown from plan.md's Technical Context. Every one
reuses a mechanism already present in this workspace or a dependency already resolved in
`Cargo.lock` — no new Cargo dependency is added anywhere in this spec except `dirs` becoming a
*direct* dependency of `wallpaper-settings` (it is already pulled in transitively by
`cosmic-config`, so this adds zero new crates to the build).

## R1: Wizard entry point — branch off the existing "Add pack folder…" flow

**Decision**: No new top-level nav entry. The wizard is triggered from `pages::packs`'s existing
"Add pack folder…" button (`crates/wallpaper-settings/src/app.rs`, `AddFolderRequested` →
`.open_folder()`). Today, `apply_add_result` hands the picked path straight to
`PackSource::resolve` + `Registry::register`, which only succeeds if `pack_loader::load_pack`
can already load a manifest. When the picked directory instead fails with the specific
`ManifestError::ManifestNotFound` (not just "add_error" text — matched on the enum variant), the
app opens the new pack-builder flow at that path instead of showing a plain error (FR-001,
FR-002). A directory that *does* have a `manifest.toml` keeps behaving exactly as it does today
— registered immediately, wizard never shown (FR-002, Edge Case 1).

**Rationale**: Reuses the exact folder-picker `cosmic::Task` code, the exact "cancel is a no-op"
handling, and the exact success path (`Registry::register`) already proven for the Packs page
(spec 008 research.md R1) — the only new branch is the `ManifestNotFound` case.

**Alternatives considered**: A brand-new "Create pack" nav page with its own folder picker —
rejected; it would duplicate the picker/error-handling code path for no behavioral difference,
and would leave two different ways to "add a folder" for the user to reconcile mentally.

## R2: Scanning a folder for candidate images

**Decision**: `wallpaper-settings` takes a direct (non-dev) dependency on the `image` crate,
pinned to the identical version/feature set `pack-loader` already uses (`jpeg, png, gif, webp,
bmp, tiff`). Scanning reads the directory's entries and, for each, attempts
`image::ImageReader::open(path)?.with_guessed_format()?.into_dimensions()` — a header-only read,
never a full decode — to decide whether it's a thumbnail-able image (FR-003, FR-018's "no usable
images" case) or gets silently skipped (Edge Case: non-image files ignored; Edge Case: unreadable
image excluded with a flag).

**Rationale**: This is a near-verbatim copy of `pack_loader::image_check::check_readable`, but
that module is private to `pack-loader` and exists to validate a manifest's *declared* files, not
to scan a directory with no manifest yet — a different caller with a different question. Given
the check itself is ~10 lines, duplicating it is cheaper and more honest than widening
`pack-loader`'s public API for a single external caller — the same "duplicate a small piece
rather than invert crate ownership" call the project already made for `wallpaper-ipc`'s D-Bus
constants and `STARTER_PACK_SYSTEM_PATH` (`crates/pack-loader/src/registry.rs`'s own comment).

**Alternatives considered**: Exporting `image_check::check_readable` from `pack-loader` —
rejected for the reason above; also considered thumbnailing via a downscaled decode up front —
rejected, `pages::packs::view` already renders full-size pack thumbnails straight through
`widget::image(path)` with no pre-scaling, so the wizard's per-row thumbnails follow the same
established, good-enough-for-a-settings-GUI precedent.

## R3: Writing `manifest.toml`

**Decision**: Add a new, symmetric write-side to `pack-loader::manifest` (the module that already
owns `parse`): a `render(manifest: &ManifestDraft) -> String` function plus a small
`format_anchor(&TimeAnchor) -> String` (the exact inverse of the existing private
`parse_anchor`), producing TOML via the `toml` crate's `Serialize` support (a local
`#[derive(Serialize)]` shape mirroring `RawManifest`/`RawManifestImage`) rather than hand-built
string interpolation.

**Rationale**: The spec's edge cases explicitly require an author name containing quotes or
non-Latin characters to round-trip correctly (Edge Cases) — hand-formatting
`format!("author = \"{name}\"")` breaks the instant `name` itself contains a `"`. Routing through
`toml`'s own serializer makes that a non-issue by construction, the same way `serde`/`toml`
already protect every other field this crate touches. Keeping `render`/`format_anchor` next to
`parse`/`parse_anchor` in the same module (rather than a new file in `wallpaper-settings`) keeps
the manifest grammar single-sourced in the crate that already owns it and versions it
(`MAX_SUPPORTED_SCHEMA_VERSION`) — `wallpaper-settings` never hand-encodes an anchor string
itself.

**Self-validation**: after writing, the wizard calls `pack_loader::load_pack` on the freshly
written path before treating Generate as successful (FR-012) — the exact same validation path a
real user's later `wallpaperctl register` would take, so "immediately valid and loadable" is a
hard postcondition, not a hope.

**Alternatives considered**: Hand-built string formatting (rejected, see above — real
correctness bug for the exact input the spec calls out). Adding `Serialize` directly to the
existing `PackManifest`/`ManifestImage` types and reusing them for both directions — rejected;
those types carry post-validation invariants (`TimeAnchor`, resolved `ScalingMode`/`Color`) that
don't need round-tripping, and keeping the write side as its own small `ManifestDraft` shape
(data-model.md) keeps "what the user is still editing" visibly distinct from "what a fully
loaded pack looks like."

## R4: Detecting scheduling conflicts (FR-008, FR-018)

**Decision**: No new duplicate-detection logic. Once every row has an assignment, the wizard
builds the same `Vec<schedule_engine::PackImage>` a real pack load would and calls
`schedule_engine::WallpaperPack::validate(images)` — this already returns `PackError::Empty`,
`PackError::TooManyAnchors`, `PackError::DuplicateImageId`, and (for clock/specific-time mode)
`PackError::DuplicateInstant` for free, all pure and already unit-tested (constitution Principle
V). For solar-period mode specifically, two more layers apply:

1. A **location-independent** literal-equality check (same `SolarEventKind` *and* same
   `Option<TimeDelta>` offset on two rows) — always available, since it needs no resolved date.
2. When a location is already configured, `ValidatedPack::check_solar_duplicate_instant(&location,
   today)` additionally catches two *different* event/offset pairs that happen to resolve to the
   same instant (`crates/schedule-engine/src/pack.rs`) — reusing `wallpaper_ipc::effective_location`
   exactly as `pages::location`/`pages::timeline` already do.

If no location is configured yet, layer 2 is simply skipped — Generate can still proceed as long
as layer 1 and `validate`'s own checks pass; a date/location-dependent collision that only layer 2
would have caught is the same category of thing `wallpaperd` already re-checks "as the daemon
crosses into a new date" (`pack.rs`'s own doc comment), not a new gap this feature introduces.

**Rationale**: Reimplementing instant-collision math in `wallpaper-settings` would duplicate
logic the constitution requires to be pure/tested/single-sourced in `schedule-engine` (Principle
V) — this is exactly the reuse the existing type signatures were built for.

## R5: Modeling "unassigned" and gating Generate

**Decision**: Each row's assignment is `Option<SolarPeriodAssignment>` or `Option<NaiveTime>`
(mode-dependent) — `None` on first render (clarification: no default guess). A pure function,
`all_assigned(&[Row]) -> bool`, gates the Generate button; `widget::dropdown`'s own
`selected: Option<usize>` and a plain `Option<NaiveTime>` for the time control map this directly
with no extra "is this row touched yet" flag needed.

**Rationale**: Matches `pages::assignment.rs`'s existing style (`Option<usize>` selection state
feeding a pure gating check) and keeps the emptiness state and the real value in one type instead
of two.

## R6: The solar-period offset control

**Decision**: Two `widget::spin_button<i32>` (generic increment control, already used nowhere
else in this codebase yet but a real exported `libcosmic` widget) — a signed-hours field
(`-12..=12`) and a minutes field (`0..=59`), combined into one `chrono::TimeDelta` by a pure
`combine_offset(hours: i32, minutes: u32) -> TimeDelta` that also enforces the ±12h magnitude cap
(clarification) by clamping minutes to `0` whenever `hours.abs() == 12`.

**Rationale**: `libcosmic` has no dedicated duration-entry widget; `spin_button` is the closest
existing primitive (bounded, steppable, generic over any `Copy + Add + Sub + PartialOrd`
numeric type) and needs no new dependency.

**Alternatives considered**: A single signed-minutes spin button (range -720..=720) — rejected,
worse UX for the common "half an hour" case (forces mental math from minutes) and a duration
grammar of `h`/`m` is what the manifest format itself already documents
(`docs/pack-manifest-schema.md`'s "Anchor grammar").

## R7: The specific-time control

**Decision**: Two `widget::spin_button<u32>` — hour (`0..=23`) and minute (`0..=59`) — combined
into a `chrono::NaiveTime` the same way R6 combines an offset. No dedicated time-picker widget
exists in the pinned `libcosmic` checkout (`src/widget/` has no `time_picker` module, only
`spin_button`), so this is the same primitive R6 uses, not a second mechanism.

## R8: The standard pack storage location and the move itself

**Decision**: "The application's standard pack storage location" (spec.md Assumptions) resolves
to `dirs::data_dir().join("cosmic-dynamic-wallpaper").join("packs")` — i.e. `$XDG_DATA_HOME/
cosmic-dynamic-wallpaper/packs` (`~/.local/share/...` by default). `dirs` is added as a direct
dependency of `wallpaper-settings`; it is already resolved in `Cargo.lock` at v6.0.0 as a
transitive dependency of `cosmic-config` itself, so this is a zero-new-crate addition. This
mirrors the existing system-wide convention (`crates/renderer/src/starter_pack.rs`'s
`STARTER_PACK_SYSTEM_PATH = "/usr/share/cosmic-dynamic-wallpaper/starter-pack"`) at the per-user
level a writable destination needs (`/usr/share` is root-owned and wrong for user-generated
content).

The move itself is **copy-then-verify-then-delete-source**, never a bare `rename`: recursively
copy the folder to the destination (using a disambiguated name if the user supplies one after a
collision — clarification), call `pack_loader::load_pack` on the copy to confirm it's intact and
valid, and only then remove the original folder. If any step fails, the destination copy (if
partial) is removed and the original folder is left completely untouched (FR-017, Edge Case: a
failed move leaves the original folder usable). A plain `std::fs::rename` is deliberately not
used as the primary mechanism — it silently fails across filesystem boundaries (`EXDEV`, e.g.
source folder on a mounted removable drive), which a copy+verify+delete sequence handles
uniformly instead of needing a separate fallback path.

**Alternatives considered**: `std::fs::rename` with an `EXDEV`-triggered copy fallback —
rejected as needlessly two code paths for the same guarantee the copy-first approach already
gives uniformly, at a cost (an extra full copy on the common same-filesystem case) the spec's
image-pack sizes (a handful of images, capped at 64) make negligible.

## R9: Wizard UI state ownership

**Decision**: `App` (`crates/wallpaper-settings/src/app.rs`) gains one new field,
`pack_builder: Option<pages::pack_builder::State>`. `Some` means the wizard is active; `App`'s
top-level `view()` renders the wizard in place of the currently selected nav page whenever it's
`Some` (the nav sidebar itself is unaffected — same "one extra piece of state gates an alternate
view" shape `pending_removal` already uses for the removal-confirmation dialog, just page-sized
instead of modal-sized, since the wizard's thumbnail grid needs real layout room a modal
overlay doesn't comfortably give it). Cancelling, a successful Generate-and-place, or a fatal
scan error all clear `pack_builder` back to `None`, returning the user to whichever nav page was
active before.

**Rationale**: Consistent with the existing pattern of one `Option<T>` field per transient UI
state (`packs::State.pending_removal`, `packs::State.add_error`) rather than introducing a
second top-level page-routing mechanism alongside `nav_model`.

## R10: Manifest defaults not exposed in the wizard (spec.md Assumptions)

**Decision**: `name` defaults to the source folder's final path component
(`Path::file_name()`); `default_scaling` defaults to `ScalingMode::Fill`; `fallback_color`
defaults to `#000000` — the identical constants `docs/pack-manifest-schema.md`'s own example and
`tools/generate-starter-pack` already use, so a wizard-generated pack looks/behaves like a
hand-authored one out of the box.
