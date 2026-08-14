// Two-texture crossfade blend (FR-001). A fullscreen triangle (no vertex buffer —
// generated from vertex_index) samples both textures per each's own scaling mode
// (Fill/Fit/Stretch/Center, FR-005) and linearly interpolates by `progress`.
//
// Field order here must byte-for-byte match `crossfade.rs`'s Rust-side `Uniforms`
// struct (verified there by `tests::uniforms_layout_matches_wgsl_alignment`) — the two
// `vec4<f32>` fallback-color fields come right after `progress` since WGSL's uniform
// layout rules require 16-byte alignment for `vec4<f32>` but only 8 for `vec2<f32>`.

struct Uniforms {
    progress: f32,
    // Letterbox fill color per texture (Fit/Center scaling) — shown where the
    // transformed UV falls outside [0, 1] instead of sampling (see fs_main below).
    outgoing_fallback: vec4<f32>,
    incoming_fallback: vec4<f32>,
    // UV scale/offset per texture, computed on the Rust side per each's `ScalingMode`:
    // sampling `uv * scale + offset` maps the fullscreen UV range onto the correctly
    // transformed portion of each source image.
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

// Sample `tex` at `uv`, or return `fallback` if `uv` falls outside [0, 1] on either
// axis — the letterboxing this pipeline needs for Fit/Center scaling. `ClampToEdge`
// (set on `tex_sampler`, crossfade.rs) alone can't express "show a color instead of
// the texture" — only repeat/clamp/mirror the texture itself — so this bounds check is
// the actual mechanism, not the sampler's address mode.
fn sample_or_fallback(tex: texture_2d<f32>, samp: sampler, uv: vec2<f32>, fallback: vec4<f32>) -> vec4<f32> {
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0))) {
        return fallback;
    }
    return textureSample(tex, samp, uv);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let outgoing_uv = in.uv * u.outgoing_scale + u.outgoing_offset;
    let incoming_uv = in.uv * u.incoming_scale + u.incoming_offset;

    // Each texture's color (sampled or letterbox-substituted) is resolved
    // independently before blending — the outgoing and incoming images may
    // legitimately have different scaling modes/fallback colors (different packs or
    // per-image overrides), so there's no need to reconcile them before this point.
    let outgoing_color = sample_or_fallback(outgoing_tex, tex_sampler, outgoing_uv, u.outgoing_fallback);
    let incoming_color = sample_or_fallback(incoming_tex, tex_sampler, incoming_uv, u.incoming_fallback);

    return mix(outgoing_color, incoming_color, u.progress);
}
