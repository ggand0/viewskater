/// BBox shader widget for rendering COCO bounding boxes
///
/// Uses WGPU to draw colored rectangles with labels over images.

use std::marker::PhantomData;
use iced_core::{Color, Rectangle, Size, Length, Vector};
use iced_core::layout::{self, Layout};
use iced_core::mouse;
use iced_core::renderer;
use iced_core::widget::tree::{self, Tree};
use iced_winit::core::{self, Element, Shell, Widget};
use iced_widget::shader::{self, Viewport, Storage};
use iced_wgpu::{wgpu, primitive};
use wgpu::util::DeviceExt;
use crate::coco_parser::ImageAnnotation;

/// A shader widget for rendering bounding boxes
pub struct BBoxShader<Message> {
    width: Length,
    height: Length,
    annotations: Vec<ImageAnnotation>,
    image_size: (u32, u32),
    zoom_scale: f32,
    zoom_offset: Vector,
    _phantom: PhantomData<Message>,
}

impl<Message> BBoxShader<Message> {
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

    /// Calculate scaling from image coordinates to display coordinates
    fn calculate_scale(&self, bounds: Rectangle) -> (f32, f32, f32, f32) {
        let image_width = self.image_size.0 as f32;
        let image_height = self.image_size.1 as f32;
        let display_width = bounds.width;
        let display_height = bounds.height;

        // ContentFit::Contain scaling
        let width_ratio = display_width / image_width;
        let height_ratio = display_height / image_height;
        let scale = width_ratio.min(height_ratio);

        let scaled_width = image_width * scale;
        let scaled_height = image_height * scale;

        // Center the image
        let offset_x = (display_width - scaled_width) / 2.0;
        let offset_y = (display_height - scaled_height) / 2.0;

        (scale, scale, offset_x, offset_y)
    }
}

/// Primitive for bbox rendering
#[derive(Debug)]
pub struct BBoxPrimitive {
    bounds: Rectangle,
    annotations: Vec<ImageAnnotation>,
    image_size: (u32, u32),
    zoom_scale: f32,
    zoom_offset: Vector,
}

// Cache for vertex buffers created in prepare()
struct BBoxBufferCache {
    buffers: Vec<wgpu::Buffer>,
}

