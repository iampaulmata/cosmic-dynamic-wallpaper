//! `wallpaper-ipc` — shared `cosmic-config` schema types and D-Bus client for the
//! dynamic wallpaper project (spec 7 research.md R2, contracts/wallpaper-ipc-crate.md).
//!
//! The single source of truth `crates/renderer`, `crates/wallpaperctl`, and
//! `crates/wallpaper-settings` all depend on, replacing three independently-defined
//! copies of the same shapes — this project has already been bitten once by exactly
//! that class of bug (see [`renderer_config`]'s module doc). Deliberately dependency-
//! light: no `wgpu`/`smithay-client-toolkit`/`wayland-client`/`calloop`, preserving the
//! property spec 4 originally established for `wallpaperctl` (never linking spec 3's
//! heavy Wayland/GPU dependencies) for the new GUI crate too.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod dbus_client;
pub mod location_config;
pub mod renderer_config;

pub use dbus_client::{DbusClient, DbusError, QueryEntry, BUS_NAME, INTERFACE, OBJECT_PATH};
pub use location_config::{effective_location, LocationConfigEntry, LocationMode, ResolutionStatus, LOCATION_CONFIG_ID};
pub use renderer_config::{effective_pack, resolve_assignment, OutputAssignment, OutputId, RendererConfig, RENDERER_CONFIG_ID};
