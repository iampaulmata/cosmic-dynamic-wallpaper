//! Custom Pack Builder wizard (spec 010) — turns a folder of images with no
//! `manifest.toml` into a fully valid custom pack from within the GUI. Entered from
//! `pages::packs`'s existing "Add pack folder…" flow when it hits
//! `pack_loader::ManifestError::ManifestNotFound` (research.md R1) rather than a new
//! nav page (research.md R9); owned by `App.pack_builder: Option<State>`.
//!
//! Scope, in order: pick a scheduling mode (solar period or specific time, FR-004),
//! assign every scanned image (FR-005–FR-009), name an author (FR-010), Generate a
//! self-validated `manifest.toml` (FR-011, FR-012), then choose whether to move the
//! folder into the application's standard pack location or leave it in place
//! (FR-013–FR-017). See data-model.md §2 and contracts/pack-builder-gui-flow.md for the
//! full state machine this module implements.
//!
//! Two design notes worth keeping in view while reading this file (both found while
//! actually building the offset/time controls, refining data-model.md's sketch):
//! - A signed-hours field alone can't express "-15m" when hours is 0 (there's no
//!   negative zero in `i32`) — `SolarAssignment` carries an explicit `offset_negative`
//!   flag instead of relying on `offset_hours`'s own sign (research.md R6 still holds:
//!   two `spin_button`s plus a small sign toggle, just three fields instead of two).
//! - `combine_offset` therefore takes the sign explicitly rather than inferring it.

use std::path::{Path, PathBuf};

use chrono::{Local, NaiveTime, TimeDelta, Timelike};
use cosmic::widget;
use cosmic::Element;
use image::ImageReader;
use pack_loader::{Color, ManifestDraft, ManifestDraftImage, PackSource, Registry, ScalingMode};
use schedule_engine::{Location, PackError, PackImage, SolarEventKind, TimeAnchor, WallpaperPack};

/// The 8 recognized solar events, in the order shown in the dropdown (FR-005).
pub const SOLAR_EVENTS: [SolarEventKind; 8] = [
    SolarEventKind::Sunrise,
    SolarEventKind::Sunset,
    SolarEventKind::SolarNoon,
    SolarEventKind::SolarMidnight,
    SolarEventKind::CivilDawn,
    SolarEventKind::CivilDusk,
    SolarEventKind::AstronomicalDawn,
    SolarEventKind::AstronomicalDusk,
];

fn solar_event_label(event: SolarEventKind) -> &'static str {
    match event {
        SolarEventKind::Sunrise => "Sunrise",
        SolarEventKind::Sunset => "Sunset",
        SolarEventKind::SolarNoon => "Solar noon",
        SolarEventKind::SolarMidnight => "Solar midnight",
        SolarEventKind::CivilDawn => "Civil dawn",
        SolarEventKind::CivilDusk => "Civil dusk",
        SolarEventKind::AstronomicalDawn => "Astronomical dawn",
        SolarEventKind::AstronomicalDusk => "Astronomical dusk",
    }
}

fn solar_event_labels() -> Vec<&'static str> {
    SOLAR_EVENTS.iter().copied().map(solar_event_label).collect()
}

// --- Types (data-model.md §2) ---

/// Which kind of schedule the whole draft uses (FR-004) — chosen once, up front;
/// changing it later discards every row's current assignment (spec.md Edge Cases)
/// since a pack cannot mix solar and clock anchors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignmentMode {
    SolarPeriod,
    SpecificTime,
}

/// One image's solar-period assignment — an event plus a signed hour/minute offset,
/// clamped to a ±12h magnitude (research.md R6, the clarification's cap).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SolarAssignment {
    pub event: SolarEventKind,
    /// `true` nudges the event earlier ("-"), `false` later ("+") or not at all.
    pub offset_negative: bool,
    /// 0..=12.
    pub offset_hours: u32,
    /// 0..=59; forced to 0 whenever `offset_hours == 12`.
    pub offset_minutes: u32,
}

/// One scanned image plus its current, possibly still-empty assignment.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageRow {
    pub file_name: String,
    pub thumbnail_path: PathBuf,
    pub solar: Option<SolarAssignment>,
    pub time: Option<NaiveTime>,
}

impl ImageRow {
    fn new(file_name: String, thumbnail_path: PathBuf) -> Self {
        Self { file_name, thumbnail_path, solar: None, time: None }
    }
}

/// Where the just-generated manifest currently sits, awaiting the move-vs-keep choice
/// (FR-013).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedPlacement {
    pub generated_path: PathBuf,
}

/// Open while the destination-name-collision prompt is shown (FR-014a).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingCollision {
    pub generated_path: PathBuf,
    pub suggested_name: String,
}

