//! Timeline page (spec.md FR-005, contracts/gui-application.md) — today's schedule
//! visualization via `wallpaper_ipc::DbusClient`'s `QueryOutput`/`QueryAll` (spec 4's
//! existing D-Bus interface, unchanged). Read-only — same "daemon unreachable"
//! fallback UX `wallpaperctl query` uses, not a new failure mode.
//!
//! Current/next thumbnails and the next-update time (labeled Output/Current/Next/
//! Updates-at columns, matching the Packs page's layout) are resolved client-side via
//! `pack_display::resolve_schedule_snapshot` rather than over D-Bus: the daemon reports
//! *which* image is active and *when* the next transition happens, but never *which*
//! image comes next, and there's nothing to extend for that — this page already has
//! everything `resolve_schedule_snapshot` needs (the assigned pack, the effective
//! location) without a wire-protocol change. `assigned`/reachability still come from
//! the live daemon query, unchanged, so "wallpaperd not running" keeps behaving exactly
//! as before.

use std::path::PathBuf;

use chrono::{DateTime, Local, TimeDelta};
use cosmic::widget;
use cosmic::Element;
use schedule_engine::Location;
use wallpaper_ipc::{effective_pack, resolve_assignment, DbusClient, DbusError, OutputId, QueryEntry, RendererConfig};

use crate::pack_display;

/// T022: maps a `DbusClient` query outcome 1:1, including the "daemon unreachable"
/// state — pure, independent of `libcosmic` rendering.
#[derive(Debug, Clone, PartialEq)]
pub enum TimelineState {
    Unreachable,
    Data(Vec<QueryEntry>),
}

pub fn from_query_result(result: Result<Vec<QueryEntry>, DbusError>) -> TimelineState {
    match result {
        Ok(entries) => TimelineState::Data(entries),
        Err(_) => TimelineState::Unreachable,
    }
}

/// One row in the Timeline page's list — an output's current/next thumbnails and when
/// the next one goes live. `current_thumbnail`/`next_thumbnail`/`next_update` are all
/// `None` for an unassigned output, and also `None` (rather than an error) for an
/// assigned output whose snapshot couldn't be resolved (pack failed to load, or a
/// solar-anchored pack with no location yet) — both are well-defined empty states, not
/// failures (FR-011 style placeholder, same posture as `pages::packs`).
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineRow {
    pub output: String,
    pub assigned: bool,
    pub current_thumbnail: Option<PathBuf>,
    pub next_thumbnail: Option<PathBuf>,
    /// Local time formatted as `HH:MM`, e.g. `"18:00"`.
    pub next_update: Option<String>,
}

/// Pure mapping from a live query result to display rows — independent of
/// `libcosmic` rendering, so this stays a plain, fast unit test. `at` is threaded in
/// (rather than read via `Local::now()` internally) purely so this stays deterministic
/// and testable (SC-003 style, mirroring `schedule_engine::ValidatedPack::query`'s own
/// contract).
pub fn rows_from_entries(
    entries: &[QueryEntry],
    renderer_config: &RendererConfig,
    location: Option<&Location>,
    crossfade_duration: TimeDelta,
    at: DateTime<Local>,
) -> Vec<TimelineRow> {
    entries
        .iter()
        .map(|entry| {
            if !entry.assigned {
                return TimelineRow {
                    output: entry.output.clone(),
                    assigned: false,
                    current_thumbnail: None,
                    next_thumbnail: None,
                    next_update: None,
                };
            }
            let assignment = resolve_assignment(&OutputId::new(entry.output.clone()), renderer_config);
            let snapshot = effective_pack(&assignment, renderer_config)
                .and_then(|source| pack_display::resolve_schedule_snapshot(source, location, crossfade_duration, at));
            match snapshot {
                Some(s) => TimelineRow {
                    output: entry.output.clone(),
                    assigned: true,
                    current_thumbnail: s.current_thumbnail,
                    next_thumbnail: s.next_thumbnail,
                    next_update: s.next_transition_at.map(|t| t.format("%H:%M").to_string()),
                },
                None => TimelineRow {
                    output: entry.output.clone(),
                    assigned: true,
                    current_thumbnail: None,
                    next_thumbnail: None,
                    next_update: None,
                },
            }
        })
        .collect()
}

#[derive(Debug, Clone)]
pub enum Message {
    Refresh,
}

pub struct State {
    pub timeline: TimelineState,
    pub renderer_config: RendererConfig,
    pub location: Option<Location>,
}

impl State {
    pub fn load(renderer_config: RendererConfig, location: Option<Location>) -> Self {
        let result = DbusClient::connect().and_then(|client| client.query_all());
        Self { timeline: from_query_result(result), renderer_config, location }
    }
}

