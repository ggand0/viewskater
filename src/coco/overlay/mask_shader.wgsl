// Pixel-level mask shader using textures

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) tex_coords: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
    @location(1) color: vec4<f32>,
};

struct Uniforms {
    color: vec4<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var mask_texture: texture_2d<f32>;
@group(0) @binding(2) var mask_sampler: sampler;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    // Position is already in NDC from CPU-side conversion
    output.position = vec4<f32>(input.position, 0.0, 1.0);
    output.tex_coords = input.tex_coords;
    output.color = uniforms.color;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // Sample binary mask texture
    let mask_value = textureSample(mask_texture, mask_sampler, input.tex_coords).r;

    // Discard background pixels
    if (mask_value < 0.5) {
        discard;
    }

    // Apply category color
    return input.color;
}
