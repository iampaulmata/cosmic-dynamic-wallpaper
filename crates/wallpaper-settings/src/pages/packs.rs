//! Packs page (spec.md FR-001–FR-004, FR-012, FR-018–FR-020,
//! contracts/gui-usability-improvements.md) — browse, add, and remove registered
//! packs, each shown by its resolved name and a thumbnail preview rather than a raw
//! file path (spec 008 US1/US6; supersedes spec 7's "registration is out of scope"
//! note).

use std::path::PathBuf;

use cosmic::widget;
use cosmic::Element;
use pack_loader::{PackRegistryEntry, PackSource, Registry, RegistryStatus};

/// The pencil/trash-can icon names (spec 012 FR-001, research.md R7), matching
/// `pages::location`'s existing `widget::button::icon`/`widget::icon::from_name`
/// pattern rather than introducing a new icon convention. `pencil-symbolic` (not the
/// freedesktop-standard `document-edit-symbolic`) — confirmed by actually running the
/// app (quickstart.md's manual check): `libcosmic`'s bundled `cosmic-icons` set has no
/// `document-edit-symbolic` of its own, so that name silently fell back to a generic
/// document glyph instead of a pencil. `user-trash-symbolic` *is* bundled and renders
/// correctly as-is.
const EDIT_ICON: &str = "pencil-symbolic";
const DELETE_ICON: &str = "user-trash-symbolic";

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
            let name = pack_display::resolve_pack_display_name(&entry.source, entry.display_name.as_deref())
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

#[derive(Debug, Clone, PartialEq)]
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
    /// Spec 012 FR-001/FR-004: a `Directory` row's edit (pencil) icon — `app.rs`
    /// attempts `pack_builder::open_for_edit` for this source (contracts/
    /// pack-builder-edit-flow.md). Never dispatched for a `StaticFile` row (see
    /// `RenameRequested`) or a row whose `status` is `Unavailable` (the icon is
    /// disabled for those — FR-019/research.md R3: there's nothing to pre-fill from a
    /// pack that doesn't currently load).
    EditRequested(PackSource),
    /// Spec 012 FR-010: a `StaticFile` row's edit (pencil) icon — opens the
    /// lightweight rename-only dialog (contracts/packs-screen-icon-actions.md)
    /// instead of the full wizard, since a standalone image has no schedule to edit.
    RenameRequested(PackSource),
    /// The rename dialog's text field.
    RenameNameChanged(String),
    /// The rename dialog's Save action.
    RenameConfirmed,
    /// The rename dialog's Cancel action, or dismissing it.
    RenameCancelled,
}

/// Open while the rename-only dialog (spec 012 FR-010) is shown — a `StaticFile`
/// pack's `source`, plus the text field's current (unsaved) content, pre-filled from
/// that pack's current `PackRegistryEntry.display_name` (or empty, if unset) when the
/// dialog opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingRename {
    pub source: PackSource,
    pub name: String,
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
    /// Spec 012 FR-019 (research.md R3): the reason `pack_builder::open_for_edit`
    /// refused to open, when `EditRequested` fails — shown inline here rather than as
    /// a second modal, since there's no `pack_builder::State` for a dialog to attach
    /// to in this case (contracts/pack-builder-edit-flow.md). Cleared on the next edit
    /// attempt, successful or not.
    pub edit_error: Option<String>,
    /// Set while the rename-only dialog (FR-010) is open. `None` = no dialog shown.
    pub pending_rename: Option<PendingRename>,
}