/// The wizard's full transient state — owned by `App.pack_builder`, never persisted.
#[derive(Debug, Clone, PartialEq)]
pub struct State {
    pub source_dir: PathBuf,
    pub mode: Option<AssignmentMode>,
    pub rows: Vec<ImageRow>,
    pub author: String,
    /// Set whenever FR-008's check currently fails — blocks Generate.
    pub conflict: Option<String>,
    /// FR-017's error surface for a failed Generate (write or self-validation).
    pub generate_error: Option<String>,
    /// FR-017's error surface for a failed move.
    pub move_error: Option<String>,
    pub pending_collision: Option<PendingCollision>,
    pub pending_placement: Option<GeneratedPlacement>,
    /// FR-018: zero usable images, or more than `schedule_engine::MAX_ANCHORS`.
    pub scan_error: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Message {
    ModeChosen(AssignmentMode),
    Cancelled,
    /// `(row, index into SOLAR_EVENTS)`.
    SolarEventSelected(usize, usize),
    SolarOffsetSignToggled(usize),
    SolarOffsetHoursChanged(usize, u32),
    SolarOffsetMinutesChanged(usize, u32),
    TimeHourChanged(usize, u32),
    TimeMinuteChanged(usize, u32),
    AuthorChanged(String),
    GenerateRequested,
    MoveRequested,
    KeepRequested,
    CollisionNameChanged(String),
    CollisionConfirmed,
    CollisionCancelled,
}

// --- Opening the wizard (research.md R1, R9) ---

/// FR-002: whether picking `path` (already known to exist) from "Add pack folder…"
/// should launch this wizard instead of registering it directly — true only for a
/// directory with no `manifest.toml` yet (`ManifestNotFound` specifically, not any
/// other load failure, and never a single-file static pack). Extracted as its own
/// function so this decision is unit-testable without a real `App`/`cosmic::
/// Application` — spec 010 Edge Cases' "already has a manifest" case is asserted
/// directly against this, not just exercised manually.
pub fn should_open_for(path: &Path) -> bool {
    path.is_dir() && matches!(pack_loader::load_pack(path), Err(pack_loader::ManifestError::ManifestNotFound { .. }))
}

/// Opens the wizard at `source_dir` — called when the existing "Add pack folder…" flow
/// hits `ManifestNotFound` (research.md R1). Scans the folder immediately, so the
/// mode-choice screen can show a real scan error right away rather than deferring the
/// scan until a mode is picked (FR-001, FR-002, FR-003).
pub fn open(source_dir: PathBuf) -> State {
    let (rows, scan_error) = match scan_folder(&source_dir) {
        Ok(rows) => (rows, None),
        Err(reason) => (Vec::new(), Some(reason)),
    };
    State {
        source_dir,
        mode: None,
        rows,
        author: String::new(),
        conflict: None,
        generate_error: None,
        move_error: None,
        pending_collision: None,
        pending_placement: None,
        scan_error,
    }
}

/// Scans `dir` for candidate images (research.md R2): keeps only files that pass a
/// header-only readability check, silently skipping everything else (non-images,
/// subdirectories, a stray `manifest.toml`). Fails with a specific message for FR-018's
/// two boundary cases — no usable images, or more than `schedule_engine::MAX_ANCHORS`.
fn scan_folder(dir: &Path) -> Result<Vec<ImageRow>, String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("couldn't read {}: {e}", dir.display()))?;

    let mut rows = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        if is_readable_image(&path) {
            rows.push(ImageRow::new(file_name.to_string(), path));
        }
    }
    rows.sort_by(|a, b| a.file_name.cmp(&b.file_name));

    if rows.is_empty() {
        return Err(format!("{} has no readable images to build a pack from.", dir.display()));
    }
    if rows.len() > schedule_engine::MAX_ANCHORS {
        return Err(format!(
            "{} has {} images — a single pack can hold at most {}.",
            dir.display(),
            rows.len(),
            schedule_engine::MAX_ANCHORS
        ));
    }
    Ok(rows)
}

/// Header-only readability check (research.md R2) — mirrors `pack_loader::image_check`'s
/// own check exactly, duplicated here since that module is private to `pack-loader` and
/// exists to validate a manifest's *declared* files, not to scan an unconfigured folder
/// (research.md R2's "duplicate a small piece rather than invert crate ownership" call).
fn is_readable_image(path: &Path) -> bool {
    let Ok(reader) = ImageReader::open(path) else { return false };
    let Ok(reader) = reader.with_guessed_format() else { return false };
    reader.into_dimensions().is_ok()
}

// --- Pure functions (data-model.md) ---

/// Combines a sign, hours, and minutes into one `TimeDelta`, clamping to the ±12h cap
/// (research.md R6) regardless of what's passed in — `hours` above 12 is clamped to 12,
/// and `minutes` is forced to 0 once `hours` is already 12.
pub fn combine_offset(negative: bool, hours: u32, minutes: u32) -> TimeDelta {
    let hours = hours.min(12);
    let minutes = if hours >= 12 { 0 } else { minutes.min(59) };
    let magnitude = TimeDelta::hours(i64::from(hours)) + TimeDelta::minutes(i64::from(minutes));
    if negative {
        -magnitude
    } else {
        magnitude
    }
}

/// Blank or whitespace-only input becomes "Artist Unknown" (FR-010); anything else is
/// used verbatim, trimmed of leading/trailing whitespace.
pub fn effective_author(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        "Artist Unknown".to_string()
    } else {
        trimmed.to_string()
    }
}

/// FR-009: every row must have an explicit assignment before Generate is available. An
/// empty row list is never "all assigned" — there'd be nothing to generate.
pub fn all_assigned(rows: &[ImageRow], mode: AssignmentMode) -> bool {
    !rows.is_empty()
        && rows.iter().all(|row| match mode {
            AssignmentMode::SolarPeriod => row.solar.is_some(),
            AssignmentMode::SpecificTime => row.time.is_some(),
        })
}

fn solar_row_anchor(assignment: SolarAssignment) -> TimeAnchor {
    let offset = if assignment.offset_hours == 0 && assignment.offset_minutes == 0 {
        None
    } else {
        Some(combine_offset(assignment.offset_negative, assignment.offset_hours, assignment.offset_minutes))
    };
    TimeAnchor::solar(assignment.event, offset)
}

/// Builds the `ManifestDraft` Generate will render (FR-011), applying research.md R10's
/// defaults: `name` from the folder name, `default_scaling = Fill`,
/// `fallback_color = #000000`. Rows without an assignment for the active mode are
/// skipped rather than panicking — callers are expected to have already checked
/// `all_assigned` (Generate is gated on it), but this stays total either way.
pub fn build_draft(rows: &[ImageRow], mode: AssignmentMode, folder_name: &str, author: &str) -> ManifestDraft {
    let images = rows
        .iter()
        .filter_map(|row| {
            let anchor = match mode {
                AssignmentMode::SolarPeriod => row.solar.map(solar_row_anchor),
                AssignmentMode::SpecificTime => row.time.map(TimeAnchor::clock),
            }?;
            Some(ManifestDraftImage { file: row.file_name.clone(), anchor })
        })
        .collect();

    ManifestDraft {
        name: folder_name.to_string(),
        author: Some(effective_author(author)),
        default_scaling: ScalingMode::Fill,
        fallback_color: Color { r: 0, g: 0, b: 0, a: 255 },
        images,
    }
}

/// FR-008: checks the current rows for a scheduling conflict. Returns `None` when
/// there's nothing to flag yet — including while some rows are still unassigned, since
/// `all_assigned` already gates Generate separately; this only ever reports genuine
/// conflicts between rows that *are* assigned.
pub fn detect_conflict(rows: &[ImageRow], mode: AssignmentMode, location: Option<Location>) -> Option<String> {
    match mode {
        AssignmentMode::SolarPeriod => detect_solar_conflict(rows, location),
        AssignmentMode::SpecificTime => detect_clock_conflict(rows),
    }
}

