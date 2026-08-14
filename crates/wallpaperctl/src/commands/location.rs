//! `wallpaperctl location get|set|clear` (FR-008).

use cosmic_config::Config;
use schedule_engine::Location;
use serde::Serialize;

use crate::config::LocationConfigEntry;
use crate::error::CliError;
use crate::output::{self, Ack};

#[derive(Debug, Serialize)]
struct LocationResponse {
    location: Option<Location>,
}

pub fn get(config: &Config, json: bool) -> String {
    let state = LocationConfigEntry::load(config);
    let response = LocationResponse { location: state.location };
    output::render(json, &response, || match response.location {
        Some(loc) => format!("{} {}", loc.latitude(), loc.longitude()),
        None => "no location set".to_string(),
    })
}

/// Scenario 3: `Location::new`'s validation runs *before* anything is written — an
/// invalid value is never partially applied.
pub fn set(config: &Config, latitude: f64, longitude: f64, json: bool) -> Result<String, CliError> {
    let location = Location::new(latitude, longitude)?;
    LocationConfigEntry { location: Some(location) }.save(config)?;
    Ok(output::render(json, &Ack::ok(), || format!("location set to {latitude} {longitude}")))
}

pub fn clear(config: &Config, json: bool) -> Result<String, CliError> {
    LocationConfigEntry { location: None }.save(config)?;
    Ok(output::render(json, &Ack::ok(), || "location cleared".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_config() -> (Config, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        (LocationConfigEntry::open_at(dir.path()).unwrap(), dir)
    }

    /// Scenario 1: setting a location persists it, and a subsequent read matches.
    #[test]
    fn set_then_get_matches() {
        let (config, _dir) = temp_config();
        set(&config, 45.5019, -73.5674, false).unwrap();
        let state = LocationConfigEntry::load(&config);
        let loc = state.location.unwrap();
        assert_eq!((loc.latitude(), loc.longitude()), (45.5019, -73.5674));
    }

    /// Scenario 2: setting a new location replaces the old one.
    #[test]
    fn setting_again_replaces_the_old_value() {
        let (config, _dir) = temp_config();
        set(&config, 45.5019, -73.5674, false).unwrap();
        set(&config, 51.5072, -0.1276, false).unwrap();
        let state = LocationConfigEntry::load(&config);
        let loc = state.location.unwrap();
        assert_eq!((loc.latitude(), loc.longitude()), (51.5072, -0.1276));
    }

    /// Scenario 3: an out-of-range value is rejected, with no partial write.
    #[test]
    fn rejects_out_of_range_value_with_no_write() {
        let (config, _dir) = temp_config();
        let result = set(&config, 200.0, 0.0, false);
        assert!(matches!(result, Err(CliError::InvalidLocation { .. })));
        assert!(LocationConfigEntry::load(&config).location.is_none());
    }

    /// Scenario 4: clearing a location removes it.
    #[test]
    fn clear_removes_the_location() {
        let (config, _dir) = temp_config();
        set(&config, 45.5019, -73.5674, false).unwrap();
        clear(&config, false).unwrap();
        assert!(LocationConfigEntry::load(&config).location.is_none());
    }
}
