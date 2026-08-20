//! Path containment check: a manifest's image entries must resolve to somewhere
//! inside the pack directory, never outside it via `..` traversal, an absolute path,
//! or a symlink.

use std::path::{Path, PathBuf};

use crate::error::ManifestError;

/// Resolve `file` (a manifest-declared relative path) against `pack_dir`, and confirm
/// the *canonicalized* result stays inside the *canonicalized* pack directory.
///
/// `canonicalize` resolves both `..` components and symlinks to their real target, so a
/// symlink planted inside the pack directory that points outside it is caught too, not
/// just a literal `../` in the manifest. Requires the target file to exist — callers
/// get [`ManifestError::MissingImageFile`] for a nonexistent entry rather than this
/// function's containment error.
pub fn resolve_and_check(pack_dir: &Path, file: &str) -> Result<PathBuf, ManifestError> {
    // `pack_dir.join(file)` below silently discards `pack_dir` when `file` is
    // absolute (`Path::join`'s documented behavior) — containment would otherwise
    // only hold because the later `starts_with(&canonical_dir)` check happens to
    // still catch it (for an absolute path that exists and resolves outside the pack
    // dir). Rejecting explicitly here removes the reliance on that incidental
    // ordering: an absolute path is never a valid manifest entry, regardless of where
    // it happens to point or whether it happens to exist.
    if Path::new(file).is_absolute() {
        return Err(ManifestError::PathEscapesPackDirectory { file: file.to_string() });
    }

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

    /// `/etc/passwd`, an absolute path that genuinely exists on any Unix host. This
    /// would otherwise only be rejected because `pack_dir.join(file)` discards
    /// `pack_dir` for an absolute `file`, and the later containment check catches the
    /// resulting `/etc/passwd` as outside `pack_dir` — an explicit check rejects it
    /// directly instead, closing the gap where a future change to how pack
    /// directories are laid out (e.g. nesting them under a shared parent) could
    /// silently reopen an escape for an absolute path that happens to still resolve
    /// somewhere nominally "inside" that new layout.
    #[test]
    fn rejects_absolute_path() {
        let dir = temp_dir("absolute");
        assert!(matches!(resolve_and_check(&dir, "/etc/passwd"), Err(ManifestError::PathEscapesPackDirectory { .. })));

        // Also rejected even when the absolute path doesn't exist at all — proving
        // this is an explicit, unconditional check, not incidentally routed through
        // `MissingImageFile` for the non-existent case.
        assert!(matches!(
            resolve_and_check(&dir, "/this/path/does/not/exist/at/all.jpg"),
            Err(ManifestError::PathEscapesPackDirectory { .. })
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