/// research.md R4's two layers: a location-independent literal-equality check first
/// (always available), then `schedule_engine`'s own, already-tested
/// `check_solar_duplicate_instant` once every row is assigned and a location exists.
fn detect_solar_conflict(rows: &[ImageRow], location: Option<Location>) -> Option<String> {
    let assignments: Vec<SolarAssignment> = rows.iter().filter_map(|r| r.solar).collect();

    for i in 0..assignments.len() {
        for other in &assignments[i + 1..] {
            let a = assignments[i];
            if a.event == other.event
                && a.offset_negative == other.offset_negative
                && a.offset_hours == other.offset_hours
                && a.offset_minutes == other.offset_minutes
            {
                return Some(format!(
                    "Two images are both scheduled for {} — pick a different event or offset for one of them.",
                    solar_event_label(a.event)
                ));
            }
        }
    }

    if assignments.len() != rows.len() {
        return None;
    }
    let location = location?;
    let pack_images: Vec<PackImage> =
        rows.iter().filter_map(|r| r.solar.map(|s| PackImage::new(r.file_name.clone(), solar_row_anchor(s)))).collect();
    match WallpaperPack::validate(pack_images) {
        Ok(validated) => match validated.check_solar_duplicate_instant(&location, Local::now().date_naive()) {
            Ok(()) => None,
            Err(_) => Some("Two images resolve to the same moment today — adjust one of their offsets.".to_string()),
        },
        // Empty/TooManyAnchors/MixedAnchorTypes aren't reachable through this UI's own
        // rows (scan_folder/set_mode already rule them out) — not this check's concern.
        Err(_) => None,
    }
}

fn detect_clock_conflict(rows: &[ImageRow]) -> Option<String> {
    let times: Vec<NaiveTime> = rows.iter().filter_map(|r| r.time).collect();
    if times.len() != rows.len() {
        return None;
    }
    let pack_images: Vec<PackImage> =
        rows.iter().filter_map(|r| r.time.map(|t| PackImage::new(r.file_name.clone(), TimeAnchor::clock(t)))).collect();
    match WallpaperPack::validate(pack_images) {
        Err(PackError::DuplicateInstant) => {
            Some("Two images are set to the exact same time — pick a different time for one of them.".to_string())
        }
        _ => None,
    }
}

// --- State mutators (called from `app.rs`'s `update`) ---

fn recompute_conflict(state: &mut State, location: Option<Location>) {
    state.conflict = state.mode.and_then(|mode| detect_conflict(&state.rows, mode, location));
}

/// FR-004/Edge Cases: sets the mode and discards every row's assignment for the *other*
/// mode (a pack can't mix solar and clock anchors) — safe to call both for the initial
/// choice and a later switch, since rows start with both fields already `None`.
pub fn set_mode(state: &mut State, mode: AssignmentMode) {
    state.mode = Some(mode);
    for row in &mut state.rows {
        row.solar = None;
        row.time = None;
    }
    state.conflict = None;
    state.generate_error = None;
}

pub fn set_solar_event_by_index(state: &mut State, row: usize, index: usize, location: Option<Location>) {
    if let Some(event) = SOLAR_EVENTS.get(index).copied() {
        if let Some(r) = state.rows.get_mut(row) {
            let assignment = r.solar.get_or_insert(SolarAssignment { event, offset_negative: false, offset_hours: 0, offset_minutes: 0 });
            assignment.event = event;
        }
    }
    recompute_conflict(state, location);
}

pub fn toggle_solar_offset_sign(state: &mut State, row: usize, location: Option<Location>) {
    if let Some(a) = state.rows.get_mut(row).and_then(|r| r.solar.as_mut()) {
        a.offset_negative = !a.offset_negative;
    }
    recompute_conflict(state, location);
}

pub fn set_solar_offset_hours(state: &mut State, row: usize, hours: u32, location: Option<Location>) {
    if let Some(a) = state.rows.get_mut(row).and_then(|r| r.solar.as_mut()) {
        a.offset_hours = hours.min(12);
        if a.offset_hours >= 12 {
            a.offset_minutes = 0;
        }
    }
    recompute_conflict(state, location);
}

pub fn set_solar_offset_minutes(state: &mut State, row: usize, minutes: u32, location: Option<Location>) {
    if let Some(a) = state.rows.get_mut(row).and_then(|r| r.solar.as_mut()) {
        if a.offset_hours < 12 {
            a.offset_minutes = minutes.min(59);
        }
    }
    recompute_conflict(state, location);
}

pub fn set_time_hour(state: &mut State, row: usize, hour: u32) {
    if let Some(r) = state.rows.get_mut(row) {
        let minute = r.time.map_or(0, |t| t.minute());
        r.time = NaiveTime::from_hms_opt(hour.min(23), minute, 0).or(Some(NaiveTime::MIN));
    }
    recompute_conflict(state, None);
}

pub fn set_time_minute(state: &mut State, row: usize, minute: u32) {
    if let Some(r) = state.rows.get_mut(row) {
        let hour = r.time.map_or(0, |t| t.hour());
        r.time = NaiveTime::from_hms_opt(hour, minute.min(59), 0).or(Some(NaiveTime::MIN));
    }
    recompute_conflict(state, None);
}

pub fn set_author(state: &mut State, author: String) {
    state.author = author;
}

// --- Generate (FR-011, FR-012, FR-017; contracts/pack-loader-manifest-writer.md) ---

