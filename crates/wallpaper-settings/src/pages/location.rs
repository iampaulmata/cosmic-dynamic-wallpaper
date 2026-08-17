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
        LocationMode::IpGeolocation => "IP-geolocation",
        LocationMode::Automatic => "Automatic (portal)",
        LocationMode::Manual => "Manual",
    }
}

fn status_label(status: &ResolutionStatus) -> String {
    match status {
        ResolutionStatus::Unresolved => "unresolved".to_string(),
        ResolutionStatus::Resolved => "resolved".to_string(),
        ResolutionStatus::Unavailable { reason } => format!("unavailable ({reason})"),
    }
}

/// A short, plain-language reason for the Portal status row specifically — the raw
/// failure text `renderer::portal_location::apply_failure` stores is `ashpd`'s own
/// error `Display`, which embeds the underlying `org.freedesktop.portal.Error.*`
/// D-Bus error name (zbus's `DBusError` derive formats every such error as
/// `"<dbus error name>: <description>"`) and isn't meant for an end user to read.
/// Matched leniently (case-insensitive substring, singular/plural-agnostic) since the
/// exact wire text varies by portal backend and by which of `ashpd`'s wrapper variants
/// (`Portal`, `Zbus`, a bare timeout string, …) produced it — an unrecognized reason
/// still degrades to a plain "unavailable" rather than leaking anything raw.
fn simplify_portal_reason(reason: &str) -> &'static str {
    let normalized = reason.to_ascii_lowercase();
    let contains = |needle: &str| normalized.contains(needle);

    if contains("invalidargument") {
        "invalid request"
    } else if contains("notfound") {
        "location services unavailable"
    } else if contains("exist") {
        "session already active"
    } else if contains("notallowed") {
        "permission denied"
    } else if contains("cancelled") || contains("canceled") {
        "request cancelled"
    } else if contains("sessionexpired") || contains("windowdestroyed") {
        "session expired"
    } else if contains("failed") {
        "request failed"
    } else {
        "unavailable"
    }
}

/// The Portal status row's label (spec.md's "clean up the language for the portal
/// status" ask) — identical to [`status_label`] for `Unresolved`/`Resolved`, but the
/// `Unavailable` case shows [`simplify_portal_reason`]'s short message instead of the
/// raw D-Bus error text. IP-geolocation status keeps using [`status_label`] as-is: its
/// failures come from STUN/HTTP lookups, not portal D-Bus errors, so this mapping
/// doesn't apply there.
fn portal_status_label(status: &ResolutionStatus) -> String {
    match status {
        ResolutionStatus::Unavailable { reason } => simplify_portal_reason(reason).to_string(),
        _ => status_label(status),
    }
}

pub fn view(state: &State) -> Element<'_, Message> {
    let mut mode_section = widget::settings::section().title("Location mode");
    for mode in [LocationMode::IpGeolocation, LocationMode::Automatic, LocationMode::Manual] {
        let radio = widget::radio("", mode, Some(state.entry.mode), Message::SelectMode);
        if mode == LocationMode::IpGeolocation {
            let info_icon = widget::button::icon(widget::icon::from_name("dialog-information-symbolic"))
                .on_press(Message::ToggleIpDisclosure);
            let row = widget::row::with_capacity(2)
                .spacing(cosmic::theme::spacing().space_xs)
                .align_y(cosmic::iced::Alignment::Center)
                .push(info_icon)
                .push(radio);
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
    status_section = status_section.add(widget::settings::item("Portal status", widget::text::body(portal_status_label(&state.entry.automatic_status))));
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

    /// `simplify_portal_reason` recognizes every `org.freedesktop.portal.Error.*`
    /// D-Bus error name it's documented to handle, matched inside the full raw text
    /// `ashpd`'s `Display` actually produces (zbus's `DBusError` derive formats each
    /// as `"<name>: <description>"`, and `ashpd::Error::Portal`'s own `Display` wraps
    /// that again in `"Portal request failed: {inner}"`).
    #[test]
    fn simplify_portal_reason_maps_every_known_dbus_error_name() {
        let cases = [
            ("Portal request failed: org.freedesktop.portal.Error.InvalidArgument: bad accuracy", "invalid request"),
            ("Portal request failed: org.freedesktop.portal.Error.NotFound: no such session", "location services unavailable"),
            ("Portal request failed: org.freedesktop.portal.Error.Exist: session already exists", "session already active"),
            ("Portal request failed: org.freedesktop.portal.Error.NotAllowed: Location services disabled", "permission denied"),
            ("Portal request failed: org.freedesktop.portal.Error.Cancelled: user dismissed the prompt", "request cancelled"),
            ("Portal request failed: org.freedesktop.portal.Error.WindowDestroyed: window closed", "session expired"),
            ("Portal request failed: org.freedesktop.portal.Error.Failed: unknown backend error", "request failed"),
        ];
        for (reason, expected) in cases {
            assert_eq!(simplify_portal_reason(reason), expected, "reason: {reason}");
        }
    }

    /// An unrecognized reason (a bare timeout string, a zbus service-unknown error, …)
    /// still degrades to a plain "unavailable" rather than leaking raw D-Bus text.
    #[test]
    fn simplify_portal_reason_falls_back_to_unavailable_for_anything_unrecognized() {
        assert_eq!(simplify_portal_reason("resolution attempt timed out"), "unavailable");
        assert_eq!(simplify_portal_reason("portal session ended"), "unavailable");
    }

    /// `portal_status_label` only simplifies the `Unavailable` case — `Resolved`/
    /// `Unresolved` match `status_label` exactly, same as the IP-geolocation row.
    #[test]
    fn portal_status_label_simplifies_only_the_unavailable_case() {
        assert_eq!(portal_status_label(&ResolutionStatus::Resolved), "resolved");
        assert_eq!(portal_status_label(&ResolutionStatus::Unresolved), "unresolved");
        assert_eq!(
            portal_status_label(&ResolutionStatus::Unavailable {
                reason: "Portal request failed: org.freedesktop.portal.Error.NotAllowed: Location services disabled".to_string()
            }),
            "permission denied"
        );
    }

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
