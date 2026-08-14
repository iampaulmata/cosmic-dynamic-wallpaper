//! `wallpaperctl` — the CLI control surface for the dynamic wallpaper daemon
//! (contracts/wallpaperctl-cli.md). See `README.md` for the config-only-vs-
//! daemon-required command split and this crate's explicit non-scope.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod commands;
mod config;
mod error;
mod output;
mod pack_ref;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use pack_loader::Registry;

use crate::commands::assign::AssignTarget;
use crate::config::{LocationConfigEntry, RendererConfig};
use crate::error::CliError;

#[derive(Parser)]
#[command(name = "wallpaperctl", about = "Control surface for the dynamic wallpaper daemon")]
struct Cli {
    /// Machine-readable output (FR-013).
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Register a pack directory or single image file (FR-001, FR-002).
    Register { path: PathBuf },
    /// List known packs or wallpaperd-managed outputs (FR-003, FR-005).
    List {
        #[command(subcommand)]
        what: ListWhat,
    },
    /// Remove a known pack from the registry outright (FR-004).
    Remove { pack_source: PathBuf },
    /// Assign a registered pack to an output, or enable same-pack-everywhere (FR-006, FR-007).
    Assign {
        #[arg(long)]
        output: Option<String>,
        #[arg(long)]
        same_everywhere: bool,
        pack_source: PathBuf,
    },
    /// Get, set, or clear the manual location for solar-anchored packs (FR-008).
    Location {
        #[command(subcommand)]
        action: LocationAction,
    },
    /// Query an output's (or every output's) current/next schedule state (FR-009).
    Query {
        #[arg(long)]
        output: Option<String>,
    },
    /// Force an immediate re-evaluation of one or all outputs (FR-010).
    Reevaluate {
        #[arg(long)]
        output: Option<String>,
    },
}

#[derive(Subcommand)]
enum ListWhat {
    Packs,
    Outputs,
}

#[derive(Subcommand)]
enum LocationAction {
    Get,
    Set {
        // `allow_hyphen_values`: clap's automatic negative-number detection doesn't
        // reliably kick in for derive'd positional f64 args behind a subcommand — found
        // via a manual smoke test (a negative longitude, e.g. most of the Americas, was
        // rejected as an unrecognized flag). Both fields need it, not just longitude,
        // since latitude can be negative too (anywhere south of the equator).
        #[arg(allow_hyphen_values = true)]
        latitude: f64,
        #[arg(allow_hyphen_values = true)]
        longitude: f64,
    },
    Clear,
    /// Enable automatic location via the portal (spec 6 FR-001/002/003). Idempotent.
    Auto,
    /// Enable IP-geolocation via a bundled offline database (spec 7 FR-012/013/014).
    /// Idempotent.
    Ip,
    /// Switch back to manual mode using whatever value is already stored, no re-entry
    /// required (spec 6 FR-007/009).
    Manual,
}

