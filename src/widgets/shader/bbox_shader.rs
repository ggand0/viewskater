/// BBox shader widget for rendering COCO bounding boxes
///
/// Uses WGPU to draw colored rectangles with labels over images.

use std::marker::PhantomData;
use iced_core::{Color, Rectangle, Size, Length};
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
    _phantom: PhantomData<Message>,
}

impl<Message> BBoxShader<Message> {
    pub fn new(annotations: Vec<ImageAnnotation>, image_size: (u32, u32)) -> Self {
        Self {
            width: Length::Fill,
            height: Length::Fill,
            annotations,
            image_size,
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

        let width_ratio = display_width / image_width;
        let height_ratio = display_height / image_height;
        let scale = width_ratio.min(height_ratio);

        let scaled_width = image_width * scale;
        let scaled_height = image_height * scale;
        let offset_x = (display_width - scaled_width) / 2.0;
        let offset_y = (display_height - scaled_height) / 2.0;

        let mut buffers = Vec::new();

        for (idx, annotation) in self.annotations.iter().enumerate() {
            let color = get_category_color(idx);

            let x = (annotation.bbox.x * scale + offset_x + self.bounds.x) * scale_factor;
            let y = (annotation.bbox.y * scale + offset_y + self.bounds.y) * scale_factor;
            let width = annotation.bbox.width * scale * scale_factor;
            let height = annotation.bbox.height * scale * scale_factor;

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
        _clip_bounds: &Rectangle<u32>,
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

/// Get color for category by index
fn get_category_color(index: usize) -> Color {
    let colors = [
        Color::from_rgb(1.0, 0.0, 0.0),     // Red
        Color::from_rgb(0.0, 1.0, 0.0),     // Green
        Color::from_rgb(0.0, 0.0, 1.0),     // Blue
        Color::from_rgb(1.0, 1.0, 0.0),     // Yellow
        Color::from_rgb(1.0, 0.0, 1.0),     // Magenta
        Color::from_rgb(0.0, 1.0, 1.0),     // Cyan
        Color::from_rgb(1.0, 0.5, 0.0),     // Orange
        Color::from_rgb(0.5, 0.0, 1.0),     // Purple
        Color::from_rgb(0.0, 1.0, 0.5),     // Spring green
        Color::from_rgb(1.0, 0.0, 0.5),     // Rose
    ];
    colors[index % colors.len()]
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
