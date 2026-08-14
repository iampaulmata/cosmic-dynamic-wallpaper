//! Crossfade page (spec.md FR-006, contracts/gui-application.md) — the duration
//! control this project's GUI work is what actually makes real (plan.md Constitution
//! Check finding 3): `RendererConfig.crossfade_duration_secs` didn't exist before
//! spec 7's Foundational phase; `surface.rs`'s `CROSSFADE_DURATION` was a plain
//! compile-time constant despite a stray doc comment claiming otherwise.

use cosmic::widget;
use cosmic::Element;
use wallpaper_ipc::RendererConfig;

/// T023: writes `RendererConfig.crossfade_duration_secs` — pure, independent of any
/// config I/O.
pub fn set_duration(config: &mut RendererConfig, seconds: u32) {
    config.crossfade_duration_secs = seconds;
}

#[derive(Debug, Clone)]
pub enum Message {
    DurationChanged(f32),
}

pub struct State {
    pub current_config: RendererConfig,
}

pub fn view(state: &State) -> Element<'_, Message> {
    let seconds = state.current_config.crossfade_duration_secs;
    let section = widget::settings::section().title("Crossfade").add(widget::settings::item(
        format!("Duration: {seconds}s"),
        widget::slider(5.0f32..=120.0, seconds as f32, Message::DurationChanged).step(5.0f32),
    ));
    widget::column::with_capacity(1).push(section).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_duration_writes_the_given_value() {
        let mut config = RendererConfig::default();
        assert_eq!(config.crossfade_duration_secs, 45);
        set_duration(&mut config, 20);
        assert_eq!(config.crossfade_duration_secs, 20);
    }
}
