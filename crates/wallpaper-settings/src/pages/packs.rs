//! Packs page (spec.md FR-001–FR-004, FR-012, FR-018–FR-020,
//! contracts/gui-usability-improvements.md) — browse, add, and remove registered
//! packs, each shown by its resolved name and a thumbnail preview rather than a raw
//! file path (spec 008 US1/US6; supersedes spec 7's "registration is out of scope"
//! note).

use std::path::PathBuf;

use cosmic::widget;
use cosmic::Element;
use pack_loader::{PackRegistryEntry, PackSource, Registry, RegistryStatus};

use crate::pack_display;

/// One row in the Packs page's list — enough to identify a pack, show its author at a
/// glance, and preview it (spec.md Acceptance Scenario 1, FR-018).
#[derive(Debug, Clone, PartialEq)]
pub struct PackRow {
    pub name: String,
    pub source: PackSource,
    pub status: &'static str,
    pub author: String,
    pub thumbnail: Option<PathBuf>,
}

/// Fixed column widths for the Name/Author/Thumbnail columns, so every row's cells
/// line up with the header above them.
const NAME_COLUMN_WIDTH: u16 = 180;
const AUTHOR_COLUMN_WIDTH: u16 = 140;
const THUMBNAIL_COLUMN_WIDTH: u16 = 48;

/// Character budgets the Name/Author columns truncate to (with `truncate_with_ellipsis`)
/// — sized to comfortably fit within the column widths above.
const NAME_MAX_CHARS: usize = 26;
const AUTHOR_MAX_CHARS: usize = 20;

/// The placeholder shown for a pack with no declared author, or one that can't be
/// loaded at all (FR-011) — mirrors `pack_display::resolve_pack_name`'s "(unnamed
/// pack)" placeholder for the same two cases.
const UNKNOWN_AUTHOR: &str = "(unknown)";

/// Pure mapping from a registry listing to display rows — independent of `libcosmic`
/// rendering, so this stays a plain, fast unit test. Name, author, and thumbnail all
/// come from `pack_display` (spec 008 research.md R2/R7), falling back to a
/// clearly-labeled placeholder (FR-011) when a source can't be loaded, rather than a
/// raw path.
pub fn rows_from_registry(entries: &[PackRegistryEntry]) -> Vec<PackRow> {
    entries
        .iter()
        .map(|entry| {
            let status = match entry.status {
                RegistryStatus::Known => "known",
                RegistryStatus::Unavailable => "unavailable",
            };
            let name = pack_display::resolve_pack_name(&entry.source)
                .unwrap_or_else(|| "(unnamed pack)".to_string());
            let author = pack_display::resolve_pack_author(&entry.source)
                .unwrap_or_else(|| UNKNOWN_AUTHOR.to_string());
            let thumbnail = pack_display::resolve_thumbnail_path(&entry.source);
            PackRow { name, source: entry.source.clone(), status, author, thumbnail }
        })
        .collect()
}

/// Truncates `s` to at most `max_chars` characters, replacing the tail with an
/// ellipsis when it doesn't fit — keeps the Name/Author columns from stretching the
/// page out when a pack declares a long one.
fn truncate_with_ellipsis(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let mut truncated: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        truncated.push('…');
        truncated
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Refresh,
    /// "Add pack folder…" button — `app.rs` opens a folder picker (research.md R1).
    AddFolderRequested,
    /// "Add image file…" button — `app.rs` opens a file picker (research.md R1).
    AddFileRequested,
    /// The file-chooser `Task` resolved with a real selection: `Ok` on success, `Err`
    /// with a specific, shown reason otherwise (FR-003).
    AddResult(Result<PathBuf, String>),
    /// The file-chooser dialog was cancelled — a no-op, not an error (research.md R1).
    AddCancelled,
    /// A row's "Remove" button — opens the confirmation dialog, no removal yet
    /// (FR-002, research.md R3).
    RemoveRequested(PackSource),
    /// The confirmation dialog's primary action.
    RemoveConfirmed,
    /// The confirmation dialog's secondary action, or dismissing it.
    RemoveCancelled,
}

pub struct State {
    pub rows: Vec<PackRow>,
    /// Set while the removal confirmation dialog is open (research.md R3,
    /// data-model.md's state machine). `None` = no dialog shown.
    pub pending_removal: Option<PackSource>,
    /// The most recent add attempt's failure, if any (FR-003). Cleared on the next add
    /// attempt or a successful one.
    pub add_error: Option<String>,
    /// `true` while a file-chooser dialog spawned by "Add pack folder…"/"Add image
    /// file…" is in flight — spec 011 US8 FR-052: guards against a rapid double-click
    /// opening two concurrent dialogs (the desktop portal's file chooser has no
    /// built-in single-instance guard of its own). Cleared on `AddResult`/
    /// `AddCancelled`, whichever comes back first.
    pub dialog_in_flight: bool,
}

