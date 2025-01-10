use iced_wgpu::wgpu;
use iced_wgpu::wgpu::util::DeviceExt;
use image::image_dimensions;

pub struct Pipeline {
    pipeline: wgpu::RenderPipeline,
    vertices: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    atlas_texture: wgpu::Texture, // Keep a reference to the texture for operations
    uniform_buffer: wgpu::Buffer,
    atlas_size: (u32, u32),
    index_buffer: wgpu::Buffer,
    num_indices: u32,
    window_size: (u32, u32),
    screen_rect_buffer: wgpu::Buffer,
}

impl Pipeline {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        image_data: Vec<(Vec<u8>, (u32, u32))>, // Vec of image data and dimensions
        atlas_size: (u32, u32),
        window_size: (u32, u32),
    ) -> Self {

        let vertices: [f32; 16] = [
            // positions    // tex_coords
            -1.0, -1.0, 0.0, 1.0, // bottom-left
             1.0, -1.0, 1.0, 1.0, // bottom-right
            -1.0,  1.0, 0.0, 0.0, // top-left
             1.0,  1.0, 1.0, 0.0, // top-right
        ];

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Quad Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let indices: &[u16] = &[0, 1, 2, 2, 1, 3];
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        });


        // Initialize the texture atlas
        let atlas_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Texture Atlas"),
            size: wgpu::Extent3d {
                width: atlas_size.0,
                height: atlas_size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        // Write image data into the atlas
        // NOTE: make sure image_data.clone() is not too resource expensive
        /*for ((data, (width, height)), (offset_x, offset_y)) in image_data.clone().into_iter().zip(image_offsets.iter()) {
            println!("offset_x: {}, offset_y: {}", offset_x, offset_y);
            queue.1(
                wgpu::ImageCopyTexture {
                    texture: &atlas_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: *offset_x,
                        y: *offset_y,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &data,
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some((width * 4).try_into().expect("Image width exceeds u32")),
                    rows_per_image: None,
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        }*/
        // Directly write the single image's data
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &atlas_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &image_data[0].0,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some((image_data[0].1 .0 * 4) as u32),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: image_data[0].1 .0,
                height: image_data[0].1 .1,
                depth_or_array_layers: 1,
            },
        );
        

        // Create texture view and bind group
        let texture_view = atlas_texture.create_view(
            &wgpu::TextureViewDescriptor::default());

        // Uniform buffer for offsets and scaling
        //let (offset_x, offset_y) = image_offsets[0]; // Assuming starting with the first image
        let (width, height) = image_data[0].1;      // Dimensions of the first image
        let image_dimensions = (width, height);

        // v1: scale texture to fit window
        let uniform_data = [
            0.0, //offset_x as f32 / atlas_size.0 as f32, // Normalized x offset within atlas
            0.0, //offset_y as f32 / atlas_size.1 as f32, // Normalized y offset within atlas
            image_dimensions.0 as f32 / atlas_size.0 as f32, // Scale x (width relative to the atlas)
            image_dimensions.1 as f32 / atlas_size.1 as f32, // Scale y (height relative to the atlas)
        ];
        println!("atlas_size: {:?}", atlas_size); // atlas_size: (8192, 8192)
        println!("image_dimensions: {:?}", image_dimensions);
        println!("uniform_data: {:?}", uniform_data); // uniform_data: [0.5, 0.0, 0.43823242, 0.29248047]

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


        // Calculate the scaled image dimensions to respect the aspect ratio
        let (image_width, image_height) = image_data[0].1;
        let window_width = window_size.0 as f32;
        let window_height = window_size.1 as f32;

        // Calculate aspect ratios
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


        // Center the image
        let offset_x = (window_width - scaled_width) / 2.0;
        let offset_y = (window_height - scaled_height) / 2.0;
        println!(
            "Calculated Offsets: offset_x = {}, offset_y = {}",
            offset_x, offset_y
        );
        println!(
            "Total Horizontal Space: {}, Left Offset: {}, Right Offset: {}",
            window_size.0 as f32 - scaled_width,
            offset_x,
            window_size.0 as f32 - scaled_width - offset_x
        );
        
        println!(
            "Total Vertical Space: {}, Top Offset: {}, Bottom Offset: {}",
            window_size.1 as f32 - scaled_height,
            offset_y,
            window_size.1 as f32 - scaled_height - offset_y
        );
        
        

        // Prepare uniform data for screen rect
        let screen_rect_data = [
            scaled_width / window_width, // Normalized scaled width
            scaled_height / window_height, // Normalized scaled height
            offset_x / window_width, // Normalized x offset
            offset_y / window_height, // Normalized y offset
        ];

        let screen_rect_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Screen Rect Buffer"),
            contents: bytemuck::cast_slice(&screen_rect_data),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
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
            vertices: vertex_buffer,
            bind_group,
            atlas_texture,
            uniform_buffer,
            atlas_size,
            index_buffer,
            num_indices: indices.len() as u32,
            window_size,
            screen_rect_buffer,
        }
    }

    // v1: scale texture to fit window
    pub fn update_uniforms(
        &self,
        queue: &wgpu::Queue, // Pass the queue as a parameter
        image_offset: (u32, u32),
        image_dimensions: (u32, u32),
        window_size: (u32, u32),
        atlas_size: (u32, u32),
    ) {
        let scale_x = image_dimensions.0 as f32 / window_size.0 as f32;
        let scale_y = image_dimensions.1 as f32 / window_size.1 as f32;
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
        let offset_x = (window_size.0 as f32 - scaled_width) / 2.0;
        let offset_y = (window_size.1 as f32 - scaled_height) / 2.0;
        

        let uniform_data = [
            image_offset.0 as f32 / atlas_size.0 as f32, // Normalized x offset within atlas
            image_offset.1 as f32 / atlas_size.1 as f32, // Normalized y offset within atlas
            image_dimensions.0 as f32 / atlas_size.0 as f32, // Scale x (width relative to the atlas)
            image_dimensions.1 as f32 / atlas_size.1 as f32, // Scale y (height relative to the atlas)
        ];

            
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&uniform_data));
    }
    
    pub fn update_screen_uniforms(
        &self,
        queue: &wgpu::Queue,
        image_dimensions: (u32, u32),
        window_size: (u32, u32),
    ) {
        let image_width = image_dimensions.0 as f32;
        let image_height = image_dimensions.1 as f32;
        let window_width = window_size.0 as f32;
        let window_height = window_size.1 as f32;
    
        // Calculate aspect ratios
        let image_aspect = image_width / image_height;
        let window_aspect = window_width / window_height;
    
        let (scaled_width, scaled_height) = if image_aspect > window_aspect {
            // Image is wider than the window
            let width = window_width;
            let height = width / image_aspect;
            (width, height)
        } else {
            // Image is taller than the window
            let height = window_height;
            let width = height * image_aspect;
            (width, height)
        };
        
        // Center the image (if needed)
        // wgpu uses Normalized device coordinates (NDC) for vertcies,
        // where the origin is at the center of the screen.
        // We initialize vertices at the four corners of the screen,
        // so if we add the offsets it'll be misplaced
        //let offset_x = (window_width - scaled_width) / 2.0;
        //let offset_y = (window_height - scaled_height) / 2.0;
        let offset_x = 0.0;
        let offset_y = 0.0;
    
        // Prepare uniform data
        let screen_rect_data = [
            scaled_width / window_width, // Normalized scaled width
            scaled_height / window_height, // Normalized scaled height
            offset_x / window_width, // Normalized x offset
            offset_y / window_height, // Normalized y offset
        ];

        queue.write_buffer(
            &self.screen_rect_buffer,
            0,
            bytemuck::cast_slice(&screen_rect_data),
        );
    }

    pub fn render(&self, target: &wgpu::TextureView, encoder: &mut wgpu::CommandEncoder) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..self.num_indices, 0, 0..1);
    }
}
