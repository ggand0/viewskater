/// Pixel-perfect mask shader widget for rendering COCO RLE segmentation masks
///
/// Uses WGPU texture-based rendering for exact pixel-level mask representation.
use std::marker::PhantomData;
use std::collections::HashMap;
use iced_core::{Color, Rectangle, Size, Length, Vector};
use iced_core::layout::{self, Layout};
use iced_core::mouse;
use iced_core::renderer;
use iced_core::widget::tree::Tree;
use iced_winit::core::{Element, Widget};
use iced_widget::shader::{self, Viewport, Storage};
use iced_wgpu::{wgpu, primitive};
use wgpu::util::DeviceExt;
use crate::coco::parser::{ImageAnnotation, CocoSegmentation};
use crate::coco::rle_decoder;

/// Maximum number of textures to cache in GPU memory
const MAX_TEXTURE_CACHE_SIZE: usize = 200;

/// A shader widget for rendering pixel-perfect masks
pub struct MaskShader<Message> {
    width: Length,
    height: Length,
    annotations: Vec<ImageAnnotation>,
    image_size: (u32, u32),
    zoom_scale: f32,
    zoom_offset: Vector,
    _phantom: PhantomData<Message>,
}

impl<Message> MaskShader<Message> {
    pub fn new(annotations: Vec<ImageAnnotation>, image_size: (u32, u32), zoom_scale: f32, zoom_offset: Vector) -> Self {
        Self {
            width: Length::Fill,
            height: Length::Fill,
            annotations,
            image_size,
            zoom_scale,
            zoom_offset,
            _phantom: PhantomData,
        }
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }
}

/// Primitive for mask rendering
#[derive(Debug)]
pub struct MaskPrimitive {
    bounds: Rectangle,
    annotations: Vec<ImageAnnotation>,
    image_size: (u32, u32),
    zoom_scale: f32,
    zoom_offset: Vector,
}

/// Cache for render resources
struct MaskBufferCache {
    quads: Vec<QuadRenderData>,
}

struct QuadRenderData {
    vertex_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

/// GPU texture cache for RLE masks
/// Maps annotation cache ID to texture handle
type MaskTextureCache = HashMap<u64, CachedTexture>;

struct CachedTexture {
    #[allow(dead_code)]
    texture: wgpu::Texture,  // Keep texture alive for the view
    view: wgpu::TextureView,
    width: u32,
    height: u32,
    last_used: std::time::Instant,
}

/// Helper to get a unique ID for caching (same as polygon shader for consistency)
fn get_annotation_cache_id(ann: &ImageAnnotation) -> u64 {
    let bbox_hash = ((ann.bbox.x * 1000.0) as u64)
        .wrapping_mul(31)
        .wrapping_add((ann.bbox.y * 1000.0) as u64)
        .wrapping_mul(31)
        .wrapping_add((ann.bbox.width * 1000.0) as u64)
        .wrapping_mul(31)
        .wrapping_add((ann.bbox.height * 1000.0) as u64);

    ann.category_id.wrapping_mul(1000000).wrapping_add(bbox_hash)
}

impl shader::Primitive for MaskPrimitive {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        storage: &mut Storage,
        _bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        // Store viewport for use in render
        storage.store(viewport.clone());

        // Create pipeline if needed
        if !storage.has::<MaskPipeline>() {
            let pipeline = MaskPipeline::new(device, format);
            storage.store(pipeline);
        }

        // Get or create texture cache
        if !storage.has::<MaskTextureCache>() {
            storage.store(MaskTextureCache::new());
        }

        // Evict old textures if cache is full (before getting mutable reference)
        {
            let texture_cache = storage.get_mut::<MaskTextureCache>().unwrap();
            if texture_cache.len() >= MAX_TEXTURE_CACHE_SIZE {
                evict_lru_textures(texture_cache, MAX_TEXTURE_CACHE_SIZE / 5);
            }
        }

        let viewport_size = viewport.physical_size();
        let scale_factor = viewport.scale_factor() as f32;

        let image_width = self.image_size.0 as f32;
        let image_height = self.image_size.1 as f32;
        let display_width = self.bounds.width;
        let display_height = self.bounds.height;

        // Base scale from ContentFit::Contain
        let width_ratio = display_width / image_width;
        let height_ratio = display_height / image_height;
        let base_scale = width_ratio.min(height_ratio);

        // Calculate zoomed image dimensions
        let zoomed_image_width = image_width * base_scale * self.zoom_scale;
        let zoomed_image_height = image_height * base_scale * self.zoom_scale;

