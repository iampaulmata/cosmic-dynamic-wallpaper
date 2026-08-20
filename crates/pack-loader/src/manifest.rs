//! [`PackManifest`], [`ManifestImage`], [`ScalingMode`], and [`Color`] — the manifest
//! schema pack authors write, plus parsing/validation from the raw on-disk TOML shape.
//!
//! Anchor strings (`anchor = "sunrise"`, `"civil_dawn-30m"`, `"12:00"`) are parsed here
//! into the scheduling engine's own [`schedule_engine::TimeAnchor`] — the anchor *type*
//! is reused verbatim from that crate, but its compact on-disk string grammar is this
//! crate's own concern, since `TimeAnchor` has no `Deserialize` impl of its own (it's a
//! pure-logic type with no I/O).

use serde::{Deserialize, Serialize};

use chrono::{NaiveTime, TimeDelta};
use schedule_engine::{SolarEventKind, TimeAnchor};

use crate::error::ManifestError;

/// The highest `schema_version` this loader understands.
pub const MAX_SUPPORTED_SCHEMA_VERSION: u32 = 1;

/// Scaling/fit mode — matches `cosmic-bg`'s existing vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalingMode {
    /// Scale to fill the output, cropping any excess (aspect ratio preserved).
    Fill,
    /// Scale to fit entirely within the output, letterboxing any gap with
    /// `fallback_color` (aspect ratio preserved).
    Fit,
    /// Scale to exactly match the output, ignoring aspect ratio.
    Stretch,
    /// Center at native size, letterboxing any gap with `fallback_color`.
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

/// An RGBA fallback fill color for letterboxed edges, parsed from a `#RRGGBB` or
/// `#RRGGBBAA` hex string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    /// Red channel, 0-255.
    pub r: u8,
    /// Green channel, 0-255.
    pub g: u8,
    /// Blue channel, 0-255.
    pub b: u8,
    /// Alpha channel, 0-255 (255 = fully opaque). Defaults to 255 when parsed from a
    /// 6-hex-digit `#RRGGBB` string with no explicit alpha.
    pub a: u8,
}

