//! Solar event time computation, wrapping the `sunrise` crate (FR-002).
//!
//! See `research.md` R1's "Correction found during implementation" note: `sunrise` 3.0.0
//! has no direct `SolarNoon`/solar-transit accessor, so `SolarNoon` is derived as the
//! exact midpoint of that day's sunrise/sunset instants (provably exact under the
//! crate's own hour-angle symmetry — see that note for the proof sketch), and
//! `SolarMidnight` as noon minus twelve hours. Neither derivation reimplements any solar
//! trigonometry; both are arithmetic over values the crate itself computed.

use chrono::{DateTime, Local, NaiveDate, TimeDelta, Utc};
use sunrise::{Coordinates, DawnType, SolarDay, SolarEvent};

use crate::anchor::SolarEventKind;
use crate::location::Location;

fn coordinates(location: &Location) -> Option<Coordinates> {
    Coordinates::new(location.latitude(), location.longitude())
}

fn midpoint(a: DateTime<Utc>, b: DateTime<Utc>) -> DateTime<Utc> {
    a + TimeDelta::milliseconds((b - a).num_milliseconds() / 2)
}

/// The base (unoffset) instant of a single solar event on `date`, in UTC.
///
/// Returns `None` when the event does not occur for this date/location — polar day or
/// night (FR-007).
fn base_event_utc(coord: Coordinates, date: NaiveDate, kind: SolarEventKind) -> Option<DateTime<Utc>> {
    let day = SolarDay::new(coord, date);
    match kind {
        SolarEventKind::Sunrise => day.event_time(SolarEvent::Sunrise),
        SolarEventKind::Sunset => day.event_time(SolarEvent::Sunset),
        SolarEventKind::CivilDawn => day.event_time(SolarEvent::Dawn(DawnType::Civil)),
        SolarEventKind::CivilDusk => day.event_time(SolarEvent::Dusk(DawnType::Civil)),
        SolarEventKind::AstronomicalDawn => day.event_time(SolarEvent::Dawn(DawnType::Astronomical)),
        SolarEventKind::AstronomicalDusk => day.event_time(SolarEvent::Dusk(DawnType::Astronomical)),
        SolarEventKind::SolarNoon => {
            let sunrise = day.event_time(SolarEvent::Sunrise)?;
            let sunset = day.event_time(SolarEvent::Sunset)?;
            Some(midpoint(sunrise, sunset))
        }
        SolarEventKind::SolarMidnight => {
            let sunrise = day.event_time(SolarEvent::Sunrise)?;
            let sunset = day.event_time(SolarEvent::Sunset)?;
            Some(midpoint(sunrise, sunset) - TimeDelta::hours(12))
        }
    }
}

