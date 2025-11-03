// Atlas Shader
// Samples from a 2D texture array using atlas entry coordinates

@group(0) @binding(0)
var atlas_texture: texture_2d_array<f32>;

@group(0) @binding(1)
var atlas_sampler: sampler;

// Push constants for atlas entry info
struct AtlasEntry {
    atlas_rect: vec4<f32>,  // [x, y, width, height] normalized
    layer: u32,
    _padding: vec3<u32>,
}

var<push_constant> entry: AtlasEntry;

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
    
    // Pass through position (already in NDC)
    out.position = vec4<f32>(position, 0.0, 1.0);
    out.tex_coords = tex_coords;
    
    return out;
}

@fragment
fn fs_main(@location(0) tex_coords: vec2<f32>) -> @location(0) vec4<f32> {
    // Map texture coordinates to atlas coordinates
    let atlas_coords = entry.atlas_rect.xy + tex_coords * entry.atlas_rect.zw;
    
    // Sample from the specific layer in the texture array
    let color = textureSample(atlas_texture, atlas_sampler, atlas_coords, entry.layer);
    
    return color;
}

