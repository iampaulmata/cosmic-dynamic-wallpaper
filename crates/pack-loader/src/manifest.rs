//! [`PackManifest`], [`ManifestImage`], [`ScalingMode`], and [`Color`] — the manifest
//! schema pack authors write (contracts/pack-loader-api.md), plus parsing/validation
//! from the raw on-disk TOML shape (FR-001, FR-002, FR-005, FR-006, FR-007).
//!
//! Anchor strings (`anchor = "sunrise"`, `"civil_dawn-30m"`, `"12:00"`) are parsed here
//! into spec 1's own [`schedule_engine::TimeAnchor`] — the anchor *type* is reused
//! verbatim from spec 1 (data-model.md), but its compact on-disk string grammar is this
//! crate's own concern, since spec 1's `TimeAnchor` has no `Deserialize` impl of its own
//! (it's a pure-logic type with no I/O, per spec 1's own scope).

use serde::Deserialize;

use chrono::{NaiveTime, TimeDelta};
use schedule_engine::{SolarEventKind, TimeAnchor};

use crate::error::ManifestError;

/// The highest `schema_version` this loader understands (FR-007, research.md R5).
pub const MAX_SUPPORTED_SCHEMA_VERSION: u32 = 1;

/// Scaling/fit mode (FR-005) — matches `cosmic-bg`'s existing vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalingMode {
    Fill,
    Fit,
    Stretch,
    Center,
}

impl ScalingMode {
    fn parse(value: &str) -> Result<Self, ManifestError> {
        match value.to_ascii_lowercase().as_str() {
            "fill" => Ok(ScalingMode::Fill),
            "fit" => Ok(ScalingMode::Fit),
            "stretch" => Ok(ScalingMode::Stretch),
            "center" => Ok(ScalingMode::Center),
            _ => Err(ManifestError::InvalidScalingMode { value: value.to_string() }),
        }
    }
}

/// An RGBA fallback fill color for letterboxed edges (FR-005), parsed from a `#RRGGBB`
/// or `#RRGGBBAA` hex string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    fn parse(value: &str) -> Result<Self, ManifestError> {
        let hex = value.strip_prefix('#').unwrap_or(value);
        let bytes = |s: &str| u8::from_str_radix(s, 16).ok();
        let invalid = || ManifestError::InvalidColor { value: value.to_string() };

        match hex.len() {
            6 => {
                let r = bytes(&hex[0..2]).ok_or_else(invalid)?;
                let g = bytes(&hex[2..4]).ok_or_else(invalid)?;
                let b = bytes(&hex[4..6]).ok_or_else(invalid)?;
                Ok(Color { r, g, b, a: 255 })
            }
            8 => {
                let r = bytes(&hex[0..2]).ok_or_else(invalid)?;
                let g = bytes(&hex[2..4]).ok_or_else(invalid)?;
                let b = bytes(&hex[4..6]).ok_or_else(invalid)?;
                let a = bytes(&hex[6..8]).ok_or_else(invalid)?;
                Ok(Color { r, g, b, a })
            }
            _ => Err(invalid()),
        }
    }
}

/// A single image entry, parsed and validated (`file`/`anchor`/`scaling` all resolved
/// from their raw TOML string forms).
#[derive(Debug, Clone)]
pub struct ManifestImage {
    pub file: String,
    pub anchor: TimeAnchor,
    pub scaling: Option<ScalingMode>,
}

/// A fully parsed, validated manifest (data-model.md `PackManifest`) — everything
/// except image-path resolution/containment/readability, which `load.rs` applies next
/// (those checks need the pack directory, which this module doesn't know about).
#[derive(Debug, Clone)]
pub struct PackManifest {
    pub schema_version: u32,
    pub name: String,
    pub author: Option<String>,
    pub default_scaling: ScalingMode,
    pub fallback_color: Color,
    pub images: Vec<ManifestImage>,
}

