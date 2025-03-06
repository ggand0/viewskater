use iced_wgpu::wgpu;
use iced_wgpu::wgpu::util::DeviceExt;
use std::sync::Arc;
use iced_core::Rectangle;

use crate::utils::timing::TimingStats;
use once_cell::sync::Lazy;
use std::sync::Mutex;

static TEXTURE_UPDATE_STATS: Lazy<Mutex<TimingStats>> = Lazy::new(|| {
    Mutex::new(TimingStats::new("Texture Update"))
});
static SHADER_RENDER_STATS: Lazy<Mutex<TimingStats>> = Lazy::new(|| {
    Mutex::new(TimingStats::new("Shader Render"))
});

pub struct TexturePipeline {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    //atlas_size: (u32, u32),
    index_buffer: wgpu::Buffer,
    num_indices: u32,
    window_size: (u32, u32),
    screen_rect_buffer: wgpu::Buffer,
    texture: Arc<wgpu::Texture>, // Store shared ownership of Texture
    bounds: (f32, f32, f32, f32), // Store shader widget bounds
    vertices: [f32; 16],
}

impl TexturePipeline {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        texture: Arc<wgpu::Texture>,
        //atlas_size: (u32, u32),
        window_size: (u32, u32),
        image_dimensions: (u32, u32),
        bounds_relative: (f32, f32, f32, f32), // Pass the shader widget bounds
    ) -> Self {
        let (x, y, width, height) = bounds_relative;
        let left = 2.0 * x - 1.0;
        let right = 2.0 * (x + width) - 1.0;
        let top = 1.0 - 2.0 * y;
        let bottom = 1.0 - 2.0 * (y + height);

        let vertices: [f32; 16] = [
            left, bottom, 0.0, 1.0, // Bottom-left
            right, bottom, 1.0, 1.0, // Bottom-right
            right, top, 1.0, 0.0, // Top-right
            left, top, 0.0, 0.0, // Top-left
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

        // Uniform buffer for offsets and scaling
        let (width, height) = image_dimensions; // Dimensions of the first image
        let image_dimensions = (width, height);

        // case 2: using individual textures
        let uniform_data = [
            0.0, //offset_x as f32 / atlas_size.0 as f32, // Normalized x offset within atlas
            0.0, //offset_y as f32 / atlas_size.1 as f32, // Normalized y offset within atlas
            1.0, // Scale x (width relative to the atlas)
            1.0, // Scale y (height relative to the atlas)
        ];
        //println!("atlas_size: {:?}", atlas_size); // atlas_size: (8192, 8192)
        //println!("image_dimensions: {:?}", image_dimensions);
        //println!("uniform_data: {:?}", uniform_data); // uniform_data: [0.5, 0.0, 0.43823242, 0.29248047]

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Uniform Buffer"),
            contents: bytemuck::cast_slice(&uniform_data),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        queue.write_buffer(
            &uniform_buffer,
            0,
            bytemuck::cast_slice(&uniform_data),
        );

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

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
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        // Compute screen_rect_data for uniform buffer
        let image_width = image_dimensions.0 as f32;
        let image_height = image_dimensions.1 as f32;
        let shader_width = window_size.0 as f32;  // Use shader widget size
        let shader_height = window_size.1 as f32;

        // Compute aspect ratios
        let image_aspect = image_width / image_height;
        let shader_aspect = shader_width / shader_height;

        // Determine scaled width and height while preserving aspect ratio
        let (scaled_width, scaled_height) = if image_aspect > shader_aspect {
            let width = shader_width;
            let height = width / image_aspect;
            (width, height)
        } else {
            let height = shader_height;
            let width = height * image_aspect;
            (width, height)
        };

        // Compute normalized offset (NDC coordinates)
        let offset_x = (shader_width - scaled_width) / 2.0;
        let offset_y = (shader_height - scaled_height) / 2.0;

        let screen_rect_data = [
            scaled_width / shader_width,  // Scale X (normalized)
            scaled_height / shader_height, // Scale Y (normalized)
            0.0,  // Offset X (NDC)
            0.0, // Offset Y (NDC, flipped)
        ];

        let screen_rect_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Screen Rect Buffer"),
            contents: bytemuck::cast_slice(&screen_rect_data),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });


        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
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
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &uniform_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &screen_rect_buffer,
                        offset: 0,
                        size: None,
                    }),
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
                    array_stride: 4 * std::mem::size_of::<f32>() as u64, // 2 for position, 2 for tex_coords
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
            vertex_buffer: vertex_buffer,
            bind_group,
            uniform_buffer,
            //atlas_size,
            index_buffer,
            num_indices: indices.len() as u32,
            window_size,
            screen_rect_buffer,
            texture,
            bounds: bounds_relative,
            vertices,
        }
    }


    #[allow(dead_code)]
    pub fn update_uniforms(
        &self,
        queue: &wgpu::Queue, // Pass the queue as a parameter
        _image_offset: (u32, u32),
        image_dimensions: (u32, u32),
        window_size: (u32, u32),
        atlas_size: (u32, u32),
    ) {
        let _scale_x = image_dimensions.0 as f32 / window_size.0 as f32;
        let _scale_y = image_dimensions.1 as f32 / window_size.1 as f32;
        let image_width = image_dimensions.0;
        let image_height = image_dimensions.1;

        // Calculate the scaled image dimensions to respect the aspect ratio
        let window_aspect = window_size.0 as f32 / window_size.1 as f32;
        let image_aspect = image_width as f32 / image_height as f32;

        let (scaled_width, scaled_height) = if image_aspect > window_aspect {
            // Image is wider than the window
            let width = window_size.0 as f32;
            let height = width / image_aspect;
            (width, height)
        } else {
            // Image is taller than the window
            let height = window_size.1 as f32;
            let width = height * image_aspect;
            (width, height)
        };
        let _offset_x = (window_size.0 as f32 - scaled_width) / 2.0;
        let _offset_y = (window_size.1 as f32 - scaled_height) / 2.0;
        
        /*let uniform_data = [
            image_offset.0 as f32 / atlas_size.0 as f32, // Normalized x offset within atlas
            image_offset.1 as f32 / atlas_size.1 as f32, // Normalized y offset within atlas
            image_dimensions.0 as f32 / atlas_size.0 as f32, // Scale x (width relative to the atlas)
            image_dimensions.1 as f32 / atlas_size.1 as f32, // Scale y (height relative to the atlas)
        ];*/

        // case 2: using individual textures
        let uniform_data = [
            0.0, //offset_x as f32 / atlas_size.0 as f32, // Normalized x offset within atlas
            0.0, //offset_y as f32 / atlas_size.1 as f32, // Normalized y offset within atlas
            image_dimensions.0 as f32 / atlas_size.0 as f32, // Scale x (width relative to the atlas)
            image_dimensions.1 as f32 / atlas_size.1 as f32, // Scale y (height relative to the atlas)
        ];

        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&uniform_data));
    }


    pub fn update_screen_uniforms(
        &self,
        queue: &wgpu::Queue,
        image_dimensions: (u32, u32),
        shader_size: (u32, u32),
        bounds_relative: (f32, f32, f32, f32),
    ) {
        let debug = false;
        let shader_width = shader_size.0 as f32;
        let shader_height = shader_size.1 as f32;
        let image_width = image_dimensions.0 as f32;
        let image_height = image_dimensions.1 as f32;
        let vertices = self.vertices;
        let (_left, bottom, _right, _top) = (vertices[0], vertices[1], vertices[2], vertices[3]);

        // Compute aspect ratios
        let image_aspect = image_width / image_height;
        let shader_aspect = shader_width / shader_height;

        // Calculate scale factors - the key is to use the SMALLER dimension to maintain aspect ratio
        let (scale_x, scale_y, fit_mode) = if image_aspect > shader_aspect {
            // Image is wider than container - fit width
            let scale = shader_width / image_width;
            (scale, scale, "FIT_WIDTH")
        } else {
            // Image is taller than container - fit height
            let scale = shader_height / image_height;
            (scale, scale, "FIT_HEIGHT")
        };

        // Apply scaling to get final dimensions
        let scaled_width = image_width * scale_x;
        let scaled_height = image_height * scale_y;
        
        // Calculate the scale factors relative to the container size
        let final_scale_x = scaled_width / shader_width;
        let final_scale_y = scaled_height / shader_height;
        
        // Calculate the vertical gap that needs to be distributed
        let gap_y = shader_height - scaled_height;
        
        // Calculate offset to center the scaled image vertically
        // Fine-tune the vertical offset with a correction factor to match Image widget
        // The bottom + 1.0 term accounts for asymmetric NDC space
        let offset_correction = 0.001; // Fine-tuning parameter (may need adjustment)
        let offset_y_ndc = (bottom + 1.0) * (1.0 - final_scale_y) / 2.0 + offset_correction;

        let screen_rect_data = [
            final_scale_x,      // Scale X 
            final_scale_y,      // Scale Y
            0.0,                // Offset X (centered horizontally)
            offset_y_ndc,       // Offset Y to center vertically
        ];

        if debug {
            println!("SHADER_DEBUG: ==============================================");
            println!("SHADER_DEBUG: Container dimensions: {}x{}", shader_width, shader_height);
            println!("SHADER_DEBUG: Image dimensions: {}x{}", image_width, image_height);
            println!("SHADER_DEBUG: Bounds relative: {:?}", bounds_relative);
            println!("SHADER_DEBUG: Image aspect: {}, Container aspect: {}", image_aspect, shader_aspect);
            println!("SHADER_DEBUG: Fit mode: {}, Scale factors: x={}, y={}", fit_mode, scale_x, scale_y);
            println!("SHADER_DEBUG: Scaled dimensions: {}x{}", scaled_width, scaled_height);
            println!("SHADER_DEBUG: Vertical gap: {}", gap_y);
            println!("SHADER_DEBUG: Final values: scale=[{}, {}], offset=[{}, {}]", 
                    final_scale_x, final_scale_y, 0.0, offset_y_ndc);
            println!("SHADER_DEBUG: ==============================================");
        }

        // Update screen rect buffer
        queue.write_buffer(
            &self.screen_rect_buffer,
            0,
            bytemuck::cast_slice(&screen_rect_data),
        );
    }
    
    pub fn update_vertices(&mut self, device: &wgpu::Device, bounds_relative: (f32, f32, f32, f32)) {
        let (x, y, width, height) = bounds_relative;
        let left = 2.0 * x - 1.0;
        let right = 2.0 * (x + width) - 1.0;
        let top = 1.0 - 2.0 * y;
        let bottom = 1.0 - 2.0 * (y + height);
    
        let vertices: [f32; 16] = [
            left, bottom, 0.0, 1.0, // Bottom-left
            right, bottom, 1.0, 1.0, // Bottom-right
            right, top, 1.0, 0.0, // Top-right
            left, top, 0.0, 0.0, // Top-left
        ];
    
        self.vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Quad Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        self.vertices = vertices;
    
        //println!("Updated vertex buffer with new bounds: {:?}", bounds_relative);
    }

    pub fn update_texture(
        &mut self,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        new_texture: Arc<wgpu::Texture>) {

        self.texture = new_texture.clone(); // Update stored texture reference

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let texture_view = self.texture.create_view(&wgpu::TextureViewDescriptor::default());

        self.bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &self.pipeline.get_bind_group_layout(0), // Ensure correct layout
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.uniform_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.screen_rect_buffer,
                        offset: 0,
                        size: None,
                    }),
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
            label: Some("Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    //load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
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
}
