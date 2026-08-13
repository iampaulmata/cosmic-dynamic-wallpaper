//! [`Location`] — manually-entered coordinates (FR-002a).

use crate::error::{CoordinateField, LocationError};

/// Manually-entered geographic coordinates used to resolve solar-anchored packs.
///
/// Construct via [`Location::new`], which validates range and finiteness rather than
/// allowing an invalid value to reach the solar calculation (FR-002a, constitution
/// Principle VIII).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Location {
    latitude: f64,
    longitude: f64,
}

impl Location {
    /// Validate and construct a [`Location`].
    ///
    /// `latitude` must be finite and in `[-90.0, 90.0]`; `longitude` must be finite and
    /// in `[-180.0, 180.0]`. Returns [`LocationError`] rather than panicking on invalid
    /// input (FR-002a).
    pub fn new(latitude: f64, longitude: f64) -> Result<Self, LocationError> {
        if !latitude.is_finite() {
            return Err(LocationError::NotFinite {
                field: CoordinateField::Latitude,
                value: latitude,
            });
        }
        if !longitude.is_finite() {
            return Err(LocationError::NotFinite {
                field: CoordinateField::Longitude,
                value: longitude,
            });
        }
        if !(-90.0..=90.0).contains(&latitude) {
            return Err(LocationError::OutOfRange {
                field: CoordinateField::Latitude,
                value: latitude,
            });
        }
        if !(-180.0..=180.0).contains(&longitude) {
            return Err(LocationError::OutOfRange {
                field: CoordinateField::Longitude,
                value: longitude,
            });
        }
        Ok(Self { latitude, longitude })
    }

    /// Latitude in degrees, `[-90.0, 90.0]`.
    pub fn latitude(&self) -> f64 {
        self.latitude
    }

    /// Longitude in degrees, `[-180.0, 180.0]`.
    pub fn longitude(&self) -> f64 {
        self.longitude
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_coordinates() {
        assert!(Location::new(43.6532, -79.3832).is_ok());
        assert!(Location::new(-90.0, -180.0).is_ok());
        assert!(Location::new(90.0, 180.0).is_ok());
        assert!(Location::new(0.0, 0.0).is_ok());
    }

    #[test]
    fn rejects_out_of_range_latitude() {
        assert_eq!(
            Location::new(90.1, 0.0),
            Err(LocationError::OutOfRange {
                field: CoordinateField::Latitude,
                value: 90.1
            })
        );
        assert!(Location::new(-90.1, 0.0).is_err());
    }

    #[test]
    fn rejects_out_of_range_longitude() {
        assert_eq!(
            Location::new(0.0, 180.1),
            Err(LocationError::OutOfRange {
                field: CoordinateField::Longitude,
                value: 180.1
            })
        );
        assert!(Location::new(0.0, -180.1).is_err());
    }

    #[test]
    fn rejects_non_finite() {
        // NaN doesn't equal itself under PartialEq, so match on the field instead of
        // asserting equality against a NaN-carrying expected value.
        assert!(matches!(
            Location::new(f64::NAN, 0.0),
            Err(LocationError::NotFinite {
                field: CoordinateField::Latitude,
                ..
            })
        ));
        assert!(Location::new(f64::INFINITY, 0.0).is_err());
        assert!(Location::new(0.0, f64::NEG_INFINITY).is_err());
    }
}
