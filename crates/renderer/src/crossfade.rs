//! [`CrossfadeTransition`] and its progress computation (data-model.md
//! `CrossfadeTransition`, FR-001, FR-002, FR-004, FR-011), plus [`CrossfadePipeline`],
//! the actual two-texture WGSL GPU blend (`shaders/crossfade.wgsl`) that consumes it.
//! The frame-callback draw loop that calls `CrossfadePipeline::render` once per tick is
//! `surface.rs`'s job (see `README.md`).

use std::time::{Duration, Instant};

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

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    progress: f32,
    _pad0: f32,
    outgoing_scale: [f32; 2],
    outgoing_offset: [f32; 2],
    incoming_scale: [f32; 2],
    incoming_offset: [f32; 2],
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
    /// into `target`, sized `output_size` — both textures use "Fill" (cover) scaling
    /// against the output (FR-005's default).
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        outgoing: &GpuTexture,
        incoming: &GpuTexture,
        progress: f32,
        output_size: (u32, u32),
    ) {
        let (outgoing_scale, outgoing_offset) = fill_uv_transform(outgoing.width, outgoing.height, output_size.0, output_size.1);
        let (incoming_scale, incoming_offset) = fill_uv_transform(incoming.width, incoming.height, output_size.0, output_size.1);

        let uniforms = Uniforms { progress, _pad0: 0.0, outgoing_scale, outgoing_offset, incoming_scale, incoming_offset };
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
}
