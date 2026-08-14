//! Re-exports of [`wallpaper_ipc`]'s shared `cosmic-config` schema types
//! ([`RendererConfig`], [`LocationConfigEntry`], [`LocationMode`], [`ResolutionStatus`])
//! — this crate no longer independently defines them (spec 7 research.md R2,
//! contracts/wallpaper-ipc-crate.md): a prior mismatch between two independently
//! -defined "identical" types across this crate and `renderer` silently produced an
//! empty map at runtime, exactly the bug class extracting a single shared crate
//! structurally prevents.

pub use wallpaper_ipc::{LocationConfigEntry, RendererConfig};
