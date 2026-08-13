//! [`PackSource`] — a pack's identity, keyed by source location rather than its
//! declared display name (FR-009).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::ManifestError;

/// Where a pack's content lives on disk — the identity key for a loaded or registered
/// pack (FR-009, data-model.md `PackSource`). `Serialize`/`Deserialize` so it can be
/// persisted as-is in a [`crate::registry::PackRegistryEntry`] (FR-010).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PackSource {
    /// A manifest-based pack: the canonicalized pack directory.
    Directory(PathBuf),
    /// A static, manifest-free pack (FR-004): the canonicalized image file.
    StaticFile(PathBuf),
}

impl PackSource {
    /// Canonicalize `path` and classify it as a [`PackSource::Directory`] or
    /// [`PackSource::StaticFile`] depending on whether it's a directory or a file.
    ///
    /// Fails if `path` doesn't exist, can't be canonicalized (e.g. a permission
    /// error), or isn't valid UTF-8 (spec.md Edge Cases: non-UTF-8 paths are rejected
    /// with a clear error rather than risking silent mishandling).
    pub fn resolve(path: &Path) -> Result<Self, ManifestError> {
        let canonical = std::fs::canonicalize(path).map_err(|e| ManifestError::Io {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;
        if canonical.to_str().is_none() {
            return Err(ManifestError::NonUtf8Path { path: canonical });
        }
        if canonical.is_dir() {
            Ok(PackSource::Directory(canonical))
        } else {
            Ok(PackSource::StaticFile(canonical))
        }
    }

    /// The underlying path, regardless of which variant this is.
    pub fn path(&self) -> &Path {
        match self {
            PackSource::Directory(p) | PackSource::StaticFile(p) => p,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_a_directory() {
        let dir = std::env::temp_dir();
        let source = PackSource::resolve(&dir).unwrap();
        assert!(matches!(source, PackSource::Directory(_)));
    }

    #[test]
    fn resolves_a_file() {
        let file = std::env::temp_dir().join(format!("pack-source-test-{}", std::process::id()));
        std::fs::write(&file, b"x").unwrap();
        let source = PackSource::resolve(&file).unwrap();
        assert!(matches!(source, PackSource::StaticFile(_)));
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn rejects_nonexistent_path() {
        let missing = std::env::temp_dir().join("definitely-does-not-exist-12345");
        assert!(PackSource::resolve(&missing).is_err());
    }

    /// spec.md Edge Cases: non-UTF-8 file/directory names are rejected with a clear
    /// error rather than silently mishandled.
    #[test]
    #[cfg(unix)]
    fn rejects_non_utf8_path() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let invalid_name = OsStr::from_bytes(b"pack-source-\xFF\xFE-invalid");
        let path = std::env::temp_dir().join(invalid_name);
        std::fs::write(&path, b"x").unwrap();

        assert!(matches!(PackSource::resolve(&path), Err(ManifestError::NonUtf8Path { .. })));

        let _ = std::fs::remove_file(&path);
    }
}
