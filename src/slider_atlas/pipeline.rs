// Atlas Rendering Pipeline
// Renders images from the atlas texture array to screen

use iced_core::Rectangle;
use iced_wgpu::wgpu::{self, util::DeviceExt};
use crate::slider_atlas::{Atlas, Entry};

#[allow(unused_imports)]
use log::{debug, info, warn};

/// Push constants for communicating atlas entry info to shader
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct AtlasEntryPushConstants {
    /// Atlas coordinates: [x, y, width, height] normalized (0.0-1.0)
    atlas_rect: [f32; 4],
    /// Layer index in the texture array
    layer: u32,
    /// Padding to align to 16 bytes
    _padding: [u32; 3],
}

#[derive(Debug)]
pub struct AtlasPipeline {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    num_indices: u32,
    sampler: wgpu::Sampler,
}

impl AtlasPipeline {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
    ) -> Self {
        // Create vertices for full-screen quad
        // Format: [position.x, position.y, texcoord.x, texcoord.y]
        let vertices: [f32; 16] = [
            -1.0, -1.0, 0.0, 1.0,   // Bottom-left
             1.0, -1.0, 1.0, 1.0,   // Bottom-right
             1.0,  1.0, 1.0, 0.0,   // Top-right
            -1.0,  1.0, 0.0, 0.0,   // Top-left
        ];
        
        let indices: &[u16] = &[0, 1, 2, 2, 3, 0];

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Atlas Quad Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Atlas Index Buffer"),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Atlas Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Bind group layout for atlas texture array + sampler
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Atlas Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,  // Array!
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Atlas Shader Module"),
            source: wgpu::ShaderSource::Wgsl(include_str!("./atlas.wgsl").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Atlas Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[
                // Push constants for atlas entry info
                wgpu::PushConstantRange {
                    stages: wgpu::ShaderStages::FRAGMENT,
                    range: 0..std::mem::size_of::<AtlasEntryPushConstants>() as u32,
                },
            ],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Atlas Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 4 * std::mem::size_of::<f32>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            offset: 0,
                            shader_location: 0,
                            format: wgpu::VertexFormat::Float32x2,
                        },
                        wgpu::VertexAttribute {
                            offset: 2 * std::mem::size_of::<f32>() as u64,
                            shader_location: 1,
                            format: wgpu::VertexFormat::Float32x2,
                        },
                    ],
                }],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
        });

        Self {
            pipeline,
            vertex_buffer,
            index_buffer,
            num_indices: indices.len() as u32,
            sampler,
        }
    }

    /// Render an atlas entry to the screen
    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        atlas: &Atlas,
        entry: &Entry,
        target: &wgpu::TextureView,
        bounds: Rectangle,
        viewport_size: (u32, u32),
        clip_bounds: &Rectangle<u32>,
    ) {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Atlas Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,  // Don't clear - overlay on existing content
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, atlas.bind_group(), &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        
        // Set scissor rect for clipping
        render_pass.set_scissor_rect(
            clip_bounds.x,
            clip_bounds.y,
            clip_bounds.width,
            clip_bounds.height,
        );

        // Render each part of the entry (could be fragmented)
        match entry {
            Entry::Contiguous(allocation) => {
                let push_constants = self.calculate_push_constants(allocation, bounds, viewport_size);
                render_pass.set_push_constants(
                    wgpu::ShaderStages::FRAGMENT,
                    0,
                    bytemuck::bytes_of(&push_constants),
                );
                render_pass.draw_indexed(0..self.num_indices, 0, 0..1);
            }
            Entry::Fragmented { fragments, .. } => {
                // For fragmented entries, render each fragment
                // TODO: This is complex - may need multiple draw calls or a different approach
                // For now, just render the first fragment as a fallback
                if let Some(fragment) = fragments.first() {
                    let push_constants = self.calculate_push_constants(&fragment.allocation, bounds, viewport_size);
                    render_pass.set_push_constants(
                        wgpu::ShaderStages::FRAGMENT,
                        0,
                        bytemuck::bytes_of(&push_constants),
                    );
                    render_pass.draw_indexed(0..self.num_indices, 0, 0..1);
                    
                    warn!("Fragmented atlas entry rendering not fully implemented - showing first fragment only");
                }
            }
        }
    }

    fn calculate_push_constants(
        &self,
        allocation: &crate::slider_atlas::Allocation,
        _bounds: Rectangle,
        _viewport_size: (u32, u32),
    ) -> AtlasEntryPushConstants {
        let (x, y) = allocation.position();
        let size = allocation.size();
        let layer = allocation.layer();
        
        // Normalize coordinates to 0.0-1.0 range for atlas
        let atlas_size = crate::slider_atlas::ATLAS_SIZE as f32;
        let atlas_rect = [
            x as f32 / atlas_size,
            y as f32 / atlas_size,
            size.width as f32 / atlas_size,
            size.height as f32 / atlas_size,
        ];
        
        AtlasEntryPushConstants {
            atlas_rect,
            layer: layer as u32,
            _padding: [0; 3],
        }
    }
}

