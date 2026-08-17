//! Assignment page (spec.md FR-003, FR-010–FR-011, FR-013–FR-017,
//! contracts/gui-usability-improvements.md) — a "same pack everywhere" toggle plus,
//! when off, an independent per-display dropdown, writing the identical
//! `RendererConfig` shape `wallpaperctl assign` already writes (spec 4 FR-006/FR-007)
//! — enforced structurally, both link `wallpaper_ipc::RendererConfig` (plan.md
//! Constitution Check finding 1). Supersedes spec 7's "assigns the first registered
//! pack" simplification (spec 008 US5).

use cosmic::widget;
use cosmic::Element;
use pack_loader::PackSource;
use wallpaper_ipc::RendererConfig;

use crate::pack_display;

/// What an assignment targets — mirrors `wallpaperctl`'s own `AssignTarget`
/// (`crates/wallpaperctl/src/commands/assign.rs`).
#[derive(Debug, Clone, PartialEq)]
pub enum AssignTarget {
    Output(String),
    SameEverywhere,
}

/// Writes the identical `RendererConfig.overrides`/`same_pack_everywhere` shape
/// `wallpaperctl assign` does — pure, independent of any config I/O.
pub fn apply_assignment(config: &mut RendererConfig, target: &AssignTarget, source: PackSource) {
    match target {
        AssignTarget::Output(id) => {
            config.overrides.insert(id.clone(), source);
        }
        AssignTarget::SameEverywhere => {
            config.same_pack_everywhere = Some(source);
        }
    }
}

