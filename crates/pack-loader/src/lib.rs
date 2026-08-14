//! Pack manifest parsing and loading for the dynamic wallpaper daemon.
//!
//! Turns a directory (manifest + images) or a single image file into a fully validated
//! [`LoadedPack`] that spec 1's scheduling engine and spec 3's renderer can consume. See
//! `README.md` for full scope and non-scope, and `contracts/pack-loader-api.md` for the
//! committed public API shape and on-disk manifest schema.
//!
//! ```
//! use pack_loader::{load_pack, Registry};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Zero-config static pack — no manifest required.
//! # let img_path = std::env::temp_dir().join("pack-loader-doctest-smoke.png");
//! # image::RgbImage::new(2, 2).save(&img_path)?;
//! let loaded = load_pack(&img_path)?;
//! assert!(loaded.pack.is_static());
//!
//! // Registry round-trip (doctest uses a scratch directory via the same test hook
//! // `tests/` uses, so this never touches the real user config directory — real
//! // callers use `Registry::open()` instead).
//! # let registry_dir = std::env::temp_dir().join("pack-loader-doctest-registry");
//! # std::fs::create_dir_all(&registry_dir)?;
//! let mut registry = Registry::open_at(&registry_dir)?;
//! registry.register(loaded.source.clone())?;
//! assert!(registry.known_packs().iter().any(|e| e.source == loaded.source));
//! # std::fs::remove_file(&img_path).ok();
//! # std::fs::remove_dir_all(&registry_dir).ok();
//! # Ok(())
//! # }
//! ```
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod error;
mod image_check;
mod load;
mod manifest;
mod pack_source;
mod path_safety;
mod registry;

pub use error::{ManifestError, RegistryError};
pub use load::{load_pack, LoadedPack, MANIFEST_FILE_NAME};
pub use manifest::{Color, ManifestImage, PackManifest, ScalingMode, MAX_SUPPORTED_SCHEMA_VERSION};
pub use pack_source::PackSource;
pub use registry::{PackOrigin, PackRegistryEntry, Registry, RegistryStatus, REGISTRY_CONFIG_ID, REMOVED_STARTER_PACKS_CONFIG_ID};