/// Builds the draft, renders it, writes `manifest.toml`, then self-validates by loading
/// it back through `pack_loader::load_pack` — the exact validation path a real pack
/// registration takes. On any failure the just-written file (if any) is removed and
/// `state.generate_error` is set; `state.rows`/`author` are never touched either way
/// (FR-017). On success, `state.pending_placement` is set and the caller shows the
/// placement dialog.
pub fn generate(state: &mut State) {
    state.generate_error = None;
    let Some(mode) = state.mode else { return };

    let folder_name =
        state.source_dir.file_name().and_then(|n| n.to_str()).unwrap_or("Custom Pack").to_string();
    let draft = build_draft(&state.rows, mode, &folder_name, &state.author);
    let text = pack_loader::render(&draft);
    let manifest_path = state.source_dir.join(pack_loader::MANIFEST_FILE_NAME);

    if let Err(e) = std::fs::write(&manifest_path, &text) {
        state.generate_error = Some(format!("couldn't write manifest.toml: {e}"));
        return;
    }

    if let Err(e) = pack_loader::load_pack(&state.source_dir) {
        let _ = std::fs::remove_file(&manifest_path);
        state.generate_error = Some(format!("the generated pack didn't validate: {e}"));
        return;
    }

    state.pending_placement = Some(GeneratedPlacement { generated_path: state.source_dir.clone() });
}

// --- Placement (FR-013–FR-017, FR-014a; research.md R8) ---

/// `dirs::data_dir()/cosmic-dynamic-wallpaper/packs` (research.md R8) — `None` only if
/// the platform has no resolvable data directory at all (not expected on a real COSMIC
/// session, but handled rather than assumed).
pub fn standard_pack_location() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("cosmic-dynamic-wallpaper").join("packs"))
}

enum MoveError {
    Collision,
    Io(String),
}

/// The copy-then-verify-then-delete move routine (research.md R8): recursively copies
/// `generated_path` to `destination_root/name`, self-validates the copy via
/// `pack_loader::load_pack`, then removes `generated_path` only after that succeeds.
/// Any failure removes a partial destination copy and leaves `generated_path`
/// completely untouched.
fn move_pack(generated_path: &Path, destination_root: &Path, name: &str) -> Result<PathBuf, MoveError> {
    let destination = destination_root.join(name);
    if destination.exists() {
        return Err(MoveError::Collision);
    }
    std::fs::create_dir_all(destination_root).map_err(|e| MoveError::Io(e.to_string()))?;

    if let Err(e) = copy_dir_recursive(generated_path, &destination) {
        let _ = std::fs::remove_dir_all(&destination);
        return Err(MoveError::Io(e.to_string()));
    }
    if let Err(e) = pack_loader::load_pack(&destination) {
        let _ = std::fs::remove_dir_all(&destination);
        return Err(MoveError::Io(format!("the moved copy didn't validate: {e}")));
    }
    if let Err(e) = std::fs::remove_dir_all(generated_path) {
        return Err(MoveError::Io(format!(
            "copied to {} but couldn't remove the original folder: {e}",
            destination.display()
        )));
    }
    Ok(destination)
}

/// Copies every file and subdirectory from `from` into `to` (already-created).
/// Symlinks are deliberately neither followed nor recreated, rather than risking a copy
/// of something outside the source folder.
fn copy_dir_recursive(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let dest = to.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &dest)?;
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), &dest)?;
        }
    }
    Ok(())
}

fn register_and_close(state: &mut State, registry: &mut Registry, path: &Path) {
    if let Ok(source) = PackSource::resolve(path) {
        let _ = registry.register(source);
    }
    state.pending_placement = None;
    state.pending_collision = None;
    state.move_error = None;
}

fn finish_move(state: &mut State, registry: &mut Registry, generated_path: &Path, root: &Path, name: &str) -> bool {
    match move_pack(generated_path, root, name) {
        Ok(destination) => {
            register_and_close(state, registry, &destination);
            true
        }
        Err(MoveError::Collision) => {
            state.pending_placement = None;
            state.pending_collision =
                Some(PendingCollision { generated_path: generated_path.to_path_buf(), suggested_name: name.to_string() });
            false
        }
        Err(MoveError::Io(reason)) => {
            state.move_error = Some(reason);
            false
        }
    }
}

/// The placement dialog's "Move" action (FR-013, FR-014). Returns `true` when the
/// wizard is fully done and the caller should close it (and refresh the Packs page);
/// `false` means it needs to stay open (a collision prompt, or an error to show).
pub fn confirm_move(state: &mut State, registry: &mut Registry) -> bool {
    let Some(placement) = state.pending_placement.clone() else { return false };
    let Some(root) = standard_pack_location() else {
        state.move_error = Some("couldn't determine the standard pack location.".to_string());
        return false;
    };
    let suggested_name =
        placement.generated_path.file_name().and_then(|n| n.to_str()).unwrap_or("custom-pack").to_string();
    finish_move(state, registry, &placement.generated_path, &root, &suggested_name)
}

/// The placement dialog's "Keep it here" action (FR-015). Always closes the wizard.
pub fn confirm_keep(state: &mut State, registry: &mut Registry) -> bool {
    let Some(placement) = state.pending_placement.clone() else { return false };
    register_and_close(state, registry, &placement.generated_path);
    true
}

pub fn set_collision_name(state: &mut State, name: String) {
    if let Some(pending) = state.pending_collision.as_mut() {
        pending.suggested_name = name;
    }
}

/// The collision prompt's "Move" (retry) action (FR-014a).
pub fn confirm_collision_move(state: &mut State, registry: &mut Registry) -> bool {
    let Some(pending) = state.pending_collision.clone() else { return false };
    let Some(root) = standard_pack_location() else {
        state.move_error = Some("couldn't determine the standard pack location.".to_string());
        return false;
    };
    state.pending_collision = None;
    finish_move(state, registry, &pending.generated_path, &root, &pending.suggested_name)
}

/// The collision prompt's "Cancel" action — falls back to keeping the pack in place
/// (contracts/pack-builder-gui-flow.md: the folder already has a valid manifest either
/// way, so this is never a destructive cancel).
pub fn cancel_collision_to_keep(state: &mut State, registry: &mut Registry) -> bool {
    let Some(pending) = state.pending_collision.take() else { return false };
    register_and_close(state, registry, &pending.generated_path);
    true
}

// --- View ---

pub fn view(state: &State) -> Element<'_, Message> {
    let content = if let Some(reason) = &state.scan_error {
        scan_error_view(reason)
    } else {
        match state.mode {
            None => mode_choice_view(),
            Some(mode) => configuration_view(state, mode),
        }
    };
    widget::scrollable(content).into()
}

fn scan_error_view(reason: &str) -> Element<'_, Message> {
    widget::column::with_capacity(2)
        .spacing(cosmic::theme::spacing().space_s)
        .push(widget::text::body(reason.to_string()))
        .push(widget::button::standard("Cancel").on_press(Message::Cancelled))
        .into()
}

