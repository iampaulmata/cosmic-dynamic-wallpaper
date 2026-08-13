//! Error types for manifest parsing/loading and (later, User Story 4) registry
//! persistence.
//!
//! Every fallible path in this crate returns one of these rather than panicking
//! (constitution Principle VIII) — see `data-model.md`'s "Error types" section.

use std::fmt;
use std::path::PathBuf;

/// Errors loading a pack — parsing its manifest, resolving its images, or handing its
/// anchors to spec 1's validation (FR-006, FR-006a).
#[derive(Debug)]
pub enum ManifestError {
    /// The manifest file couldn't be found where expected.
    ManifestNotFound { path: PathBuf },
    /// The manifest failed to parse as TOML, or didn't match the expected schema shape.
    ParseFailure { path: PathBuf, message: String },
    /// The manifest declared a `schema_version` newer than this loader supports.
    UnsupportedSchemaVersion { found: u32, max_supported: u32 },
    /// An image entry's `anchor` string didn't match any recognized anchor grammar.
    InvalidAnchor { file: String, value: String },
    /// A manifest image entry names a file that isn't present in the pack directory.
    MissingImageFile { file: String },
    /// An image entry's resolved path falls outside the pack directory (FR-006a).
    PathEscapesPackDirectory { file: String },
    /// An image file exists but isn't a readable/decodable image.
    UnreadableImage { file: String, reason: String },
    /// A `default_scaling` or per-image `scaling` value wasn't a recognized mode name.
    InvalidScalingMode { value: String },
    /// A `fallback_color` value wasn't a well-formed color.
    InvalidColor { value: String },
    /// The path (file or directory name) contained invalid (non-UTF-8) characters.
    NonUtf8Path { path: PathBuf },
    /// Spec 1's own pack-validation rejected the resolved anchor list (FR-003) — mixed
    /// anchor types, too many anchors, or a duplicate-instant tie.
    InvalidPack(schedule_engine::PackError),
    /// An underlying filesystem operation failed (permission denied, I/O error, etc.).
    Io { path: PathBuf, message: String },
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ManifestError::ManifestNotFound { path } => {
                write!(f, "no manifest file found at {}", path.display())
            }
            ManifestError::ParseFailure { path, message } => {
                write!(f, "failed to parse manifest {}: {message}", path.display())
            }
            ManifestError::UnsupportedSchemaVersion { found, max_supported } => {
                write!(f, "manifest schema_version {found} is newer than the supported maximum ({max_supported})")
            }
            ManifestError::InvalidAnchor { file, value } => {
                write!(f, "image {file:?} has an invalid anchor value {value:?}")
            }
            ManifestError::MissingImageFile { file } => {
                write!(f, "manifest references image file {file:?}, which does not exist in the pack directory")
            }
            ManifestError::PathEscapesPackDirectory { file } => {
                write!(f, "image entry {file:?} resolves to a path outside the pack directory")
            }
            ManifestError::UnreadableImage { file, reason } => {
                write!(f, "image file {file:?} is not a readable image: {reason}")
            }
            ManifestError::InvalidScalingMode { value } => {
                write!(f, "{value:?} is not a valid scaling mode (expected Fill, Fit, Stretch, or Center)")
            }
            ManifestError::InvalidColor { value } => {
                write!(f, "{value:?} is not a valid fallback color")
            }
            ManifestError::NonUtf8Path { path } => {
                write!(f, "path {} contains invalid (non-UTF-8) characters", path.display())
            }
            ManifestError::InvalidPack(e) => write!(f, "pack validation failed: {e}"),
            ManifestError::Io { path, message } => {
                write!(f, "I/O error accessing {}: {message}", path.display())
            }
        }
    }
}

impl std::error::Error for ManifestError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ManifestError::InvalidPack(e) => Some(e),
            _ => None,
        }
    }
}

impl From<schedule_engine::PackError> for ManifestError {
    fn from(e: schedule_engine::PackError) -> Self {
        ManifestError::InvalidPack(e)
    }
}

