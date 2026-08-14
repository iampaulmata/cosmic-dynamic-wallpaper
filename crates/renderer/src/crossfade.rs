//! [`CrossfadeTransition`] and its progress computation (data-model.md
//! `CrossfadeTransition`, FR-001, FR-002, FR-004, FR-011), plus [`CrossfadePipeline`],
//! the actual two-texture WGSL GPU blend (`shaders/crossfade.wgsl`) that consumes it.
//! The frame-callback draw loop that calls `CrossfadePipeline::render` once per tick is
//! `surface.rs`'s job (see `README.md`).

use std::time::{Duration, Instant};

use pack_loader::{Color, ScalingMode};

use crate::texture::GpuTexture;

/// The active-transition state for one output (data-model.md `CrossfadeTransition`).
/// **Scope note**: `outgoing_texture`/`incoming_texture` in the full data model are GPU
/// texture handles; here they're the image identifiers alone (`schedule_engine::ImageId`)
/// — everything the pure logic needs to know *which* images are involved, without
/// depending on `wgpu`.
#[derive(Debug, Clone, PartialEq)]
pub struct CrossfadeTransition {
    /// The image fading out.
    pub outgoing: schedule_engine::ImageId,
    /// The image fading in.
    pub incoming: schedule_engine::ImageId,
    /// Local to this transition; not persisted. `Instant` (monotonic), not
    /// `DateTime<Local>`, since a system clock adjustment mid-transition must not
    /// perturb an already-running animation.
    pub started_at: Instant,
    /// Fixed 45s default (FR-002), configurable.
    pub duration: Duration,
}

impl CrossfadeTransition {
    /// Recompute progress from `started_at`/`duration` at `now` — called once per
    /// frame-callback tick in the real draw loop; deterministic given the same `now`
    /// (FR-004, monotonic non-decreasing as `now` advances, always in `[0.0, 1.0]`).
    ///
    /// A zero-duration transition is immediately complete (`1.0`) rather than
    /// dividing by zero.
    pub fn progress_at(&self, now: Instant) -> f64 {
        if self.duration.is_zero() {
            return 1.0;
        }
        let elapsed = now.saturating_duration_since(self.started_at);
        (elapsed.as_secs_f64() / self.duration.as_secs_f64()).clamp(0.0, 1.0)
    }

    /// `true` once `progress_at(now)` has reached `1.0` — the draw loop's cue to
    /// unsubscribe from frame callbacks and return to `IdleWaitState` (FR-004).
    pub fn is_complete_at(&self, now: Instant) -> bool {
        self.progress_at(now) >= 1.0
    }
}

// Field order/padding matter here: this must byte-for-byte match `shaders/
// crossfade.wgsl`'s `Uniforms` struct under WGSL's uniform-address-space layout
// rules (`vec4<f32>` needs 16-byte alignment, `vec2<f32>` needs 8-byte alignment, and
// the struct's total size must round up to its largest member's alignment). The two
// `vec4<f32>` fallback-color fields are placed right after `progress` (with explicit
// padding to reach the required 16-byte boundary) rather than appended at the end, so
// the whole layout falls out of simple sequential field placement with no trailing
// padding — verified byte-for-byte by `tests::uniforms_layout_matches_wgsl_alignment`.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    progress: f32,
    _pad0: [f32; 3],
    outgoing_fallback: [f32; 4],
    incoming_fallback: [f32; 4],
    outgoing_scale: [f32; 2],
    outgoing_offset: [f32; 2],
    incoming_scale: [f32; 2],
    incoming_offset: [f32; 2],
}

/// `pack_loader::Color`'s 0-255 `u8` channels, normalized to WGSL's expected `[0.0,
/// 1.0]` `vec4<f32>` range.
fn color_to_f32(c: Color) -> [f32; 4] {
    [c.r as f32 / 255.0, c.g as f32 / 255.0, c.b as f32 / 255.0, c.a as f32 / 255.0]
}