impl Color {
    fn parse(value: &str) -> Result<Self, ManifestError> {
        let hex = value.strip_prefix('#').unwrap_or(value);
        let bytes = |s: &str| u8::from_str_radix(s, 16).ok();
        let invalid = || ManifestError::InvalidColor { value: value.to_string() };

        // `hex.len()` below counts *bytes*, and the slices further down (`hex[0..2]`
        // etc.) are byte-offset slices — safe only
        // when every byte offset is also a char boundary, which ASCII guarantees and
        // non-ASCII input does not (e.g. "#€AAA" is 6 bytes but "€" alone spans bytes
        // 0..3, so `hex[0..2]` would panic with "byte index 2 is not a char boundary").
        // Reject non-ASCII input up front rather than switching every slice below to
        // char-boundary-aware indexing — a manifest color is hex digits by definition,
        // so non-ASCII is always invalid either way.
        if !hex.is_ascii() {
            return Err(invalid());
        }

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
    /// Path to the image file, relative to the pack directory.
    pub file: String,
    /// When this image becomes active (the scheduling engine's `TimeAnchor`, reused
    /// verbatim).
    pub anchor: TimeAnchor,
    /// Per-image scaling override, if declared (falls back to the pack's
    /// `default_scaling` otherwise).
    pub scaling: Option<ScalingMode>,
}

/// A fully parsed, validated manifest — everything except image-path resolution/
/// containment/readability, which `load.rs` applies next
/// (those checks need the pack directory, which this module doesn't know about).
#[derive(Debug, Clone)]
pub struct PackManifest {
    /// The manifest's declared schema version (checked against
    /// [`MAX_SUPPORTED_SCHEMA_VERSION`] before this struct is ever constructed).
    pub schema_version: u32,
    /// Pack display name.
    pub name: String,
    /// Optional author/license note.
    pub author: Option<String>,
    /// Pack-level default scaling mode, applied to any image with no per-image
    /// override.
    pub default_scaling: ScalingMode,
    /// Fallback fill color for letterboxed edges under `Fit`/`Center` scaling.
    pub fallback_color: Color,
    /// The pack's images, in manifest order.
    pub images: Vec<ManifestImage>,
}

/// The write-side counterpart to [`PackManifest`] — what a caller (the pack-builder GUI
/// wizard) hands to [`render`] to produce `manifest.toml` text, rather than what
/// [`parse`] produces from reading one.
#[derive(Debug, Clone)]
pub struct ManifestDraft {
    /// Pack display name.
    pub name: String,
    /// Optional author/license note — omitted from the rendered TOML entirely when
    /// `None` (matches the format's own "author is optional" rule).
    pub author: Option<String>,
    /// Pack-level default scaling mode.
    pub default_scaling: ScalingMode,
    /// Fallback fill color for letterboxed edges under `Fit`/`Center` scaling.
    pub fallback_color: Color,
    /// The pack's images, in the order they should appear in the rendered manifest.
    pub images: Vec<ManifestDraftImage>,
}

/// A single image entry in a [`ManifestDraft`]. A wizard-generated *new* pack never
/// sets `scaling` — always `None`, inheriting the pack's `default_scaling`. The edit
/// flow is the first caller that ever sets it, specifically to carry an existing
/// per-image override forward unchanged when only the schedule/author/name is what
/// the user actually edited.
#[derive(Debug, Clone)]
pub struct ManifestDraftImage {
    /// File name, relative to the pack directory.
    pub file: String,
    /// When this image becomes active.
    pub anchor: TimeAnchor,
    /// Per-image scaling override, if any — `None` inherits the pack's
    /// `default_scaling` (omitted from the rendered TOML entirely, matching
    /// [`ManifestDraft::author`]'s own "omit when unset" rule).
    pub scaling: Option<ScalingMode>,
}

/// The raw `#[derive(Deserialize)]` shape read directly from TOML, before any semantic
/// validation into the validated `PackManifest`.
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

/// Parse and validate manifest TOML text into a [`PackManifest`]. `path` is used only
/// to name the file in error messages.
pub fn parse(text: &str, path: &std::path::Path) -> Result<PackManifest, ManifestError> {
    let raw: RawManifest = toml::from_str(text)
        .map_err(|e| ManifestError::ParseFailure { path: path.to_path_buf(), message: e.to_string() })?;

    if raw.schema_version > MAX_SUPPORTED_SCHEMA_VERSION {
        return Err(ManifestError::UnsupportedSchemaVersion {
            found: raw.schema_version,
            max_supported: MAX_SUPPORTED_SCHEMA_VERSION,
        });
    }
    // No older schema version exists yet to migrate from (this is version 1) — the
    // migration `match` has a single documented arm today and grows as new versions
    // are introduced.

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

/// Parse a compact anchor string into the scheduling engine's [`TimeAnchor`].
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

/// The `#[derive(Serialize)]` shape [`render`] actually emits — the write-side mirror of
/// [`RawManifest`]/[`RawManifestImage`] above, kept as its own (smaller) shape since a
/// draft never carries a per-image `scaling` override ([`ManifestDraftImage`]).
#[derive(Debug, Serialize)]
struct RawManifestOut {
    schema_version: u32,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    author: Option<String>,
    default_scaling: String,
    fallback_color: String,
    images: Vec<RawManifestImageOut>,
}

#[derive(Debug, Serialize)]
struct RawManifestImageOut {
    file: String,
    anchor: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    scaling: Option<String>,
}

/// Render a [`ManifestDraft`] into `manifest.toml` text — the write-side counterpart
/// to [`parse`]. Always writes `schema_version = 1`. Every string value is routed
/// through `toml`'s own `Serialize` machinery rather than hand-built interpolation, so
/// a `name`/`author`/`file` containing a `"` or non-ASCII text round-trips correctly
/// instead of producing broken TOML.
pub fn render(draft: &ManifestDraft) -> String {
    let raw = RawManifestOut {
        schema_version: 1,
        name: draft.name.clone(),
        author: draft.author.clone(),
        default_scaling: format_scaling(draft.default_scaling).to_string(),
        fallback_color: format_color(draft.fallback_color),
        images: draft
            .images
            .iter()
            .map(|img| RawManifestImageOut {
                file: img.file.clone(),
                anchor: format_anchor(&img.anchor),
                scaling: img.scaling.map(|s| format_scaling(s).to_string()),
            })
            .collect(),
    };
    // `RawManifestOut`'s fields are all plain, always-serializable types (String/u32/a
    // Vec of a plain-string struct) — there is no input shape that can make this fail,
    // so an error here would be a bug in this function, not a caller mistake. Matching
    // this crate's own `unwrap_used`/`expect_used = "deny"` lint, an empty string is a
    // harmless, honest fallback for a case that should be unreachable in practice,
    // rather than a panic.
    toml::to_string(&raw).unwrap_or_default()
}

fn format_scaling(mode: ScalingMode) -> &'static str {
    match mode {
        ScalingMode::Fill => "Fill",
        ScalingMode::Fit => "Fit",
        ScalingMode::Stretch => "Stretch",
        ScalingMode::Center => "Center",
    }
}

/// Inverse of [`Color::parse`]: 6 hex digits when fully opaque (the common case, and
/// what every hand-authored example in `docs/pack-manifest-schema.md` uses), 8 when an
/// explicit alpha is needed.
fn format_color(color: Color) -> String {
    if color.a == 255 {
        format!("#{:02X}{:02X}{:02X}", color.r, color.g, color.b)
    } else {
        format!("#{:02X}{:02X}{:02X}{:02X}", color.r, color.g, color.b, color.a)
    }
}

/// The exact inverse of [`parse_anchor`]: [`TimeAnchor::Clock`] → `"HH:MM"` (never the
/// also-accepted `HH:MM:SS` form — a wizard-authored clock anchor is always whole
/// minutes); [`TimeAnchor::Solar`] → the bare event name, or `"<event><sign><duration>"`
/// when an offset is set. Round-trip contract, asserted directly by this module's own
/// unit tests: `parse_anchor(&format_anchor(&a)) == Ok(a)` for every `a` this crate's
/// callers can construct (whole-minute clock times, offsets with no sub-second part).
pub fn format_anchor(anchor: &TimeAnchor) -> String {
    match anchor {
        TimeAnchor::Clock(time) => time.format("%H:%M").to_string(),
        TimeAnchor::Solar { event, offset } => {
            let event_str = format_solar_event(*event);
            match offset {
                None => event_str.to_string(),
                Some(delta) => format!("{event_str}{}", format_offset(*delta)),
            }
        }
    }
}

fn format_solar_event(event: SolarEventKind) -> &'static str {
    match event {
        SolarEventKind::Sunrise => "sunrise",
        SolarEventKind::Sunset => "sunset",
        SolarEventKind::SolarNoon => "solar_noon",
        SolarEventKind::SolarMidnight => "solar_midnight",
        SolarEventKind::CivilDawn => "civil_dawn",
        SolarEventKind::CivilDusk => "civil_dusk",
        SolarEventKind::AstronomicalDawn => "astronomical_dawn",
        SolarEventKind::AstronomicalDusk => "astronomical_dusk",
    }
}

