struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
};

struct Uniforms {
    atlas_coords: vec4<f32>,  // x, y, width, height in atlas
    layer: f32,               // atlas layer
    image_size: vec2<f32>,    // original image dimensions
    _padding: f32,
};

@group(0) @binding(0) var t_atlas: texture_2d_array<f32>;
@group(0) @binding(1) var s_atlas: sampler;
@group(0) @binding(2) var<uniform> uniforms: Uniforms;

@vertex
fn vs_main(
    @location(0) position: vec2<f32>,
    @location(1) tex_coords: vec2<f32>,
) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4<f32>(position, 0.0, 1.0);
    out.tex_coords = tex_coords;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Transform texture coordinates to atlas coordinates
    let atlas_x = uniforms.atlas_coords.x + in.tex_coords.x * uniforms.atlas_coords.z;
    let atlas_y = uniforms.atlas_coords.y + in.tex_coords.y * uniforms.atlas_coords.w;
    
    // Sample from the correct layer
    let color = textureSample(
        t_atlas, 
        s_atlas, 
        vec2<f32>(atlas_x, atlas_y), 
        i32(uniforms.layer)
    );
    
    return color;
}