        // Centering offset after zoom
        let center_offset_x = (display_width - zoomed_image_width) / 2.0;
        let center_offset_y = (display_height - zoomed_image_height) / 2.0;

        // Get pipeline references we'll need (before mutable borrow)
        let bind_group_layout = &storage.get::<MaskPipeline>().unwrap().bind_group_layout as *const _;
        let sampler = &storage.get::<MaskPipeline>().unwrap().sampler as *const _;

        // Now get mutable reference to cache
        let texture_cache = storage.get_mut::<MaskTextureCache>().unwrap();
        let mut quads = Vec::new();

        // Safety: These pointers are valid for the lifetime of the function
        // and we're not modifying the pipeline while using them
        let bind_group_layout = unsafe { &*bind_group_layout };
        let sampler = unsafe { &*sampler };

        log::debug!("MaskShader: Processing {} annotations", self.annotations.len());

        for annotation in self.annotations.iter() {
            // Only process RLE masks
            if let Some(CocoSegmentation::Rle(rle)) = &annotation.segmentation {
                log::debug!("MaskShader: Found RLE mask, size: {:?}", rle.size);
                let cache_id = get_annotation_cache_id(annotation);

                // Check if texture is already cached
                let texture_data = if let Some(cached) = texture_cache.get_mut(&cache_id) {
                    // Update last used time
                    cached.last_used = std::time::Instant::now();
                    cached
                } else {
                    // Decode RLE and create texture
                    let mut mask = rle_decoder::decode_rle(rle);

                    if mask.is_empty() || rle.size.len() != 2 {
                        log::warn!("MaskShader: Empty mask or invalid size, skipping");
                        continue;
                    }

                    // Convert 0/1 values to 0/255 for R8Unorm texture
                    // R8Unorm maps 0 -> 0.0 and 255 -> 1.0 when sampled
                    for pixel in mask.iter_mut() {
                        *pixel = if *pixel > 0 { 255 } else { 0 };
                    }

                    log::debug!("MaskShader: Decoded mask, {} bytes", mask.len());

                    let mask_height = rle.size[0];
                    let mask_width = rle.size[1];

                    // Create R8Unorm texture
                    let texture = device.create_texture(&wgpu::TextureDescriptor {
                        label: Some("Mask Texture"),
                        size: wgpu::Extent3d {
                            width: mask_width,
                            height: mask_height,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::R8Unorm,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                        view_formats: &[],
                    });

                    // Upload mask data
                    queue.write_texture(
                        wgpu::ImageCopyTexture {
                            texture: &texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        &mask,
                        wgpu::ImageDataLayout {
                            offset: 0,
                            bytes_per_row: Some(mask_width),
                            rows_per_image: Some(mask_height),
                        },
                        wgpu::Extent3d {
                            width: mask_width,
                            height: mask_height,
                            depth_or_array_layers: 1,
                        },
                    );

                    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

                    let cached_texture = CachedTexture {
                        texture,
                        view,
                        width: mask_width,
                        height: mask_height,
                        last_used: std::time::Instant::now(),
                    };

                    texture_cache.insert(cache_id, cached_texture);
                    texture_cache.get(&cache_id).unwrap()
                };

                // Calculate mask dimensions (may differ from image size)
                let mask_width = texture_data.width as f32;
                let mask_height = texture_data.height as f32;

                // Check if scaling is needed
                let needs_scaling = (mask_width - image_width).abs() > 1.0
                    || (mask_height - image_height).abs() > 1.0;

                let (final_width, final_height) = if needs_scaling {
                    (image_width, image_height)
                } else {
                    (mask_width, mask_height)
                };

                // Calculate bbox in screen coordinates (same as other shaders)
                let scaled_x = 0.0 * base_scale * self.zoom_scale;
                let scaled_y = 0.0 * base_scale * self.zoom_scale;
                let scaled_width = final_width * base_scale * self.zoom_scale;
                let scaled_height = final_height * base_scale * self.zoom_scale;

                let screen_x = (scaled_x + center_offset_x - self.zoom_offset.x + self.bounds.x) * scale_factor;
                let screen_y = (scaled_y + center_offset_y - self.zoom_offset.y + self.bounds.y) * scale_factor;
                let screen_width = scaled_width * scale_factor;
                let screen_height = scaled_height * scale_factor;

                // Create quad vertices in NDC
                let vertices = create_quad_vertices(
                    screen_x,
                    screen_y,
                    screen_width,
                    screen_height,
                    viewport_size,
                );

                let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Mask Quad Vertex Buffer"),
                    contents: bytemuck::cast_slice(&vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });

                // Get category color
                let color = get_category_color(annotation.category_id);

                // Create uniform buffer with color
                let uniform_data = MaskUniforms {
                    color: [color.r, color.g, color.b, color.a * 0.5], // Apply transparency
                };

                let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Mask Uniform Buffer"),
                    contents: bytemuck::cast_slice(&[uniform_data]),
                    usage: wgpu::BufferUsages::UNIFORM,
                });

                // Create bind group
                let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Mask Bind Group"),
                    layout: bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: uniform_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&texture_data.view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(sampler),
                        },
                    ],
                });

                quads.push(QuadRenderData {
                    vertex_buffer,
                    bind_group,
                });

                log::debug!("MaskShader: Created quad for mask at screen ({}, {}), size ({}, {})",
                    screen_x, screen_y, screen_width, screen_height);
            }
        }

        log::debug!("MaskShader: Total quads created: {}", quads.len());
        storage.store(MaskBufferCache { quads });
    }

    fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        storage: &Storage,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        if self.annotations.is_empty() {
            return;
        }

        if let Some(pipeline) = storage.get::<MaskPipeline>() {
            if let Some(cache) = storage.get::<MaskBufferCache>() {
                for quad in &cache.quads {
                    let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Mask Render Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: target,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });

                    render_pass.set_scissor_rect(
                        clip_bounds.x,
                        clip_bounds.y,
                        clip_bounds.width,
                        clip_bounds.height,
                    );

                    render_pass.set_pipeline(&pipeline.render_pipeline);
                    render_pass.set_bind_group(0, &quad.bind_group, &[]);
                    render_pass.set_vertex_buffer(0, quad.vertex_buffer.slice(..));
                    render_pass.draw(0..6, 0..1); // 6 vertices for 2 triangles
                }
            }
        }
    }
}

