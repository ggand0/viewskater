/// Polygon mask shader widget for rendering COCO segmentation masks
///
/// Uses WGPU to draw filled polygons with proper triangulation.

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
use crate::coco_parser::{ImageAnnotation, CocoSegmentation};

/// A shader widget for rendering segmentation masks
pub struct PolygonShader<Message> {
    width: Length,
    height: Length,
    annotations: Vec<ImageAnnotation>,
    image_size: (u32, u32),
    zoom_scale: f32,
    zoom_offset: Vector,
    _phantom: PhantomData<Message>,
}

impl<Message> PolygonShader<Message> {
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

/// Primitive for polygon rendering
#[derive(Debug)]
pub struct PolygonPrimitive {
    bounds: Rectangle,
    annotations: Vec<ImageAnnotation>,
    image_size: (u32, u32),
    zoom_scale: f32,
    zoom_offset: Vector,
}

// Cache for vertex buffers created in prepare()
struct PolygonBufferCache {
    buffers: Vec<(wgpu::Buffer, u32)>, // (buffer, vertex_count)
}

impl shader::Primitive for PolygonPrimitive {
    fn prepare(
        &self,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        storage: &mut Storage,
        _bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        storage.store(viewport.clone());

        // Create pipeline if needed
        if !storage.has::<PolygonPipeline>() {
            let pipeline = PolygonPipeline::new(device, format);
            storage.store(pipeline);
        }

        // Pre-create all vertex buffers for polygons
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

        let mut buffers = Vec::new();

        for annotation in self.annotations.iter() {
            if let Some(ref segmentation) = annotation.segmentation {
                let color = get_category_color(annotation.category_id);
                let mask_color = Color::from_rgba(color.r, color.g, color.b, 0.4); // 40% opacity

                match segmentation {
                    CocoSegmentation::Polygon(polygons) => {
                        for polygon in polygons {
                            if polygon.len() < 6 {
                                continue; // Need at least 3 points
                            }

                            // Transform polygon vertices to screen coordinates
                            let mut screen_points = Vec::new();
                            for i in (0..polygon.len()).step_by(2) {
                                if i + 1 >= polygon.len() {
                                    break;
                                }

                                let x = polygon[i];
                                let y = polygon[i + 1];

                                // Apply same transformation as bboxes
                                let scaled_x = x * base_scale * self.zoom_scale;
                                let scaled_y = y * base_scale * self.zoom_scale;

                                let screen_x = (scaled_x + center_offset_x - self.zoom_offset.x + self.bounds.x) * scale_factor;
                                let screen_y = (scaled_y + center_offset_y - self.zoom_offset.y + self.bounds.y) * scale_factor;

                                screen_points.push((screen_x, screen_y));
                            }

                            // Triangulate polygon using ear clipping
                            if screen_points.len() >= 3 {
                                // Convert to flat array for earcutr
                                let mut coords: Vec<f64> = Vec::with_capacity(screen_points.len() * 2);
                                for (x, y) in &screen_points {
                                    coords.push(*x as f64);
                                    coords.push(*y as f64);
                                }

                                // Perform ear clipping triangulation
                                let triangles = earcutr::earcut(&coords, &[], 2);

                                if let Ok(indices) = triangles {
                                    let mut vertices = Vec::new();

                                    // Create vertices from triangulated indices
                                    for idx in indices.chunks(3) {
                                        if idx.len() == 3 {
                                            let p0 = screen_points[idx[0]];
                                            let p1 = screen_points[idx[1]];
                                            let p2 = screen_points[idx[2]];

                                            // Convert to NDC
                                            let ndc0 = self.to_ndc(p0, viewport_size);
                                            let ndc1 = self.to_ndc(p1, viewport_size);
                                            let ndc2 = self.to_ndc(p2, viewport_size);

                                            vertices.push(PolygonVertex {
                                                position: ndc0,
                                                color: [mask_color.r, mask_color.g, mask_color.b, mask_color.a],
                                            });
                                            vertices.push(PolygonVertex {
                                                position: ndc1,
                                                color: [mask_color.r, mask_color.g, mask_color.b, mask_color.a],
                                            });
                                            vertices.push(PolygonVertex {
                                                position: ndc2,
                                                color: [mask_color.r, mask_color.g, mask_color.b, mask_color.a],
                                            });
                                        }
                                    }

                                    if !vertices.is_empty() {
                                        let vertex_count = vertices.len() as u32;
                                        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                            label: Some("Polygon Vertex Buffer"),
                                            contents: bytemuck::cast_slice(&vertices),
                                            usage: wgpu::BufferUsages::VERTEX,
                                        });

                                        buffers.push((buffer, vertex_count));
                                    }
                                }
                            }
                        }
                    }
                    CocoSegmentation::RLE(_rle) => {
                        // RLE not yet implemented
                    }
                }
            }
        }

        storage.store(PolygonBufferCache { buffers });
    }

    fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        storage: &Storage,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        if let Some(pipeline) = storage.get::<PolygonPipeline>() {
            if let Some(cache) = storage.get::<PolygonBufferCache>() {
                for (buffer, vertex_count) in &cache.buffers {
                    let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Polygon Render Pass"),
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
                    render_pass.draw(0..*vertex_count, 0..1);
                }
            }
        }
    }
}

impl PolygonPrimitive {
    fn to_ndc(&self, point: (f32, f32), viewport_size: iced_core::Size<u32>) -> [f32; 2] {
        [
            (point.0 / viewport_size.width as f32) * 2.0 - 1.0,
            1.0 - (point.1 / viewport_size.height as f32) * 2.0, // Inverted Y
        ]
    }
}

/// Vertex data for polygon rendering
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct PolygonVertex {
    position: [f32; 2],
    color: [f32; 4],
}

impl PolygonVertex {
    const ATTRIBS: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4];

    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<PolygonVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

/// WGPU pipeline for drawing filled polygons
#[derive(Debug)]
struct PolygonPipeline {
    render_pipeline: wgpu::RenderPipeline,
}

impl PolygonPipeline {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Polygon Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("polygon_shader.wgsl").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Polygon Pipeline Layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Polygon Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[PolygonVertex::desc()],
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

        Self { render_pipeline }
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
impl<Message, Theme, R> Widget<Message, Theme, R> for PolygonShader<Message>
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

        let primitive = PolygonPrimitive {
            bounds,
            annotations: self.annotations.clone(),
            image_size: self.image_size,
            zoom_scale: self.zoom_scale,
            zoom_offset: self.zoom_offset,
        };

        renderer.draw_primitive(bounds, primitive);
    }
}

impl<'a, Message, Theme, R> From<PolygonShader<Message>> for Element<'a, Message, Theme, R>
where
    Message: 'a,
    R: primitive::Renderer + 'a,
{
    fn from(shader: PolygonShader<Message>) -> Self {
        Element::new(shader)
    }
}