impl shader::Primitive for BBoxPrimitive {
    fn prepare(
        &self,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        storage: &mut Storage,
        _bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        // Store viewport for use in render
        storage.store(viewport.clone());

        // Create pipeline if needed
        if !storage.has::<BBoxPipeline>() {
            let pipeline = BBoxPipeline::new(device, format);
            storage.store(pipeline);
        }

        // Pre-create all vertex buffers for bboxes
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

        // Calculate zoomed image dimensions (changes with zoom)
        let zoomed_image_width = image_width * base_scale * self.zoom_scale;
        let zoomed_image_height = image_height * base_scale * self.zoom_scale;

        // Centering offset after zoom (changes as image grows/shrinks)
        let center_offset_x = (display_width - zoomed_image_width) / 2.0;
        let center_offset_y = (display_height - zoomed_image_height) / 2.0;

        let mut buffers = Vec::new();

        for annotation in self.annotations.iter() {
            let color = get_category_color(annotation.category_id);

            // Scale bbox coordinates by base_scale and zoom_scale
            let scaled_bbox_x = annotation.bbox.x * base_scale * self.zoom_scale;
            let scaled_bbox_y = annotation.bbox.y * base_scale * self.zoom_scale;

            // Apply centering offset and pan offset (subtract offset like ImageShader does)
            let x = (scaled_bbox_x + center_offset_x - self.zoom_offset.x + self.bounds.x) * scale_factor;
            let y = (scaled_bbox_y + center_offset_y - self.zoom_offset.y + self.bounds.y) * scale_factor;
            let width = annotation.bbox.width * base_scale * self.zoom_scale * scale_factor;
            let height = annotation.bbox.height * base_scale * self.zoom_scale * scale_factor;

            // Create 5 vertices for rectangle outline in NDC
            // Note: Invert y-axis because NDC has y=-1 at top, y=1 at bottom (opposite of screen coords)
            let vertices = [
                BBoxVertex {
                    position: [
                        (x / viewport_size.width as f32) * 2.0 - 1.0,
                        1.0 - (y / viewport_size.height as f32) * 2.0,  // Inverted
                    ],
                    color: [color.r, color.g, color.b, color.a],
                },
                BBoxVertex {
                    position: [
                        ((x + width) / viewport_size.width as f32) * 2.0 - 1.0,
                        1.0 - (y / viewport_size.height as f32) * 2.0,  // Inverted
                    ],
                    color: [color.r, color.g, color.b, color.a],
                },
                BBoxVertex {
                    position: [
                        ((x + width) / viewport_size.width as f32) * 2.0 - 1.0,
                        1.0 - ((y + height) / viewport_size.height as f32) * 2.0,  // Inverted
                    ],
                    color: [color.r, color.g, color.b, color.a],
                },
                BBoxVertex {
                    position: [
                        (x / viewport_size.width as f32) * 2.0 - 1.0,
                        1.0 - ((y + height) / viewport_size.height as f32) * 2.0,  // Inverted
                    ],
                    color: [color.r, color.g, color.b, color.a],
                },
                BBoxVertex {
                    position: [
                        (x / viewport_size.width as f32) * 2.0 - 1.0,
                        1.0 - (y / viewport_size.height as f32) * 2.0,  // Inverted
                    ],
                    color: [color.r, color.g, color.b, color.a],
                },
            ];

            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("BBox Vertex Buffer"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

            buffers.push(buffer);
        }

        storage.store(BBoxBufferCache { buffers });
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

        if let Some(pipeline) = storage.get::<BBoxPipeline>() {
            if let Some(cache) = storage.get::<BBoxBufferCache>() {
                for buffer in &cache.buffers {
                    let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("BBox Render Pass"),
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

                    // Set scissor rectangle to clip rendering to bounds
                    render_pass.set_scissor_rect(
                        clip_bounds.x,
                        clip_bounds.y,
                        clip_bounds.width,
                        clip_bounds.height,
                    );

                    render_pass.set_pipeline(&pipeline.render_pipeline);
                    render_pass.set_vertex_buffer(0, buffer.slice(..));
                    render_pass.draw(0..5, 0..1);
                }
            }
        }
    }
}

/// Vertex data for bbox rendering
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct BBoxVertex {
    position: [f32; 2],
    color: [f32; 4],
}

impl BBoxVertex {
    const ATTRIBS: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4];

    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<BBoxVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

/// Simple WGPU pipeline for drawing rectangles
#[derive(Debug)]
struct BBoxPipeline {
    render_pipeline: wgpu::RenderPipeline,
}

impl BBoxPipeline {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("BBox Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("bbox_shader.wgsl").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("BBox Pipeline Layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("BBox Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[BBoxVertex::desc()],
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
                topology: wgpu::PrimitiveTopology::LineStrip,
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

        Self { render_pipeline }
    }
}

/// Get color for category using YOLO/YOLOX color scheme
/// Based on https://github.com/Megvii-BaseDetection/YOLOX/blob/main/yolox/utils/visualize.py
fn get_category_color(category_id: u64) -> Color {
    // YOLO color palette for 80 COCO classes
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

    let idx = (category_id - 1) as usize % colors.len(); // COCO category_id starts at 1
    let rgb = colors[idx];
    Color::from_rgb(rgb[0], rgb[1], rgb[2])
}

// Implement Widget trait
impl<Message, Theme, R> Widget<Message, Theme, R> for BBoxShader<Message>
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

        if !self.annotations.is_empty() {
            let primitive = BBoxPrimitive {
                bounds,
                annotations: self.annotations.clone(),
                image_size: self.image_size,
                zoom_scale: self.zoom_scale,
                zoom_offset: self.zoom_offset,
            };

            renderer.draw_primitive(bounds, primitive);
        }
    }
}

impl<'a, Message, Theme, R> From<BBoxShader<Message>> for Element<'a, Message, Theme, R>
where
    Message: 'a,
    R: primitive::Renderer + 'a,
{
    fn from(shader: BBoxShader<Message>) -> Self {
        Element::new(shader)
    }
}