/// Create quad vertices (2 triangles) in NDC coordinates
fn create_quad_vertices(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    viewport_size: Size<u32>,
) -> [MaskVertex; 6] {
    // Convert to NDC
    let x1_ndc = (x / viewport_size.width as f32) * 2.0 - 1.0;
    let y1_ndc = 1.0 - (y / viewport_size.height as f32) * 2.0;
    let x2_ndc = ((x + width) / viewport_size.width as f32) * 2.0 - 1.0;
    let y2_ndc = 1.0 - ((y + height) / viewport_size.height as f32) * 2.0;

    [
        // Triangle 1
        MaskVertex {
            position: [x1_ndc, y1_ndc],
            tex_coords: [0.0, 0.0],
        },
        MaskVertex {
            position: [x2_ndc, y1_ndc],
            tex_coords: [1.0, 0.0],
        },
        MaskVertex {
            position: [x1_ndc, y2_ndc],
            tex_coords: [0.0, 1.0],
        },
        // Triangle 2
        MaskVertex {
            position: [x2_ndc, y1_ndc],
            tex_coords: [1.0, 0.0],
        },
        MaskVertex {
            position: [x2_ndc, y2_ndc],
            tex_coords: [1.0, 1.0],
        },
        MaskVertex {
            position: [x1_ndc, y2_ndc],
            tex_coords: [0.0, 1.0],
        },
    ]
}

/// Evict least-recently-used textures from cache
fn evict_lru_textures(cache: &mut MaskTextureCache, count: usize) {
    let mut entries: Vec<_> = cache.iter().map(|(id, tex)| (*id, tex.last_used)).collect();
    entries.sort_by_key(|(_, time)| *time);

    let to_remove: Vec<_> = entries.iter().take(count).map(|(id, _)| *id).collect();
    for id in to_remove {
        cache.remove(&id);
    }

    log::debug!("Evicted {} textures from mask cache, {} remaining", count, cache.len());
}

/// Vertex data for mask quad rendering
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct MaskVertex {
    position: [f32; 2],
    tex_coords: [f32; 2],
}

impl MaskVertex {
    const ATTRIBS: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2];

    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<MaskVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

/// Uniform data for mask rendering
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct MaskUniforms {
    color: [f32; 4],
}

/// WGPU pipeline for rendering textured masks
#[derive(Debug)]
struct MaskPipeline {
    render_pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

impl MaskPipeline {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Mask Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("mask_shader.wgsl").into()),
        });

        // Create nearest-neighbor sampler for pixel-perfect rendering
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Mask Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Mask Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Mask Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Mask Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[MaskVertex::desc()],
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
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        Self {
            render_pipeline,
            bind_group_layout,
            sampler,
        }
    }
}

