//! `wallpaperctl location get|set|clear|auto|manual|ip` (spec 4 FR-008; spec 6 FR-001/
//! FR-002/FR-003/FR-007/FR-009; spec 7 FR-012/FR-013/FR-014). All six subcommands are
//! daemon-optional (spec 6 FR-012) — every one reads/writes `cosmic-config` only.

use cosmic_config::Config;
use schedule_engine::Location;
use serde::Serialize;
use wallpaper_ipc::{effective_location, LocationConfigEntry, LocationMode, ResolutionStatus, IP_GEOLOCATION_DISCLOSURE};

use crate::error::CliError;
use crate::output::{self, Ack};

/// The `--json` shape for a resolution status field, matching this project's
/// established convention: `{"state":"resolved"}` / `{"state":"unresolved"}` /
/// `{"state":"unavailable","reason":"..."}`.
#[derive(Debug, Serialize)]
#[serde(tag = "state", rename_all = "lowercase")]
enum StatusJson {
    Unresolved,
    Resolved,
    Unavailable { reason: String },
}

impl From<&ResolutionStatus> for StatusJson {
    fn from(status: &ResolutionStatus) -> Self {
        match status {
            ResolutionStatus::Unresolved => StatusJson::Unresolved,
            ResolutionStatus::Resolved => StatusJson::Resolved,
            ResolutionStatus::Unavailable { reason } => StatusJson::Unavailable { reason: reason.clone() },
        }
    }
}

fn status_human(status: &ResolutionStatus) -> String {
    match status {
        ResolutionStatus::Unresolved => "unresolved".to_string(),
        ResolutionStatus::Resolved => "resolved".to_string(),
        ResolutionStatus::Unavailable { reason } => format!("unavailable ({reason})"),
    }
}

fn mode_str(mode: LocationMode) -> &'static str {
    match mode {
        LocationMode::Manual => "manual",
        LocationMode::Automatic => "automatic",
        LocationMode::IpGeolocation => "ip_geolocation",
    }
}

/// The resolution status relevant to the *currently active* mode — `Manual` mode has
/// no resolution attempt of its own, so it's reported as `Unresolved` regardless of
/// whatever `automatic_status`/`ip_status` happen to hold from a previously-active
/// mode (spec 6/7's own posture: those fields are preserved, not reset, but showing a
/// stale non-Manual status while in Manual mode would be misleading).
fn current_status(state: &LocationConfigEntry) -> ResolutionStatus {
    match state.mode {
        LocationMode::Manual => ResolutionStatus::Unresolved,
        LocationMode::Automatic => state.automatic_status.clone(),
        LocationMode::IpGeolocation => state.ip_status.clone(),
    }
}

#[derive(Debug, Serialize)]
struct LocationGetResponse {
    mode: &'static str,
    status: StatusJson,
    location: Option<Location>,
    manual_location: Option<Location>,
    automatic_location: Option<Location>,
    ip_location: Option<Location>,
    /// Spec 011 US6 FR-023 (research.md R18): `true` only if the on-disk config was
    /// genuinely unreadable/corrupt (not the ordinary "never configured" case), which
    /// this response's other fields can't otherwise distinguish from "no location was
    /// ever set."
    config_read_error: bool,
}

/// spec.md US4 Scenario 1, SC-004 (spec 6); extended for spec 7's third mode: reports
/// `mode`, the active mode's `status`, and the effective location
/// (`wallpaper_ipc::effective_location`) for every `(mode, status)` combination,
/// daemon-optional.
pub fn get(config: &Config, json: bool) -> String {
    // Spec 011 US6 FR-023 (research.md R18): distinguishes a genuinely corrupted
    // config file from "never configured" — both previously reported identically as
    // "no location available." Exit code stays 0 in both cases regardless (neither is
    // a fatal error, constitution Principle VIII); this only makes the *message*
    // accurate.
    let (state, config_read_error) = LocationConfigEntry::load_reporting_corruption(config);
    let effective = effective_location(&state);
    let status = current_status(&state);
    // The displayed value's provenance, matching `effective_location()`'s own
    // priority (mode-specific resolved value before the manual fallback).
    let from_automatic = matches!(state.mode, LocationMode::Automatic) && state.automatic_location.is_some();
    let from_ip = matches!(state.mode, LocationMode::IpGeolocation) && state.ip_location.is_some();

    let response = LocationGetResponse {
        mode: mode_str(state.mode),
        status: (&status).into(),
        location: effective,
        manual_location: state.location,
        automatic_location: state.automatic_location,
        ip_location: state.ip_location,
        config_read_error,
    };

    output::render(json, &response, || {
        let location_line = match effective {
            Some(loc) if from_automatic => format!("{} {}  (from automatic resolution)", loc.latitude(), loc.longitude()),
            Some(loc) if from_ip => format!("{} {}  (from IP-geolocation)", loc.latitude(), loc.longitude()),
            Some(loc) => format!("{} {}", loc.latitude(), loc.longitude()),
            None if config_read_error => "unavailable — the location config could not be read (corrupted?); treating as unset".to_string(),
            None => "no location available".to_string(),
        };
        format!("mode: {}\nstatus: {}\nlocation: {location_line}", mode_str(state.mode), status_human(&status))
    })
}