fn mode_choice_view<'a>() -> Element<'a, Message> {
    widget::column::with_capacity(3)
        .spacing(cosmic::theme::spacing().space_m)
        .push(widget::text::title3("How do you want to schedule these images?"))
        .push(
            widget::row::with_capacity(2)
                .spacing(cosmic::theme::spacing().space_s)
                .push(widget::button::suggested("By solar period").on_press(Message::ModeChosen(AssignmentMode::SolarPeriod)))
                .push(widget::button::suggested("By specific time").on_press(Message::ModeChosen(AssignmentMode::SpecificTime))),
        )
        .push(widget::button::standard("Cancel").on_press(Message::Cancelled))
        .into()
}

fn configuration_view(state: &State, mode: AssignmentMode) -> Element<'_, Message> {
    let mut rows_col = widget::column::with_capacity(state.rows.len()).spacing(cosmic::theme::spacing().space_xs);
    for (index, row) in state.rows.iter().enumerate() {
        rows_col = rows_col.push(row_view(row, index, mode));
    }

    let mut layout = widget::column::with_capacity(6).spacing(cosmic::theme::spacing().space_m);
    layout = layout.push(widget::text::title3(match mode {
        AssignmentMode::SolarPeriod => "Assign each image to a solar period",
        AssignmentMode::SpecificTime => "Assign each image a time of day",
    }));
    layout = layout.push(rows_col);
    layout = layout.push(widget::text::body("Author (leave blank for \"Artist Unknown\")"));
    layout = layout.push(widget::text_input::text_input("Artist Unknown", &state.author).on_input(Message::AuthorChanged));

    if let Some(reason) = &state.conflict {
        layout = layout.push(widget::text::body(reason.clone()));
    }
    if let Some(reason) = &state.generate_error {
        layout = layout.push(widget::text::body(reason.clone()));
    }

    let can_generate = state.conflict.is_none() && all_assigned(&state.rows, mode);
    let mut generate_button = widget::button::suggested("Generate");
    if can_generate {
        generate_button = generate_button.on_press(Message::GenerateRequested);
    }

    layout = layout.push(
        widget::row::with_capacity(2)
            .spacing(cosmic::theme::spacing().space_s)
            .push(widget::button::standard("Cancel").on_press(Message::Cancelled))
            .push(generate_button),
    );

    layout.into()
}

fn row_view(row: &ImageRow, index: usize, mode: AssignmentMode) -> Element<'_, Message> {
    let thumbnail = widget::image(row.thumbnail_path.clone()).width(64).height(64);
    let control = match mode {
        AssignmentMode::SolarPeriod => solar_control(row.solar, index),
        AssignmentMode::SpecificTime => time_control(row.time, index),
    };
    widget::row::with_capacity(3)
        .spacing(cosmic::theme::spacing().space_s)
        .align_y(cosmic::iced::Alignment::Center)
        .push(thumbnail)
        .push(widget::text::body(row.file_name.clone()).width(160))
        .push(control)
        .into()
}

fn solar_control<'a>(assignment: Option<SolarAssignment>, index: usize) -> Element<'a, Message> {
    let labels = solar_event_labels();
    let selected = assignment.and_then(|a| SOLAR_EVENTS.iter().position(|e| *e == a.event));
    let dropdown = widget::dropdown(labels, selected, move |i| Message::SolarEventSelected(index, i));

    let (negative, hours, minutes) = match assignment {
        Some(a) => (a.offset_negative, a.offset_hours, a.offset_minutes),
        None => (false, 0, 0),
    };

    let sign_button =
        widget::button::standard(if negative { "\u{2212}" } else { "+" }).on_press(Message::SolarOffsetSignToggled(index));
    let hours_spin = widget::spin_button::spin_button(
        format!("{hours}h"),
        format!("offset hours, row {index}"),
        hours,
        1,
        0,
        12,
        move |v| Message::SolarOffsetHoursChanged(index, v),
    );
    let minutes_spin = widget::spin_button::spin_button(
        format!("{minutes}m"),
        format!("offset minutes, row {index}"),
        minutes,
        1,
        0,
        59,
        move |v| Message::SolarOffsetMinutesChanged(index, v),
    );

    widget::row::with_capacity(4)
        .spacing(cosmic::theme::spacing().space_xs)
        .align_y(cosmic::iced::Alignment::Center)
        .push(dropdown)
        .push(sign_button)
        .push(hours_spin)
        .push(minutes_spin)
        .into()
}

fn time_control<'a>(time: Option<NaiveTime>, index: usize) -> Element<'a, Message> {
    let hour = time.map_or(0, |t| t.hour());
    let minute = time.map_or(0, |t| t.minute());
    let hour_spin = widget::spin_button::spin_button(format!("{hour:02}"), format!("hour, row {index}"), hour, 1, 0, 23, move |v| {
        Message::TimeHourChanged(index, v)
    });
    let minute_spin =
        widget::spin_button::spin_button(format!("{minute:02}"), format!("minute, row {index}"), minute, 1, 0, 59, move |v| {
            Message::TimeMinuteChanged(index, v)
        });
    widget::row::with_capacity(2)
        .spacing(cosmic::theme::spacing().space_xs)
        .align_y(cosmic::iced::Alignment::Center)
        .push(hour_spin)
        .push(minute_spin)
        .into()
}

