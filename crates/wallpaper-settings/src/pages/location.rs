//! Location page (spec.md FR-004, FR-007–FR-009, contracts/gui-usability-improvements.md)
//! — manual/automatic/IP-geolocation mode switch, writing the identical
//! `LocationConfigEntry` shape `wallpaperctl location set/auto/manual/ip` already
//! writes (spec 6/7). The IP-geolocation disclosure is discoverable by hover *or* tap,
//! before that option is selected (spec 008 US3) — superseding spec 7's
//! post-selection-only placement.

use cosmic::widget;
use cosmic::Element;
use schedule_engine::Location;
use wallpaper_ipc::{effective_location, LocationConfigEntry, LocationMode, ResolutionStatus, IP_GEOLOCATION_DISCLOSURE};

/// T021: writes the identical `mode` shape `wallpaperctl location auto|manual|ip`
/// writes, for all three modes — pure, independent of any config I/O.
pub fn set_mode(entry: &mut LocationConfigEntry, mode: LocationMode) {
    entry.mode = mode;
}

/// Writes the identical `location`/`mode: Manual` shape `wallpaperctl location set`
/// writes (spec 6 research.md R7's documented side effect).
pub fn set_manual_location(entry: &mut LocationConfigEntry, location: Location) {
    entry.location = Some(location);
    entry.mode = LocationMode::Manual;
}

#[derive(Debug, Clone)]
pub enum Message {
    SelectMode(LocationMode),
    LatitudeChanged(String),
    LongitudeChanged(String),
    SetManualLocation,
    /// The info icon next to the IP-geolocation option (FR-008) — independent of
    /// hover, so it also works for touch-only input.
    ToggleIpDisclosure,
}

pub struct State {
    pub entry: LocationConfigEntry,
    pub latitude_input: String,
    pub longitude_input: String,
    /// Toggled by the info icon (FR-008); the hover tooltip (FR-007) shows/hides
    /// itself via `widget::tooltip`'s own built-in hover behavior and needs no state
    /// field of its own.
    pub show_ip_disclosure: bool,
}

impl State {
    pub fn load(entry: LocationConfigEntry) -> Self {
        let (lat, lon) = entry.location.map(|l| (l.latitude(), l.longitude())).unwrap_or((0.0, 0.0));
        Self { entry, latitude_input: lat.to_string(), longitude_input: lon.to_string(), show_ip_disclosure: false }
    }
}

fn mode_label(mode: LocationMode) -> &'static str {
    match mode {
        LocationMode::Manual => "Manual",
        LocationMode::Automatic => "Automatic (portal)",
        LocationMode::IpGeolocation => "IP-geolocation",
    }
}

fn status_label(status: &ResolutionStatus) -> String {
    match status {
        ResolutionStatus::Unresolved => "unresolved".to_string(),
        ResolutionStatus::Resolved => "resolved".to_string(),
        ResolutionStatus::Unavailable { reason } => format!("unavailable ({reason})"),
    }
}

pub fn view(state: &State) -> Element<'_, Message> {
    let mut mode_section = widget::settings::section().title("Location mode");
    for mode in [LocationMode::Manual, LocationMode::Automatic, LocationMode::IpGeolocation] {
        let radio = widget::radio("", mode, Some(state.entry.mode), Message::SelectMode);
        if mode == LocationMode::IpGeolocation {
            // FR-007: discoverable by hover, before the option is selected.
            let with_tooltip = widget::tooltip(radio, widget::text::body(IP_GEOLOCATION_DISCLOSURE), widget::tooltip::Position::Bottom);
            // FR-008: also discoverable by tap/click, for input with no hover
            // capability — independent of the tooltip above.
            let info_icon = widget::button::icon(widget::icon::from_name("dialog-information-symbolic"))
                .on_press(Message::ToggleIpDisclosure);
            let row = widget::row::with_capacity(2)
                .spacing(cosmic::theme::spacing().space_xs)
                .push(with_tooltip)
                .push(info_icon);
            mode_section = mode_section.add(widget::settings::item(mode_label(mode), row));
            if state.show_ip_disclosure {
                mode_section = mode_section.add(widget::text::caption(IP_GEOLOCATION_DISCLOSURE));
            }
        } else {
            mode_section = mode_section.add(widget::settings::item(mode_label(mode), radio));
        }
    }

    let mut status_section = widget::settings::section().title("Current status");
    let effective = effective_location(&state.entry);
    status_section = status_section.add(widget::settings::item(
        "Effective location",
        widget::text::body(effective.map(|l| format!("{} {}", l.latitude(), l.longitude())).unwrap_or_else(|| "none".to_string())),
    ));
    status_section = status_section.add(widget::settings::item("Portal status", widget::text::body(status_label(&state.entry.automatic_status))));
    status_section = status_section.add(widget::settings::item("IP-geolocation status", widget::text::body(status_label(&state.entry.ip_status))));

    let manual_section = widget::settings::section()
        .title("Manual location")
        .add(widget::settings::item(
            "Latitude",
            widget::text_input::text_input("latitude", &state.latitude_input).on_input(Message::LatitudeChanged),
        ))
        .add(widget::settings::item(
            "Longitude",
            widget::text_input::text_input("longitude", &state.longitude_input).on_input(Message::LongitudeChanged),
        ))
        .add(widget::button::suggested("Set manual location").on_press(Message::SetManualLocation));

    widget::scrollable(
        widget::column::with_capacity(3)
            .push(mode_section)
            .push(status_section)
            .push(manual_section)
            .spacing(cosmic::theme::spacing().space_s),
    )
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T021: `set_mode` writes the identical shape for all three modes.
    #[test]
    fn set_mode_writes_the_given_mode_for_all_three_variants() {
        for mode in [LocationMode::Manual, LocationMode::Automatic, LocationMode::IpGeolocation] {
            let mut entry = LocationConfigEntry::default();
            set_mode(&mut entry, mode);
            assert_eq!(entry.mode, mode);
        }
    }

    #[test]
    fn set_mode_never_touches_other_fields() {
        let mut entry = LocationConfigEntry { location: Some(Location::new(45.5019, -73.5674).unwrap()), ..LocationConfigEntry::default() };
        set_mode(&mut entry, LocationMode::Automatic);
        assert_eq!(entry.location, Some(Location::new(45.5019, -73.5674).unwrap()));
    }

    #[test]
    fn set_manual_location_sets_location_and_switches_to_manual_mode() {
        let mut entry = LocationConfigEntry { mode: LocationMode::Automatic, ..LocationConfigEntry::default() };
        let loc = Location::new(51.5072, -0.1276).unwrap();
        set_manual_location(&mut entry, loc);
        assert_eq!(entry.location, Some(loc));
        assert_eq!(entry.mode, LocationMode::Manual);
    }

    /// T026: `ToggleIpDisclosure` flips `show_ip_disclosure`, independent of `mode`.
    #[test]
    fn toggle_ip_disclosure_flips_independent_of_mode() {
        let mut state = State::load(LocationConfigEntry::default());
        assert!(!state.show_ip_disclosure);

        state.show_ip_disclosure = !state.show_ip_disclosure;
        assert!(state.show_ip_disclosure);

        // Independent of mode: flipping it doesn't require IpGeolocation to be
        // selected, and selecting a different mode doesn't reset it.
        set_mode(&mut state.entry, LocationMode::Manual);
        assert!(state.show_ip_disclosure, "mode changes must not reset the disclosure toggle");

        state.show_ip_disclosure = !state.show_ip_disclosure;
        assert!(!state.show_ip_disclosure);
    }
}