/// The toggle's on/off transition (spec.md FR-014/FR-015, spec 008 research.md R6).
/// Switching on clears `overrides` so the toggle's choice applies to every display
/// unconditionally — **a deliberate, user-confirmed divergence from `wallpaperctl
/// assign --same-everywhere`**, which leaves `overrides` untouched; do not "fix" this
/// to match the CLI without re-reading research.md R6 first. Switching on with nothing
/// already chosen pre-selects `default_pack` rather than leaving the toggle "on" with
/// no pack selected. Switching off simply clears `same_pack_everywhere`, leaving
/// `overrides` untouched so per-display dropdowns keep whatever they already had.
pub fn set_same_everywhere_enabled(config: &mut RendererConfig, enabled: bool, default_pack: Option<PackSource>) {
    if enabled {
        config.overrides.clear();
        if config.same_pack_everywhere.is_none() {
            config.same_pack_everywhere = default_pack;
        }
    } else {
        config.same_pack_everywhere = None;
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    /// The toggle switch (FR-014).
    ToggleSameEverywhere(bool),
    /// The single dropdown shown when the toggle is on; index into `available_packs`
    /// (FR-013).
    SameEverywherePackSelected(usize),
    /// A per-display dropdown shown when the toggle is off: `(output_id, index into
    /// available_packs)` (FR-013).
    OutputPackSelected(String, usize),
}

pub struct State {
    pub known_outputs: Vec<String>,
    pub available_packs: Vec<PackSource>,
    /// Spec 012 SC-006: each pack's registry-level display-name override, same order/
    /// index as `available_packs` — `app.rs` builds both from the same
    /// `Registry::known_packs()` pass. `None` at an index means that pack has no
    /// override set (falls back to `pack_display::resolve_pack_name`'s usual
    /// resolution, same as before this field existed).
    pub available_pack_display_names: Vec<Option<String>>,
    pub current_config: RendererConfig,
}

/// User Story 4 (spec.md FR-010/FR-011, "Assignment page shows pack names, not
/// paths") is satisfied here by construction, not by a separate code path: every
/// dropdown's option labels *and* its selected-value display are `pack_display::
/// resolve_pack_display_name` results (spec 012 SC-006 extended this to also honor a
/// pack's custom display name, not just its manifest/file-derived one) — there is
/// deliberately no `source.path().display()`
/// or `current.path().display()` anywhere below (plan.md finding 3, contracts/
/// gui-usability-improvements.md). If you're looking for US4's own implementation,
/// this is it.
pub fn view(state: &State) -> Element<'_, Message> {
    let mut section = widget::settings::section().title("Assignment");

    if state.available_packs.is_empty() {
        // FR-016: a clear message, not an empty/broken dropdown.
        section = section.add(widget::text::body("No packs registered yet — add one on the Packs page first."));
        return widget::scrollable(widget::column::with_capacity(1).push(section)).into();
    }

    let labels: Vec<String> = state
        .available_packs
        .iter()
        .zip(state.available_pack_display_names.iter())
        .map(|(s, override_name)| {
            pack_display::resolve_pack_display_name(s, override_name.as_deref()).unwrap_or_else(|| "(unnamed pack)".to_string())
        })
        .collect();

    let enabled = state.current_config.same_pack_everywhere.is_some();
    let toggle = widget::toggler(enabled)
        .label("Same pack everywhere".to_string())
        .spacing(cosmic::theme::spacing().space_xs)
        .on_toggle(Message::ToggleSameEverywhere);
    section = section.add(widget::settings::item("Same pack everywhere", toggle));

    if enabled {
        let selected =
            state.available_packs.iter().position(|p| Some(p) == state.current_config.same_pack_everywhere.as_ref());
        let dropdown = widget::dropdown(labels.clone(), selected, Message::SameEverywherePackSelected);
        section = section.add(widget::settings::item("Pack", dropdown));
    } else {
        for output in &state.known_outputs {
            let selected = state.available_packs.iter().position(|p| Some(p) == state.current_config.overrides.get(output));
            let output_id = output.clone();
            let dropdown = widget::dropdown(labels.clone(), selected, move |index| {
                Message::OutputPackSelected(output_id.clone(), index)
            });
            section = section.add(widget::settings::item(output.clone(), dropdown));
        }
    }

    widget::scrollable(widget::column::with_capacity(1).push(section)).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_target_writes_an_override() {
        let mut config = RendererConfig::default();
        apply_assignment(&mut config, &AssignTarget::Output("DP-3".to_string()), PackSource::StaticFile("/a.jpg".into()));
        assert_eq!(config.overrides.get("DP-3"), Some(&PackSource::StaticFile("/a.jpg".into())));
        assert!(config.same_pack_everywhere.is_none());
    }

    #[test]
    fn same_everywhere_target_writes_the_toggle() {
        let mut config = RendererConfig::default();
        apply_assignment(&mut config, &AssignTarget::SameEverywhere, PackSource::StaticFile("/a.jpg".into()));
        assert_eq!(config.same_pack_everywhere, Some(PackSource::StaticFile("/a.jpg".into())));
        assert!(config.overrides.is_empty());
    }

    /// T018: switching on clears existing per-display overrides and pre-selects a
    /// default only if nothing was already chosen (FR-014, FR-015).
    #[test]
    fn enabling_clears_overrides_and_preselects_only_if_unset() {
        let mut config = RendererConfig::default();
        config.overrides.insert("DP-3".to_string(), PackSource::StaticFile("/old.jpg".into()));
        config.overrides.insert("DP-4".to_string(), PackSource::StaticFile("/old2.jpg".into()));

        set_same_everywhere_enabled(&mut config, true, Some(PackSource::StaticFile("/default.jpg".into())));

        assert!(config.overrides.is_empty(), "existing per-display overrides must be cleared");
        assert_eq!(config.same_pack_everywhere, Some(PackSource::StaticFile("/default.jpg".into())));
    }

    #[test]
    fn enabling_does_not_override_an_already_chosen_same_everywhere_pack() {
        let mut config = RendererConfig { same_pack_everywhere: Some(PackSource::StaticFile("/chosen.jpg".into())), ..RendererConfig::default() };

        set_same_everywhere_enabled(&mut config, true, Some(PackSource::StaticFile("/default.jpg".into())));

        assert_eq!(config.same_pack_everywhere, Some(PackSource::StaticFile("/chosen.jpg".into())));
    }

    /// T019: switching off clears `same_pack_everywhere` and leaves `overrides`
    /// untouched.
    #[test]
    fn disabling_clears_the_toggle_and_leaves_overrides_untouched() {
        let mut config = RendererConfig {
            same_pack_everywhere: Some(PackSource::StaticFile("/a.jpg".into())),
            ..RendererConfig::default()
        };
        config.overrides.insert("DP-3".to_string(), PackSource::StaticFile("/b.jpg".into()));

        set_same_everywhere_enabled(&mut config, false, None);

        assert_eq!(config.same_pack_everywhere, None);
        assert_eq!(config.overrides.get("DP-3"), Some(&PackSource::StaticFile("/b.jpg".into())));
    }

    /// T020: selecting from either dropdown writes through the existing
    /// `apply_assignment` with the same shapes `wallpaperctl assign` writes.
    #[test]
    fn pack_selected_messages_write_through_apply_assignment() {
        let mut config = RendererConfig::default();
        let packs = [PackSource::StaticFile("/a.jpg".into()), PackSource::StaticFile("/b.jpg".into())];

        apply_assignment(&mut config, &AssignTarget::SameEverywhere, packs[1].clone());
        assert_eq!(config.same_pack_everywhere, Some(packs[1].clone()));

        apply_assignment(&mut config, &AssignTarget::Output("DP-3".to_string()), packs[0].clone());
        assert_eq!(config.overrides.get("DP-3"), Some(&packs[0]));
    }
}