/// "Fill" (cover) scaling (FR-005's default): scale/offset such that sampling
/// `uv * scale + offset` maps the fullscreen UV range onto the correctly-cropped
/// portion of a `(image_w, image_h)` image displayed on a `(output_w, output_h)`
/// output, covering the whole output with no letterboxing.
fn fill_uv_transform(image_w: u32, image_h: u32, output_w: u32, output_h: u32) -> ([f32; 2], [f32; 2]) {
    let image_aspect = image_w as f32 / image_h as f32;
    let output_aspect = output_w as f32 / output_h as f32;
    if image_aspect > output_aspect {
        let scale_x = output_aspect / image_aspect;
        ([scale_x, 1.0], [(1.0 - scale_x) / 2.0, 0.0])
    } else {
        let scale_y = image_aspect / output_aspect;
        ([1.0, scale_y], [0.0, (1.0 - scale_y) / 2.0])
    }
}

/// "Stretch" scaling: the image is scaled to exactly match the output, aspect ratio
/// ignored — the fullscreen UV range already spans the whole image 1:1, so this is
/// the identity transform.
fn stretch_uv_transform() -> ([f32; 2], [f32; 2]) {
    ([1.0, 1.0], [0.0, 0.0])
}

/// Given `frac` (the fraction of the output's extent along one axis that the image
/// actually occupies once positioned/sized — always centered), the `(scale, offset)`
/// pair that maps *output* UV to *image* UV on that axis.
///
/// This is deliberately the **inverse** relationship of [`fill_uv_transform`]'s: Fill
/// only ever needs to select a *smaller* sub-range of the image's own UV space (a
/// crop), so multiplying by a `<= 1.0` scale is enough and the result never leaves
/// `[0, 1]`. Fit/Center instead need to detect when an *output* pixel falls outside
/// the region the image actually covers (a letterbar) — which means the transform
/// must *expand* image-UV beyond `[0, 1]` at the output's edges whenever `frac < 1.0`,
/// the exact opposite direction. Deriving it: the image covers output-UV range
/// `[offset_out, offset_out + frac]` where `offset_out = (1 - frac) / 2` (centered);
/// solving `image_uv = (output_uv - offset_out) / frac` for the linear-map coefficients
/// gives `scale = 1 / frac`, `offset = -offset_out / frac`. Sanity checks: `frac ==
/// 1.0` (aspect/size matches exactly) gives the identity `(1.0, 0.0)`; `frac < 1.0`
/// (image smaller than its output extent) gives `scale > 1.0`, pushing `image_uv`
/// outside `[0, 1]` for output UVs beyond the covered region — a letterbox; `frac >
/// 1.0` (image *larger* than its output extent, e.g. Center with an oversized image)
/// gives `scale < 1.0`, which crops instead — the same general formula handles both
/// without a separate branch, this is not incidental.
///
/// **Found by the offscreen GPU pixel test, not the pure-math unit tests below**: an
/// earlier version of this function reused `fill_uv_transform`'s crop-direction
/// formula verbatim for Fit/Center (just relabeling which axis is "letterboxed"),
/// which is self-consistent enough that pure unit tests checking scale/offset against
/// hand-derived-the-same-wrong-way expected values still passed — only
/// `tests/gpu_render.rs`'s actual pixel readback (expecting `fallback_color` at a
/// letterboxed coordinate) caught that the transformed UV never actually left `[0,
/// 1]`, because the crop-direction formula can't produce out-of-bounds values at all.
fn letterbox_scale_offset(frac: f32) -> (f32, f32) {
    let offset_out = (1.0 - frac) / 2.0;
    (1.0 / frac, -offset_out / frac)
}

/// "Fit" scaling: the image is scaled to fit entirely *within* the output (aspect
/// ratio preserved), letterboxing whichever axis has leftover space. See
/// [`letterbox_scale_offset`] for the scale/offset derivation.
fn fit_uv_transform(image_w: u32, image_h: u32, output_w: u32, output_h: u32) -> ([f32; 2], [f32; 2]) {
    let image_aspect = image_w as f32 / image_h as f32;
    let output_aspect = output_w as f32 / output_h as f32;
    if image_aspect > output_aspect {
        // Image relatively wider than the output: full width, letterbox top/bottom.
        let (scale_y, offset_y) = letterbox_scale_offset(output_aspect / image_aspect);
        ([1.0, scale_y], [0.0, offset_y])
    } else {
        // Image relatively taller than (or equal to) the output: full height,
        // letterbox left/right.
        let (scale_x, offset_x) = letterbox_scale_offset(image_aspect / output_aspect);
        ([scale_x, 1.0], [offset_x, 0.0])
    }
}