/// Scenario 3: `Location::new`'s validation runs *before* anything is written — an
/// invalid value is never partially applied. Also sets `mode: Manual` (spec 6
/// research.md R7) — setting a manual value while remaining in a non-manual mode would
/// have no observable effect, a worse default than switching modes explicitly.
pub fn set(config: &Config, latitude: f64, longitude: f64, json: bool) -> Result<String, CliError> {
    let location = Location::new(latitude, longitude)?;
    let mut state = LocationConfigEntry::load(config);
    state.location = Some(location);
    state.mode = LocationMode::Manual;
    state.save(config)?;
    Ok(output::render(json, &Ack::ok(), || format!("location set to {latitude} {longitude}")))
}

/// Clears `location` only — `mode` and every resolution-status field are left
/// untouched (spec 6 research.md R7): expanding `clear`'s scope to also flip mode
/// would be a surprising side effect for existing users of this command.
pub fn clear(config: &Config, json: bool) -> Result<String, CliError> {
    let mut state = LocationConfigEntry::load(config);
    state.location = None;
    state.save(config)?;
    Ok(output::render(json, &Ack::ok(), || "location cleared".to_string()))
}

/// Enables automatic (portal) mode (spec 6 FR-001/FR-002/FR-003). Idempotent — calling
/// it while already in automatic mode is a no-op success, not an error. Writes
/// `mode: Automatic` only: doesn't touch `location`/`automatic_location`/
/// `automatic_status`, and doesn't itself attempt a resolution (that's `wallpaperd`'s
/// job, once running).
pub fn auto(config: &Config, json: bool) -> Result<String, CliError> {
    let mut state = LocationConfigEntry::load(config);
    state.mode = LocationMode::Automatic;
    state.save(config)?;
    Ok(output::render(json, &Ack::ok(), || "automatic location enabled (resolving…)".to_string()))
}

/// Enables IP-geolocation mode (spec 7 FR-012/FR-013). Idempotent, same posture as
/// [`auto`] — writes `mode: IpGeolocation` only; the actual STUN/`maxminddb`
/// resolution is `wallpaperd`'s job (`ip_geolocation.rs`). The success message itself
/// carries [`IP_GEOLOCATION_DISCLOSURE`] (FR-014) so the one external touchpoint is
/// disclosed at the moment of opting in, not buried in documentation.
pub fn ip(config: &Config, json: bool) -> Result<String, CliError> {
    let mut state = LocationConfigEntry::load(config);
    state.mode = LocationMode::IpGeolocation;
    state.save(config)?;
    Ok(output::render(json, &Ack::ok(), || format!("Enabled — {IP_GEOLOCATION_DISCLOSURE} Resolving…")))
}

