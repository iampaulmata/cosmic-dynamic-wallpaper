//! `renderer` — the wallpaper renderer daemon (`wallpaperd`) for the dynamic wallpaper
//! project. Implements the pure-logic subset (`output`, `crossfade`'s progress math,
//! `config`, `scheduler_bridge`, `dbus_types`) plus a real Wayland/GPU rendering path
//! (`gpu`, `texture`, `crossfade`'s pipeline, `surface`, the `wallpaperd` binary) and a
//! live D-Bus service (`dbus_service`) — see `README.md` for exactly what's
//! implemented, what's simplified, and what's still open (hotplug resize handling,
//! non-Fill scaling modes).
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod config;
pub mod crossfade;
pub mod dbus_service;
pub mod dbus_types;
pub mod error;
pub mod gpu;
pub mod output;
pub mod portal_location;
pub mod scheduler_bridge;
pub mod surface;
pub mod texture;

pub use config::{effective_location, AutomaticStatus, LocationMode, LocationSource, LOCATION_CONFIG_ID, RENDERER_CONFIG_ID};
pub use crossfade::CrossfadeTransition;
pub use dbus_types::QueryResponse;
pub use error::RendererError;
pub use output::{effective_pack, resolve_assignment, IdleWaitState, OutputAssignment, OutputId, RendererConfig, RendererState};
