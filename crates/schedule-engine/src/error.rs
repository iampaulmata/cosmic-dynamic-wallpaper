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
        }
    }
}

impl std::error::Error for PackError {}