/// Errors from pack registry persistence (User Story 4, FR-010–FR-012). Wraps
/// underlying storage failures; never panics (constitution Principle VIII).
#[derive(Debug)]
pub enum RegistryError {
    /// The underlying persistence layer failed to read or write.
    Storage { message: String },
    /// A registry operation (e.g. `remove`) referenced a source that isn't registered.
    NotFound { source: String },
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RegistryError::Storage { message } => write!(f, "pack registry storage error: {message}"),
            RegistryError::NotFound { source } => write!(f, "no registered pack at {source}"),
        }
    }
}

impl std::error::Error for RegistryError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_error_display_is_readable() {
        assert_eq!(
            ManifestError::ManifestNotFound { path: PathBuf::from("/x/manifest.toml") }.to_string(),
            "no manifest file found at /x/manifest.toml"
        );
        assert_eq!(
            ManifestError::ParseFailure { path: PathBuf::from("/x/manifest.toml"), message: "oops".into() }
                .to_string(),
            "failed to parse manifest /x/manifest.toml: oops"
        );
        assert_eq!(
            ManifestError::UnsupportedSchemaVersion { found: 5, max_supported: 1 }.to_string(),
            "manifest schema_version 5 is newer than the supported maximum (1)"
        );
        assert_eq!(
            ManifestError::InvalidAnchor { file: "a.jpg".into(), value: "moonrise".into() }.to_string(),
            "image \"a.jpg\" has an invalid anchor value \"moonrise\""
        );
        assert_eq!(
            ManifestError::MissingImageFile { file: "a.jpg".into() }.to_string(),
            "manifest references image file \"a.jpg\", which does not exist in the pack directory"
        );
        assert_eq!(
            ManifestError::PathEscapesPackDirectory { file: "../a.jpg".into() }.to_string(),
            "image entry \"../a.jpg\" resolves to a path outside the pack directory"
        );
        assert_eq!(
            ManifestError::UnreadableImage { file: "a.jpg".into(), reason: "bad header".into() }.to_string(),
            "image file \"a.jpg\" is not a readable image: bad header"
        );
        assert_eq!(
            ManifestError::InvalidScalingMode { value: "Zoom".into() }.to_string(),
            "\"Zoom\" is not a valid scaling mode (expected Fill, Fit, Stretch, or Center)"
        );
        assert_eq!(
            ManifestError::InvalidColor { value: "nope".into() }.to_string(),
            "\"nope\" is not a valid fallback color"
        );
        assert_eq!(
            ManifestError::NonUtf8Path { path: PathBuf::from("/x") }.to_string(),
            "path /x contains invalid (non-UTF-8) characters"
        );
        assert_eq!(
            ManifestError::Io { path: PathBuf::from("/x"), message: "denied".into() }.to_string(),
            "I/O error accessing /x: denied"
        );
    }

    #[test]
    fn invalid_pack_display_and_source_delegate_to_the_wrapped_pack_error() {
        let e = ManifestError::from(schedule_engine::PackError::Empty);
        assert_eq!(e.to_string(), "pack validation failed: pack contains no images");
        assert!(std::error::Error::source(&e).is_some());
    }

    #[test]
    fn other_manifest_error_variants_have_no_source() {
        let e = ManifestError::MissingImageFile { file: "a.jpg".into() };
        assert!(std::error::Error::source(&e).is_none());
    }

    #[test]
    fn registry_error_display_is_readable() {
        assert_eq!(
            RegistryError::Storage { message: "disk full".into() }.to_string(),
            "pack registry storage error: disk full"
        );
        assert_eq!(
            RegistryError::NotFound { source: "/x".into() }.to_string(),
            "no registered pack at /x"
        );
    }

    #[test]
    fn errors_implement_std_error() {
        fn assert_error<E: std::error::Error>() {}
        assert_error::<ManifestError>();
        assert_error::<RegistryError>();
    }
}
