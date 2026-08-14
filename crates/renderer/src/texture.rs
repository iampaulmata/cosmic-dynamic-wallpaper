//! Full-resolution image decode (via `image`) and GPU texture upload (T014,
//! research.md R5). Unlike spec 2's `pack-loader::image_check`, which only
//! header-validates readability, this decodes complete pixel data — the cost spec 2's
//! research.md explicitly deferred to this crate, and only for images actually being
//! displayed, not a whole pack up front.

use std::path::Path;

use crate::error::RendererError;

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

impl GpuTexture {
    /// Decode `path` and upload it to the GPU. Fails with
    /// [`RendererError::TextureUploadFailed`] rather than panicking — spec 2 already
    /// header-validated this file is *a* readable image before assigning it, but full
    /// decode can still fail (truncated file, decoder bug, out-of-memory) and this must
    /// degrade only the affected output (FR-013), never the whole daemon.
    pub fn load(device: &wgpu::Device, queue: &wgpu::Queue, path: &Path) -> Result<Self, RendererError> {
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
