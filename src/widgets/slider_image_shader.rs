// Slider Image Shader Widget
// Renders images from atlas during slider navigation

#[allow(unused_imports)]
use log::{debug, info, warn, error};

use std::marker::PhantomData;
use std::sync::Arc;
use iced_core::ContentFit;
use iced_core::layout::Layout;
use iced_winit::core::{layout, mouse, renderer, widget, Element, Length, Rectangle, Size};
use iced_winit::core::widget::Tree;
use iced_widget::shader::{self, Viewport, Storage};
use iced_wgpu::{wgpu, primitive};
use iced_wgpu::engine::CompressionStrategy;
use iced_wgpu::wgpu::util::DeviceExt;

use crate::slider_atlas::{Atlas, AtlasPipeline, Entry};

/// Simplified widget for rendering images from atlas during slider movement
pub struct SliderImageShader<Message> {
    pane_idx: usize,
    image_idx: usize,
    image_bytes: Vec<u8>,  // RGBA8 image data
    image_size: (u32, u32),
    width: Length,
    height: Length,
    content_fit: ContentFit,
    _phantom: PhantomData<Message>,
}

impl<Message> SliderImageShader<Message> {
    /// Create a new SliderImageShader
    pub fn new(
        pane_idx: usize,
        image_idx: usize,
        image_bytes: Vec<u8>,
        image_size: (u32, u32),
    ) -> Self {
        Self {
            pane_idx,
            image_idx,
            image_bytes,
            image_size,
            width: Length::Fill,
            height: Length::Fill,
            content_fit: ContentFit::Contain,
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

    pub fn content_fit(mut self, content_fit: ContentFit) -> Self {
        self.content_fit = content_fit;
        self
    }
}

// Implement Widget trait
impl<Message, Theme, R> widget::Widget<Message, Theme, R> for SliderImageShader<Message>
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

        let primitive = SliderImagePrimitive {
            pane_idx: self.pane_idx,
            image_idx: self.image_idx,
            image_bytes: self.image_bytes.clone(),
            image_size: self.image_size,
            bounds,
            content_fit: self.content_fit,
        };

        renderer.draw_primitive(bounds, primitive);
    }
}

// Convert to Element
impl<'a, Message, Theme, R> From<SliderImageShader<Message>> for Element<'a, Message, Theme, R>
where
    Message: 'a,
    R: primitive::Renderer + 'a,
{
    fn from(shader: SliderImageShader<Message>) -> Self {
        Element::new(shader)
    }
}

// Primitive for rendering
#[derive(Debug)]
struct SliderImagePrimitive {
    pane_idx: usize,
    image_idx: usize,
    image_bytes: Vec<u8>,
    image_size: (u32, u32),
    bounds: Rectangle,
    content_fit: ContentFit,
}

impl shader::Primitive for SliderImagePrimitive {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        storage: &mut Storage,
        _bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        // Create atlas if not exists
        if !storage.has::<SliderAtlasState>() {
            debug!("Creating new SliderAtlasState");
            let state = SliderAtlasState::new(device, wgpu::Backend::Vulkan);  // TODO: Get actual backend
            storage.store(state);
        }

        // Create pipeline if not exists
        if !storage.has::<AtlasPipeline>() {
            debug!("Creating new AtlasPipeline");
            let pipeline = AtlasPipeline::new(device, format);
            storage.store(pipeline);
        }

        // Get mutable access to atlas state
        let state = storage.get_mut::<SliderAtlasState>().unwrap();
        
        // Upload image to atlas (or get cached entry)
        let key = AtlasKey {
            pane_idx: self.pane_idx,
            image_idx: self.image_idx,
        };