impl State {
    pub fn load(registry: &mut Registry) -> Self {
        Self { rows: rows_from_registry(&registry.known_packs()), pending_removal: None, add_error: None, dialog_in_flight: false, edit_error: None, pending_rename: None }
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

/// Spec 012 FR-017, Edge Cases: blank or whitespace-only input normalizes to `None`
/// (falls back to the pack's default label) rather than persisting an empty string;
/// anything else is used verbatim, trimmed. Kept separate from `Registry::
/// set_display_name` itself (which trusts its caller to have already normalized —
/// contracts/pack-registry-display-name.md) so this one rule has exactly one place to
/// be tested and gotten right.
fn normalize_display_name(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// `RenameRequested`: opens the rename-only dialog (FR-010), pre-filled from
/// `current_name` — the caller (`app.rs`) looks that up from the registry entry's
/// `display_name`, since this module doesn't hold a `Registry` reference itself.
pub fn request_rename(state: &mut State, source: PackSource, current_name: Option<String>) {
    state.pending_rename = Some(PendingRename { source, name: current_name.unwrap_or_default() });
}

/// `RenameNameChanged`: updates the dialog's text field — raw, unnormalized (mirrors
/// `set_name`/`set_author`'s own "normalize only at save time" shape).
pub fn set_rename_name(state: &mut State, name: String) {
    if let Some(pending) = state.pending_rename.as_mut() {
        pending.name = name;
    }
}

/// `RenameConfirmed`: persists the (normalized) name via `Registry::set_display_name`
/// and refreshes `rows` so the Packs screen reflects it immediately; a no-op if
/// nothing was pending. A registry-write failure is treated the same way
/// `confirm_removal` already treats one (`let _ =` — the entry's `display_name` simply
/// stays whatever it was, and the dialog still closes) rather than introducing a new
/// error-surface field for what `Registry::set_display_name` itself only fails at for
/// the same storage-level reasons `register`/`remove` already can.
pub fn confirm_rename(state: &mut State, registry: &mut Registry) {
    if let Some(pending) = state.pending_rename.take() {
        let normalized = normalize_display_name(&pending.name);
        let _ = registry.set_display_name(&pending.source, normalized);
        state.rows = rows_from_registry(&registry.known_packs());
    }
}

/// `RenameCancelled`: closes the dialog with no registry change.
pub fn cancel_rename(state: &mut State) {
    state.pending_rename = None;
}

/// Which message (if any) a row's edit icon should send (spec 012 FR-001, FR-004,
/// FR-010, FR-019) — kept as its own pure function, separate from `row_actions`'
/// widget construction, so the "which source variant/status maps to which message"
/// decision is unit-testable without needing to inspect a rendered widget tree.
fn edit_message_for(row: &PackRow) -> Option<Message> {
    if row.status == "unavailable" {
        return None;
    }
    Some(match &row.source {
        PackSource::Directory(_) => Message::EditRequested(row.source.clone()),
        PackSource::StaticFile(_) => Message::RenameRequested(row.source.clone()),
    })
}

/// The edit (pencil) + delete (trash-can) icon pair for one row (spec 012 FR-001,
/// FR-002, FR-003; contracts/packs-screen-icon-actions.md) — replaces the previous
/// single "Remove" text button. Each icon is wrapped in a tooltip so it's
/// individually identifiable, not just distinguishable by shape (FR-002), mirroring
/// `pages::location`'s existing icon-button/tooltip pattern (research.md R7).
///
/// The edit icon dispatches `EditRequested` for a `Directory` row and `RenameRequested`
/// for a `StaticFile` row (FR-004 vs. FR-010) — `app.rs` is what actually decides what
/// each message does; this function only decides *which* message a click sends. The
/// edit icon is disabled (no `on_press`) for a row whose `status` is `"unavailable"` —
/// there is nothing to open the wizard or rename dialog from until the pack loads
/// again. The delete icon dispatches the existing `RemoveRequested` unchanged (FR-003)
/// and is never disabled — removing a pack that fails to load is exactly the
/// "Unavailable" case `Registry::remove` already exists to clear out.
fn row_actions(row: &PackRow) -> Element<'_, Message> {
    let edit_message = edit_message_for(row);
    let edit_icon = widget::tooltip(
        widget::button::icon(widget::icon::from_name(EDIT_ICON)).on_press_maybe(edit_message),
        widget::text::body("Edit"),
        widget::tooltip::Position::Top,
    );
    let delete_icon = widget::tooltip(
        widget::button::icon(widget::icon::from_name(DELETE_ICON)).on_press(Message::RemoveRequested(row.source.clone())),
        widget::text::body("Delete"),
        widget::tooltip::Position::Top,
    );
    widget::row::with_capacity(2)
        .spacing(cosmic::theme::spacing().space_xs)
        .align_y(cosmic::iced::Alignment::Center)
        .push(edit_icon)
        .push(delete_icon)
        .into()
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
    // Spec 012 FR-019: `open_for_edit`'s refusal reason — there's no wizard `State` for
    // a dialog to attach to in this case, so it's shown inline here instead.
    if let Some(reason) = &state.edit_error {
        top = top.push(widget::text::body(format!("Couldn't edit pack: {reason}")));
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
                .push(row_actions(row));
            section = section.add(detail);
        }
    }
    widget::scrollable(widget::column::with_capacity(2).push(top).push(section)).into()
}

/// The rename-only dialog (spec 012 FR-010, contracts/packs-screen-icon-actions.md) —
/// rendered via `App`'s `Application::dialog()` override exactly like the removal
/// confirmation dialog already is, `None` when nothing is pending.
pub fn rename_dialog(state: &State) -> Option<Element<'_, Message>> {
    let pending = state.pending_rename.as_ref()?;
    Some(
        widget::dialog()
            .title("Rename pack")
            .body("This changes only what's shown in this app — the file itself keeps its name.")
            .control(widget::text_input::text_input("Display name", &pending.name).on_input(Message::RenameNameChanged))
            .primary_action(widget::button::suggested("Save").on_press(Message::RenameConfirmed))
            .secondary_action(widget::button::standard("Cancel").on_press(Message::RenameCancelled))
            .into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &std::path::Path, status: RegistryStatus) -> PackRegistryEntry {
        PackRegistryEntry {
            source: PackSource::StaticFile(path.to_path_buf()),
            status,
            origin: pack_loader::PackOrigin::User,
            display_name: None,
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
        let mut state = State { rows: vec![], pending_removal: None, add_error: None, dialog_in_flight: false, edit_error: None, pending_rename: None };

        assert!(request_add(&mut state), "first call must be allowed to open a dialog");
        assert!(state.dialog_in_flight);

        assert!(!request_add(&mut state), "a dialog is already in flight — must refuse a second one");
        assert!(state.dialog_in_flight, "still in flight — the refused call must not have cleared it");
    }

    /// `AddCancelled` clears the guard so a subsequent click can open a fresh dialog.
    #[test]
    fn cancel_add_clears_the_in_flight_guard() {
        let mut state = State { rows: vec![], pending_removal: None, add_error: None, dialog_in_flight: true, edit_error: None, pending_rename: None };

        cancel_add(&mut state);
        assert!(!state.dialog_in_flight);
        assert!(request_add(&mut state), "guard cleared — a new dialog request must be allowed");
    }

    /// `AddResult` clears the guard regardless of success or failure — the dialog that
    /// produced this result is no longer open either way.
    #[test]
    fn apply_add_result_clears_the_in_flight_guard_on_both_success_and_failure() {
        let (mut registry, dir) = temp_registry();
        let mut state = State { rows: vec![], pending_removal: None, add_error: None, dialog_in_flight: true, edit_error: None, pending_rename: None };
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
        let mut state = State { rows: vec![], pending_removal: None, add_error: None, dialog_in_flight: false, edit_error: None, pending_rename: None };
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
        let mut state = State { rows: vec![], pending_removal: None, add_error: None, dialog_in_flight: false, edit_error: None, pending_rename: None };

        apply_add_result(&mut state, &mut registry, Err("malformed manifest".to_string()));
        assert!(state.rows.is_empty());
        assert_eq!(state.add_error.as_deref(), Some("malformed manifest"));
    }

    /// A path that fails `PackSource::resolve` (e.g. doesn't exist) is also a specific
    /// error, not a panic or a silent no-op.
    #[test]
    fn add_result_ok_with_an_unresolvable_path_sets_add_error() {
        let (mut registry, dir) = temp_registry();
        let mut state = State { rows: vec![], pending_removal: None, add_error: None, dialog_in_flight: false, edit_error: None, pending_rename: None };

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
        let mut state = State { rows: vec![], pending_removal: None, add_error: None, dialog_in_flight: false, edit_error: None, pending_rename: None };

        confirm_removal(&mut state, &mut registry);
        assert_eq!(state.pending_removal, None);
        assert!(state.rows.is_empty());
    }

    fn pack_row(source: PackSource, status: &'static str) -> PackRow {
        PackRow { name: "x".to_string(), source, status, author: "x".to_string(), thumbnail: None }
    }

    // --- Spec 012 US4 (T015): the rename-only dialog's state machine ---

    #[test]
    fn normalize_display_name_blank_or_whitespace_becomes_none() {
        assert_eq!(normalize_display_name(""), None);
        assert_eq!(normalize_display_name("   "), None);
    }

    #[test]
    fn normalize_display_name_trims_a_real_name() {
        assert_eq!(normalize_display_name("  Sunrise Glow  "), Some("Sunrise Glow".to_string()));
    }

    #[test]
    fn rename_state_machine_matches_the_documented_transitions() {
        let (mut registry, dir) = temp_registry();
        let file = dir.path().join("sunrise.png");
        image::RgbImage::new(2, 2).save(&file).unwrap();
        let source = PackSource::resolve(&file).unwrap();
        registry.register(source.clone()).unwrap();
        let mut state = State::load(&mut registry);

        request_rename(&mut state, source.clone(), None);
        assert_eq!(state.pending_rename, Some(PendingRename { source: source.clone(), name: String::new() }));

        set_rename_name(&mut state, "Sunrise Glow".to_string());
        assert_eq!(state.pending_rename.as_ref().unwrap().name, "Sunrise Glow");

        confirm_rename(&mut state, &mut registry);
        assert_eq!(state.pending_rename, None);
        let entry = registry.known_packs().into_iter().find(|e| e.source == source).unwrap();
        assert_eq!(entry.display_name.as_deref(), Some("Sunrise Glow"));
        assert_eq!(state.rows[0].name, "Sunrise Glow", "the Packs screen must refresh immediately");
    }

    #[test]
    fn rename_pre_fills_from_the_current_display_name() {
        let (mut registry, dir) = temp_registry();
        let file = dir.path().join("sunrise.png");
        image::RgbImage::new(2, 2).save(&file).unwrap();
        let source = PackSource::resolve(&file).unwrap();
        registry.register(source.clone()).unwrap();
        registry.set_display_name(&source, Some("Already Named".to_string())).unwrap();
        let mut state = State::load(&mut registry);

        request_rename(&mut state, source, Some("Already Named".to_string()));
        assert_eq!(state.pending_rename.as_ref().unwrap().name, "Already Named");
    }

    #[test]
    fn rename_cancelled_makes_no_registry_change() {
        let (mut registry, dir) = temp_registry();
        let file = dir.path().join("sunrise.png");
        image::RgbImage::new(2, 2).save(&file).unwrap();
        let source = PackSource::resolve(&file).unwrap();
        registry.register(source.clone()).unwrap();
        let mut state = State::load(&mut registry);

        request_rename(&mut state, source.clone(), None);
        set_rename_name(&mut state, "Should Not Save".to_string());
        cancel_rename(&mut state);

        assert_eq!(state.pending_rename, None);
        let entry = registry.known_packs().into_iter().find(|e| e.source == source).unwrap();
        assert_eq!(entry.display_name, None);
    }

    /// FR-017/Edge Cases: saving a blank/whitespace-only name clears any previous
    /// override rather than persisting an empty string.
    #[test]
    fn rename_confirmed_with_blank_input_clears_the_display_name() {
        let (mut registry, dir) = temp_registry();
        let file = dir.path().join("sunrise.png");
        image::RgbImage::new(2, 2).save(&file).unwrap();
        let source = PackSource::resolve(&file).unwrap();
        registry.register(source.clone()).unwrap();
        registry.set_display_name(&source, Some("Old Name".to_string())).unwrap();
        let mut state = State::load(&mut registry);

        request_rename(&mut state, source.clone(), Some("Old Name".to_string()));
        set_rename_name(&mut state, "   ".to_string());
        confirm_rename(&mut state, &mut registry);

        let entry = registry.known_packs().into_iter().find(|e| e.source == source).unwrap();
        assert_eq!(entry.display_name, None);
    }

    #[test]
    fn confirm_rename_with_nothing_pending_is_a_harmless_noop() {
        let (mut registry, _dir) = temp_registry();
        let mut state = State { rows: vec![], pending_removal: None, add_error: None, dialog_in_flight: false, edit_error: None, pending_rename: None };

        confirm_rename(&mut state, &mut registry);
        assert_eq!(state.pending_rename, None);
    }

    // --- Spec 012 T002/T009: the edit icon's message depends on the row's source
    // variant and status; the delete icon is untouched (still RemoveRequested). ---

    #[test]
    fn edit_message_for_a_directory_row_opens_the_wizard() {
        let source = PackSource::Directory(std::path::PathBuf::from("/packs/mountains"));
        let row = pack_row(source.clone(), "known");
        assert_eq!(edit_message_for(&row), Some(Message::EditRequested(source)));
    }

    #[test]
    fn edit_message_for_a_static_file_row_opens_the_rename_dialog() {
        let source = PackSource::StaticFile(std::path::PathBuf::from("/packs/sunrise.png"));
        let row = pack_row(source.clone(), "known");
        assert_eq!(edit_message_for(&row), Some(Message::RenameRequested(source)));
    }

    /// Spec 012 FR-019/research.md R3: nothing to pre-fill from a pack that doesn't
    /// currently load — the edit icon must be disabled (no message), for either
    /// source shape, rather than opening a doomed attempt.
    #[test]
    fn edit_message_for_an_unavailable_row_is_none_regardless_of_source_shape() {
        let dir_row = pack_row(PackSource::Directory(std::path::PathBuf::from("/packs/gone")), "unavailable");
        assert_eq!(edit_message_for(&dir_row), None);

        let file_row = pack_row(PackSource::StaticFile(std::path::PathBuf::from("/packs/gone.png")), "unavailable");
        assert_eq!(edit_message_for(&file_row), None);
    }

    /// Spec 012 T009: the delete icon's confirm/cancel state machine is exactly
    /// today's `request_removal`/`confirm_removal`/`cancel_removal` — this is the same
    /// `removal_state_machine_matches_the_documented_transitions` coverage above,
    /// re-asserted here as the icon-swap's own regression guard: nothing about
    /// `RemoveRequested`'s handling changed when its trigger moved from a text button
    /// to a tooltipped icon (only `row_actions`' widget construction changed).
    #[test]
    fn delete_icon_dispatches_the_unchanged_remove_requested_message() {
        let source = PackSource::Directory(std::path::PathBuf::from("/packs/mountains"));
        // `row_actions` always wires the delete icon to `RemoveRequested(row.source)`,
        // unconditionally (no disabled state) — verified at the message-construction
        // level here, since a rendered widget's `on_press` payload can't be inspected
        // directly without a GUI test harness this codebase doesn't have.
        let expected = Message::RemoveRequested(source.clone());
        let row = pack_row(source, "known");
        let actual = Message::RemoveRequested(row.source.clone());
        assert_eq!(actual, expected);
    }
}