/// Formats a signed offset the way [`parse_anchor`] accepts it back: a leading `+`/`-`,
/// then the magnitude as compact `<h>h<m>m<s>s` components — only non-zero components
/// included, `0s` if the magnitude is exactly zero so the output is never empty.
fn format_offset(delta: TimeDelta) -> String {
    let negative = delta < TimeDelta::zero();
    let sign = if negative { '-' } else { '+' };
    let magnitude = if negative { -delta } else { delta };
    let total_seconds = magnitude.num_seconds();
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    let mut out = String::new();
    if hours > 0 {
        out.push_str(&format!("{hours}h"));
    }
    if minutes > 0 {
        out.push_str(&format!("{minutes}m"));
    }
    if seconds > 0 || out.is_empty() {
        out.push_str(&format!("{seconds}s"));
    }
    format!("{sign}{out}")
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

    /// A hex-color string containing a non-ASCII byte must return `Err`, never panic
    /// on a non-char-boundary byte slice. "€" is 3 bytes (0xE2 0x82 0xAC), so "#€AAA"
    /// is 6 bytes total, reaching the 6-hex-digit branch — without the ASCII check
    /// above, `hex[0..2]` would panic on it.
    #[test]
    fn color_parse_rejects_non_ascii_hex() {
        assert!(Color::parse("#€AAA").is_err());
        assert!(Color::parse("€AAAAA").is_err());
        // A non-ASCII value whose byte length doesn't even match 6/8 either way —
        // guards the same path via the `_ =>` arm, not just the 6-byte arm above.
        assert!(Color::parse("#€").is_err());
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

    // --- Custom Pack Builder write-side tests ---

    /// `format_anchor` is the exact inverse of `parse_anchor` for every anchor shape
    /// this crate's callers can construct.
    #[test]
    fn format_anchor_round_trips_clock_and_solar_anchors() {
        let cases = [
            TimeAnchor::clock(NaiveTime::from_hms_opt(6, 30, 0).unwrap()),
            TimeAnchor::clock(NaiveTime::from_hms_opt(0, 0, 0).unwrap()),
            TimeAnchor::solar(SolarEventKind::Sunrise, None),
            TimeAnchor::solar(SolarEventKind::SolarMidnight, None),
            TimeAnchor::solar(SolarEventKind::CivilDawn, Some(-TimeDelta::minutes(30))),
            TimeAnchor::solar(SolarEventKind::Sunset, Some(TimeDelta::hours(1))),
            TimeAnchor::solar(SolarEventKind::AstronomicalDusk, Some(TimeDelta::minutes(75))),
            TimeAnchor::solar(SolarEventKind::CivilDusk, Some(-TimeDelta::hours(12))),
        ];
        for anchor in cases {
            let formatted = format_anchor(&anchor);
            assert_eq!(
                parse_anchor(&formatted),
                Ok(anchor),
                "format_anchor({anchor:?}) -> {formatted:?} didn't parse back to the original value"
            );
        }
    }

    fn draft_color() -> Color {
        Color { r: 0, g: 0, b: 0, a: 255 }
    }

    /// An author name containing a `"` and non-ASCII text round-trips through
    /// `render` → `parse` byte-identical — the reason `render` routes through `toml`'s
    /// serializer instead of hand-built string interpolation.
    #[test]
    fn render_escapes_quotes_and_unicode_in_author_and_name() {
        let draft = ManifestDraft {
            name: "Jane's \"Favorites\" — 日本語".to_string(),
            author: Some("Jane \"J.\" Author — CC-BY-4.0".to_string()),
            default_scaling: ScalingMode::Fill,
            fallback_color: draft_color(),
            images: vec![ManifestDraftImage {
                file: "dawn.jpg".to_string(),
                anchor: TimeAnchor::solar(SolarEventKind::Sunrise, None),
                scaling: None,
            }],
        };

        let text = render(&draft);
        let parsed = parse(&text, std::path::Path::new("manifest.toml")).unwrap();

        assert_eq!(parsed.name, draft.name);
        assert_eq!(parsed.author, draft.author);
        assert_eq!(parsed.images.len(), 1);
        assert_eq!(parsed.images[0].file, "dawn.jpg");
    }

    /// `author: None` omits the key entirely rather than writing an empty string.
    #[test]
    fn render_omits_author_when_none() {
        let draft = ManifestDraft {
            name: "No Author".to_string(),
            author: None,
            default_scaling: ScalingMode::Fit,
            fallback_color: draft_color(),
            images: vec![ManifestDraftImage {
                file: "a.png".to_string(),
                anchor: TimeAnchor::clock(NaiveTime::from_hms_opt(12, 0, 0).unwrap()),
                scaling: None,
            }],
        };

        let text = render(&draft);
        assert!(!text.contains("author"), "expected no `author` key, got:\n{text}");
        let parsed = parse(&text, std::path::Path::new("manifest.toml")).unwrap();
        assert_eq!(parsed.author, None);
    }

    /// `render`'s output is immediately valid, parseable TOML matching every field of
    /// the draft it was built from — the postcondition the pack-builder wizard's own
    /// self-validation (`pack_loader::load_pack`) relies on.
    #[test]
    fn render_produces_a_fully_valid_manifest() {
        let draft = ManifestDraft {
            name: "Solar Draft".to_string(),
            author: Some("Test Author".to_string()),
            default_scaling: ScalingMode::Fill,
            fallback_color: Color { r: 255, g: 0, b: 0, a: 128 },
            images: vec![
                ManifestDraftImage {
                    file: "a.png".to_string(),
                    anchor: TimeAnchor::solar(SolarEventKind::Sunrise, None),
                    scaling: None,
                },
                ManifestDraftImage {
                    file: "b.png".to_string(),
                    anchor: TimeAnchor::solar(SolarEventKind::Sunset, Some(TimeDelta::minutes(-30))),
                    scaling: None,
                },
            ],
        };

        let text = render(&draft);
        let parsed = parse(&text, std::path::Path::new("manifest.toml")).unwrap();

        assert_eq!(parsed.schema_version, 1);
        assert_eq!(parsed.name, "Solar Draft");
        assert_eq!(parsed.author.as_deref(), Some("Test Author"));
        assert_eq!(parsed.default_scaling, ScalingMode::Fill);
        assert_eq!(parsed.fallback_color, Color { r: 255, g: 0, b: 0, a: 128 });
        assert_eq!(parsed.images.len(), 2);
        assert_eq!(parsed.images[0].anchor, TimeAnchor::solar(SolarEventKind::Sunrise, None));
        assert_eq!(
            parsed.images[1].anchor,
            TimeAnchor::solar(SolarEventKind::Sunset, Some(TimeDelta::minutes(-30)))
        );
    }

    /// A `ManifestDraftImage.scaling` override round-trips through `render`/`parse`
    /// unchanged, and `None` still omits the `scaling` key entirely rather than
    /// writing an empty/default one — the edit flow's own preserved-field
    /// carry-forward depends on both halves of this.
    #[test]
    fn render_round_trips_a_per_image_scaling_override_and_omits_it_when_none() {
        let draft = ManifestDraft {
            name: "Mixed Scaling".to_string(),
            author: None,
            default_scaling: ScalingMode::Fill,
            fallback_color: Color { r: 0, g: 0, b: 0, a: 255 },
            images: vec![
                ManifestDraftImage {
                    file: "a.png".to_string(),
                    anchor: TimeAnchor::solar(SolarEventKind::Sunrise, None),
                    scaling: Some(ScalingMode::Center),
                },
                ManifestDraftImage {
                    file: "b.png".to_string(),
                    anchor: TimeAnchor::solar(SolarEventKind::Sunset, None),
                    scaling: None,
                },
            ],
        };

        let text = render(&draft);
        assert!(text.contains("scaling = \"Center\""), "explicit override must be written: {text}");
        let parsed = parse(&text, std::path::Path::new("manifest.toml")).unwrap();

        assert_eq!(parsed.images[0].scaling, Some(ScalingMode::Center));
        assert_eq!(parsed.images[1].scaling, None, "an image with no override must parse back to None, not Fill");
    }
}
