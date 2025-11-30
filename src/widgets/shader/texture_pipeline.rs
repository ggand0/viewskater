use std::sync::Arc;
use std::sync::Mutex;
use once_cell::sync::Lazy;
use iced_core::Rectangle;
use iced_wgpu::wgpu::{self, util::DeviceExt};
use crate::utils::timing::TimingStats;

static _TEXTURE_UPDATE_STATS: Lazy<Mutex<TimingStats>> = Lazy::new(|| {
    Mutex::new(TimingStats::new("Texture Update"))
});
static _SHADER_RENDER_STATS: Lazy<Mutex<TimingStats>> = Lazy::new(|| {
    Mutex::new(TimingStats::new("Shader Render"))
});

#[derive(Debug)]
pub struct TexturePipeline {
    pub pipeline: wgpu::RenderPipeline,
    pub vertex_buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
    pub index_buffer: wgpu::Buffer,
    pub num_indices: u32,
    pub texture: Arc<wgpu::Texture>,
}

impl TexturePipeline {
    pub fn new(
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        texture: Arc<wgpu::Texture>,
        _render_size: (u32, u32),
        _image_size: (u32, u32),
        bounds_relative: (f32, f32, f32, f32),
        use_nearest_filter: bool,
    ) -> Self {
        let debug = false;
        let (x, y, width, height) = bounds_relative;
        
        if debug {
            println!("PIPELINE_INIT: Bounds relative: x={}, y={}, w={}, h={}", x, y, width, height);
        }
        
        // Convert to NDC coordinates (-1 to 1)
        let left = 2.0 * x - 1.0;
        let right = 2.0 * (x + width) - 1.0;
        let top = 1.0 - 2.0 * y;
        let bottom = 1.0 - 2.0 * (y + height);
        
        if debug {
            println!("PIPELINE_INIT: NDC coords: left={}, right={}, top={}, bottom={}", 
                    left, right, top, bottom);
        }

        // Create vertices - each vertex has position and texture coordinates
        // Format: [position.x, position.y, texcoord.x, texcoord.y]
        let vertices: [f32; 16] = [
            left, bottom, 0.0, 1.0,   // Bottom-left
            right, bottom, 1.0, 1.0,  // Bottom-right
            right, top, 1.0, 0.0,     // Top-right
            left, top, 0.0, 0.0,      // Top-left
        ];
        
        let indices: &[u16] = &[0, 1, 2, 2, 3, 0];

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Quad Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let filter_mode = if use_nearest_filter {
            wgpu::FilterMode::Nearest
        } else {
            wgpu::FilterMode::Linear
        };

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: filter_mode,
            min_filter: filter_mode,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        
        // Simplified binding layout - we don't need complex uniform buffers
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
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
        
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
            label: Some("Bind Group"),
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader Module"),
            source: wgpu::ShaderSource::Wgsl(include_str!("./texture.wgsl").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
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
            primitive: wgpu::PrimitiveState::default(),
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
            bind_group,
            index_buffer,
            num_indices: indices.len() as u32,
            texture,
        }
    }

    pub fn update_texture(
        &mut self,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        new_texture: Arc<wgpu::Texture>,
        use_nearest_filter: bool,
    ) {
        if Arc::ptr_eq(&self.texture, &new_texture) {
            return; // No update needed
        }

        self.texture = new_texture;

        let filter_mode = if use_nearest_filter {
            wgpu::FilterMode::Nearest
        } else {
            wgpu::FilterMode::Linear
        };

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: filter_mode,
            min_filter: filter_mode,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let texture_view = self.texture.create_view(&wgpu::TextureViewDescriptor::default());
        
        self.bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
            label: Some("Updated Bind Group"),
        });
    }

    pub fn render(
        &self,
        target: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
        clip_bounds: &Rectangle<u32>,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Texture Pipeline Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        
        pass.set_scissor_rect(
            clip_bounds.x,
            clip_bounds.y,
            clip_bounds.width,
            clip_bounds.height,
        );
        
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..self.num_indices, 0, 0..1);
    }
    
    pub fn update_vertices(
        &self,
        _device: &wgpu::Device,
        _bounds_relative: (f32, f32, f32, f32),
    ) {
        // No-op: In our new design, we don't update vertices after creation
        // This is intentional to prevent jiggling
    }
    
    pub fn update_screen_uniforms(
        &self,
        _queue: &wgpu::Queue,
        _image_dimensions: (u32, u32),
        _shader_size: (u32, u32),
        _bounds_relative: (f32, f32, f32, f32),
    ) {
        // No-op: In our new design, we don't use screen uniforms anymore
        // This is intentional to simplify the pipeline
    }
}
