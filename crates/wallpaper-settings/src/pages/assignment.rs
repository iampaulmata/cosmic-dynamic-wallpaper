//! Assignment page (spec.md FR-003, contracts/gui-application.md) — per-output /
//! same-everywhere pack assignment, writing the identical `RendererConfig` shape
//! `wallpaperctl assign` already writes (spec 4 FR-006/FR-007) — enforced
//! structurally, both link `wallpaper_ipc::RendererConfig` (plan.md Constitution Check
//! finding 1).

use cosmic::widget;
use cosmic::Element;
use pack_loader::PackSource;
use wallpaper_ipc::RendererConfig;

/// What an assignment targets — mirrors `wallpaperctl`'s own `AssignTarget`
/// (`crates/wallpaperctl/src/commands/assign.rs`).
#[derive(Debug, Clone, PartialEq)]
pub enum AssignTarget {
    Output(String),
    SameEverywhere,
}

/// T020: writes the identical `RendererConfig.overrides`/`same_pack_everywhere` shape
/// `wallpaperctl assign` does — pure, independent of any config I/O, so this is a
/// plain unit test rather than needing a real `cosmic-config` round trip to verify the
/// write shape itself (a real round trip is still covered by `wallpaper-ipc`'s own
/// `RendererConfig` tests, which this function's caller ultimately persists through).
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

/// **Simplification**: assigns the first registered pack (`available_packs[0]`)
/// rather than a full pack-picker dropdown — a reasonable, documented scope cut given
/// this page's actual contract requirement (write the identical `RendererConfig` shape
/// `wallpaperctl assign` does, contracts/gui-application.md), not a complete picker
/// UX. See `crates/wallpaper-settings/README.md`.
#[derive(Debug, Clone)]
pub enum Message {
    AssignFirstPackToOutput(String),
    SetFirstPackSameEverywhere,
}

pub struct State {
    pub known_outputs: Vec<String>,
    pub available_packs: Vec<PackSource>,
    pub current_config: RendererConfig,
}

pub fn view(state: &State) -> Element<'_, Message> {
    let mut section = widget::settings::section().title("Assignment");
    let first_pack_name = state.available_packs.first().map(|s| s.path().display().to_string());

    let toggle_widget: Element<'_, Message> = match (&state.current_config.same_pack_everywhere, &first_pack_name) {
        (Some(current), _) => widget::text::body(format!("on: {}", current.path().display())).into(),
        (None, Some(_)) => widget::button::standard("Enable with first registered pack").on_press(Message::SetFirstPackSameEverywhere).into(),
        (None, None) => widget::text::body("off (register a pack first)").into(),
    };
    section = section.add(widget::settings::item("Same pack everywhere", toggle_widget));

    for output in &state.known_outputs {
        let widget: Element<'_, Message> = match (state.current_config.overrides.get(output), &first_pack_name) {
            (Some(source), _) => widget::text::body(source.path().display().to_string()).into(),
            (None, Some(_)) => widget::button::standard("Assign first registered pack").on_press(Message::AssignFirstPackToOutput(output.clone())).into(),
            (None, None) => widget::text::body("follows toggle").into(),
        };
        section = section.add(widget::settings::item(output.clone(), widget));
    }
    widget::column::with_capacity(1).push(section).into()
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
}
