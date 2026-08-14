//! Packs page (spec.md FR-002, contracts/gui-application.md) — browse
//! already-registered packs. Registration itself remains `wallpaperctl register`'s job
//! (or a future task's scope) — spec.md's own text only requires "browse registered
//! packs".

use cosmic::widget;
use cosmic::Element;
use pack_loader::{PackRegistryEntry, PackSource, Registry, RegistryStatus};

/// One row in the Packs page's list (T019) — enough to identify a pack and show its
/// reachability at a glance, plus a preview reference (spec.md Acceptance Scenario 1).
/// **Simplification**: the preview is shown as its file path, not a rendered
/// thumbnail — an actual `<image>` widget is a reasonable follow-up, not required by
/// contracts/gui-application.md's own text (only "browse... with preview").
#[derive(Debug, Clone, PartialEq)]
pub struct PackRow {
    pub name: String,
    pub source: PackSource,
    pub status: &'static str,
    pub preview: Option<String>,
}

/// Pure mapping from a registry listing to display rows (T019) — independent of
/// `libcosmic` rendering, so this stays a plain, fast unit test.
pub fn rows_from_registry(entries: &[PackRegistryEntry]) -> Vec<PackRow> {
    entries
        .iter()
        .map(|entry| {
            let status = match entry.status {
                RegistryStatus::Known => "known",
                RegistryStatus::Unavailable => "unavailable",
            };
            // Best-effort preview: the pack's own source path if it's a single
            // static image; the first image found alongside a manifest, if loadable.
            // Never fails the row itself if this doesn't succeed (FR-013 posture:
            // browsing must not error out over one bad entry).
            let preview = match &entry.source {
                PackSource::StaticFile(path) => Some(path.display().to_string()),
                PackSource::Directory(_) => pack_loader::load_pack(entry.source.path())
                    .ok()
                    .and_then(|loaded| loaded.image_paths.values().next().map(|p| p.display().to_string())),
            };
            PackRow { name: entry.source.path().display().to_string(), source: entry.source.clone(), status, preview }
        })
        .collect()
}

#[derive(Debug, Clone)]
pub enum Message {
    Refresh,
}

pub struct State {
    pub rows: Vec<PackRow>,
}

impl State {
    pub fn load(registry: &mut Registry) -> Self {
        Self { rows: rows_from_registry(&registry.known_packs()) }
    }
}

pub fn view(state: &State) -> Element<'_, Message> {
    let mut section = widget::settings::section().title("Registered packs");
    if state.rows.is_empty() {
        section = section.add(widget::text::body("No packs registered yet. Run `wallpaperctl register <path>` first."));
    } else {
        for row in &state.rows {
            let preview_text = row.preview.clone().unwrap_or_else(|| "(no preview available)".to_string());
            let detail = widget::column::with_capacity(2)
                .push(widget::text::body(format!("status: {}", row.status)))
                .push(widget::text::caption(preview_text));
            section = section.add(widget::settings::item(row.name.clone(), detail));
        }
    }
    let refresh = widget::button::standard("Refresh").on_press(Message::Refresh);
    widget::column::with_capacity(2).push(refresh).push(section).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, status: RegistryStatus) -> PackRegistryEntry {
        PackRegistryEntry { source: PackSource::StaticFile(path.into()), status, origin: pack_loader::PackOrigin::User }
    }

    /// T019: maps a registry listing into display rows independent of rendering.
    #[test]
    fn maps_registry_entries_to_rows_with_status_and_preview() {
        let entries = vec![entry("/a.jpg", RegistryStatus::Known), entry("/b.jpg", RegistryStatus::Unavailable)];
        let rows = rows_from_registry(&entries);

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].status, "known");
        assert_eq!(rows[0].preview.as_deref(), Some("/a.jpg"));
        assert_eq!(rows[1].status, "unavailable");
    }

    #[test]
    fn empty_registry_maps_to_no_rows() {
        assert!(rows_from_registry(&[]).is_empty());
    }
}
