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

/// One row in the Packs page's list — enough to identify a pack, show its
/// reachability at a glance, and preview it (spec.md Acceptance Scenario 1, FR-018).
#[derive(Debug, Clone, PartialEq)]
pub struct PackRow {
    pub name: String,
    pub source: PackSource,
    pub status: &'static str,
    pub thumbnail: Option<PathBuf>,
}

/// Pure mapping from a registry listing to display rows — independent of `libcosmic`
/// rendering, so this stays a plain, fast unit test. Name and thumbnail both come from
/// `pack_display` (spec 008 research.md R2/R7), falling back to a clearly-labeled
/// placeholder (FR-011) when a source can't be loaded, rather than a raw path.
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
            let thumbnail = pack_display::resolve_thumbnail_path(&entry.source);
            PackRow { name, source: entry.source.clone(), status, thumbnail }
        })
        .collect()
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
}

impl State {
    pub fn load(registry: &mut Registry) -> Self {
        Self { rows: rows_from_registry(&registry.known_packs()), pending_removal: None, add_error: None }
    }
}

/// `AddFolderRequested`/`AddFileRequested`: a fresh attempt clears any previous error
/// (data-model.md).
pub fn request_add(state: &mut State) {
    state.add_error = None;
}

/// `AddResult`: resolves and registers on success (identical call `wallpaperctl
/// register` makes — FR-003), records a specific error otherwise, never partially
/// registers (data-model.md).
pub fn apply_add_result(state: &mut State, registry: &mut Registry, result: Result<PathBuf, String>) {
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
    let add_row = widget::row::with_capacity(2)
        .spacing(cosmic::theme::spacing().space_xs)
        .push(widget::button::standard("Add pack folder…").on_press(Message::AddFolderRequested))
        .push(widget::button::standard("Add image file…").on_press(Message::AddFileRequested));

    let mut top = widget::column::with_capacity(2).push(add_row);
    if let Some(reason) = &state.add_error {
        top = top.push(widget::text::body(format!("Couldn't add pack: {reason}")));
    }

    let mut section = widget::settings::section().title("Registered packs");
    if state.rows.is_empty() {
        section = section.add(widget::text::body("No packs registered yet. Add one above."));
    } else {
        for row in &state.rows {
            let thumbnail: Element<'_, Message> = match &row.thumbnail {
                Some(path) => widget::image(path.clone()).width(48).height(48).into(),
                None => widget::text::caption("(no preview available)").into(),
            };
            let detail = widget::row::with_capacity(3)
                .spacing(cosmic::theme::spacing().space_xs)
                .push(thumbnail)
                .push(widget::text::body(format!("status: {}", row.status)))
                .push(widget::button::destructive("Remove").on_press(Message::RemoveRequested(row.source.clone())));
            section = section.add(widget::settings::item(row.name.clone(), detail));
        }
    }
    let refresh = widget::button::standard("Refresh").on_press(Message::Refresh);
    widget::scrollable(widget::column::with_capacity(3).push(top).push(refresh).push(section)).into()
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
    /// thumbnails instead of raw paths.
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
        assert_eq!(rows[0].thumbnail.as_deref(), Some(file.as_path()));
    }

    #[test]
    fn a_source_that_fails_to_load_falls_back_to_the_placeholder_name() {
        let entries = vec![entry(std::path::Path::new("/does/not/exist.png"), RegistryStatus::Unavailable)];
        let rows = rows_from_registry(&entries);

        assert_eq!(rows[0].name, "(unnamed pack)");
        assert_eq!(rows[0].thumbnail, None);
    }

    #[test]
    fn empty_registry_maps_to_no_rows() {
        assert!(rows_from_registry(&[]).is_empty());
    }

    /// T003: a successful add registers the pack and refreshes `rows`; re-adding the
    /// same path is idempotent (spec.md Acceptance Scenarios 1, 4).
    #[test]
    fn add_result_ok_registers_and_refreshes_rows() {
        let (mut registry, dir) = temp_registry();
        let mut state = State { rows: vec![], pending_removal: None, add_error: None };
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
        let mut state = State { rows: vec![], pending_removal: None, add_error: None };

        apply_add_result(&mut state, &mut registry, Err("malformed manifest".to_string()));
        assert!(state.rows.is_empty());
        assert_eq!(state.add_error.as_deref(), Some("malformed manifest"));
    }

    /// A path that fails `PackSource::resolve` (e.g. doesn't exist) is also a specific
    /// error, not a panic or a silent no-op.
    #[test]
    fn add_result_ok_with_an_unresolvable_path_sets_add_error() {
        let (mut registry, dir) = temp_registry();
        let mut state = State { rows: vec![], pending_removal: None, add_error: None };

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
        let mut state = State { rows: vec![], pending_removal: None, add_error: None };

        confirm_removal(&mut state, &mut registry);
        assert_eq!(state.pending_removal, None);
        assert!(state.rows.is_empty());
    }
}