/// The raw `#[derive(Deserialize)]` shape read directly from TOML, before any semantic
/// validation (data-model.md's distinction between the on-disk shape and the validated
/// `PackManifest`).
#[derive(Debug, Deserialize)]
struct RawManifest {
    schema_version: u32,
    name: String,
    #[serde(default)]
    author: Option<String>,
    default_scaling: String,
    fallback_color: String,
    #[serde(default)]
    images: Vec<RawManifestImage>,
}

#[derive(Debug, Deserialize)]
struct RawManifestImage {
    file: String,
    anchor: String,
    #[serde(default)]
    scaling: Option<String>,
}

/// Parse and validate manifest TOML text into a [`PackManifest`] (FR-002, FR-006,
/// FR-007). `path` is used only to name the file in error messages.
pub fn parse(text: &str, path: &std::path::Path) -> Result<PackManifest, ManifestError> {
    let raw: RawManifest = toml::from_str(text)
        .map_err(|e| ManifestError::ParseFailure { path: path.to_path_buf(), message: e.to_string() })?;

    if raw.schema_version > MAX_SUPPORTED_SCHEMA_VERSION {
        return Err(ManifestError::UnsupportedSchemaVersion {
            found: raw.schema_version,
            max_supported: MAX_SUPPORTED_SCHEMA_VERSION,
        });
    }
    // No older schema version exists yet to migrate from (this is version 1) —
    // research.md R5's migration `match` has a single documented arm today and grows
    // as new versions are introduced.

    let default_scaling = ScalingMode::parse(&raw.default_scaling)?;
    let fallback_color = Color::parse(&raw.fallback_color)?;

    let images = raw
        .images
        .into_iter()
        .map(|img| {
            let anchor = parse_anchor(&img.anchor)
                .map_err(|_| ManifestError::InvalidAnchor { file: img.file.clone(), value: img.anchor.clone() })?;
            let scaling = img.scaling.as_deref().map(ScalingMode::parse).transpose()?;
            Ok(ManifestImage { file: img.file, anchor, scaling })
        })
        .collect::<Result<Vec<_>, ManifestError>>()?;

    Ok(PackManifest {
        schema_version: raw.schema_version,
        name: raw.name,
        author: raw.author,
        default_scaling,
        fallback_color,
        images,
    })
}