/// Switches back to manual mode using whatever value is already stored in `location`,
/// with no re-entry (spec 6 FR-007/FR-009). Writes `mode: Manual` only. Never fails —
/// there is no invalid state to reject.
pub fn manual(config: &Config, json: bool) -> Result<String, CliError> {
    let mut state = LocationConfigEntry::load(config);
    let restored = state.location;
    state.mode = LocationMode::Manual;
    state.save(config)?;
    Ok(output::render(json, &Ack::ok(), || match restored {
        Some(loc) => format!("manual location restored: {} {}", loc.latitude(), loc.longitude()),
        None => "manual mode set (no location stored — only clock-anchored packs usable)".to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmic_config::CosmicConfigEntry;

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

    /// `set` also writes `mode: Manual` — a documented deliberate side effect
    /// (research.md R7), not accidental scope creep.
    #[test]
    fn set_also_switches_mode_to_manual() {
        let (config, _dir) = temp_config();
        auto(&config, false).unwrap();
        assert_eq!(LocationConfigEntry::load(&config).mode, LocationMode::Automatic);

        set(&config, 45.5019, -73.5674, false).unwrap();
        assert_eq!(LocationConfigEntry::load(&config).mode, LocationMode::Manual);
    }

    /// `clear` continues to leave `mode`/resolution-status fields untouched.
    #[test]
    fn clear_leaves_mode_and_automatic_fields_untouched() {
        let (config, _dir) = temp_config();
        auto(&config, false).unwrap();
        clear(&config, false).unwrap();
        let state = LocationConfigEntry::load(&config);
        assert_eq!(state.mode, LocationMode::Automatic);
        assert_eq!(state.location, None);
    }

    /// `auto` sets `mode: Automatic` only, is idempotent, and never touches
    /// `location`/`automatic_location`/`automatic_status`.
    #[test]
    fn auto_sets_mode_only_and_is_idempotent() {
        let (config, _dir) = temp_config();
        set(&config, 45.5019, -73.5674, false).unwrap();

        auto(&config, false).unwrap();
        let first = LocationConfigEntry::load(&config);
        assert_eq!(first.mode, LocationMode::Automatic);
        assert_eq!(first.location.unwrap().latitude(), 45.5019);
        assert_eq!(first.automatic_location, None);
        assert_eq!(first.automatic_status, ResolutionStatus::Unresolved);

        // Calling again is a no-op success, not an error, and changes nothing else.
        auto(&config, false).unwrap();
        let second = LocationConfigEntry::load(&config);
        assert_eq!(second, first);
    }

    /// T052 (US3): `ip` sets `mode: IpGeolocation` only, is idempotent, and never
    /// touches `location`/`ip_location`/`ip_status` — same posture as `auto`.
    #[test]
    fn ip_sets_mode_only_and_is_idempotent() {
        let (config, _dir) = temp_config();
        set(&config, 45.5019, -73.5674, false).unwrap();

        let output = ip(&config, false).unwrap();
        let first = LocationConfigEntry::load(&config);
        assert_eq!(first.mode, LocationMode::IpGeolocation);
        assert_eq!(first.location.unwrap().latitude(), 45.5019);
        assert_eq!(first.ip_location, None);
        assert_eq!(first.ip_status, ResolutionStatus::Unresolved);

        ip(&config, false).unwrap();
        let second = LocationConfigEntry::load(&config);
        assert_eq!(second, first);

        // T054: the STUN-disclosure copy is present in the command's own output.
        assert!(output.contains("STUN"));
    }

    /// `manual` sets `mode: Manual` only, leaves `location` untouched, and handles the
    /// "no manual value was ever stored" case cleanly.
    #[test]
    fn manual_sets_mode_only_and_leaves_location_untouched() {
        let (config, _dir) = temp_config();
        set(&config, 45.5019, -73.5674, false).unwrap();
        auto(&config, false).unwrap();

        let output = manual(&config, false).unwrap();
        assert!(output.contains("45.5019"));
        let state = LocationConfigEntry::load(&config);
        assert_eq!(state.mode, LocationMode::Manual);
        assert_eq!(state.location.unwrap().latitude(), 45.5019);
    }

    #[test]
    fn manual_with_no_stored_location_reports_that_cleanly() {
        let (config, _dir) = temp_config();
        auto(&config, false).unwrap();

        let output = manual(&config, false).unwrap();
        assert!(output.contains("no location stored"));
        assert_eq!(LocationConfigEntry::load(&config).mode, LocationMode::Manual);
    }

    /// `get`'s human and `--json` output both report `mode`, `status`, and the
    /// effective location for every `(mode, status)` combination, including the new
    /// `IpGeolocation` mode.
    #[test]
    fn get_reports_mode_status_and_effective_location_for_every_combination() {
        let (config, _dir) = temp_config();

        // Fresh default: manual, nothing stored.
        let human = get(&config, false);
        assert!(human.contains("mode: manual"));
        assert!(human.contains("status: unresolved"));
        assert!(human.contains("no location available"));
        let json = get(&config, true);
        assert!(json.contains(r#""mode":"manual""#));
        assert!(json.contains(r#""state":"unresolved""#));
        assert!(json.contains(r#""location":null"#));

        // Manual with a stored value.
        set(&config, 45.5019, -73.5674, false).unwrap();
        let human = get(&config, false);
        assert!(human.contains("mode: manual"));
        assert!(human.contains("45.5019"));
        assert!(!human.contains("from automatic resolution"));

        // Automatic, still unresolved: falls back to the manual value.
        auto(&config, false).unwrap();
        let human = get(&config, false);
        assert!(human.contains("mode: automatic"));
        assert!(human.contains("status: unresolved"));
        assert!(human.contains("45.5019"));
        assert!(!human.contains("from automatic resolution"));

        // Automatic, resolved: reports the automatic value with the provenance suffix.
        let mut state = LocationConfigEntry::load(&config);
        state.automatic_location = Some(Location::new(51.5072, -0.1276).unwrap());
        state.automatic_status = ResolutionStatus::Resolved;
        state.save(&config).unwrap();
        let human = get(&config, false);
        assert!(human.contains("status: resolved"));
        assert!(human.contains("51.5072"));
        assert!(human.contains("from automatic resolution"));
        let json = get(&config, true);
        assert!(json.contains(r#""state":"resolved""#));
        assert!(json.contains(r#""manual_location":{"latitude":45.5019,"longitude":-73.5674}"#));

        // Automatic, unavailable: falls back to the manual value, reason surfaced.
        let mut state = LocationConfigEntry::load(&config);
        state.automatic_location = None;
        state.automatic_status = ResolutionStatus::Unavailable { reason: "Location services disabled".into() };
        state.save(&config).unwrap();
        let human = get(&config, false);
        assert!(human.contains("status: unavailable (Location services disabled)"));
        assert!(human.contains("45.5019"));
        let json = get(&config, true);
        assert!(json.contains(r#""state":"unavailable","reason":"Location services disabled""#));

        // IP-geolocation, resolved: reports the ip value with its own provenance suffix.
        let mut state = LocationConfigEntry::load(&config);
        state.mode = LocationMode::IpGeolocation;
        state.ip_location = Some(Location::new(40.7128, -74.006).unwrap());
        state.ip_status = ResolutionStatus::Resolved;
        state.save(&config).unwrap();
        let human = get(&config, false);
        assert!(human.contains("mode: ip_geolocation"));
        assert!(human.contains("status: resolved"));
        assert!(human.contains("40.7128"));
        assert!(human.contains("from IP-geolocation"));
        // Switching to IP-geolocation mode doesn't fabricate a stale "automatic"
        // status from the prior mode's own fields.
        assert!(!human.contains("status: unavailable"));

        // IP-geolocation, unavailable: falls back to the manual value.
        let mut state = LocationConfigEntry::load(&config);
        state.ip_location = None;
        state.ip_status = ResolutionStatus::Unavailable { reason: "public IP discovery failed: STUN request timed out".into() };
        state.save(&config).unwrap();
        let human = get(&config, false);
        assert!(human.contains("mode: ip_geolocation"));
        assert!(human.contains("status: unavailable (public IP discovery failed: STUN request timed out)"));
        assert!(human.contains("45.5019"));
    }

    /// Spec 011 US6 FR-023 (research.md R18) — the audit's own reproduction: a
    /// corrupted location config previously reported "no location available"
    /// identically to "never configured." `get`'s message must now distinguish them,
    /// while exit code/behavior otherwise stays the same (still degrades to defaults,
    /// never fails the command).
    #[test]
    fn get_distinguishes_corrupted_config_from_never_configured() {
        let (config, dir) = temp_config();

        let never_configured = get(&config, false);
        assert!(never_configured.contains("no location available"));
        assert!(!never_configured.contains("could not be read"));

        LocationConfigEntry::default().save(&config).unwrap();
        let mode_key_path =
            dir.path().join("cosmic").join(wallpaper_ipc::LOCATION_CONFIG_ID).join(format!("v{}", LocationConfigEntry::VERSION)).join("mode");
        assert!(mode_key_path.exists());
        std::fs::write(&mode_key_path, b"not valid RON {{{").unwrap();

        let corrupted = get(&config, false);
        assert!(corrupted.contains("could not be read"), "expected a distinguishing message, got: {corrupted}");
        assert!(!corrupted.contains("no location available"), "must not read identically to never-configured, got: {corrupted}");

        let json = get(&config, true);
        assert!(json.contains(r#""config_read_error":true"#), "expected --json to also carry the flag: {json}");
    }
}
