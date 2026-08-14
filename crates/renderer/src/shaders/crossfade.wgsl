// Two-texture crossfade blend (FR-001). A fullscreen triangle (no vertex buffer —
// generated from vertex_index) samples both textures with "Fill" (cover) scaling and
// linearly interpolates by `progress`.

struct Uniforms {
    progress: f32,
    // Fill-mode UV scale/offset per texture: sampling `uv * scale + offset` maps the
    // fullscreen UV range onto the correctly-cropped portion of each source image.
    outgoing_scale: vec2<f32>,
    outgoing_offset: vec2<f32>,
    incoming_scale: vec2<f32>,
    incoming_offset: vec2<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var outgoing_tex: texture_2d<f32>;
@group(0) @binding(2) var incoming_tex: texture_2d<f32>;
@group(0) @binding(3) var tex_sampler: sampler;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // Fullscreen triangle covering the whole clip-space quad (standard trick: 3
    // vertices, no vertex buffer, clipped to the viewport by the rasterizer).
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let pos = positions[vertex_index];

    var out: VertexOutput;
    out.clip_position = vec4<f32>(pos, 0.0, 1.0);
    // UV: [0,0] at top-left, [1,1] at bottom-right of the visible output.
    out.uv = vec2<f32>(pos.x * 0.5 + 0.5, 1.0 - (pos.y * 0.5 + 0.5));
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let outgoing_uv = in.uv * u.outgoing_scale + u.outgoing_offset;
    let incoming_uv = in.uv * u.incoming_scale + u.incoming_offset;

    let outgoing_color = textureSample(outgoing_tex, tex_sampler, outgoing_uv);
    let incoming_color = textureSample(incoming_tex, tex_sampler, incoming_uv);

    return mix(outgoing_color, incoming_color, u.progress);
}
