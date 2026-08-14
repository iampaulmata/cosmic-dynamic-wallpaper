//! Path containment check (FR-006a, research.md R3): a manifest's image entries must
//! resolve to somewhere inside the pack directory, never outside it via `..`
//! traversal, an absolute path, or a symlink.

use std::path::{Path, PathBuf};

use crate::error::ManifestError;

/// Resolve `file` (a manifest-declared relative path) against `pack_dir`, and confirm
/// the *canonicalized* result stays inside the *canonicalized* pack directory.
///
/// `canonicalize` resolves both `..` components and symlinks to their real target, so a
/// symlink planted inside the pack directory that points outside it is caught too, not
/// just a literal `../` in the manifest (research.md R3). Requires the target file to
/// exist — callers get [`ManifestError::MissingImageFile`] for a nonexistent entry
/// rather than this function's containment error.
pub fn resolve_and_check(pack_dir: &Path, file: &str) -> Result<PathBuf, ManifestError> {
    let candidate = pack_dir.join(file);
    if !candidate.exists() {
        return Err(ManifestError::MissingImageFile { file: file.to_string() });
    }

    let canonical_dir = std::fs::canonicalize(pack_dir).map_err(|e| ManifestError::Io {
        path: pack_dir.to_path_buf(),
        message: e.to_string(),
    })?;
    let canonical_file = std::fs::canonicalize(&candidate).map_err(|e| ManifestError::Io {
        path: candidate.clone(),
        message: e.to_string(),
    })?;

    if canonical_file.to_str().is_none() {
        return Err(ManifestError::NonUtf8Path { path: canonical_file });
    }

    if !canonical_file.starts_with(&canonical_dir) {
        return Err(ManifestError::PathEscapesPackDirectory { file: file.to_string() });
    }

    Ok(canonical_file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pack-loader-path-safety-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn accepts_a_plain_sibling_file() {
        let dir = temp_dir("plain");
        fs::write(dir.join("image.jpg"), b"x").unwrap();
        let resolved = resolve_and_check(&dir, "image.jpg").unwrap();
        assert!(resolved.starts_with(std::fs::canonicalize(&dir).unwrap()));
    }

    #[test]
    fn rejects_missing_file() {
        let dir = temp_dir("missing");
        assert!(matches!(
            resolve_and_check(&dir, "nope.jpg"),
            Err(ManifestError::MissingImageFile { .. })
        ));
    }

    #[test]
    fn rejects_dot_dot_traversal() {
        let dir = temp_dir("traversal");
        let outside = dir.parent().unwrap().join(format!("outside-{}.jpg", std::process::id()));
        fs::write(&outside, b"x").unwrap();
        let rel = format!("../{}", outside.file_name().unwrap().to_str().unwrap());
        assert!(matches!(
            resolve_and_check(&dir, &rel),
            Err(ManifestError::PathEscapesPackDirectory { .. })
        ));
        let _ = fs::remove_file(&outside);
    }

    #[test]
    fn rejects_symlink_pointing_outside() {
        let dir = temp_dir("symlink");
        let outside = dir.parent().unwrap().join(format!("symlink-target-{}.jpg", std::process::id()));
        fs::write(&outside, b"x").unwrap();
        let link = dir.join("sneaky.jpg");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        #[cfg(unix)]
        assert!(matches!(
            resolve_and_check(&dir, "sneaky.jpg"),
            Err(ManifestError::PathEscapesPackDirectory { .. })
        ));
        let _ = fs::remove_file(&outside);
    }
}
