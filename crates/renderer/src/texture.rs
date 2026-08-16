//! Full-resolution image decode (via `image`) and GPU texture upload (T014,
//! research.md R5). Unlike spec 2's `pack-loader::image_check`, which only
//! header-validates readability, this decodes complete pixel data — the cost spec 2's
//! research.md explicitly deferred to this crate, and only for images actually being
//! displayed, not a whole pack up front.

use std::path::Path;

use crate::error::RendererError;

/// The decoded-RGBA8 byte-count ceiling checked before a full decode runs (spec 011
/// US3 FR-012, clarified value: 256 MB — comfortably covers even an 8K (7680x4320)
/// wallpaper image, ~132 MB decoded, while still blocking a true decompression-bomb
/// image whose *declared* dimensions are legal but whose decoded size would not be).
pub const MAX_DECODED_IMAGE_BYTES: u64 = 256 * 1024 * 1024;

/// A decoded image uploaded to the GPU as an RGBA8 texture, ready to bind into the
/// crossfade pipeline.
pub struct GpuTexture {
    /// The uploaded GPU texture resource.
    pub texture: wgpu::Texture,
    /// A default view over the whole texture, ready to bind into a shader.
    pub view: wgpu::TextureView,
    /// The image's native width in pixels.
    pub width: u32,
    /// The image's native height in pixels.
    pub height: u32,
}

/// Spec 011 US3 FR-012 (research.md R9): read `path`'s *declared* (header-only,
/// undecoded) dimensions and reject before any full decode is attempted, if they
/// exceed `max_dimension` (the GPU's own `max_texture_dimension_2d`) or would decode
/// to more than `max_decoded_bytes`. Extracted as a standalone, GPU-independent
/// function — takes the limits as parameters rather than reading them from a real
/// `wgpu::Device` — so this specific gate is directly unit-testable with nothing but a
/// file on disk, no GPU adapter required.
fn check_declared_size(path: &Path, max_dimension: u32, max_decoded_bytes: u64) -> Result<(u32, u32), RendererError> {
    let (width, height) = image::ImageReader::open(path)
        .and_then(image::ImageReader::with_guessed_format)
        .map_err(|e| RendererError::TextureUploadFailed { path: path.to_path_buf(), reason: e.to_string() })?
        .into_dimensions()
        .map_err(|e| RendererError::TextureUploadFailed { path: path.to_path_buf(), reason: e.to_string() })?;
    let decoded_bytes = u64::from(width) * u64::from(height) * 4;
    if width > max_dimension || height > max_dimension || decoded_bytes > max_decoded_bytes {
        return Err(RendererError::TextureTooLarge { path: path.to_path_buf(), width, height });
    }
    Ok((width, height))
}

impl GpuTexture {
    /// Decode `path` and upload it to the GPU. Fails with
    /// [`RendererError::TextureUploadFailed`] rather than panicking — spec 2 already
    /// header-validated this file is *a* readable image before assigning it, but full
    /// decode can still fail (truncated file, decoder bug, out-of-memory) and this must
    /// degrade only the affected output (FR-013), never the whole daemon.
    pub fn load(device: &wgpu::Device, queue: &wgpu::Queue, path: &Path) -> Result<Self, RendererError> {
        // Spec 011 US3 FR-012 (research.md R9): spec 2 (`pack-loader::image_check`)
        // only header-validates that this file *is* a readable image — dimension/
        // decode-bomb limits are explicitly this crate's responsibility (that crate's
        // own doc comment). This is the only gate between untrusted pack image content
        // and the GPU, so it has to run before the costly full decode below, not after.
        check_declared_size(path, device.limits().max_texture_dimension_2d, MAX_DECODED_IMAGE_BYTES)?;

        let img = image::open(path)
            .map_err(|e| RendererError::TextureUploadFailed { path: path.to_path_buf(), reason: e.to_string() })?
            .to_rgba8();
        let (width, height) = img.dimensions();

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&path.to_string_lossy()),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::ImageCopyTexture { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            &img,
            wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(4 * width), rows_per_image: Some(height) },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Ok(Self { texture, view, width, height })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_solid_png(path: &Path, width: u32, height: u32) {
        // A solid-color image compresses to a tiny file regardless of declared
        // dimensions — this writes a real, valid, fully-decodable PNG (not a crafted/
        // truncated header) so the test exercises the genuine header-read path, not a
        // hand-built edge case the real decoder would never actually produce.
        let img = image::RgbaImage::from_pixel(width, height, image::Rgba([255, 0, 0, 255]));
        img.save(path).expect("failed to encode test fixture PNG");
    }

