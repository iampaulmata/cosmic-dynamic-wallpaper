//! [`TimeAnchor`] and [`SolarEventKind`] — how a single pack image is scheduled (FR-6).

use chrono::{NaiveTime, TimeDelta};

/// The eight solar events FR-6 recognizes.
///
/// `SolarNoon` and `SolarMidnight` are not directly exposed by the underlying `sunrise`
/// crate and are derived at query time — see `solar.rs` and `research.md` R1's
/// "Correction found during implementation" note.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SolarEventKind {
    /// The moment the sun's upper edge crosses the horizon in the morning.
    Sunrise,
    /// The moment the sun's upper edge crosses the horizon in the evening.
    Sunset,
    /// The sun's highest point in the sky for the day (derived, see above).
    SolarNoon,
    /// The sun's lowest point in the sky for the day (derived, see above).
    SolarMidnight,
    /// Civil dawn: sun 6° below the horizon, morning.
    CivilDawn,
    /// Civil dusk: sun 6° below the horizon, evening.
    CivilDusk,
    /// Astronomical dawn: sun 18° below the horizon, morning.
    AstronomicalDawn,
    /// Astronomical dusk: sun 18° below the horizon, evening.
    AstronomicalDusk,
}

/// How a single [`crate::PackImage`] is scheduled — exactly one of a solar event
/// (optionally offset) or an absolute clock time (data-model.md `TimeAnchor`).
///
/// A single [`crate::WallpaperPack`] must use only one variant across all its anchors
/// (FR-6, FR-001) — see [`crate::PackError::MixedAnchorTypes`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimeAnchor {
    /// Anchored to a named solar event, optionally offset (e.g. "sunset - 30m").
    Solar {
        /// Which solar event.
        event: SolarEventKind,
        /// Signed offset applied to the event's computed instant, if any.
        offset: Option<TimeDelta>,
    },
    /// Anchored to an absolute wall-clock time of day (FR-11).
    Clock(NaiveTime),
}

impl TimeAnchor {
    /// `true` if this is a [`TimeAnchor::Solar`] anchor.
    pub fn is_solar(&self) -> bool {
        matches!(self, TimeAnchor::Solar { .. })
    }

    /// `true` if this is a [`TimeAnchor::Clock`] anchor.
    pub fn is_clock(&self) -> bool {
        matches!(self, TimeAnchor::Clock(_))
    }
}
