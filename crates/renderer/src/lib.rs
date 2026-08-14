//! `renderer` — the wallpaper renderer daemon (`wallpaperd`) for the dynamic wallpaper
//! project.
//!
//! **This crate currently implements the pure-logic subset only** — see `README.md`
//! for exactly what that covers and what it deliberately doesn't (the actual Wayland
//! layer-shell surfaces and `wgpu` crossfade rendering, which need a real compositor
//! and GPU to write and verify correctly, neither available in the environment this
//! pass was implemented in).
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod config;
pub mod crossfade;
pub mod dbus_types;
pub mod error;
pub mod output;
pub mod scheduler_bridge;

pub use config::{LocationSource, LOCATION_CONFIG_ID, RENDERER_CONFIG_ID};
pub use crossfade::CrossfadeTransition;
pub use dbus_types::QueryResponse;
pub use error::RendererError;
pub use output::{effective_pack, resolve_assignment, IdleWaitState, OutputAssignment, OutputId, RendererConfig, RendererState};