/// "Center" scaling: the image is displayed at its native size, centered, with no
/// scaling at all — `frac` per axis is simply the image's size as a fraction of the
/// output's size (see [`letterbox_scale_offset`] for the scale/offset derivation).
/// Unlike Fill/Fit (which always preserve aspect ratio and only ever shrink-to-fit),
/// an axis where the image is *larger* than the output gets `frac > 1.0`, which
/// `letterbox_scale_offset`'s general formula correctly turns into a crop (no
/// special-casing needed here — it falls out of the general rule).
fn center_uv_transform(image_w: u32, image_h: u32, output_w: u32, output_h: u32) -> ([f32; 2], [f32; 2]) {
    let (scale_x, offset_x) = letterbox_scale_offset(image_w as f32 / output_w as f32);
    let (scale_y, offset_y) = letterbox_scale_offset(image_h as f32 / output_h as f32);
    ([scale_x, scale_y], [offset_x, offset_y])
}

/// Dispatch to the transform matching `mode` (FR-005).
fn uv_transform(mode: ScalingMode, image_w: u32, image_h: u32, output_w: u32, output_h: u32) -> ([f32; 2], [f32; 2]) {
    match mode {
        ScalingMode::Fill => fill_uv_transform(image_w, image_h, output_w, output_h),
        ScalingMode::Fit => fit_uv_transform(image_w, image_h, output_w, output_h),
        ScalingMode::Stretch => stretch_uv_transform(),
        ScalingMode::Center => center_uv_transform(image_w, image_h, output_w, output_h),
    }
}

/// One texture's scaling mode plus the fallback color to show outside its
/// transformed bounds (Fit/Center letterboxing) — bundled so [`CrossfadePipeline::
/// render`] takes one parameter per texture instead of two more loose ones.
#[derive(Debug, Clone, Copy)]
pub struct ImageScaling {
    pub mode: ScalingMode,
    pub fallback_color: Color,
}

/// The GPU crossfade blend pipeline (T015) — one instance shared across every managed
/// output (the pipeline/shader are output-independent; only the bind group, built
/// fresh per `render` call, references a specific pair of textures).
pub struct CrossfadePipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

