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

use pack_loader::{Color, ScalingMode};
use renderer::crossfade::{CrossfadePipeline, ImageScaling};
use renderer::texture::GpuTexture;

/// `ImageScaling` for plain "Fill" with an opaque-black fallback (irrelevant for
/// Fill, which never letterboxes) — the default every pre-existing test used
/// implicitly before `render`'s signature grew a per-texture `ImageScaling`.
fn fill_scaling() -> ImageScaling {
    ImageScaling { mode: ScalingMode::Fill, fallback_color: Color { r: 0, g: 0, b: 0, a: 255 } }
}

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
        pipeline.render(&device, &queue, &target_view, &red, fill_scaling(), &blue, fill_scaling(), progress, (4, 4));

        let pixel = read_back_pixel(&device, &queue, &target, 0, 0);
        // Allow +-2 for rounding through the GPU's fixed-point blend/format conversion.
        for (channel, (got, want)) in pixel.iter().zip(expected.iter()).enumerate() {
            assert!((*got as i16 - *want as i16).abs() <= 2, "progress={progress}: channel {channel} = {got}, expected ~{want}");
        }
    }
}

/// A GPU adapter, plain-format device/queue, and pipeline shared by the scaling-mode
/// tests below — `None` (with a printed skip reason) if no adapter is available,
/// mirroring `crossfade_shader_blends_two_solid_colors_correctly`'s own skip pattern.
fn setup() -> Option<(wgpu::Device, wgpu::Queue, CrossfadePipeline, wgpu::TextureFormat)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::VULKAN | wgpu::Backends::GL, ..Default::default() });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default(), None)).unwrap();
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let pipeline = CrossfadePipeline::new(&device, format);
    Some((device, queue, pipeline, format))
}

fn offscreen_target(device: &wgpu::Device, format: wgpu::TextureFormat, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("offscreen-target"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

/// "Fit" scaling letterboxes a relatively-taller image on a relatively-wider output —
/// exercises the new shader-level bounds-check-and-substitute logic
/// (`sample_or_fallback` in `shaders/crossfade.wgsl`), not just the pure Rust
/// transform math (`fit_uv_transform`'s own unit tests in `crossfade.rs` already cover
/// that in isolation). A 2x2 (1:1) source image via Fit onto an 8x4 (2:1) output:
/// `fit_uv_transform` picks the "image relatively taller" branch (aspect 1.0 < 2.0),
/// giving `scale_x = 0.5`, `offset_x = 0.25` — the image occupies output x in [2, 6),
/// so x=0/x=7 are letterboxed (fallback) and x=4 (mid) shows the image.
#[test]
fn fit_scaling_letterboxes_with_fallback_color() {
    let Some((device, queue, pipeline, format)) = setup() else {
        eprintln!("skipping: no GPU adapter available in this environment");
        return;
    };

    let green = solid_color_texture(&device, &queue, [0, 255, 0, 255]);
    let fallback = Color { r: 255, g: 0, b: 255, a: 255 }; // magenta — distinct from both the image color and the render pass's own black clear.
    let scaling = ImageScaling { mode: ScalingMode::Fit, fallback_color: fallback };

    let target = offscreen_target(&device, format, 8, 4);
    let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
    // progress=0.0 -> pure outgoing, so only `green`/`scaling` (the outgoing side) is
    // under test; the incoming texture/scaling are present but contribute weight 0.
    pipeline.render(&device, &queue, &target_view, &green, scaling, &green, fill_scaling(), 0.0, (8, 4));

    let letterboxed_left = read_back_pixel(&device, &queue, &target, 0, 2);
    let letterboxed_right = read_back_pixel(&device, &queue, &target, 7, 2);
    let image_region = read_back_pixel(&device, &queue, &target, 4, 2);

    assert_pixel_close(letterboxed_left, [255, 0, 255, 255], "letterboxed left edge");
    assert_pixel_close(letterboxed_right, [255, 0, 255, 255], "letterboxed right edge");
    assert_pixel_close(image_region, [0, 255, 0, 255], "image region");
}

/// "Center" scaling at less-than-native size letterboxes on *all* sides (unlike Fit,
/// which always fills at least one axis) — a distinct code path worth its own test. A
/// 2x2 source image via Center onto an 8x8 output: `center_uv_transform` gives `scale
/// = image/output = 0.25` on both axes, `offset = 0.375` — the image occupies output
/// x,y in [3, 5), so a corner (0,0) is letterboxed and the center (4,4) shows the image.
#[test]
fn center_scaling_letterboxes_at_native_size() {
    let Some((device, queue, pipeline, format)) = setup() else {
        eprintln!("skipping: no GPU adapter available in this environment");
        return;
    };

    let green = solid_color_texture(&device, &queue, [0, 255, 0, 255]);
    let fallback = Color { r: 255, g: 0, b: 255, a: 255 };
    let scaling = ImageScaling { mode: ScalingMode::Center, fallback_color: fallback };

    let target = offscreen_target(&device, format, 8, 8);
    let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
    pipeline.render(&device, &queue, &target_view, &green, scaling, &green, fill_scaling(), 0.0, (8, 8));

    let corner = read_back_pixel(&device, &queue, &target, 0, 0);
    let center = read_back_pixel(&device, &queue, &target, 4, 4);

    assert_pixel_close(corner, [255, 0, 255, 255], "letterboxed corner");
    assert_pixel_close(center, [0, 255, 0, 255], "centered image region");
}

fn assert_pixel_close(got: [u8; 4], want: [u8; 4], label: &str) {
    // Allow +-2 for rounding through the GPU's fixed-point blend/format conversion,
    // same tolerance as `crossfade_shader_blends_two_solid_colors_correctly`.
    for (channel, (g, w)) in got.iter().zip(want.iter()).enumerate() {
        assert!((*g as i16 - *w as i16).abs() <= 2, "{label}: channel {channel} = {g}, expected ~{w} (full pixel {got:?}, want {want:?})");
    }
}

fn read_back_pixel(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture, x: u32, y: u32) -> [u8; 4] {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: 256 * 4, // one row, 256-byte-aligned (wgpu's copy alignment requirement)
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        wgpu::ImageCopyTexture { texture, mip_level: 0, origin: wgpu::Origin3d { x, y, z: 0 }, aspect: wgpu::TextureAspect::All },
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
