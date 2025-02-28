use crate::atlas::atlas::{Atlas, SIZE};
use crate::atlas::entry::Entry;
use iced_winit::core::{Rectangle, Rectangle as IcedRectangle};
use iced_widget::shader::Viewport;
use std::sync::Arc;
use iced_wgpu::wgpu;
use bytemuck::{Pod, Zeroable};
use iced_wgpu::wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct Vertex {
    position: [f32; 2],
    tex_coords: [f32; 2],
}

impl Vertex {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

// Uniforms to pass to the shader
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
struct AtlasUniforms {
    atlas_coords: [f32; 4],  // x, y, width, height in atlas
    layer: f32,               // atlas layer
    image_size: [f32; 2],    // original image dimensions
    _padding: f32,           // Padding for alignment
}

pub struct AtlasPipeline {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    vertices_len: u32,
}

impl AtlasPipeline {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        atlas: &Atlas,
        image_size: (u32, u32),
        bounds: &Rectangle,
        viewport: &Viewport,
    ) -> Self {
        // Create shader module with atlas-specific sampling
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Atlas Shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(include_str!("atlas_texture.wgsl"))),
        });

        // Get the texture view from atlas - we need to see how Atlas's API works
        // Assuming Atlas has a method to get its texture view
        let texture_view = atlas.texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("Atlas Texture View"),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        
        // Create a sampler directly
        let atlas_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Atlas Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // Create the render pipeline layout
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Atlas Bind Group Layout"),
            entries: &[
                // Atlas texture binding
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                // Sampler binding
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // Uniform buffer binding
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Atlas Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        // Create the render pipeline
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Atlas Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[Vertex::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        // Create the vertices for a quad
        let vertices = Self::create_vertices(bounds, viewport);
        let vertices_len = vertices.len() as u32;

        // Create a buffer for the vertices
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Atlas Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        // Create a buffer for the uniforms
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Atlas Uniform Buffer"),
            size: std::mem::size_of::<AtlasUniforms>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create the bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Atlas Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&atlas_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &uniform_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
            ],
        });

        AtlasPipeline {
            pipeline,
            vertex_buffer,
            uniform_buffer,
            bind_group,
            vertices_len,
        }
    }

    // Helper function to create quad vertices
    fn create_vertices(bounds: &Rectangle, viewport: &Viewport) -> Vec<Vertex> {
        // ... existing code ...
        // Return a vector of vertices
        vec![
            // Top-left
            Vertex {
                position: [-1.0, -1.0],
                tex_coords: [0.0, 0.0],
            },
            // Bottom-left
            Vertex {
                position: [-1.0, 1.0],
                tex_coords: [0.0, 1.0],
            },
            // Bottom-right
            Vertex {
                position: [1.0, 1.0],
                tex_coords: [1.0, 1.0],
            },
            // Top-left
            Vertex {
                position: [-1.0, -1.0],
                tex_coords: [0.0, 0.0],
            },
            // Bottom-right
            Vertex {
                position: [1.0, 1.0],
                tex_coords: [1.0, 1.0],
            },
            // Top-right
            Vertex {
                position: [1.0, -1.0],
                tex_coords: [1.0, 0.0],
            },
        ]
    }

    pub fn update_vertices(
        &mut self,
        device: &wgpu::Device,
        bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        let vertices = Self::create_vertices(bounds, viewport);
        self.vertices_len = vertices.len() as u32;

        self.vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Updated Atlas Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
    }

    pub fn update_uniforms(
        &self,
        queue: &wgpu::Queue,
        entry: &Entry,
        image_size: (u32, u32),
        atlas: &Atlas,
    ) {
        // Get the atlas size from the constant SIZE
        let atlas_size = SIZE as f32;

        // Create the uniforms based on the entry type
        let uniforms = match entry {
            Entry::Contiguous(allocation) => {
                // Get position and size from allocation using position() method
                let (x, y) = allocation.position();
                let width = allocation.size().width;
                let height = allocation.size().height;
                
                AtlasUniforms {
                    atlas_coords: [
                        x as f32 / atlas_size,
                        y as f32 / atlas_size,
                        width as f32 / atlas_size,
                        height as f32 / atlas_size,
                    ],
                    layer: allocation.layer() as f32,
                    image_size: [image_size.0 as f32, image_size.1 as f32],
                    _padding: 0.0,
                }
            }
            Entry::Fragmented { fragments, size } => {
                if let Some(first_fragment) = fragments.first() {
                    // Use position() which returns a tuple
                    let (x, y) = first_fragment.position;
                    let allocation = &first_fragment.allocation;
                    let layer = allocation.layer();
                    
                    AtlasUniforms {
                        atlas_coords: [
                            x as f32 / atlas_size,
                            y as f32 / atlas_size,
                            allocation.size().width as f32 / atlas_size,
                            allocation.size().height as f32 / atlas_size,
                        ],
                        layer: layer as f32,
                        image_size: [size.width as f32, size.height as f32],
                        _padding: 0.0,
                    }
                } else {
                    // Default values if no fragments
                    AtlasUniforms {
                        atlas_coords: [0.0, 0.0, 1.0, 1.0],
                        layer: 0.0,
                        image_size: [image_size.0 as f32, image_size.1 as f32],
                        _padding: 0.0,
                    }
                }
            }
        };

        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));
    }

    pub fn render(
        &self,
        target: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
        clip_bounds: &Rectangle<u32>,
    ) {
        // Create a render pass and draw the quad with the atlas texture
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Atlas Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        
        // Set scissor rect based on clip bounds
        render_pass.set_scissor_rect(
            clip_bounds.x,
            clip_bounds.y,
            clip_bounds.width,
            clip_bounds.height,
        );
        
        render_pass.draw(0..self.vertices_len, 0..1);
    }
}