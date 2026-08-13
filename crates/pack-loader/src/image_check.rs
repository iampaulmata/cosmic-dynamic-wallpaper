//! Header-only image readability validation (FR-006, research.md R2): confirms a file
//! is a decodable image of a real format and reads its header, without decoding full
//! pixel data — that cost belongs to the renderer (spec 3), which only decodes images
//! actually being displayed.

use std::path::Path;

use image::ImageReader;

use crate::error::ManifestError;

/// Confirm `path` is a readable, recognizable image format. Returns the error a caller
/// should surface (naming `file`) if not.
pub fn check_readable(path: &Path, file: &str) -> Result<(), ManifestError> {
    let reader = ImageReader::open(path)
        .map_err(|e| ManifestError::UnreadableImage { file: file.to_string(), reason: e.to_string() })?;
    let reader = reader
        .with_guessed_format()
        .map_err(|e| ManifestError::UnreadableImage { file: file.to_string(), reason: e.to_string() })?;
    reader
        .into_dimensions()
        .map_err(|e| ManifestError::UnreadableImage { file: file.to_string(), reason: e.to_string() })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("pack-loader-image-check-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn accepts_a_real_png() {
        let dir = temp_dir("valid");
        let path = dir.join("test.png");
        let img = image::RgbImage::new(4, 4);
        img.save(&path).unwrap();
        assert!(check_readable(&path, "test.png").is_ok());
    }

    #[test]
    fn rejects_garbage_bytes() {
        let dir = temp_dir("garbage");
        let path = dir.join("not_an_image.jpg");
        fs::write(&path, b"this is definitely not an image file").unwrap();
        assert!(matches!(
            check_readable(&path, "not_an_image.jpg"),
            Err(ManifestError::UnreadableImage { .. })
        ));
    }

    #[test]
    fn rejects_nonexistent_file() {
        let dir = temp_dir("missing");
        let path = dir.join("ghost.png");
        assert!(matches!(
            check_readable(&path, "ghost.png"),
            Err(ManifestError::UnreadableImage { .. })
        ));
    }
}
