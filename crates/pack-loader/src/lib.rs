//! Pack manifest parsing and loading for the dynamic wallpaper daemon.
//!
//! Turns a directory (manifest + images) or a single image file into a fully validated
//! [`LoadedPack`] that spec 1's scheduling engine and spec 3's renderer can consume. See
//! `README.md` for full scope and non-scope, and `contracts/pack-loader-api.md` for the
//! committed public API shape and on-disk manifest schema.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod error;
mod image_check;
mod load;
mod manifest;
mod pack_source;
mod path_safety;

pub use error::{ManifestError, RegistryError};
pub use load::{load_pack, LoadedPack, MANIFEST_FILE_NAME};
pub use manifest::{Color, ManifestImage, PackManifest, ScalingMode, MAX_SUPPORTED_SCHEMA_VERSION};
pub use pack_source::PackSource;