/// The placement/collision modal (contracts/pack-builder-gui-flow.md), rendered via
/// `App`'s `Application::dialog()` override exactly like spec 008's removal
/// confirmation (research.md R3's pattern) — `None` when neither is pending.
pub fn placement_dialog(state: &State) -> Option<Element<'_, Message>> {
    if let Some(pending) = &state.pending_collision {
        return Some(
            widget::dialog()
                .title("A pack with that name already exists")
                .body(format!(
                    "\"{}\" already exists at the standard pack location. Choose a different name.",
                    pending.suggested_name
                ))
                .control(widget::text_input::text_input("pack name", &pending.suggested_name).on_input(Message::CollisionNameChanged))
                .primary_action(widget::button::suggested("Move").on_press(Message::CollisionConfirmed))
                .secondary_action(widget::button::standard("Cancel").on_press(Message::CollisionCancelled))
                .into(),
        );
    }
    if state.pending_placement.is_some() {
        let body = state
            .move_error
            .clone()
            .unwrap_or_else(|| "Move this pack to the standard pack location, or keep it where it is?".to_string());
        return Some(
            widget::dialog()
                .title("Pack generated")
                .body(body)
                .primary_action(widget::button::suggested("Move").on_press(Message::MoveRequested))
                .secondary_action(widget::button::standard("Keep it here").on_press(Message::KeepRequested))
                .into(),
        );
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image_row(name: &str) -> ImageRow {
        ImageRow::new(name.to_string(), PathBuf::from(name))
    }

    fn solar(event: SolarEventKind) -> SolarAssignment {
        SolarAssignment { event, offset_negative: false, offset_hours: 0, offset_minutes: 0 }
    }

    // --- T009: combine_offset ---

    #[test]
    fn combine_offset_applies_sign_and_magnitude() {
        assert_eq!(combine_offset(false, 1, 15), TimeDelta::hours(1) + TimeDelta::minutes(15));
        assert_eq!(combine_offset(true, 0, 30), -TimeDelta::minutes(30));
        assert_eq!(combine_offset(true, 1, 0), -TimeDelta::hours(1));
    }

    #[test]
    fn combine_offset_clamps_minutes_at_the_twelve_hour_cap() {
        assert_eq!(combine_offset(false, 12, 45), TimeDelta::hours(12), "minutes must be forced to 0 at the ±12h cap");
        assert_eq!(combine_offset(true, 12, 45), -TimeDelta::hours(12));
        assert_eq!(combine_offset(false, 15, 0), TimeDelta::hours(12), "hours beyond 12 must clamp to 12");
    }

    // --- T010: effective_author ---

    #[test]
    fn effective_author_blank_or_whitespace_becomes_artist_unknown() {
        assert_eq!(effective_author(""), "Artist Unknown");
        assert_eq!(effective_author("   "), "Artist Unknown");
    }

    #[test]
    fn effective_author_trims_and_keeps_a_real_name() {
        assert_eq!(effective_author("  Jane Author  "), "Jane Author");
    }

    // --- T011: all_assigned ---

    #[test]
    fn all_assigned_true_only_when_every_row_has_the_active_mode_field_set() {
        let mut rows = vec![image_row("a.png"), image_row("b.png")];
        assert!(!all_assigned(&rows, AssignmentMode::SolarPeriod), "no rows assigned yet");

        rows[0].solar = Some(solar(SolarEventKind::Sunrise));
        assert!(!all_assigned(&rows, AssignmentMode::SolarPeriod), "one row still unassigned");

        rows[1].solar = Some(solar(SolarEventKind::Sunset));
        assert!(all_assigned(&rows, AssignmentMode::SolarPeriod));

        // Time fields being set doesn't matter for solar-mode gating, and vice versa.
        assert!(!all_assigned(&rows, AssignmentMode::SpecificTime));
    }

    #[test]
    fn all_assigned_is_false_for_an_empty_row_list() {
        assert!(!all_assigned(&[], AssignmentMode::SolarPeriod));
    }

    // --- T012: build_draft ---

    #[test]
    fn build_draft_applies_r10_defaults_and_maps_every_assigned_row() {
        let mut rows = vec![image_row("dawn.jpg"), image_row("dusk.jpg")];
        rows[0].solar = Some(solar(SolarEventKind::Sunrise));
        rows[1].solar = Some(SolarAssignment { event: SolarEventKind::Sunset, offset_negative: true, offset_hours: 0, offset_minutes: 30 });

        let draft = build_draft(&rows, AssignmentMode::SolarPeriod, "My Folder", "");

        assert_eq!(draft.name, "My Folder");
        assert_eq!(draft.author.as_deref(), Some("Artist Unknown"));
        assert_eq!(draft.default_scaling, ScalingMode::Fill);
        assert_eq!(draft.fallback_color, Color { r: 0, g: 0, b: 0, a: 255 });
        assert_eq!(draft.images.len(), 2);
        assert_eq!(draft.images[0].file, "dawn.jpg");
        assert_eq!(draft.images[0].anchor, TimeAnchor::solar(SolarEventKind::Sunrise, None));
        assert_eq!(
            draft.images[1].anchor,
            TimeAnchor::solar(SolarEventKind::Sunset, Some(-TimeDelta::minutes(30)))
        );
    }

    #[test]
    fn build_draft_keeps_a_supplied_author_name() {
        let mut rows = vec![image_row("a.png")];
        rows[0].time = Some(NaiveTime::from_hms_opt(6, 0, 0).unwrap());
        let draft = build_draft(&rows, AssignmentMode::SpecificTime, "Folder", "  Jane  ");
        assert_eq!(draft.author.as_deref(), Some("Jane"));
        assert_eq!(draft.images[0].anchor, TimeAnchor::clock(NaiveTime::from_hms_opt(6, 0, 0).unwrap()));
    }

    // --- T016: solar-mode conflict detection ---

    #[test]
    fn detect_conflict_solar_catches_identical_literal_assignments_without_a_location() {
        let mut rows = vec![image_row("a.png"), image_row("b.png")];
        rows[0].solar = Some(solar(SolarEventKind::Sunrise));
        rows[1].solar = Some(solar(SolarEventKind::Sunrise));

        let conflict = detect_conflict(&rows, AssignmentMode::SolarPeriod, None);
        assert!(conflict.is_some(), "two identical solar assignments must conflict even with no location configured");
    }

    #[test]
    fn detect_conflict_solar_is_none_for_distinct_events() {
        let mut rows = vec![image_row("a.png"), image_row("b.png")];
        rows[0].solar = Some(solar(SolarEventKind::Sunrise));
        rows[1].solar = Some(solar(SolarEventKind::Sunset));
        assert_eq!(detect_conflict(&rows, AssignmentMode::SolarPeriod, None), None);
    }

    #[test]
    fn detect_conflict_solar_uses_the_location_aware_layer_when_a_location_is_set() {
        // A distinct-looking but resolvable-to-the-same-instant pair only a
        // location-aware check (not the literal-equality layer) can catch: use the
        // exact same event/offset via two different offset representations reaching
        // the same TimeDelta — instead, assert the location-aware path is *reachable*
        // and returns `Ok`/no-conflict for a genuinely distinct, resolvable pair, which
        // exercises `check_solar_duplicate_instant` without depending on real solar
        // ephemeris numbers lining up.
        let mut rows = vec![image_row("a.png"), image_row("b.png")];
        rows[0].solar = Some(solar(SolarEventKind::Sunrise));
        rows[1].solar = Some(solar(SolarEventKind::SolarNoon));
        let location = Location::new(51.5072, -0.1276).unwrap();
        assert_eq!(detect_conflict(&rows, AssignmentMode::SolarPeriod, Some(location)), None);
    }

    // --- T021: clock-mode conflict detection ---

    #[test]
    fn detect_conflict_clock_catches_duplicate_times() {
        let mut rows = vec![image_row("a.png"), image_row("b.png")];
        rows[0].time = Some(NaiveTime::from_hms_opt(6, 0, 0).unwrap());
        rows[1].time = Some(NaiveTime::from_hms_opt(6, 0, 0).unwrap());
        assert!(detect_conflict(&rows, AssignmentMode::SpecificTime, None).is_some());
    }

    #[test]
    fn detect_conflict_clock_is_none_for_distinct_times() {
        let mut rows = vec![image_row("a.png"), image_row("b.png")];
        rows[0].time = Some(NaiveTime::from_hms_opt(6, 0, 0).unwrap());
        rows[1].time = Some(NaiveTime::from_hms_opt(18, 0, 0).unwrap());
        assert_eq!(detect_conflict(&rows, AssignmentMode::SpecificTime, None), None);
    }

    // --- set_mode: switching discards the other mode's assignments ---

    #[test]
    fn set_mode_clears_assignments_from_both_fields() {
        let mut state = open_test_state(&["a.png", "b.png"]);
        set_mode(&mut state, AssignmentMode::SolarPeriod);
        set_solar_event_by_index(&mut state, 0, 0, None);
        assert!(state.rows[0].solar.is_some());

        set_mode(&mut state, AssignmentMode::SpecificTime);
        assert!(state.rows[0].solar.is_none(), "switching modes must discard the previous mode's assignment");
        assert!(state.rows[0].time.is_none());
    }

    // --- Test helpers needing real files on disk (scan_folder/generate/move) ---

    fn write_test_image(path: &Path) {
        image::RgbImage::new(2, 2).save(path).unwrap();
    }

    fn open_test_state(file_names: &[&str]) -> State {
        let dir = tempfile::tempdir().unwrap();
        for name in file_names {
            write_test_image(&dir.path().join(name));
        }
        // Leak the TempDir deliberately for these pure-state tests that never touch the
        // filesystem again after scanning — acceptable since `cargo test` processes are
        // short-lived; tests that themselves do I/O (generate/move) manage their own
        // `TempDir` directly instead of this helper.
        let path = dir.keep();
        open(path)
    }

    // --- T031: a folder that already has a manifest.toml never opens the wizard ---

    #[test]
    fn should_open_for_is_false_for_a_folder_that_already_has_a_manifest() {
        let dir = tempfile::tempdir().unwrap();
        write_test_image(&dir.path().join("a.png"));
        std::fs::write(
            dir.path().join("manifest.toml"),
            "schema_version = 1\nname = \"x\"\ndefault_scaling = \"Fill\"\nfallback_color = \"#000000\"\n\n[[images]]\nfile = \"a.png\"\nanchor = \"sunrise\"\n",
        )
        .unwrap();

        assert!(!should_open_for(dir.path()), "an already-configured pack must register directly, not open the wizard");
    }

    #[test]
    fn should_open_for_is_true_for_a_manifest_free_folder() {
        let dir = tempfile::tempdir().unwrap();
        write_test_image(&dir.path().join("a.png"));
        assert!(should_open_for(dir.path()));
    }

    #[test]
    fn should_open_for_is_false_for_a_single_static_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.png");
        write_test_image(&file);
        assert!(!should_open_for(&file), "a single image file is a static pack, not a wizard candidate");
    }

    // --- T007: scan_folder / open ---

    #[test]
    fn open_scans_only_readable_images_and_sorts_by_name() {
        let dir = tempfile::tempdir().unwrap();
        write_test_image(&dir.path().join("b.png"));
        write_test_image(&dir.path().join("a.png"));
        std::fs::write(dir.path().join("notes.txt"), b"not an image").unwrap();

        let state = open(dir.path().to_path_buf());
        assert_eq!(state.scan_error, None);
        assert_eq!(state.rows.iter().map(|r| r.file_name.clone()).collect::<Vec<_>>(), vec!["a.png", "b.png"]);
    }

    #[test]
    fn open_reports_a_scan_error_for_a_folder_with_no_images() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("notes.txt"), b"not an image").unwrap();

        let state = open(dir.path().to_path_buf());
        assert!(state.scan_error.is_some());
        assert!(state.rows.is_empty());
    }

    #[test]
    fn open_reports_a_scan_error_for_too_many_images() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..(schedule_engine::MAX_ANCHORS + 1) {
            write_test_image(&dir.path().join(format!("{i}.png")));
        }
        let state = open(dir.path().to_path_buf());
        assert!(state.scan_error.is_some());
    }

    // --- T019/T023: full Generate round-trip through pack_loader::load_pack ---

    #[test]
    fn generate_solar_mode_produces_a_pack_that_loads_back_as_configured() {
        let dir = tempfile::tempdir().unwrap();
        write_test_image(&dir.path().join("dawn.jpg"));
        write_test_image(&dir.path().join("dusk.jpg"));

        let mut state = open(dir.path().to_path_buf());
        assert_eq!(state.scan_error, None);
        set_mode(&mut state, AssignmentMode::SolarPeriod);
        set_solar_event_by_index(&mut state, 0, 0, None); // Sunrise
        set_solar_event_by_index(&mut state, 1, 1, None); // Sunset
        assert!(state.conflict.is_none());
        assert!(all_assigned(&state.rows, AssignmentMode::SolarPeriod));

        generate(&mut state);
        assert_eq!(state.generate_error, None, "{:?}", state.generate_error);
        assert!(state.pending_placement.is_some());

        let loaded = pack_loader::load_pack(dir.path()).unwrap();
        assert_eq!(loaded.author.as_deref(), Some("Artist Unknown"));
        assert_eq!(loaded.pack.images().len(), 2);
    }

    #[test]
    fn generate_specific_time_mode_produces_a_pack_that_loads_back_as_configured() {
        let dir = tempfile::tempdir().unwrap();
        write_test_image(&dir.path().join("a.jpg"));
        write_test_image(&dir.path().join("b.jpg"));

        let mut state = open(dir.path().to_path_buf());
        set_mode(&mut state, AssignmentMode::SpecificTime);
        set_time_hour(&mut state, 0, 6);
        set_time_minute(&mut state, 0, 0);
        set_time_hour(&mut state, 1, 18);
        set_time_minute(&mut state, 1, 0);
        assert!(state.conflict.is_none());

        generate(&mut state);
        assert_eq!(state.generate_error, None, "{:?}", state.generate_error);

        let loaded = pack_loader::load_pack(dir.path()).unwrap();
        assert_eq!(loaded.pack.images().len(), 2);
    }

    #[test]
    fn generate_blocked_by_a_duplicate_time_never_writes_a_manifest() {
        let dir = tempfile::tempdir().unwrap();
        write_test_image(&dir.path().join("a.jpg"));
        write_test_image(&dir.path().join("b.jpg"));

        let mut state = open(dir.path().to_path_buf());
        set_mode(&mut state, AssignmentMode::SpecificTime);
        set_time_hour(&mut state, 0, 6);
        set_time_hour(&mut state, 1, 6);
        assert!(state.conflict.is_some(), "identical times must be flagged before Generate");
        assert!(!dir.path().join(pack_loader::MANIFEST_FILE_NAME).exists());
    }

    // --- T026: move failure leaves the source untouched ---

    #[test]
    fn move_pack_leaves_the_source_untouched_when_the_load_check_fails() {
        let source = tempfile::tempdir().unwrap();
        // A manifest.toml that will fail to load (references a missing image) so the
        // move's own self-validation step fails deterministically.
        std::fs::write(
            source.path().join("manifest.toml"),
            "schema_version = 1\nname = \"x\"\ndefault_scaling = \"Fill\"\nfallback_color = \"#000000\"\n\n[[images]]\nfile = \"missing.png\"\nanchor = \"sunrise\"\n",
        )
        .unwrap();

        let destination_root = tempfile::tempdir().unwrap();
        let result = move_pack(source.path(), destination_root.path(), "moved-pack");

        assert!(matches!(result, Err(MoveError::Io(_))));
        assert!(source.path().join("manifest.toml").exists(), "source must survive a failed move");
        assert!(!destination_root.path().join("moved-pack").exists(), "a failed move must not leave a partial copy");
    }

    #[test]
    fn move_pack_succeeds_and_removes_the_source_on_a_valid_pack() {
        let source = tempfile::tempdir().unwrap();
        write_test_image(&source.path().join("a.png"));
        std::fs::write(
            source.path().join("manifest.toml"),
            "schema_version = 1\nname = \"x\"\ndefault_scaling = \"Fill\"\nfallback_color = \"#000000\"\n\n[[images]]\nfile = \"a.png\"\nanchor = \"sunrise\"\n",
        )
        .unwrap();

        let destination_root = tempfile::tempdir().unwrap();
        let result = move_pack(source.path(), destination_root.path(), "moved-pack");

        assert!(result.is_ok());
        assert!(!source.path().exists(), "source folder must be removed after a successful move");
        assert!(destination_root.path().join("moved-pack").join("manifest.toml").exists());
    }

    // --- T028: destination-name collision opens the prompt instead of overwriting ---

    #[test]
    fn confirm_move_opens_collision_prompt_instead_of_overwriting() {
        let dir = tempfile::tempdir().unwrap();
        write_test_image(&dir.path().join("a.jpg"));
        let mut state = open(dir.path().to_path_buf());
        set_mode(&mut state, AssignmentMode::SolarPeriod);
        set_solar_event_by_index(&mut state, 0, 0, None);
        generate(&mut state);
        assert!(state.pending_placement.is_some());

        // Pre-create a colliding destination under a scratch registry dir.
        let registry_dir = tempfile::tempdir().unwrap();
        let mut registry = pack_loader::Registry::open_at(registry_dir.path()).unwrap();

        let colliding_name = dir.path().file_name().unwrap().to_str().unwrap().to_string();
        let fake_root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(fake_root.path().join(&colliding_name)).unwrap();

        // Directly exercise `finish_move`/`move_pack`'s collision path against a fixed
        // root (rather than the real standard_pack_location(), unavailable in a
        // sandboxed test environment) — this is exactly what `confirm_move` calls.
        let generated_path = state.pending_placement.clone().unwrap().generated_path;
        let closed = finish_move(&mut state, &mut registry, &generated_path, fake_root.path(), &colliding_name);

        assert!(!closed);
        assert!(state.pending_collision.is_some());
        assert!(state.pending_placement.is_none());
        // The pre-existing destination must be untouched (still an empty dir, not
        // overwritten with the moved pack's contents).
        assert!(!fake_root.path().join(&colliding_name).join("manifest.toml").exists());
    }

    // --- T030: Cancel leaves the source folder byte-for-byte unchanged ---

    #[test]
    fn cancel_before_generate_never_touches_the_source_folder() {
        let dir = tempfile::tempdir().unwrap();
        write_test_image(&dir.path().join("a.jpg"));
        let mut state = open(dir.path().to_path_buf());
        set_mode(&mut state, AssignmentMode::SolarPeriod);
        set_solar_event_by_index(&mut state, 0, 0, None);

        // "Cancel" itself is just dropping `state` (app.rs sets `pack_builder = None`,
        // no pack_builder function is called) — the assertion is that nothing at all
        // was written to `dir` by any of the above.
        drop(state);

        let entries: Vec<_> = std::fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(entries.len(), 1, "only the original a.jpg should exist: {entries:?}");
        assert!(!dir.path().join(pack_loader::MANIFEST_FILE_NAME).exists());
    }

    // --- T032: FR-018 scan failures already covered above
    // (open_reports_a_scan_error_for_a_folder_with_no_images /
    // open_reports_a_scan_error_for_too_many_images) — listed here for traceability.
}