/// Fixed column widths for the Output/Current/Next/Updates-at columns, so every row's
/// cells line up with the header above them (matches `pages::packs`'s layout).
const OUTPUT_COLUMN_WIDTH: u16 = 100;
const THUMBNAIL_COLUMN_WIDTH: u16 = 48;
const UPDATE_COLUMN_WIDTH: u16 = 90;

fn thumbnail_or_placeholder(path: &Option<PathBuf>) -> Element<'static, Message> {
    match path {
        Some(p) => widget::image(p.clone()).width(THUMBNAIL_COLUMN_WIDTH).height(THUMBNAIL_COLUMN_WIDTH).into(),
        None => widget::text::caption("(no preview)").into(),
    }
}

pub fn view(state: &State) -> Element<'_, Message> {
    let mut section = widget::settings::section().title("Today's schedule");
    match &state.timeline {
        TimelineState::Unreachable => {
            section = section.add(widget::text::body("wallpaperd is not running or not reachable — start it to see live schedule data."));
        }
        TimelineState::Data(entries) if entries.is_empty() => {
            section = section.add(widget::text::body("No outputs managed yet."));
        }
        TimelineState::Data(entries) => {
            let crossfade_duration = TimeDelta::seconds(i64::from(state.renderer_config.crossfade_duration_secs));
            let rows = rows_from_entries(entries, &state.renderer_config, state.location.as_ref(), crossfade_duration, chrono::Local::now());

            let header = widget::row::with_capacity(4)
                .spacing(cosmic::theme::spacing().space_m)
                .push(widget::text::caption_heading("Output").width(OUTPUT_COLUMN_WIDTH))
                .push(widget::text::caption_heading("Current").width(THUMBNAIL_COLUMN_WIDTH))
                .push(widget::text::caption_heading("Next").width(THUMBNAIL_COLUMN_WIDTH))
                .push(widget::text::caption_heading("Updates at").width(UPDATE_COLUMN_WIDTH));
            section = section.add(header);

            for row in &rows {
                let update_text = if !row.assigned {
                    "unassigned".to_string()
                } else {
                    row.next_update.clone().unwrap_or_else(|| "—".to_string())
                };
                let detail = widget::row::with_capacity(4)
                    .spacing(cosmic::theme::spacing().space_m)
                    .align_y(cosmic::iced::Alignment::Center)
                    .push(widget::text::body(row.output.clone()).width(OUTPUT_COLUMN_WIDTH))
                    .push(thumbnail_or_placeholder(&row.current_thumbnail))
                    .push(thumbnail_or_placeholder(&row.next_thumbnail))
                    .push(widget::text::body(update_text).width(UPDATE_COLUMN_WIDTH));
                section = section.add(detail);
            }
        }
    }
    let refresh = widget::button::standard("Refresh").on_press(Message::Refresh);
    widget::scrollable(widget::column::with_capacity(2).push(refresh).push(section)).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pack_loader::PackSource;

    /// T022: matches `DbusClient`'s query response shape 1:1, including the "daemon
    /// unreachable" state.
    #[test]
    fn daemon_unreachable_maps_to_the_unreachable_state() {
        assert_eq!(from_query_result(Err(DbusError::DaemonUnreachable)), TimelineState::Unreachable);
    }

    #[test]
    fn output_not_found_also_maps_to_the_unreachable_state() {
        // Timeline queries all outputs (QueryAll), which never returns
        // OutputNotFound — but this page's mapping is deliberately total over every
        // DbusError variant, not just the one QueryAll can actually produce.
        assert_eq!(
            from_query_result(Err(DbusError::OutputNotFound { id: "DP-3".to_string(), detail: None })),
            TimelineState::Unreachable
        );
    }

    #[test]
    fn successful_query_maps_to_the_data_state() {
        let entries = vec![QueryEntry { output: "DP-3".to_string(), assigned: true, active_image: "dawn.jpg".to_string(), next_transition_at: "t".to_string() }];
        assert_eq!(from_query_result(Ok(entries.clone())), TimelineState::Data(entries));
    }

    fn write_pack_dir(dir: &std::path::Path, manifest_body: &str, images: &[&str]) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("manifest.toml"), manifest_body).unwrap();
        for name in images {
            image::RgbImage::new(2, 2).save(dir.join(name)).unwrap();
        }
    }

    fn local_at(hh: u32, mm: u32) -> DateTime<Local> {
        use chrono::TimeZone;
        let today = Local::now().date_naive();
        Local.from_local_datetime(&today.and_hms_opt(hh, mm, 0).unwrap()).single().unwrap()
    }

    fn entry(output: &str, assigned: bool) -> QueryEntry {
        QueryEntry { output: output.to_string(), assigned, active_image: String::new(), next_transition_at: String::new() }
    }

    #[test]
    fn unassigned_entries_map_to_a_row_with_no_thumbnails_or_update_time() {
        let entries = vec![entry("DP-3", false)];
        let config = RendererConfig::default();
        let rows = rows_from_entries(&entries, &config, None, TimeDelta::minutes(1), local_at(12, 0));

        assert_eq!(rows.len(), 1);
        assert!(!rows[0].assigned);
        assert_eq!(rows[0].current_thumbnail, None);
        assert_eq!(rows[0].next_thumbnail, None);
        assert_eq!(rows[0].next_update, None);
    }

    /// An output the daemon reports as `assigned` but this GUI's own `RendererConfig`
    /// snapshot resolves to `Unassigned` (a rare config-read race) degrades to the
    /// same empty row shape as a load failure — never a panic or a stale guess.
    #[test]
    fn assigned_entry_with_no_resolvable_pack_source_has_no_thumbnails() {
        let entries = vec![entry("DP-3", true)];
        let config = RendererConfig::default(); // no toggle, no overrides
        let rows = rows_from_entries(&entries, &config, None, TimeDelta::minutes(1), local_at(12, 0));

        assert!(rows[0].assigned);
        assert_eq!(rows[0].current_thumbnail, None);
        assert_eq!(rows[0].next_update, None);
    }

    #[test]
    fn assigned_entry_with_a_resolvable_pack_reports_current_next_and_update_time() {
        let dir = tempfile::tempdir().unwrap();
        let pack_dir = dir.path().join("clock-pack");
        write_pack_dir(
            &pack_dir,
            r##"
                schema_version = 1
                name = "Clock Pack"
                default_scaling = "Fill"
                fallback_color = "#000000"
                [[images]]
                file = "dawn.png"
                anchor = "06:00"
                [[images]]
                file = "dusk.png"
                anchor = "18:00"
            "##,
            &["dawn.png", "dusk.png"],
        );
        let source = PackSource::resolve(&pack_dir).unwrap();
        let config = RendererConfig { same_pack_everywhere: Some(source), ..RendererConfig::default() };
        let entries = vec![entry("DP-3", true)];

        let rows = rows_from_entries(&entries, &config, None, TimeDelta::minutes(1), local_at(12, 0));

        assert_eq!(rows[0].current_thumbnail.as_ref().and_then(|p| p.file_name()), Some(std::ffi::OsStr::new("dawn.png")));
        assert_eq!(rows[0].next_thumbnail.as_ref().and_then(|p| p.file_name()), Some(std::ffi::OsStr::new("dusk.png")));
        assert_eq!(rows[0].next_update.as_deref(), Some("18:00"));
    }

    #[test]
    fn per_output_overrides_take_precedence_over_the_same_everywhere_toggle() {
        let dir = tempfile::tempdir().unwrap();
        let toggle_pack_dir = dir.path().join("toggle-pack");
        write_pack_dir(
            &toggle_pack_dir,
            r##"
                schema_version = 1
                name = "Toggle Pack"
                default_scaling = "Fill"
                fallback_color = "#000000"
                [[images]]
                file = "a.png"
                anchor = "06:00"
                [[images]]
                file = "b.png"
                anchor = "18:00"
            "##,
            &["a.png", "b.png"],
        );
        let override_pack_dir = dir.path().join("override-pack");
        write_pack_dir(
            &override_pack_dir,
            r##"
                schema_version = 1
                name = "Override Pack"
                default_scaling = "Fill"
                fallback_color = "#000000"
                [[images]]
                file = "c.png"
                anchor = "06:00"
                [[images]]
                file = "d.png"
                anchor = "18:00"
            "##,
            &["c.png", "d.png"],
        );
        let toggle_source = PackSource::resolve(&toggle_pack_dir).unwrap();
        let override_source = PackSource::resolve(&override_pack_dir).unwrap();
        let mut config = RendererConfig { same_pack_everywhere: Some(toggle_source), ..RendererConfig::default() };
        config.overrides.insert("DP-3".to_string(), override_source);
        let entries = vec![entry("DP-3", true)];

        let rows = rows_from_entries(&entries, &config, None, TimeDelta::minutes(1), local_at(12, 0));

        assert_eq!(rows[0].current_thumbnail.as_ref().and_then(|p| p.file_name()), Some(std::ffi::OsStr::new("c.png")));
    }
}