impl CrossfadePipeline {
    /// Build the pipeline for a given output/render-target color format (each output
    /// may negotiate a different surface format, so this isn't cached process-wide).
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("crossfade"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/crossfade.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("crossfade-bind-group-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("crossfade-pipeline-layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("crossfade-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: "vs_main", buffers: &[], compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState { format: surface_format, blend: Some(wgpu::BlendState::REPLACE), write_mask: wgpu::ColorWrites::ALL })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("crossfade-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Self { pipeline, bind_group_layout, sampler }
    }

    /// Render one frame of the blend between `outgoing` and `incoming` at `progress`
    /// into `target`, sized `output_size` — each texture scaled per its own
    /// `ImageScaling` (FR-005; independently, since the outgoing/incoming images can
    /// belong to different packs with different scaling modes).
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        outgoing: &GpuTexture,
        outgoing_scaling: ImageScaling,
        incoming: &GpuTexture,
        incoming_scaling: ImageScaling,
        progress: f32,
        output_size: (u32, u32),
    ) {
        let (outgoing_scale, outgoing_offset) = uv_transform(outgoing_scaling.mode, outgoing.width, outgoing.height, output_size.0, output_size.1);
        let (incoming_scale, incoming_offset) = uv_transform(incoming_scaling.mode, incoming.width, incoming.height, output_size.0, output_size.1);
        let outgoing_fallback = color_to_f32(outgoing_scaling.fallback_color);
        let incoming_fallback = color_to_f32(incoming_scaling.fallback_color);

        let uniforms =
            Uniforms { progress, _pad0: [0.0; 3], outgoing_fallback, incoming_fallback, outgoing_scale, outgoing_offset, incoming_scale, incoming_offset };
        let uniform_buffer = wgpu::util::DeviceExt::create_buffer_init(
            device,
            &wgpu::util::BufferInitDescriptor { label: Some("crossfade-uniforms"), contents: bytemuck::bytes_of(&uniforms), usage: wgpu::BufferUsages::UNIFORM },
        );

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("crossfade-bind-group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&outgoing.view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&incoming.view) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("crossfade-encoder") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("crossfade-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit(Some(encoder.finish()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use schedule_engine::ImageId;

    fn transition(duration: Duration) -> CrossfadeTransition {
        CrossfadeTransition {
            outgoing: ImageId::new("a.jpg"),
            incoming: ImageId::new("b.jpg"),
            started_at: Instant::now(),
            duration,
        }
    }

    #[test]
    fn progress_starts_at_zero_and_ends_at_one() {
        let t = transition(Duration::from_secs(45));
        assert_eq!(t.progress_at(t.started_at), 0.0);
        assert_eq!(t.progress_at(t.started_at + Duration::from_secs(45)), 1.0);
        assert!(t.is_complete_at(t.started_at + Duration::from_secs(45)));
    }

    #[test]
    fn progress_is_monotonic_and_clamped() {
        let t = transition(Duration::from_secs(10));
        let p1 = t.progress_at(t.started_at + Duration::from_secs(3));
        let p2 = t.progress_at(t.started_at + Duration::from_secs(6));
        assert!(p1 < p2);
        assert!((0.0..=1.0).contains(&p1));

        // Beyond the duration, progress clamps at 1.0 rather than overshooting.
        let overshoot = t.progress_at(t.started_at + Duration::from_secs(999));
        assert_eq!(overshoot, 1.0);
    }

    #[test]
    fn zero_duration_is_immediately_complete() {
        let t = transition(Duration::ZERO);
        assert_eq!(t.progress_at(t.started_at), 1.0);
        assert!(t.is_complete_at(t.started_at));
    }

    /// FR-011: a new transition triggered while one is already mid-flight is simply a
    /// *new* `CrossfadeTransition` value (fresh `started_at`) replacing the old one —
    /// there's no stacking representation possible in this data shape at all, which is
    /// exactly the "cleanly supersede, never stack" requirement.
    #[test]
    fn a_new_transition_value_cleanly_replaces_an_in_flight_one() {
        let first = transition(Duration::from_secs(45));
        let mid_progress = first.progress_at(first.started_at + Duration::from_secs(10));
        assert!(mid_progress > 0.0 && mid_progress < 1.0);

        let second = CrossfadeTransition {
            outgoing: ImageId::new("b.jpg"),
            incoming: ImageId::new("c.jpg"),
            started_at: Instant::now(),
            duration: Duration::from_secs(45),
        };
        assert_eq!(second.progress_at(second.started_at), 0.0);
    }

    #[test]
    fn fill_transform_is_identity_for_matching_aspect_ratio() {
        let (scale, offset) = fill_uv_transform(1920, 1080, 1920, 1080);
        assert_eq!(scale, [1.0, 1.0]);
        assert_eq!(offset, [0.0, 0.0]);
    }

    #[test]
    fn fill_transform_crops_width_for_a_wider_image() {
        // A 2:1 image on a 16:9 output is relatively wider — crop left/right, full height.
        let (scale, offset) = fill_uv_transform(2000, 1000, 1920, 1080);
        assert!(scale[0] < 1.0);
        assert_eq!(scale[1], 1.0);
        assert!((offset[0] - (1.0 - scale[0]) / 2.0).abs() < 1e-6);
        assert_eq!(offset[1], 0.0);
    }

    #[test]
    fn fill_transform_crops_height_for_a_taller_image() {
        // A 1:1 image on a 16:9 output is relatively taller — crop top/bottom, full width.
        let (scale, offset) = fill_uv_transform(1000, 1000, 1920, 1080);
        assert_eq!(scale[0], 1.0);
        assert!(scale[1] < 1.0);
        assert_eq!(offset[0], 0.0);
        assert!((offset[1] - (1.0 - scale[1]) / 2.0).abs() < 1e-6);
    }

    #[test]
    fn stretch_transform_is_always_identity() {
        assert_eq!(stretch_uv_transform(), ([1.0, 1.0], [0.0, 0.0]));
    }

    #[test]
    fn fit_transform_letterboxes_top_bottom_for_a_wider_image() {
        // A 2:1 image on a 16:9 output is relatively wider — full width, letterbox
        // top/bottom. Unlike Fill's crop-direction scale (< 1.0), a letterboxed axis's
        // scale is *> 1.0* here (see `letterbox_scale_offset`'s doc for why) — hand-
        // computed independent of the implementation: frac = output_aspect/image_aspect
        // = (1920/1080)/(2000/1000) = 0.888..., scale_y = 1/frac = 1.125, offset_y =
        // -((1-frac)/2)/frac = -0.0625.
        let (scale, offset) = fit_uv_transform(2000, 1000, 1920, 1080);
        assert_eq!(scale[0], 1.0);
        assert!((scale[1] - 1.125).abs() < 1e-5, "scale[1] = {}", scale[1]);
        assert_eq!(offset[0], 0.0);
        assert!((offset[1] - (-0.0625)).abs() < 1e-5, "offset[1] = {}", offset[1]);

        // The transformed UV must actually leave [0, 1] near the output's top/bottom
        // edges — that's the signal the shader's bounds check substitutes
        // `fallback_color` on (this is the exact bug a prior, crop-direction version
        // of this formula had: it was self-consistent with hand-derived-the-same-wrong-
        // way expected values, but could never produce an out-of-bounds UV at all —
        // only caught by `tests/gpu_render.rs`'s actual pixel readback).
        let top_edge_uv = 0.0 * scale[1] + offset[1];
        let bottom_edge_uv = 1.0 * scale[1] + offset[1];
        assert!(!(0.0..=1.0).contains(&top_edge_uv), "top edge uv {top_edge_uv} should be out of bounds");
        assert!(!(0.0..=1.0).contains(&bottom_edge_uv), "bottom edge uv {bottom_edge_uv} should be out of bounds");
    }

    #[test]
    fn fit_transform_letterboxes_left_right_for_a_taller_image() {
        // A 1:1 image on a 16:9 output is relatively taller — full height, letterbox
        // left/right. frac = image_aspect/output_aspect = 1.0/(1920/1080) = 0.5625,
        // scale_x = 1/frac = 1.77778, offset_x = -((1-frac)/2)/frac = -0.388889.
        let (scale, offset) = fit_uv_transform(1000, 1000, 1920, 1080);
        assert!((scale[0] - 1.777778).abs() < 1e-4, "scale[0] = {}", scale[0]);
        assert_eq!(scale[1], 1.0);
        assert!((offset[0] - (-0.388889)).abs() < 1e-4, "offset[0] = {}", offset[0]);
        assert_eq!(offset[1], 0.0);

        let left_edge_uv = 0.0 * scale[0] + offset[0];
        let right_edge_uv = 1.0 * scale[0] + offset[0];
        assert!(!(0.0..=1.0).contains(&left_edge_uv), "left edge uv {left_edge_uv} should be out of bounds");
        assert!(!(0.0..=1.0).contains(&right_edge_uv), "right edge uv {right_edge_uv} should be out of bounds");
    }

    #[test]
    fn fit_transform_is_identity_for_matching_aspect_ratio() {
        let (scale, offset) = fit_uv_transform(1920, 1080, 1920, 1080);
        assert_eq!(scale, [1.0, 1.0]);
        assert_eq!(offset, [0.0, 0.0]);
    }

    #[test]
    fn center_transform_is_identity_at_native_size() {
        let (scale, offset) = center_uv_transform(1920, 1080, 1920, 1080);
        assert_eq!(scale, [1.0, 1.0]);
        assert_eq!(offset, [0.0, 0.0]);
    }

    #[test]
    fn center_transform_scale_exceeds_one_and_letterboxes_a_smaller_image() {
        // A 960x540 image (half the output's linear size) at native size on a
        // 1920x1080 output: frac = 0.5 on both axes, so scale = 1/frac = 2.0 (*not*
        // 0.5 — the letterbox-detecting scale is the inverse of the image's on-screen
        // size fraction, see `letterbox_scale_offset`'s doc), offset = -0.5, centered.
        let (scale, offset) = center_uv_transform(960, 540, 1920, 1080);
        assert_eq!(scale, [2.0, 2.0]);
        assert_eq!(offset, [-0.5, -0.5]);

        // The transformed UV must actually leave [0, 1] near the output's edges (the
        // corner especially) — the shader's cue to substitute `fallback_color`.
        let corner_uv = 0.0 * scale[0] + offset[0];
        let center_uv = 0.5 * scale[0] + offset[0];
        assert!(!(0.0..=1.0).contains(&corner_uv), "corner uv {corner_uv} should be out of bounds");
        assert!((0.0..=1.0).contains(&center_uv), "center uv {center_uv} should be in bounds");
    }

    #[test]
    fn center_transform_scale_is_below_one_and_crops_an_oversized_image() {
        // A 3840x2160 image (double the output's linear size) at native size on a
        // 1920x1080 output: frac = 2.0 on both axes, so scale = 1/frac = 0.5 — the same
        // general formula naturally produces a crop (never leaves [0, 1]) rather than a
        // letterbox for an oversized image, with no special-casing needed.
        let (scale, offset) = center_uv_transform(3840, 2160, 1920, 1080);
        assert_eq!(scale, [0.5, 0.5]);
        assert_eq!(offset, [0.25, 0.25]);

        let left_edge_uv = 0.0 * scale[0] + offset[0];
        let right_edge_uv = 1.0 * scale[0] + offset[0];
        assert!((0.0..=1.0).contains(&left_edge_uv), "left edge uv {left_edge_uv} should stay in bounds (crop, not letterbox)");
        assert!((0.0..=1.0).contains(&right_edge_uv), "right edge uv {right_edge_uv} should stay in bounds (crop, not letterbox)");
    }

    #[test]
    fn uv_transform_dispatches_to_the_matching_mode() {
        assert_eq!(uv_transform(ScalingMode::Stretch, 1000, 1000, 1920, 1080), stretch_uv_transform());
        assert_eq!(uv_transform(ScalingMode::Fill, 1000, 1000, 1920, 1080), fill_uv_transform(1000, 1000, 1920, 1080));
        assert_eq!(uv_transform(ScalingMode::Fit, 1000, 1000, 1920, 1080), fit_uv_transform(1000, 1000, 1920, 1080));
        assert_eq!(uv_transform(ScalingMode::Center, 1000, 1000, 1920, 1080), center_uv_transform(1000, 1000, 1920, 1080));
    }

    #[test]
    fn color_to_f32_normalizes_u8_channels_to_the_zero_one_range() {
        assert_eq!(color_to_f32(Color { r: 0, g: 0, b: 0, a: 0 }), [0.0, 0.0, 0.0, 0.0]);
        assert_eq!(color_to_f32(Color { r: 255, g: 255, b: 255, a: 255 }), [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(color_to_f32(Color { r: 128, g: 0, b: 0, a: 255 }), [128.0 / 255.0, 0.0, 0.0, 1.0]);
    }

    /// The `Uniforms` struct must byte-for-byte match `shaders/crossfade.wgsl`'s
    /// `Uniforms` struct under WGSL's uniform-address-space alignment rules
    /// (`vec4<f32>` 16-byte aligned, `vec2<f32>` 8-byte aligned, total size a multiple
    /// of the largest member's alignment) — catches a silent GPU-side corruption bug
    /// (wrong bytes landing in the wrong shader field) at `cargo test` time rather
    /// than only visually, the way the RON-parsing bug documented in `config.rs`/
    /// `README.md` was originally missed.
    #[test]
    fn uniforms_layout_matches_wgsl_alignment() {
        assert_eq!(std::mem::offset_of!(Uniforms, progress), 0);
        assert_eq!(std::mem::offset_of!(Uniforms, outgoing_fallback), 16);
        assert_eq!(std::mem::offset_of!(Uniforms, incoming_fallback), 32);
        assert_eq!(std::mem::offset_of!(Uniforms, outgoing_scale), 48);
        assert_eq!(std::mem::offset_of!(Uniforms, outgoing_offset), 56);
        assert_eq!(std::mem::offset_of!(Uniforms, incoming_scale), 64);
        assert_eq!(std::mem::offset_of!(Uniforms, incoming_offset), 72);
        assert_eq!(std::mem::size_of::<Uniforms>(), 80);
        assert_eq!(std::mem::size_of::<Uniforms>() % 16, 0);
    }
}