fn main() {
    let cli = Cli::parse();
    match run(cli) {
        Ok(text) => {
            if !text.is_empty() {
                println!("{text}");
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(e.exit_code());
        }
    }
}

fn run(cli: Cli) -> Result<String, CliError> {
    let json = cli.json;
    match cli.command {
        Command::Register { path } => {
            let mut registry = Registry::open()?;
            commands::register::run(&path, &mut registry, json)
        }
        Command::List { what: ListWhat::Packs } => {
            let mut registry = Registry::open()?;
            Ok(commands::list::packs(&mut registry, json))
        }
        Command::List { what: ListWhat::Outputs } => commands::list::outputs(json),
        Command::Remove { pack_source } => {
            let mut registry = Registry::open()?;
            commands::remove::run(&pack_source, &mut registry, json)
        }
        Command::Assign { output, same_everywhere, pack_source } => {
            let target = match (output, same_everywhere) {
                (Some(id), false) => AssignTarget::Output(id),
                (None, true) => AssignTarget::SameEverywhere,
                _ => {
                    // Clap can't easily express "exactly one of" across an Option and
                    // a bool flag declaratively without a group; validated here
                    // instead — still a specific, actionable message + non-zero exit
                    // (FR-012), just not routed through CliError's daemon/config
                    // variants since it's a pure usage error.
                    eprintln!("error: specify exactly one of --output <id> or --same-everywhere");
                    std::process::exit(1);
                }
            };
            let registry = Registry::open()?;
            let renderer_config = RendererConfig::open()?;
            commands::assign::run(target, &pack_source, &registry, &renderer_config, json)
        }
        Command::Location { action } => {
            let config = LocationConfigEntry::open()?;
            match action {
                LocationAction::Get => Ok(commands::location::get(&config, json)),
                LocationAction::Set { latitude, longitude } => {
                    commands::location::set(&config, latitude, longitude, json)
                }
                LocationAction::Clear => commands::location::clear(&config, json),
                LocationAction::Auto => commands::location::auto(&config, json),
                LocationAction::Ip => commands::location::ip(&config, json),
                LocationAction::Manual => commands::location::manual(&config, json),
            }
        }
        Command::Query { output } => commands::query::run(output.as_deref(), json),
        Command::Reevaluate { output } => commands::reevaluate::run(output.as_deref(), json),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Registry`/`RendererConfig`/`LocationConfigEntry::open()` all resolve through
    /// `cosmic-config`'s `dirs::config_dir()`, which respects `XDG_CONFIG_HOME` — so
    /// pointing it at a scratch directory exercises `run()`'s *real* dispatch path
    /// (clap parsing aside) without ever touching the real user config. No other test
    /// in this crate reads/writes `XDG_CONFIG_HOME` (all others use the `open_at`
    /// tempdir hook directly), but the tests *in this module* that use this helper
    /// would race each other via this same process-wide env var if run concurrently
    /// (Rust's test harness runs tests in parallel by default) — serialized with a
    /// mutex rather than relying on `--test-threads=1`.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn with_scratch_xdg_config_home<T>(f: impl FnOnce() -> T) -> T {
        let _guard = ENV_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: serialized by ENV_LOCK above — no concurrent access to this
        // process-wide env var from anywhere else in this binary.
        unsafe { std::env::set_var("XDG_CONFIG_HOME", dir.path()) };
        let result = f();
        unsafe { std::env::remove_var("XDG_CONFIG_HOME") };
        result
    }

    fn cli(json: bool, command: Command) -> Cli {
        Cli { json, command }
    }

    #[test]
    fn register_then_list_packs_dispatches_correctly() {
        with_scratch_xdg_config_home(|| {
            let file = std::env::temp_dir().join(format!("wallpaperctl-main-test-{}.png", std::process::id()));
            image::RgbImage::new(2, 2).save(&file).unwrap();

            let register_result = run(cli(false, Command::Register { path: file.clone() }));
            assert!(register_result.is_ok(), "{register_result:?}");

            let list_result = run(cli(false, Command::List { what: ListWhat::Packs }));
            assert!(list_result.unwrap().contains("known"));

            let _ = std::fs::remove_file(&file);
        });
    }

    #[test]
    fn location_get_set_clear_dispatch_correctly() {
        with_scratch_xdg_config_home(|| {
            let get_before = run(cli(false, Command::Location { action: LocationAction::Get }));
            assert!(get_before.unwrap().contains("no location available"));

            let set_result = run(cli(
                false,
                Command::Location { action: LocationAction::Set { latitude: 45.5019, longitude: -73.5674 } },
            ));
            assert!(set_result.is_ok());

            let get_after = run(cli(true, Command::Location { action: LocationAction::Get }));
            assert!(get_after.unwrap().contains("45.5019"));

            let clear_result = run(cli(false, Command::Location { action: LocationAction::Clear }));
            assert!(clear_result.is_ok());
        });
    }

    /// spec 6: `location auto`/`manual` dispatch correctly through `main.rs`.
    #[test]
    fn location_auto_manual_dispatch_correctly() {
        with_scratch_xdg_config_home(|| {
            let auto_result = run(cli(false, Command::Location { action: LocationAction::Auto }));
            assert!(auto_result.is_ok());

            let get_after_auto = run(cli(false, Command::Location { action: LocationAction::Get }));
            assert!(get_after_auto.unwrap().contains("mode: automatic"));

            let manual_result = run(cli(false, Command::Location { action: LocationAction::Manual }));
            assert!(manual_result.is_ok());

            let get_after_manual = run(cli(false, Command::Location { action: LocationAction::Get }));
            assert!(get_after_manual.unwrap().contains("mode: manual"));
        });
    }

    /// `query`/`reevaluate` dispatch straight to the D-Bus client, which needs no
    /// config directory at all — real (not mocked), same rationale as
    /// `dbus_client.rs`'s own tests.
    #[test]
    fn query_and_reevaluate_dispatch_and_fail_fast_without_a_daemon() {
        if zbus::blocking::Connection::session().is_err() {
            return;
        }
        assert!(matches!(
            run(cli(false, Command::Query { output: None })),
            Err(CliError::DaemonUnreachable)
        ));
        assert!(matches!(
            run(cli(false, Command::Reevaluate { output: None })),
            Err(CliError::DaemonUnreachable)
        ));
    }
}