    /// Spec 011 US3 FR-012 (research.md R9) — an image whose declared dimensions
    /// exceed the (test-supplied, small) `max_dimension` is rejected via the
    /// header-only read; a small `max_dimension` here stands in for
    /// `device.limits().max_texture_dimension_2d` without needing a real GPU device.
    #[test]
    fn oversized_image_rejected_before_decode() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("oversized.png");
        write_solid_png(&path, 500, 500);

        let result = check_declared_size(&path, /* max_dimension */ 100, MAX_DECODED_IMAGE_BYTES);
        assert!(matches!(result, Err(RendererError::TextureTooLarge { width: 500, height: 500, .. })), "expected TextureTooLarge, got {result:?}");
    }

    #[test]
    fn image_exceeding_only_the_decoded_byte_ceiling_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bomb.png");
        // 2000x2000x4 = 16,000,000 bytes — within a generous max_dimension, but over a
        // deliberately tiny byte ceiling supplied only by this test.
        write_solid_png(&path, 2000, 2000);

        let result = check_declared_size(&path, /* max_dimension */ 8192, /* max_decoded_bytes */ 1_000_000);
        assert!(matches!(result, Err(RendererError::TextureTooLarge { .. })), "expected TextureTooLarge, got {result:?}");
    }

    #[test]
    fn image_within_both_limits_is_accepted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fine.png");
        write_solid_png(&path, 64, 64);

        let result = check_declared_size(&path, 8192, MAX_DECODED_IMAGE_BYTES);
        assert_eq!(result.unwrap(), (64, 64));
    }

    /// Spec 011 US3 FR-013 — end-to-end: `pack_loader::load_pack` only header-validates
    /// that an image is *readable* (`pack-loader::image_check`'s own doc comment
    /// explicitly defers dimension/decode-bomb limits to this crate) — confirms a pack
    /// containing a legitimately-oversized image loads successfully at the pack-loader
    /// layer, and only this crate's `check_declared_size` actually stops it, proving
    /// the documented boundary is enforced somewhere, not just described.
    #[test]
    fn pack_loader_to_renderer_size_boundary_is_actually_enforced() {
        let dir = tempfile::tempdir().unwrap();
        // A synthetic small `max_dimension` below stands in for a real GPU limit, so
        // this fixture only needs to be "big enough to exceed a small test threshold,"
        // not a genuinely enormous image — keeps this test's PNG encode fast.
        write_solid_png(&dir.path().join("huge.png"), 500, 500);
        std::fs::write(
            dir.path().join("manifest.toml"),
            "schema_version = 1\nname = \"x\"\ndefault_scaling = \"Fill\"\nfallback_color = \"#000000\"\n\n[[images]]\nfile = \"huge.png\"\nanchor = \"sunrise\"\n",
        )
        .unwrap();

        // pack-loader accepts it — it only checks readability, not dimensions.
        let loaded = pack_loader::load_pack(dir.path()).expect("pack-loader only header-validates readability, not size");
        let image_path = loaded.image_paths.values().next().expect("one image");

        // This crate's own check is what actually rejects it — a real
        // `max_texture_dimension_2d` would be much larger (typically 4096-16384); a
        // small threshold here is standing in for it the same way the other tests in
        // this module do, to keep the fixture image (and this test) fast.
        let result = check_declared_size(image_path, 100, MAX_DECODED_IMAGE_BYTES);
        assert!(matches!(result, Err(RendererError::TextureTooLarge { .. })), "expected the renderer-side boundary to reject it, got {result:?}");
    }
}
