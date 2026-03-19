@group(0) @binding(0) var t: texture_2d<f32>;
@group(0) @binding(1) var s: sampler;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@location(0) pos: vec2<f32>, @location(1) uv: vec2<f32>) -> VsOut {
    return VsOut(vec4<f32>(pos, 0.0, 1.0), uv);
}

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let alpha = textureSample(t, s, uv).r;
    return vec4<f32>(vec3<f32>(1.0, 1.0, 1.0) * alpha, 1.0);
}
