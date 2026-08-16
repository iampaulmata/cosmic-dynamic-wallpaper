//! [`CliError`] — every command's failure surface, plus its exit-code mapping
//! (data-model.md `CliError`, contracts/wallpaperctl-cli.md Exit codes, FR-012).

use std::fmt;
use std::path::PathBuf;

/// Every way a `wallpaperctl` command can fail. Never panics on a user-input or
/// daemon-connectivity condition (constitution Principle VIII) — every fallible path
/// returns one of these.
#[derive(Debug)]
pub enum CliError {
    /// A D-Bus-dependent command (`list outputs`, `query`, `reevaluate`) found no
    /// running `wallpaperd` (FR-011).
    DaemonUnreachable,
    /// `assign`/`remove` referenced a pack not in spec 2's local registry (FR-007) —
    /// checked without needing a daemon.
    PackNotFound { source: PathBuf },
    /// `query`/`reevaluate` named an output `wallpaperd` doesn't currently manage
    /// (spec 3 FR-016). **Not** used by `assign` — see FR-007's "configure ahead of
    /// time" case, which only warns, never fails.
    OutputNotFound { id: String },
    /// `location set` was given an out-of-range/malformed latitude or longitude —
    /// wraps spec 1's `LocationError` verbatim (FR-008, FR-013).
    InvalidLocation { reason: String },
    /// `register` failed — wraps spec 2's `ManifestError` verbatim (FR-001).
    PackLoadFailed { source: PathBuf, reason: String },
    /// A `cosmic-config` read/write failed (registry, renderer config, or location
    /// config).
    ConfigError { reason: String },
    /// `assign --output <id>` was given an empty, overlong, or oddly-shaped value
    /// (spec 011 US5 FR-019) — same validation class as `PackNotFound`/
    /// `OutputNotFound`/`InvalidLocation` (a specific, actionable usage error), not a
    /// daemon/config failure.
    InvalidOutputId { reason: String },
    /// A pure CLI usage error not otherwise covered above — currently just `assign`'s
    /// "specify exactly one of `--output`/`--same-everywhere`" check (spec 011 US6
    /// FR-029, research.md R24), previously a direct `eprintln!` + `process::exit(1)`
    /// that bypassed this type entirely. Deliberately maps to the same exit code
    /// `clap`'s own built-in parse errors use (see `exit_code`'s doc) — both are "you
    /// used the CLI wrong," and that code is now safe to share since `DaemonUnreachable`
    /// no longer also claims it (FR-028).
    UsageError { message: String },
}

impl CliError {
    /// The process exit code this error maps to (contracts/wallpaperctl-cli-hardening.md
    /// Exit codes, FR-012, FR-028, FR-029).
    ///
    /// Spec 011 US7 FR-028 (research.md R23): `DaemonUnreachable` moved from `2` to
    /// `4` — code `2` collided with `clap`'s own built-in usage-error exit code
    /// (reproduced by the audit via a plain argument typo), making the two
    /// indistinguishable to a caller gating on exit code alone. `UsageError` (FR-029)
    /// now deliberately shares `2` with `clap`'s usage errors, since both are the same
    /// *class* of problem ("you used the CLI wrong") — that sharing is only safe now
    /// that `DaemonUnreachable` no longer also claims that code.
    pub fn exit_code(&self) -> i32 {
        match self {
            CliError::InvalidLocation { .. }
            | CliError::PackNotFound { .. }
            | CliError::OutputNotFound { .. }
            | CliError::InvalidOutputId { .. } => 1,
            CliError::UsageError { .. } => 2,
            CliError::PackLoadFailed { .. } | CliError::ConfigError { .. } => 3,
            CliError::DaemonUnreachable => 4,
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::DaemonUnreachable => {
                write!(f, "wallpaperd is not running or not reachable on the session bus")
            }
            CliError::PackNotFound { source } => {
                write!(f, "no registered pack at {} — register it first", source.display())
            }
            CliError::OutputNotFound { id } => {
                write!(f, "wallpaperd does not currently manage an output named {id:?}")
            }
            CliError::InvalidLocation { reason } => write!(f, "invalid location: {reason}"),
            CliError::PackLoadFailed { source, reason } => {
                write!(f, "failed to load pack at {}: {reason}", source.display())
            }
            CliError::ConfigError { reason } => write!(f, "configuration storage error: {reason}"),
            CliError::InvalidOutputId { reason } => write!(f, "invalid --output value: {reason}"),
            CliError::UsageError { message } => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for CliError {}

impl From<schedule_engine::LocationError> for CliError {
    fn from(e: schedule_engine::LocationError) -> Self {
        CliError::InvalidLocation { reason: e.to_string() }
    }
}

impl From<pack_loader::RegistryError> for CliError {
    fn from(e: pack_loader::RegistryError) -> Self {
        CliError::ConfigError { reason: e.to_string() }
    }
}

impl From<cosmic_config::Error> for CliError {
    fn from(e: cosmic_config::Error) -> Self {
        CliError::ConfigError { reason: e.to_string() }
    }
}

impl From<wallpaper_ipc::DbusError> for CliError {
    fn from(e: wallpaper_ipc::DbusError) -> Self {
        match e {
            wallpaper_ipc::DbusError::DaemonUnreachable => CliError::DaemonUnreachable,
            wallpaper_ipc::DbusError::OutputNotFound { id } => CliError::OutputNotFound { id },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_match_the_contract() {
        assert_eq!(CliError::InvalidLocation { reason: "x".into() }.exit_code(), 1);
        assert_eq!(CliError::PackNotFound { source: "/x".into() }.exit_code(), 1);
        assert_eq!(CliError::OutputNotFound { id: "DP-3".into() }.exit_code(), 1);
        assert_eq!(CliError::PackLoadFailed { source: "/x".into(), reason: "y".into() }.exit_code(), 3);
        assert_eq!(CliError::ConfigError { reason: "z".into() }.exit_code(), 3);
        assert_eq!(CliError::InvalidOutputId { reason: "z".into() }.exit_code(), 1);
        assert_eq!(CliError::UsageError { message: "z".into() }.exit_code(), 2);
    }

    /// Spec 011 US7 FR-028 (research.md R23) — the audit's own reproduction: a plain
    /// `clap` usage error (e.g. a typo'd argument) exits with `clap`'s own built-in
    /// code `2`. `DaemonUnreachable` must no longer also be `2`, or the two remain
    /// indistinguishable to a script gating on exit code alone.
    #[test]
    fn daemon_unreachable_exit_code_is_four_not_two() {
        assert_eq!(CliError::DaemonUnreachable.exit_code(), 4);
        assert_ne!(CliError::DaemonUnreachable.exit_code(), 2, "must not collide with clap's own usage-error exit code");
    }

    #[test]
    fn display_messages_are_specific_and_actionable() {
        assert!(CliError::DaemonUnreachable.to_string().contains("wallpaperd"));
        assert!(
            CliError::PackNotFound { source: "/foo".into() }.to_string().contains("/foo")
        );
        assert!(CliError::OutputNotFound { id: "DP-3".into() }.to_string().contains("DP-3"));
    }
}