impl State {
    pub fn load(registry: &mut Registry) -> Self {
        Self { rows: rows_from_registry(&registry.known_packs()), pending_removal: None, add_error: None, dialog_in_flight: false }
    }
}

/// `AddFolderRequested`/`AddFileRequested`: returns `true` (clearing any previous
/// error and setting the in-flight guard) only if no file-chooser dialog is already in
/// flight; `false` means a dialog is already open and the caller must not spawn a
/// second one (spec 011 US8 FR-052).
#[must_use]
pub fn request_add(state: &mut State) -> bool {
    if state.dialog_in_flight {
        return false;
    }
    state.dialog_in_flight = true;
    state.add_error = None;
    true
}

/// `AddCancelled`: the file-chooser dialog was cancelled — clears the in-flight guard
/// (FR-052) so the next click can open a fresh dialog.
pub fn cancel_add(state: &mut State) {
    state.dialog_in_flight = false;
}

/// `AddResult`: resolves and registers on success (identical call `wallpaperctl
/// register` makes — FR-003), records a specific error otherwise, never partially
/// registers (data-model.md). Always clears the in-flight guard (FR-052) — the dialog
/// this result came from is no longer open regardless of outcome.
pub fn apply_add_result(state: &mut State, registry: &mut Registry, result: Result<PathBuf, String>) {
    state.dialog_in_flight = false;
    let outcome = result.and_then(|path| {
        PackSource::resolve(&path)
            .map_err(|e| e.to_string())
            .and_then(|source| registry.register(source).map_err(|e| e.to_string()))
    });
    match outcome {
        Ok(()) => {
            state.add_error = None;
            state.rows = rows_from_registry(&registry.known_packs());
        }
        Err(reason) => state.add_error = Some(reason),
    }
}

/// `RemoveRequested`: opens the confirmation dialog — no registry change yet.
pub fn request_removal(state: &mut State, source: PackSource) {
    state.pending_removal = Some(source);
}

/// `RemoveConfirmed`: removes the pending source (identical call `wallpaperctl remove`
/// makes — FR-004) and refreshes `rows`; a no-op if nothing was pending.
pub fn confirm_removal(state: &mut State, registry: &mut Registry) {
    if let Some(source) = state.pending_removal.take() {
        let _ = registry.remove(&source);
        state.rows = rows_from_registry(&registry.known_packs());
    }
}

/// `RemoveCancelled`: closes the dialog with no registry change.
pub fn cancel_removal(state: &mut State) {
    state.pending_removal = None;
}