/// Parse a compact anchor string into spec 1's [`TimeAnchor`].
///
/// Grammar:
/// - `HH:MM` (contains `:`) → [`TimeAnchor::Clock`].
/// - `<event>` or `<event><+|-><duration>` → [`TimeAnchor::Solar`], where `<event>` is
///   one of the eight snake_case names (`sunrise`, `sunset`, `solar_noon`,
///   `solar_midnight`, `civil_dawn`, `civil_dusk`, `astronomical_dawn`,
///   `astronomical_dusk`) and `<duration>` is any `humantime`-parseable duration (e.g.
///   `30m`, `1h`, `1h30m`).
fn parse_anchor(value: &str) -> Result<TimeAnchor, ()> {
    if value.contains(':') {
        let time = NaiveTime::parse_from_str(value, "%H:%M")
            .or_else(|_| NaiveTime::parse_from_str(value, "%H:%M:%S"))
            .map_err(|_| ())?;
        return Ok(TimeAnchor::clock(time));
    }

    // Split at the first '+'/'-' after position 0 (event names never start with one).
    let split_at = value.char_indices().skip(1).find(|(_, c)| *c == '+' || *c == '-').map(|(i, _)| i);

    let (event_str, offset) = match split_at {
        None => (value, None),
        Some(i) => {
            let (event_str, rest) = value.split_at(i);
            let sign = if rest.starts_with('-') { -1 } else { 1 };
            let magnitude = &rest[1..];
            let duration = humantime::parse_duration(magnitude).map_err(|_| ())?;
            let millis = i64::try_from(duration.as_millis()).map_err(|_| ())?;
            (event_str, Some(TimeDelta::milliseconds(sign * millis)))
        }
    };

    let event = match event_str {
        "sunrise" => SolarEventKind::Sunrise,
        "sunset" => SolarEventKind::Sunset,
        "solar_noon" => SolarEventKind::SolarNoon,
        "solar_midnight" => SolarEventKind::SolarMidnight,
        "civil_dawn" => SolarEventKind::CivilDawn,
        "civil_dusk" => SolarEventKind::CivilDusk,
        "astronomical_dawn" => SolarEventKind::AstronomicalDawn,
        "astronomical_dusk" => SolarEventKind::AstronomicalDusk,
        _ => return Err(()),
    };

    Ok(TimeAnchor::solar(event, offset))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_solar_event() {
        assert_eq!(parse_anchor("sunrise"), Ok(TimeAnchor::solar(SolarEventKind::Sunrise, None)));
        assert_eq!(parse_anchor("solar_noon"), Ok(TimeAnchor::solar(SolarEventKind::SolarNoon, None)));
    }

    #[test]
    fn parses_offset_solar_event() {
        assert_eq!(
            parse_anchor("civil_dawn-30m"),
            Ok(TimeAnchor::solar(SolarEventKind::CivilDawn, Some(-TimeDelta::minutes(30))))
        );
        assert_eq!(
            parse_anchor("sunset+1h"),
            Ok(TimeAnchor::solar(SolarEventKind::Sunset, Some(TimeDelta::hours(1))))
        );
    }

    #[test]
    fn parses_clock_time() {
        assert_eq!(
            parse_anchor("06:30"),
            Ok(TimeAnchor::clock(NaiveTime::from_hms_opt(6, 30, 0).unwrap()))
        );
    }

    #[test]
    fn rejects_unknown_event_name() {
        assert!(parse_anchor("moonrise").is_err());
    }

    #[test]
    fn rejects_malformed_offset() {
        assert!(parse_anchor("sunrise-notaduration").is_err());
    }

    #[test]
    fn scaling_mode_parses_case_insensitively() {
        assert_eq!(ScalingMode::parse("Fill").unwrap(), ScalingMode::Fill);
        assert_eq!(ScalingMode::parse("fit").unwrap(), ScalingMode::Fit);
        assert_eq!(ScalingMode::parse("STRETCH").unwrap(), ScalingMode::Stretch);
        assert_eq!(ScalingMode::parse("Center").unwrap(), ScalingMode::Center);
        assert!(ScalingMode::parse("Zoom").is_err());
    }

    #[test]
    fn color_parses_rgb_and_rgba_hex() {
        assert_eq!(Color::parse("#000000").unwrap(), Color { r: 0, g: 0, b: 0, a: 255 });
        assert_eq!(Color::parse("#FFFFFF").unwrap(), Color { r: 255, g: 255, b: 255, a: 255 });
        assert_eq!(Color::parse("#FF000080").unwrap(), Color { r: 255, g: 0, b: 0, a: 128 });
        assert!(Color::parse("not-a-color").is_err());
        assert!(Color::parse("#ZZZZZZ").is_err());
    }

    #[test]
    fn parses_full_manifest_toml() {
        let toml = r##"
schema_version = 1
name = "Example Pack"
author = "Jane Author"
default_scaling = "Fill"
fallback_color = "#000000"

[[images]]
file = "dawn.jpg"
anchor = "sunrise"

[[images]]
file = "noon.jpg"
anchor = "solar_noon"
scaling = "Fit"
"##;
        let manifest = parse(toml, std::path::Path::new("manifest.toml")).unwrap();
        assert_eq!(manifest.name, "Example Pack");
        assert_eq!(manifest.images.len(), 2);
        assert_eq!(manifest.images[1].scaling, Some(ScalingMode::Fit));
    }

    #[test]
    fn rejects_unsupported_schema_version() {
        let toml = r##"
schema_version = 99
name = "Future Pack"
default_scaling = "Fill"
fallback_color = "#000000"
"##;
        let result = parse(toml, std::path::Path::new("manifest.toml"));
        assert!(matches!(result, Err(ManifestError::UnsupportedSchemaVersion { found: 99, .. })));
    }

    #[test]
    fn rejects_malformed_toml() {
        let result = parse("this is not { valid toml", std::path::Path::new("manifest.toml"));
        assert!(matches!(result, Err(ManifestError::ParseFailure { .. })));
    }
}
