//! Error types for [`crate::Location`] validation and [`crate::WallpaperPack`] validation.
//!
//! Every fallible path in this crate returns one of these rather than panicking
//! (constitution Principle VIII) — see `data-model.md`'s "Error types" section.

use core::fmt;

/// Errors constructing a [`crate::Location`] (FR-002a).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LocationError {
    /// Latitude or longitude was outside its valid range
    /// (`[-90.0, 90.0]` for latitude, `[-180.0, 180.0]` for longitude).
    OutOfRange {
        /// Which field was out of range.
        field: CoordinateField,
        /// The offending value.
        value: f64,
    },
    /// Latitude or longitude was not a finite number (NaN or ±infinity).
    NotFinite {
        /// Which field was non-finite.
        field: CoordinateField,
        /// The offending value.
        value: f64,
    },
}

/// Which coordinate field a [`LocationError`] refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateField {
    /// Latitude, degrees.
    Latitude,
    /// Longitude, degrees.
    Longitude,
}

impl fmt::Display for CoordinateField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CoordinateField::Latitude => write!(f, "latitude"),
            CoordinateField::Longitude => write!(f, "longitude"),
        }
    }
}

impl fmt::Display for LocationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LocationError::OutOfRange { field, value } => {
                write!(f, "{field} {value} is out of range")
            }
            LocationError::NotFinite { field, value } => {
                write!(f, "{field} {value} is not a finite number")
            }
        }
    }
}

impl std::error::Error for LocationError {}

/// Errors validating a [`crate::WallpaperPack`] (FR-001, FR-006, FR-006a).
#[derive(Debug, Clone, PartialEq)]
pub enum PackError {
    /// The pack contained no images at all (FR-1).
    Empty,
    /// The pack contained more than 64 anchors (FR-001).
    TooManyAnchors {
        /// How many anchors were actually supplied.
        count: usize,
    },
    /// The pack mixed `Solar` and `Clock` anchors (FR-6, FR-006).
    MixedAnchorTypes,
    /// Two or more anchors resolved to the exact same instant (FR-006a).
    DuplicateInstant,
    /// Two or more images shared the same image identifier.
    DuplicateImageId,
    /// A solar-anchored image's offset exceeded [`crate::pack::MAX_SOLAR_OFFSET_HOURS`]
    /// (spec 011 US1 FR-004) — unbounded, this reaches `DateTime + TimeDelta`
    /// arithmetic at query time and overflows/panics; bounding it here keeps that
    /// unreachable.
    SolarOffsetOutOfRange {
        /// The solar event the out-of-range offset was attached to.
        event: crate::anchor::SolarEventKind,
        /// The offending offset.
        offset: chrono::TimeDelta,
    },
}

impl fmt::Display for PackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PackError::Empty => write!(f, "pack contains no images"),
            PackError::TooManyAnchors { count } => {
                write!(f, "pack contains {count} anchors, which exceeds the limit of 64")
            }
            PackError::MixedAnchorTypes => {
                write!(f, "pack mixes solar-anchored and clock-anchored images")
            }
            PackError::DuplicateInstant => {
                write!(f, "two or more anchors resolve to the exact same instant")
            }
            PackError::DuplicateImageId => write!(f, "two or more images share the same id"),
            PackError::SolarOffsetOutOfRange { event, offset } => {
                // `offset:?` (chrono's own `Debug` for `TimeDelta`), not a hand-computed
                // `.num_hours()` — this variant exists specifically for offsets large
                // enough that further arithmetic on them (even just formatting) should
                // stay within chrono's own, already-overflow-safe `Debug` impl rather
                // than this crate re-deriving a magnitude by hand.
                write!(f, "solar offset {offset:?} on {event:?} exceeds the {}-hour limit", crate::pack::MAX_SOLAR_OFFSET_HOURS)
            }
        }
    }
}

impl std::error::Error for PackError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn location_error_display_is_readable() {
        assert_eq!(
            LocationError::OutOfRange { field: CoordinateField::Latitude, value: 200.0 }.to_string(),
            "latitude 200 is out of range"
        );
        assert_eq!(
            LocationError::NotFinite { field: CoordinateField::Longitude, value: f64::NAN }.to_string(),
            "longitude NaN is not a finite number"
        );
    }

    #[test]
    fn coordinate_field_display() {
        assert_eq!(CoordinateField::Latitude.to_string(), "latitude");
        assert_eq!(CoordinateField::Longitude.to_string(), "longitude");
    }

    #[test]
    fn pack_error_display_is_readable() {
        assert_eq!(PackError::Empty.to_string(), "pack contains no images");
        assert_eq!(
            PackError::TooManyAnchors { count: 65 }.to_string(),
            "pack contains 65 anchors, which exceeds the limit of 64"
        );
        assert_eq!(
            PackError::MixedAnchorTypes.to_string(),
            "pack mixes solar-anchored and clock-anchored images"
        );
        assert_eq!(
            PackError::DuplicateInstant.to_string(),
            "two or more anchors resolve to the exact same instant"
        );
        assert_eq!(PackError::DuplicateImageId.to_string(), "two or more images share the same id");
    }

    #[test]
    fn errors_implement_std_error() {
        fn assert_error<E: std::error::Error>() {}
        assert_error::<LocationError>();
        assert_error::<PackError>();
    }
}
