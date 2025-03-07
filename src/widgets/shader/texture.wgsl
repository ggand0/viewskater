@group(0) @binding(0)
var my_texture: texture_2d<f32>;

@group(0) @binding(1)
var my_sampler: sampler;

@group(0) @binding(2)
var<uniform> texture_rect: vec4<f32>; // {offset_x, offset_y, scale_x, scale_y}

@group(0) @binding(3)
var<uniform> screen_rect: vec4<f32>; // {scaled_width, scaled_height, offset_x, offset_y}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
};

@vertex
fn vs_main(
    @location(0) position: vec2<f32>,
    @location(1) tex_coords: vec2<f32>,
) -> VertexOutput {
    var out: VertexOutput;
    
    // Simple pass-through of the position
    out.position = vec4<f32>(position, 0.0, 1.0);
    out.tex_coords = tex_coords;
    
    return out;
}

@fragment
fn fs_main(@location(0) tex_coords: vec2<f32>) -> @location(0) vec4<f32> {
    let color = textureSample(my_texture, my_sampler, tex_coords);
    return color;
}