/// Get color for category using YOLO/YOLOX color scheme
fn get_category_color(category_id: u64) -> Color {
    let colors = [
        [0.000, 0.447, 0.741], [0.850, 0.325, 0.098], [0.929, 0.694, 0.125],
        [0.494, 0.184, 0.556], [0.466, 0.674, 0.188], [0.301, 0.745, 0.933],
        [0.635, 0.078, 0.184], [0.300, 0.300, 0.300], [0.600, 0.600, 0.600],
        [1.000, 0.000, 0.000], [1.000, 0.500, 0.000], [0.749, 0.749, 0.000],
        [0.000, 1.000, 0.000], [0.000, 0.000, 1.000], [0.667, 0.000, 1.000],
        [0.333, 0.333, 0.000], [0.333, 0.667, 0.000], [0.333, 1.000, 0.000],
        [0.667, 0.333, 0.000], [0.667, 0.667, 0.000], [0.667, 1.000, 0.000],
        [1.000, 0.333, 0.000], [1.000, 0.667, 0.000], [1.000, 1.000, 0.000],
        [0.000, 0.333, 0.500], [0.000, 0.667, 0.500], [0.000, 1.000, 0.500],
        [0.333, 0.000, 0.500], [0.333, 0.333, 0.500], [0.333, 0.667, 0.500],
        [0.333, 1.000, 0.500], [0.667, 0.000, 0.500], [0.667, 0.333, 0.500],
        [0.667, 0.667, 0.500], [0.667, 1.000, 0.500], [1.000, 0.000, 0.500],
        [1.000, 0.333, 0.500], [1.000, 0.667, 0.500], [1.000, 1.000, 0.500],
        [0.000, 0.333, 1.000], [0.000, 0.667, 1.000], [0.000, 1.000, 1.000],
        [0.333, 0.000, 1.000], [0.333, 0.333, 1.000], [0.333, 0.667, 1.000],
        [0.333, 1.000, 1.000], [0.667, 0.000, 1.000], [0.667, 0.333, 1.000],
        [0.667, 0.667, 1.000], [0.667, 1.000, 1.000], [1.000, 0.000, 1.000],
        [1.000, 0.333, 1.000], [1.000, 0.667, 1.000], [0.333, 0.000, 0.000],
        [0.500, 0.000, 0.000], [0.667, 0.000, 0.000], [0.833, 0.000, 0.000],
        [1.000, 0.000, 0.000], [0.000, 0.167, 0.000], [0.000, 0.333, 0.000],
        [0.000, 0.500, 0.000], [0.000, 0.667, 0.000], [0.000, 0.833, 0.000],
        [0.000, 1.000, 0.000], [0.000, 0.000, 0.167], [0.000, 0.000, 0.333],
        [0.000, 0.000, 0.500], [0.000, 0.000, 0.667], [0.000, 0.000, 0.833],
        [0.000, 0.000, 1.000], [0.000, 0.000, 0.000], [0.143, 0.143, 0.143],
        [0.286, 0.286, 0.286], [0.429, 0.429, 0.429], [0.571, 0.571, 0.571],
        [0.714, 0.714, 0.714], [0.857, 0.857, 0.857], [0.000, 0.447, 0.741],
        [0.314, 0.717, 0.741], [0.500, 0.500, 0.000],
    ];

    let idx = (category_id - 1) as usize % colors.len();
    let rgb = colors[idx];
    Color::from_rgb(rgb[0], rgb[1], rgb[2])
}

// Implement Widget trait
impl<Message, Theme, R> Widget<Message, Theme, R> for MaskShader<Message>
where
    R: primitive::Renderer,
{
    fn size(&self) -> Size<Length> {
        Size {
            width: self.width,
            height: self.height,
        }
    }

    fn layout(
        &self,
        _tree: &mut Tree,
        _renderer: &R,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::atomic(limits, self.width, self.height)
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut R,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();

        let primitive = MaskPrimitive {
            bounds,
            annotations: self.annotations.clone(),
            image_size: self.image_size,
            zoom_scale: self.zoom_scale,
            zoom_offset: self.zoom_offset,
        };

        renderer.draw_primitive(bounds, primitive);
    }
}

impl<'a, Message, Theme, R> From<MaskShader<Message>> for Element<'a, Message, Theme, R>
where
    Message: 'a,
    R: primitive::Renderer + 'a,
{
    fn from(shader: MaskShader<Message>) -> Self {
        Element::new(shader)
    }
}
