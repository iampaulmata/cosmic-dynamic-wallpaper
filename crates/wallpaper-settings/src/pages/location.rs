//! Location page (spec.md FR-004, contracts/gui-application.md) — manual/automatic/
//! IP-geolocation mode switch, writing the identical `LocationConfigEntry` shape
//! `wallpaperctl location set/auto/manual/ip` already writes (spec 6/7).

use cosmic::widget;
use cosmic::Element;
use schedule_engine::Location;
use wallpaper_ipc::{effective_location, LocationConfigEntry, LocationMode, ResolutionStatus};

/// STUN-disclosure copy FR-014 requires before a user opts into IP-geolocation
/// (spec 7 research.md R4) — the identical wording `wallpaperctl location ip` already
/// surfaces (`crates/wallpaperctl/src/commands/location.rs::IP_GEOLOCATION_DISCLOSURE`)
/// so the two control surfaces never say different things about the one external
/// touchpoint (T054).
pub const IP_GEOLOCATION_DISCLOSURE: &str = "uses a bundled offline database for the location lookup; briefly asks a STUN server what this machine's public IP address is, since that's not something a bundled database can tell you on its own";

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
}

pub struct State {
    pub entry: LocationConfigEntry,
    pub latitude_input: String,
    pub longitude_input: String,
}

impl State {
    pub fn load(entry: LocationConfigEntry) -> Self {
        let (lat, lon) = entry.location.map(|l| (l.latitude(), l.longitude())).unwrap_or((0.0, 0.0));
        Self { entry, latitude_input: lat.to_string(), longitude_input: lon.to_string() }
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
        mode_section = mode_section.add(widget::settings::item(
            mode_label(mode),
            widget::radio("", mode, Some(state.entry.mode), Message::SelectMode),
        ));
    }
    if state.entry.mode == LocationMode::IpGeolocation {
        mode_section = mode_section.add(widget::text::caption(IP_GEOLOCATION_DISCLOSURE));
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

    widget::column::with_capacity(3).push(mode_section).push(status_section).push(manual_section).spacing(cosmic::theme::spacing().space_s).into()
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
}