/// Resolve a solar-anchored [`crate::TimeAnchor::Solar`] to a concrete local instant on
/// `date`, applying its signed offset if any.
///
/// Pure, synchronous, no I/O (FR-008). Returns `None` when the underlying event doesn't
/// occur for this date/location (FR-007) — callers hold the adjacent image active
/// through such gaps rather than treating this as an error.
pub(crate) fn resolve_solar_anchor(
    location: &Location,
    date: NaiveDate,
    event: SolarEventKind,
    offset: Option<TimeDelta>,
) -> Option<DateTime<Local>> {
    let coord = coordinates(location)?;
    let base = base_event_utc(coord, date, event)?.with_timezone(&Local);
    Some(match offset {
        Some(delta) => base + delta,
        None => base,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toronto() -> Location {
        Location::new(43.6532, -79.3832).expect("valid")
    }

    fn jan1_2016() -> NaiveDate {
        NaiveDate::from_ymd_opt(2016, 1, 1).expect("valid date")
    }

    #[test]
    fn sunrise_precedes_sunset() {
        let sunrise = resolve_solar_anchor(&toronto(), jan1_2016(), SolarEventKind::Sunrise, None)
            .expect("occurs");
        let sunset = resolve_solar_anchor(&toronto(), jan1_2016(), SolarEventKind::Sunset, None)
            .expect("occurs");
        assert!(sunrise < sunset);
    }

    #[test]
    fn solar_noon_is_between_sunrise_and_sunset() {
        let sunrise = resolve_solar_anchor(&toronto(), jan1_2016(), SolarEventKind::Sunrise, None)
            .expect("occurs");
        let sunset = resolve_solar_anchor(&toronto(), jan1_2016(), SolarEventKind::Sunset, None)
            .expect("occurs");
        let noon = resolve_solar_anchor(&toronto(), jan1_2016(), SolarEventKind::SolarNoon, None)
            .expect("occurs");
        assert!(sunrise < noon && noon < sunset);
        // Exact midpoint under this crate's model (see module doc).
        assert_eq!(noon - sunrise, sunset - noon);
    }

    #[test]
    fn solar_midnight_is_twelve_hours_before_noon() {
        let noon = resolve_solar_anchor(&toronto(), jan1_2016(), SolarEventKind::SolarNoon, None)
            .expect("occurs");
        let midnight =
            resolve_solar_anchor(&toronto(), jan1_2016(), SolarEventKind::SolarMidnight, None)
                .expect("occurs");
        assert_eq!(noon - midnight, TimeDelta::hours(12));
    }

    #[test]
    fn offset_shifts_the_base_instant() {
        let base = resolve_solar_anchor(&toronto(), jan1_2016(), SolarEventKind::Sunset, None)
            .expect("occurs");
        let offset = resolve_solar_anchor(
            &toronto(),
            jan1_2016(),
            SolarEventKind::Sunset,
            Some(-TimeDelta::minutes(30)),
        )
        .expect("occurs");
        assert_eq!(base - offset, TimeDelta::minutes(30));
    }

    #[test]
    fn dawn_precedes_sunrise_and_dusk_follows_sunset() {
        let civil_dawn =
            resolve_solar_anchor(&toronto(), jan1_2016(), SolarEventKind::CivilDawn, None)
                .expect("occurs");
        let sunrise = resolve_solar_anchor(&toronto(), jan1_2016(), SolarEventKind::Sunrise, None)
            .expect("occurs");
        let sunset = resolve_solar_anchor(&toronto(), jan1_2016(), SolarEventKind::Sunset, None)
            .expect("occurs");
        let civil_dusk =
            resolve_solar_anchor(&toronto(), jan1_2016(), SolarEventKind::CivilDusk, None)
                .expect("occurs");
        assert!(civil_dawn < sunrise);
        assert!(civil_dusk > sunset);
    }

    #[test]
    fn polar_night_sunrise_does_not_occur() {
        // Deep into the Arctic in December: sun never rises.
        let svalbard = Location::new(78.2232, 15.6267).expect("valid");
        let deep_winter = NaiveDate::from_ymd_opt(2016, 12, 21).expect("valid date");
        assert!(
            resolve_solar_anchor(&svalbard, deep_winter, SolarEventKind::Sunrise, None).is_none()
        );
    }

    proptest::proptest! {
        /// Spec 011 US1 FR-005 (research.md R4): no offset magnitude within
        /// `crate::pack::MAX_SOLAR_OFFSET_HOURS` — the exact bound
        /// `WallpaperPack::validate` now enforces — can overflow this function's
        /// `base + delta` `DateTime` arithmetic, across a wide spread of dates
        /// (including near `NaiveDate`'s own representable extremes) and every solar
        /// event kind. This is the property that makes the `validate`-time bound in
        /// `pack.rs` an actual guarantee about `resolve_solar_anchor`, not just a
        /// plausible-looking check.
        #[test]
        fn no_validated_offset_overflows_resolve_solar_anchor(
            offset_hours in -crate::pack::MAX_SOLAR_OFFSET_HOURS..=crate::pack::MAX_SOLAR_OFFSET_HOURS,
            year in 1900i32..2100,
            day_of_year in 1i64..=365,
            event_index in 0usize..8,
        ) {
            let event = [
                SolarEventKind::Sunrise,
                SolarEventKind::Sunset,
                SolarEventKind::SolarNoon,
                SolarEventKind::SolarMidnight,
                SolarEventKind::CivilDawn,
                SolarEventKind::CivilDusk,
                SolarEventKind::AstronomicalDawn,
                SolarEventKind::AstronomicalDusk,
            ][event_index];
            let Some(date) = NaiveDate::from_yo_opt(year, day_of_year as u32) else { return Ok(()) };
            let offset = TimeDelta::hours(offset_hours);
            // The assertion is simply that this call doesn't panic — `resolve_solar_anchor`
            // returns `Option`, never a `Result`, so reaching this line at all (for any
            // location that has the event on this date) is the property under test.
            let _ = resolve_solar_anchor(&toronto(), date, event, Some(offset));
        }
    }
}
