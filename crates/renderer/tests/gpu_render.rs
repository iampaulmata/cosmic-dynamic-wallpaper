//! Real-GPU pixel-correctness test for [`renderer::crossfade::CrossfadePipeline`] —
//! renders offscreen (no Wayland surface needed) and reads pixels back, verifying the
//! actual WGSL blend shader produces correct output. This is independent of whatever
//! is currently visible on screen (a screenshot can't prove the shader is correct if
//! other windows occlude the background layer — this test doesn't have that problem,
//! and is a stronger check anyway: it verifies exact pixel values, not "looks blended
//! to the eye").
//!
//! Skips (rather than fails) if no GPU adapter is available, since CI environments
//! commonly have none — this mirrors `wallpaperctl`'s own pattern for tests that need
//! real system resources not guaranteed to exist everywhere.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use renderer::crossfade::CrossfadePipeline;
use renderer::texture::GpuTexture;

fn solid_color_texture(device: &wgpu::Device, queue: &wgpu::Queue, rgba: [u8; 4]) -> GpuTexture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("solid"),
        size: wgpu::Extent3d { width: 2, height: 2, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let pixels = [rgba; 4].concat();
    queue.write_texture(
        wgpu::ImageCopyTexture { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        &pixels,
        wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(8), rows_per_image: Some(2) },
        wgpu::Extent3d { width: 2, height: 2, depth_or_array_layers: 1 },
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    GpuTexture { texture, view, width: 2, height: 2 }
}

#[test]
fn crossfade_shader_blends_two_solid_colors_correctly() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::VULKAN | wgpu::Backends::GL, ..Default::default() });
    let Some(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else {
        eprintln!("skipping: no GPU adapter available in this environment");
        return;
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default(), None)).unwrap();

    // Plain (non-sRGB) format throughout so the blend math is directly checkable in
    // raw byte values, with no gamma-correction round-tripping to account for.
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let pipeline = CrossfadePipeline::new(&device, format);

    let red = solid_color_texture(&device, &queue, [255, 0, 0, 255]);
    let blue = solid_color_texture(&device, &queue, [0, 0, 255, 255]);

    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("offscreen-target"),
        size: wgpu::Extent3d { width: 4, height: 4, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());

    // progress=0.0 -> pure outgoing (red); progress=1.0 -> pure incoming (blue);
    // progress=0.5 -> exact average.
    for (progress, expected) in [(0.0f32, [255u8, 0, 0, 255]), (1.0, [0, 0, 255, 255]), (0.5, [127, 0, 127, 255])] {
        pipeline.render(&device, &queue, &target_view, &red, &blue, progress, (4, 4));

        let pixel = read_back_pixel(&device, &queue, &target);
        // Allow +-2 for rounding through the GPU's fixed-point blend/format conversion.
        for (channel, (got, want)) in pixel.iter().zip(expected.iter()).enumerate() {
            assert!((*got as i16 - *want as i16).abs() <= 2, "progress={progress}: channel {channel} = {got}, expected ~{want}");
        }
    }
}

fn read_back_pixel(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> [u8; 4] {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: 256 * 4, // one row, 256-byte-aligned (wgpu's copy alignment requirement)
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        wgpu::ImageCopyTexture { texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::ImageCopyBuffer { buffer: &buffer, layout: wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(256), rows_per_image: Some(1) } },
        wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
    );
    queue.submit(Some(encoder.finish()));

    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        tx.send(result).unwrap();
    });
    device.poll(wgpu::Maintain::Wait);
    rx.recv().unwrap().unwrap();

    let data = slice.get_mapped_range();
    let pixel = [data[0], data[1], data[2], data[3]];
    drop(data);
    buffer.unmap();
    pixel
}