pub fn view(state: &State) -> Element<'_, Message> {
    // Spec 011 US8 FR-052: disabled (not just guarded in `update`) while a dialog is
    // already in flight, so a rapid double-click can't even queue a second press.
    let add_folder_message = if state.dialog_in_flight { None } else { Some(Message::AddFolderRequested) };
    let add_file_message = if state.dialog_in_flight { None } else { Some(Message::AddFileRequested) };
    let add_row = widget::row::with_capacity(3)
        .spacing(cosmic::theme::spacing().space_xs)
        .push(widget::button::standard("Add pack folder…").on_press_maybe(add_folder_message))
        .push(widget::button::standard("Add image file…").on_press_maybe(add_file_message))
        .push(widget::button::standard("Refresh").on_press(Message::Refresh));

    let mut top = widget::column::with_capacity(2).push(add_row);
    if let Some(reason) = &state.add_error {
        top = top.push(widget::text::body(format!("Couldn't add pack: {reason}")));
    }

    let mut section = widget::settings::section().title("Registered packs");
    if state.rows.is_empty() {
        section = section.add(widget::text::body("No packs registered yet. Add one above."));
    } else {
        // Labeled Name/Author/Thumbnail columns (status is intentionally not shown
        // here), spaced apart so the row doesn't feel crowded; Name and Author are
        // fixed-width and truncated with an ellipsis so a long one can't stretch the
        // page or crowd its neighbors.
        let header = widget::row::with_capacity(3)
            .spacing(cosmic::theme::spacing().space_m)
            .push(widget::text::caption_heading("Name").width(NAME_COLUMN_WIDTH))
            .push(widget::text::caption_heading("Author").width(AUTHOR_COLUMN_WIDTH))
            .push(widget::text::caption_heading("Thumbnail").width(THUMBNAIL_COLUMN_WIDTH));
        section = section.add(header);

        for row in &state.rows {
            let thumbnail: Element<'_, Message> = match &row.thumbnail {
                Some(path) => widget::image(path.clone()).width(THUMBNAIL_COLUMN_WIDTH).height(THUMBNAIL_COLUMN_WIDTH).into(),
                None => widget::text::caption("(no preview available)").into(),
            };
            let detail = widget::row::with_capacity(4)
                .spacing(cosmic::theme::spacing().space_m)
                .align_y(cosmic::iced::Alignment::Center)
                .push(widget::text::body(truncate_with_ellipsis(&row.name, NAME_MAX_CHARS)).width(NAME_COLUMN_WIDTH))
                .push(widget::text::body(truncate_with_ellipsis(&row.author, AUTHOR_MAX_CHARS)).width(AUTHOR_COLUMN_WIDTH))
                .push(thumbnail)
                .push(widget::button::destructive("Remove").on_press(Message::RemoveRequested(row.source.clone())));
            section = section.add(detail);
        }
    }
    widget::scrollable(widget::column::with_capacity(2).push(top).push(section)).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &std::path::Path, status: RegistryStatus) -> PackRegistryEntry {
        PackRegistryEntry {
            source: PackSource::StaticFile(path.to_path_buf()),
            status,
            origin: pack_loader::PackOrigin::User,
        }
    }

    fn temp_registry() -> (Registry, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let registry = Registry::open_at(dir.path()).unwrap();
        (registry, dir)
    }

    /// Preserves the original "browse registered packs" coverage, now against names/
    /// thumbnails instead of raw paths. A static-file pack has no manifest, so its
    /// author is always the placeholder.
    #[test]
    fn maps_registry_entries_to_rows_with_status_and_name() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("sunrise.png");
        image::RgbImage::new(2, 2).save(&file).unwrap();
        let entries = vec![entry(&file, RegistryStatus::Known)];
        let rows = rows_from_registry(&entries);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, "known");
        assert_eq!(rows[0].name, "sunrise");
        assert_eq!(rows[0].author, UNKNOWN_AUTHOR);
        assert_eq!(rows[0].thumbnail.as_deref(), Some(file.as_path()));
    }

    #[test]
    fn a_source_that_fails_to_load_falls_back_to_the_placeholder_name_and_author() {
        let entries = vec![entry(std::path::Path::new("/does/not/exist.png"), RegistryStatus::Unavailable)];
        let rows = rows_from_registry(&entries);

        assert_eq!(rows[0].name, "(unnamed pack)");
        assert_eq!(rows[0].author, UNKNOWN_AUTHOR);
        assert_eq!(rows[0].thumbnail, None);
    }

    #[test]
    fn truncate_with_ellipsis_leaves_short_strings_untouched() {
        assert_eq!(truncate_with_ellipsis("sunrise", 26), "sunrise");
        assert_eq!(truncate_with_ellipsis("exactly-ten", 11), "exactly-ten");
    }

    #[test]
    fn truncate_with_ellipsis_shortens_and_marks_long_strings() {
        let truncated = truncate_with_ellipsis("a very long pack name indeed", 10);
        assert_eq!(truncated, "a very lo…");
        assert_eq!(truncated.chars().count(), 10, "stays within the character budget, ellipsis included");
    }

    #[test]
    fn empty_registry_maps_to_no_rows() {
        assert!(rows_from_registry(&[]).is_empty());
    }

    /// Spec 011 US8 FR-052: a second `request_add` call while a dialog is already in
    /// flight must be refused (`false`), not silently allowed to open a second dialog
    /// — the audit's own reproduction was a rapid double-click on "Add pack folder…".
    #[test]
    fn request_add_refuses_a_second_call_while_a_dialog_is_in_flight() {
        let mut state = State { rows: vec![], pending_removal: None, add_error: None, dialog_in_flight: false };

        assert!(request_add(&mut state), "first call must be allowed to open a dialog");
        assert!(state.dialog_in_flight);

        assert!(!request_add(&mut state), "a dialog is already in flight — must refuse a second one");
        assert!(state.dialog_in_flight, "still in flight — the refused call must not have cleared it");
    }

    /// `AddCancelled` clears the guard so a subsequent click can open a fresh dialog.
    #[test]
    fn cancel_add_clears_the_in_flight_guard() {
        let mut state = State { rows: vec![], pending_removal: None, add_error: None, dialog_in_flight: true };

        cancel_add(&mut state);
        assert!(!state.dialog_in_flight);
        assert!(request_add(&mut state), "guard cleared — a new dialog request must be allowed");
    }

    /// `AddResult` clears the guard regardless of success or failure — the dialog that
    /// produced this result is no longer open either way.
    #[test]
    fn apply_add_result_clears_the_in_flight_guard_on_both_success_and_failure() {
        let (mut registry, dir) = temp_registry();
        let mut state = State { rows: vec![], pending_removal: None, add_error: None, dialog_in_flight: true };
        let file = dir.path().join("sunrise.png");
        image::RgbImage::new(2, 2).save(&file).unwrap();

        apply_add_result(&mut state, &mut registry, Ok(file));
        assert!(!state.dialog_in_flight, "a successful result must clear the guard");

        state.dialog_in_flight = true;
        apply_add_result(&mut state, &mut registry, Err("boom".to_string()));
        assert!(!state.dialog_in_flight, "a failed result must also clear the guard");
    }

    /// T003: a successful add registers the pack and refreshes `rows`; re-adding the
    /// same path is idempotent (spec.md Acceptance Scenarios 1, 4).
    #[test]
    fn add_result_ok_registers_and_refreshes_rows() {
        let (mut registry, dir) = temp_registry();
        let mut state = State { rows: vec![], pending_removal: None, add_error: None, dialog_in_flight: false };
        let file = dir.path().join("sunrise.png");
        image::RgbImage::new(2, 2).save(&file).unwrap();

        apply_add_result(&mut state, &mut registry, Ok(file.clone()));
        assert_eq!(state.rows.len(), 1);
        assert_eq!(state.add_error, None);

        // Idempotent re-add.
        apply_add_result(&mut state, &mut registry, Ok(file));
        assert_eq!(state.rows.len(), 1);
    }

    /// T004: a failed add leaves `rows` unchanged and records a specific reason — no
    /// partial registration (spec.md Acceptance Scenario 3).
    #[test]
    fn add_result_err_leaves_rows_unchanged_and_sets_add_error() {
        let (mut registry, _dir) = temp_registry();
        let mut state = State { rows: vec![], pending_removal: None, add_error: None, dialog_in_flight: false };

        apply_add_result(&mut state, &mut registry, Err("malformed manifest".to_string()));
        assert!(state.rows.is_empty());
        assert_eq!(state.add_error.as_deref(), Some("malformed manifest"));
    }

    /// A path that fails `PackSource::resolve` (e.g. doesn't exist) is also a specific
    /// error, not a panic or a silent no-op.
    #[test]
    fn add_result_ok_with_an_unresolvable_path_sets_add_error() {
        let (mut registry, dir) = temp_registry();
        let mut state = State { rows: vec![], pending_removal: None, add_error: None, dialog_in_flight: false };

        apply_add_result(&mut state, &mut registry, Ok(dir.path().join("never-created.png")));
        assert!(state.rows.is_empty());
        assert!(state.add_error.is_some());
    }

    /// T005: the removal state machine (data-model.md).
    #[test]
    fn removal_state_machine_matches_the_documented_transitions() {
        let (mut registry, dir) = temp_registry();
        let file = dir.path().join("sunrise.png");
        image::RgbImage::new(2, 2).save(&file).unwrap();
        let source = PackSource::resolve(&file).unwrap();
        registry.register(source.clone()).unwrap();
        let mut state = State::load(&mut registry);
        assert_eq!(state.rows.len(), 1);

        request_removal(&mut state, source.clone());
        assert_eq!(state.pending_removal, Some(source.clone()));
        assert_eq!(state.rows.len(), 1, "no removal yet, just opened the dialog");

        cancel_removal(&mut state);
        assert_eq!(state.pending_removal, None);
        assert_eq!(state.rows.len(), 1, "cancelling makes no registry change");

        request_removal(&mut state, source);
        confirm_removal(&mut state, &mut registry);
        assert_eq!(state.pending_removal, None);
        assert!(state.rows.is_empty());
    }

    #[test]
    fn confirm_removal_with_nothing_pending_is_a_harmless_noop() {
        let (mut registry, _dir) = temp_registry();
        let mut state = State { rows: vec![], pending_removal: None, add_error: None, dialog_in_flight: false };

        confirm_removal(&mut state, &mut registry);
        assert_eq!(state.pending_removal, None);
        assert!(state.rows.is_empty());
    }
}