        // Check if already uploaded
        if !state.entries.contains_key(&key) {
            debug!("Uploading image to atlas: pane={}, image={}", self.pane_idx, self.image_idx);
            
            // Upload to atlas
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Slider Atlas Upload"),
            });
            
            if let Some(entry) = state.atlas.upload(
                device,
                &mut encoder,
                self.image_size.0,
                self.image_size.1,
                &self.image_bytes,
            ) {
                queue.submit(Some(encoder.finish()));
                state.entries.insert(key, entry);
                debug!("Successfully uploaded to atlas");
            } else {
                warn!("Failed to upload image to atlas");
            }
        } else {
            debug!("Atlas entry already exists: pane={}, image={}", self.pane_idx, self.image_idx);
        }

        // Calculate content bounds (ContentFit logic)
        let content_bounds = self.calculate_content_bounds(viewport);
        
        // Create vertex buffer for this render
        let viewport_size = viewport.physical_size();
        let (x, y, width, height) = (
            content_bounds.x / viewport_size.width as f32,
            content_bounds.y / viewport_size.height as f32,
            content_bounds.width / viewport_size.width as f32,
            content_bounds.height / viewport_size.height as f32,
        );
        
        let left = 2.0 * x - 1.0;
        let right = 2.0 * (x + width) - 1.0;
        let top = 1.0 - 2.0 * y;
        let bottom = 1.0 - 2.0 * (y + height);
        
        let vertices: [f32; 16] = [
            left, bottom, 0.0, 1.0,   // Bottom-left
            right, bottom, 1.0, 1.0,  // Bottom-right
            right, top, 1.0, 0.0,     // Top-right
            left, top, 0.0, 0.0,      // Top-left
        ];
        
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Slider Atlas Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        
        // Store prepared info
        storage.store(PreparedSliderImage {
            key,
            content_bounds,
            viewport: viewport.clone(),
            vertex_buffer,
        });
    }

    fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        storage: &Storage,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        let Some(pipeline) = storage.get::<AtlasPipeline>() else {
            warn!("AtlasPipeline not found in storage");
            return;
        };

        let Some(state) = storage.get::<SliderAtlasState>() else {
            warn!("SliderAtlasState not found in storage");
            return;
        };

        let Some(prepared) = storage.get::<PreparedSliderImage>() else {
            warn!("PreparedSliderImage not found in storage");
            return;
        };

        let Some(entry) = state.entries.get(&prepared.key) else {
            warn!("Atlas entry not found for key: {:?}", prepared.key);
            return;
        };

        // Render from atlas using the prepared vertex buffer
        let viewport_size = prepared.viewport.physical_size();
        pipeline.render(
            &prepared.vertex_buffer,
            encoder,
            &state.atlas,
            entry,
            target,
            prepared.content_bounds,
            (viewport_size.width, viewport_size.height),
            clip_bounds,
        );
    }
}

impl SliderImagePrimitive {
    fn calculate_content_bounds(&self, viewport: &Viewport) -> Rectangle {
        let scale_factor = viewport.scale_factor() as f32;
        
        // Image size
        let image_size = Size::new(self.image_size.0 as f32, self.image_size.1 as f32);
        
        // Available bounds
        let bounds_size = self.bounds.size();
        
        // Calculate fitted size based on ContentFit
        let (width, height) = match self.content_fit {
            ContentFit::Contain => {
                let ratio = (bounds_size.width / image_size.width)
                    .min(bounds_size.height / image_size.height);
                (image_size.width * ratio, image_size.height * ratio)
            }
            ContentFit::Fill => (bounds_size.width, bounds_size.height),
            ContentFit::Cover => {
                let ratio = (bounds_size.width / image_size.width)
                    .max(bounds_size.height / image_size.height);
                (image_size.width * ratio, image_size.height * ratio)
            }
            ContentFit::ScaleDown => {
                let ratio = (bounds_size.width / image_size.width)
                    .min(bounds_size.height / image_size.height)
                    .min(1.0);
                (image_size.width * ratio, image_size.height * ratio)
            }
            ContentFit::None => (image_size.width, image_size.height),
        };
        
        // Center the image
        let x = self.bounds.x + (bounds_size.width - width) / 2.0;
        let y = self.bounds.y + (bounds_size.height - height) / 2.0;
        
        // Apply scale factor for physical coordinates
        Rectangle {
            x: x * scale_factor,
            y: y * scale_factor,
            width: width * scale_factor,
            height: height * scale_factor,
        }
    }
}

// Key for identifying atlas entries
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct AtlasKey {
    pane_idx: usize,
    image_idx: usize,
}

// Atlas state stored in shader Storage
#[derive(Debug)]
struct SliderAtlasState {
    atlas: Atlas,
    entries: std::collections::HashMap<AtlasKey, Entry>,
}

impl SliderAtlasState {
    fn new(device: &wgpu::Device, backend: wgpu::Backend) -> Self {
        let bind_group_layout = Arc::new(device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("slider_atlas_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                },
                count: None,
            }],
        }));
        
        let atlas = Atlas::new(
            device,
            backend,
            bind_group_layout,
            CompressionStrategy::Bc1,  // Use BC1 by default
        );
        
        Self {
            atlas,
            entries: std::collections::HashMap::new(),
        }
    }
}

// Prepared rendering info
struct PreparedSliderImage {
    key: AtlasKey,
    content_bounds: Rectangle,
    viewport: Viewport,
    vertex_buffer: wgpu::Buffer,
}

impl std::fmt::Debug for PreparedSliderImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedSliderImage")
            .field("key", &self.key)
            .field("content_bounds", &self.content_bounds)
            .field("viewport", &self.viewport)
            .field("vertex_buffer", &"wgpu::Buffer")
            .finish()
    }
}

