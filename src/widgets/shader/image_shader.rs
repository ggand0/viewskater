use iced_winit::core::{self, layout, mouse, renderer, widget::{self, tree::{self, Tree}}, Element, Length, Rectangle, Shell, Size};
use iced_wgpu::{wgpu, primitive};
use std::marker::PhantomData;
use std::sync::Arc;
use iced_widget::shader::{self, Viewport, Storage};
use iced_core::ContentFit;
use iced_core::{Vector, Point};
use iced_core::layout::Layout;
use iced_core::{self, event::Event};
use iced_core::clipboard::Clipboard;
use iced_core::event;


#[allow(unused_imports)]
use log::{Level, debug, info, warn, error};

use crate::cache::img_cache::CachedData;
use crate::widgets::shader::texture_pipeline::TexturePipeline;
use crate::Scene;  // Import the Scene type

/// A specialized shader widget for displaying images with proper aspect ratio.
pub struct ImageShader<Message> {
    width: Length,
    height: Length,
    scene: Option<Scene>,
    content_fit: ContentFit,
    min_scale: f32,
    max_scale: f32,
    scale_step: f32,
    _phantom: PhantomData<Message>,
}

impl<Message> ImageShader<Message> {
    /// Create a new ImageShader widget that works with Scene
    pub fn new(scene: Option<&Scene>) -> Self {
        // Clone the Scene if it exists
        let scene_clone = scene.cloned();
        
        // Add debug output to track scene creation
        if scene.is_some() {
            debug!("ImageShader::new - Created with a scene");
            if let Some(ref s) = scene {
                if s.get_texture().is_some() {
                    debug!("ImageShader::new - Scene has a texture");
                } else {
                    debug!("ImageShader::new - Scene has NO texture!");
                }
            }
        } else {
            debug!("ImageShader::new - Created with NO scene");
        }
        
        Self {
            width: Length::Fill,
            height: Length::Fill,
            scene: scene_clone,
            content_fit: ContentFit::Contain,
            min_scale: 0.25,
            max_scale: 10.0,
            scale_step: 0.10,
            _phantom: PhantomData,
        }
    }
    
    /// Set the width of the widget
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }
    
    /// Set the height of the widget
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }
    
    /// Set how the image should fit within the widget bounds
    pub fn content_fit(mut self, content_fit: ContentFit) -> Self {
        self.content_fit = content_fit;
        self
    }
    
    /// Update the scene
    pub fn update_scene(&mut self, new_scene: Scene) {
        debug!("ImageShader::update_scene - Updating scene");
        self.scene = Some(new_scene);
    }
    
    /// Sets the max scale applied to the image.
    ///
    /// Default is `10.0`
    pub fn max_scale(mut self, max_scale: f32) -> Self {
        self.max_scale = max_scale;
        self
    }

    /// Sets the min scale applied to the image.
    ///
    /// Default is `0.25`
    pub fn min_scale(mut self, min_scale: f32) -> Self {
        self.min_scale = min_scale;
        self
    }

    /// Sets the percentage the image will be scaled by when zoomed in / out.
    ///
    /// Default is `0.10`
    pub fn scale_step(mut self, scale_step: f32) -> Self {
        self.scale_step = scale_step;
        self
    }
    
    /// Calculate the layout bounds that preserve aspect ratio
    fn calculate_layout(&self, bounds: Rectangle) -> Rectangle {
        if let Some(ref scene) = self.scene {
            if let Some(texture) = scene.get_texture() {
                debug!("ImageShader::calculate_layout - Got texture {}x{}", texture.width(), texture.height());
                
                let texture_size = Size::new(texture.width() as f32, texture.height() as f32);
                let bounds_size = bounds.size();
                
                // Calculate image size based on content fit
                let (width, height) = match self.content_fit {
                    ContentFit::Fill => (bounds_size.width, bounds_size.height),
                    ContentFit::Contain => {
                        let width_ratio = bounds_size.width / texture_size.width;
                        let height_ratio = bounds_size.height / texture_size.height;
                        let ratio = width_ratio.min(height_ratio);
                        
                        (texture_size.width * ratio, texture_size.height * ratio)
                    },
                    ContentFit::Cover => {
                        let width_ratio = bounds_size.width / texture_size.width;
                        let height_ratio = bounds_size.height / texture_size.height;
                        let ratio = width_ratio.max(height_ratio);
                        
                        (texture_size.width * ratio, texture_size.height * ratio)
                    },
                    ContentFit::ScaleDown => {
                        let width_ratio = bounds_size.width / texture_size.width;
                        let height_ratio = bounds_size.height / texture_size.height;
                        let ratio = width_ratio.min(height_ratio).min(1.0);
                        
                        (texture_size.width * ratio, texture_size.height * ratio)
                    },
                    ContentFit::None => (texture_size.width, texture_size.height),
                };
                
                // Calculate image position to center it
                let diff_w = bounds_size.width - width;
                let diff_h = bounds_size.height - height;
                
                let x = bounds.x + diff_w / 2.0;
                let y = bounds.y + diff_h / 2.0;
                
                // NEW: Apply 1px padding on all sides to avoid border overlap
                let padding = 1.0;
                let padded_rect = Rectangle {
                    x: x + padding,
                    y: y + padding,
                    width: width - 2.0 * padding,
                    height: height - 2.0 * padding,
                };
                
                debug!("ImageShader::calculate_layout - Calculated content bounds: ({}, {}, {}, {})", 
                       padded_rect.x, padded_rect.y, padded_rect.width, padded_rect.height);
                
                return padded_rect;
            } else {
                debug!("ImageShader::calculate_layout - Scene has NO texture!");
            }
        } else {
            debug!("ImageShader::calculate_layout - No scene available");
        }
        
        // Fallback to original bounds if no texture
        bounds
    }
}

// Expanded ImageShaderState to track zoom and pan
#[derive(Debug, Clone, Copy, Default)]
pub struct ImageShaderState {
    scale: f32,
    starting_offset: Vector,
    current_offset: Vector,
    cursor_grabbed_at: Option<Point>,
}

impl ImageShaderState {
    pub fn new() -> Self {
        Self {
            scale: 1.0,
            starting_offset: Vector::default(),
            current_offset: Vector::default(),
            cursor_grabbed_at: None,
        }
    }
    
    /// Returns if the cursor is currently grabbed
    pub fn is_cursor_grabbed(&self) -> bool {
        self.cursor_grabbed_at.is_some()
    }
    
    /// Returns the current offset, clamped to prevent image from going too far off-screen
    fn offset(&self, bounds: Rectangle, image_size: Size) -> Vector {
        let hidden_width = (image_size.width - bounds.width / 2.0).max(0.0).round();
        let hidden_height = (image_size.height - bounds.height / 2.0).max(0.0).round();

        Vector::new(
            self.current_offset.x.clamp(-hidden_width, hidden_width),
            self.current_offset.y.clamp(-hidden_height, hidden_height),
        )
    }
}

// This is our specialized primitive for image rendering
#[derive(Debug)]
pub struct ImagePrimitive {
    scene: Scene,
    bounds: Rectangle,
    content_bounds: Rectangle,
    scale: f32,
    offset: Vector,
}

impl shader::Primitive for ImagePrimitive {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue, 
        format: wgpu::TextureFormat,
        storage: &mut Storage,
        bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        // Make sure the viewport is stored in storage for later use in render
        storage.store(viewport.clone());
        
        let scale_factor = viewport.scale_factor() as f32;
        let viewport_size = viewport.physical_size();
        
        debug!("ImagePrimitive::prepare - Starting prepare");
        debug!("ImagePrimitive::prepare - Content bounds: {:?}", self.content_bounds);
        debug!("ImagePrimitive::prepare - Viewport: {:?}, scale: {}", viewport_size, scale_factor);
        
        // Get texture from scene
        if let Some(texture) = self.scene.get_texture() {
            debug!("ImagePrimitive::prepare - Got texture {}x{}", texture.width(), texture.height());
            
            let texture_size = (texture.width(), texture.height());
            
            // Calculate normalized device coordinates for viewport
            let x_rel = self.content_bounds.x * scale_factor / viewport_size.width as f32;
            let y_rel = self.content_bounds.y * scale_factor / viewport_size.height as f32;
            let width_rel = self.content_bounds.width * scale_factor / viewport_size.width as f32;
            let height_rel = self.content_bounds.height * scale_factor / viewport_size.height as f32;
            
            let bounds_relative = (x_rel, y_rel, width_rel, height_rel);
            
            debug!("ImagePrimitive::prepare - Relative bounds: {:?}", bounds_relative);
            
            // Create a unique pipeline key based on these bounds
            let pipeline_key = format!("img_pipeline_{:.4}_{:.4}_{:.4}_{:.4}", 
                                      bounds_relative.0, bounds_relative.1,
                                      bounds_relative.2, bounds_relative.3);
            
            // Ensure we have a registry to store pipelines
            if !storage.has::<PipelineRegistry>() {
                debug!("ImagePrimitive::prepare - Creating new PipelineRegistry");
                storage.store(PipelineRegistry::default());
            }
            
            let registry = storage.get_mut::<PipelineRegistry>().unwrap();
            
            // Create pipeline if it doesn't exist or reuse existing one
            if !registry.pipelines.contains_key(&pipeline_key) {
                debug!("ImagePrimitive::prepare - Creating new pipeline for key {}", pipeline_key);
                
                let pipeline = TexturePipeline::new(
                    device,
                    queue,
                    format,
                    Arc::clone(texture),
                    (viewport_size.width, viewport_size.height),
                    texture_size,
                    bounds_relative,
                );
                
                registry.pipelines.insert(pipeline_key.clone(), pipeline);
                debug!("ImagePrimitive::prepare - Pipeline created and stored");
            } else {
                debug!("ImagePrimitive::prepare - Reusing existing pipeline for key {}", pipeline_key);
                
                // Update the texture in the existing pipeline
                if let Some(pipeline) = registry.pipelines.get_mut(&pipeline_key) {
                    debug!("ImagePrimitive::prepare - Updating texture in existing pipeline");
                    pipeline.update_texture(device, queue, Arc::clone(texture));
                }
            }
        } else {
            debug!("ImagePrimitive::prepare - Scene has NO texture!");
        }
    }
    
    fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        storage: &Storage,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        debug!("ImagePrimitive::render - Starting render");
        
        // Get texture from scene
        if let Some(texture) = self.scene.get_texture() {
            debug!("ImagePrimitive::render - Got texture {}x{}", texture.width(), texture.height());
            
            // Access the pipeline registry
            if let Some(registry) = storage.get::<PipelineRegistry>() {
                // Store the viewport in prepare and retrieve it here
                if let Some(viewport) = storage.get::<Viewport>() {
                    // Same code as before to calculate the key
                    let scale_factor = viewport.scale_factor() as f32;
                    let viewport_size = viewport.physical_size();
                    
                    let x_rel = self.content_bounds.x * scale_factor / viewport_size.width as f32;
                    let y_rel = self.content_bounds.y * scale_factor / viewport_size.height as f32;
                    let width_rel = self.content_bounds.width * scale_factor / viewport_size.width as f32;
                    let height_rel = self.content_bounds.height * scale_factor / viewport_size.height as f32;
                    
                    let bounds_relative = (x_rel, y_rel, width_rel, height_rel);
                    
                    let pipeline_key = format!("img_pipeline_{:.4}_{:.4}_{:.4}_{:.4}", 
                                            bounds_relative.0, bounds_relative.1,
                                            bounds_relative.2, bounds_relative.3);
                    
                    debug!("ImagePrimitive::render - Looking for pipeline with key: {}", pipeline_key);
                    
                    if let Some(pipeline) = registry.pipelines.get(&pipeline_key) {
                        debug!("ImagePrimitive::render - Found pipeline, rendering");
                        pipeline.render(target, encoder, clip_bounds);
                    } else {
                        debug!("ImagePrimitive::render - Pipeline NOT found for key: {}", pipeline_key);
                    }
                } else {
                    // NEW CODE: Fall back to iterating over all pipelines
                    debug!("ImagePrimitive::render - No Viewport found in storage, trying all pipelines");
                    
                    // Find any pipeline that might be related to our texture and use it
                    let mut rendered = false;
                    for (key, pipeline) in &registry.pipelines {
                        debug!("ImagePrimitive::render - Trying pipeline with key: {}", key);
                        pipeline.render(target, encoder, clip_bounds);
                        rendered = true;
                        debug!("ImagePrimitive::render - Successfully rendered with pipeline: {}", key);
                        break;  // Just use the first one we find
                    }
                    
                    if !rendered {
                        debug!("ImagePrimitive::render - No pipelines found in registry");
                    }
                }
            } else {
                debug!("ImagePrimitive::render - No PipelineRegistry found in storage");
            }
        } else {
            debug!("ImagePrimitive::render - Scene has NO texture!");
        }
    }
}

// Registry to store pipelines
#[derive(Debug, Default)]
pub struct PipelineRegistry {
    pipelines: std::collections::HashMap<String, TexturePipeline>,
}

// Implement Widget for our ImageShader
impl<Message, Theme, Renderer> widget::Widget<Message, Theme, Renderer>
for ImageShader<Message>
where
    Renderer: primitive::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<ImageShaderState>()
    }
    
    fn state(&self) -> tree::State {
        tree::State::new(ImageShaderState::new())
    }
    
    fn size(&self) -> Size<Length> {
        Size {
            width: self.width,
            height: self.height,
        }
    }
    
    fn layout(
        &self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::atomic(limits, self.width, self.height)
    }
    
    fn on_event(
        &mut self,
        tree: &mut Tree,
        event: core::Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        _shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) -> event::Status {
        let bounds = layout.bounds();
        
        match event {
            core::Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let Some(cursor_position) = cursor.position_over(bounds) else {
                    return event::Status::Ignored;
                };
                
                match delta {
                    mouse::ScrollDelta::Lines { y, .. }
                    | mouse::ScrollDelta::Pixels { y, .. } => {
                        let state = tree.state.downcast_mut::<ImageShaderState>();
                        let previous_scale = state.scale;
                        
                        if y < 0.0 && previous_scale > self.min_scale
                            || y > 0.0 && previous_scale < self.max_scale
                        {
                            state.scale = (if y > 0.0 {
                                state.scale * (1.0 + self.scale_step)
                            } else {
                                state.scale / (1.0 + self.scale_step)
                            })
                            .clamp(self.min_scale, self.max_scale);
                            
                            debug!("ImageShader::on_event - New scale: {}", state.scale);
                            
                            // Calculate the scaled size
                            let scaled_size = self.calculate_scaled_size(bounds.size(), state.scale);
                            
                            let factor = state.scale / previous_scale - 1.0;
                            
                            let cursor_to_center = cursor_position - bounds.center();
                            
                            let adjustment = cursor_to_center * factor
                                + state.current_offset * factor;
                            
                            state.current_offset = Vector::new(
                                if scaled_size.width > bounds.width {
                                    state.current_offset.x + adjustment.x
                                } else {
                                    0.0
                                },
                                if scaled_size.height > bounds.height {
                                    state.current_offset.y + adjustment.y
                                } else {
                                    0.0
                                },
                            );
                            
                            debug!("ImageShader::on_event - New offset: {:?}", state.current_offset);
                        }
                    }
                }
                
                event::Status::Captured
            }
            core::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(cursor_position) = cursor.position_over(bounds) else {
                    return event::Status::Ignored;
                };
                
                let state = tree.state.downcast_mut::<ImageShaderState>();
                
                state.cursor_grabbed_at = Some(cursor_position);
                state.starting_offset = state.current_offset;
                
                debug!("ImageShader::on_event - Mouse grabbed at: {:?}", cursor_position);
                
                event::Status::Captured
            }
            core::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let state = tree.state.downcast_mut::<ImageShaderState>();
                
                if state.cursor_grabbed_at.is_some() {
                    state.cursor_grabbed_at = None;
                    debug!("ImageShader::on_event - Mouse released");
                    
                    event::Status::Captured
                } else {
                    event::Status::Ignored
                }
            }
            core::Event::Mouse(mouse::Event::CursorMoved { position }) => {
                let state = tree.state.downcast_mut::<ImageShaderState>();
                
                if let Some(origin) = state.cursor_grabbed_at {
                    let scaled_size = self.calculate_scaled_size(bounds.size(), state.scale);
                    
                    let hidden_width = (scaled_size.width - bounds.width / 2.0)
                        .max(0.0)
                        .round();
                    
                    let hidden_height = (scaled_size.height - bounds.height / 2.0)
                        .max(0.0)
                        .round();
                    
                    let delta = position - origin;
                    
                    let x = if bounds.width < scaled_size.width {
                        (state.starting_offset.x - delta.x)
                            .clamp(-hidden_width, hidden_width)
                    } else {
                        0.0
                    };
                    
                    let y = if bounds.height < scaled_size.height {
                        (state.starting_offset.y - delta.y)
                            .clamp(-hidden_height, hidden_height)
                    } else {
                        0.0
                    };
                    
                    state.current_offset = Vector::new(x, y);
                    debug!("ImageShader::on_event - Panning, new offset: {:?}", state.current_offset);
                    
                    event::Status::Captured
                } else {
                    event::Status::Ignored
                }
            }
            _ => event::Status::Ignored,
        }
    }
    
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<ImageShaderState>();
        let bounds = layout.bounds();
        let is_mouse_over = cursor.is_over(bounds);
        
        if state.is_cursor_grabbed() {
            mouse::Interaction::Grabbing
        } else if is_mouse_over {
            mouse::Interaction::Grab
        } else {
            mouse::Interaction::None
        }
    }
    
    fn draw(
        &self,
        tree: &widget::Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: layout::Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        debug!("ImageShader::draw - Drawing widget");
        
        if let Some(scene) = &self.scene {
            debug!("ImageShader::draw - Scene available");
            
            let bounds = layout.bounds();
            debug!("ImageShader::draw - Layout bounds: {:?}", bounds);
            
            let state = tree.state.downcast_ref::<ImageShaderState>();
            
            // Calculate scaled content bounds with proper aspect ratio
            let scaled_size = self.calculate_scaled_size(bounds.size(), state.scale);
            
            // Apply offset
            let offset = state.offset(bounds, scaled_size);
            
            // Apply content fit with scaling
            let content_bounds = self.calculate_content_bounds(bounds, scaled_size, offset);
            
            debug!("ImageShader::draw - Content bounds: {:?}", content_bounds);
            
            if scene.get_texture().is_some() {
                debug!("ImageShader::draw - Scene has texture, creating primitive");
                
                let primitive = ImagePrimitive {
                    scene: scene.clone(),
                    bounds,
                    content_bounds,
                    scale: state.scale,
                    offset,
                };
                
                debug!("ImageShader::draw - Calling renderer.draw_primitive");
                renderer.draw_primitive(bounds, primitive);
            } else {
                debug!("ImageShader::draw - Scene has NO texture! Skipping primitive creation");
            }
        } else {
            debug!("ImageShader::draw - No scene available, nothing to draw");
        }
    }
}

impl<'a, Message, Theme, Renderer> From<ImageShader<Message>>
for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Renderer: primitive::Renderer + 'a,
{
    fn from(shader: ImageShader<Message>) -> Self {
        Element::new(shader)
    }
}

impl<Message> ImageShader<Message> {
    // Helper method to calculate scaled size based on content fit
    fn calculate_scaled_size(&self, bounds_size: Size, scale: f32) -> Size {
        if let Some(ref scene) = self.scene {
            if let Some(texture) = scene.get_texture() {
                let texture_size = Size::new(texture.width() as f32, texture.height() as f32);
                
                // Calculate base size according to content fit
                let base_size = match self.content_fit {
                    ContentFit::Fill => bounds_size,
                    ContentFit::Contain => {
                        let width_ratio = bounds_size.width / texture_size.width;
                        let height_ratio = bounds_size.height / texture_size.height;
                        let ratio = width_ratio.min(height_ratio);
                        
                        Size::new(texture_size.width * ratio, texture_size.height * ratio)
                    },
                    ContentFit::Cover => {
                        let width_ratio = bounds_size.width / texture_size.width;
                        let height_ratio = bounds_size.height / texture_size.height;
                        let ratio = width_ratio.max(height_ratio);
                        
                        Size::new(texture_size.width * ratio, texture_size.height * ratio)
                    },
                    ContentFit::ScaleDown => {
                        let width_ratio = bounds_size.width / texture_size.width;
                        let height_ratio = bounds_size.height / texture_size.height;
                        let ratio = width_ratio.min(height_ratio).min(1.0);
                        
                        Size::new(texture_size.width * ratio, texture_size.height * ratio)
                    },
                    ContentFit::None => texture_size,
                };
                
                // Apply zoom scale
                return Size::new(base_size.width * scale, base_size.height * scale);
            }
        }
        
        // Fallback to original bounds if no texture
        bounds_size
    }
    
    // Helper method to calculate content bounds considering zoom and pan
    fn calculate_content_bounds(&self, bounds: Rectangle, scaled_size: Size, offset: Vector) -> Rectangle {
        // Calculate image position to center it
        let diff_w = bounds.width - scaled_size.width;
        let diff_h = bounds.height - scaled_size.height;
        
        let x = bounds.x + diff_w / 2.0 - offset.x;
        let y = bounds.y + diff_h / 2.0 - offset.y;
        
        // Apply 1px padding on all sides to avoid border overlap
        let padding = 1.0;
        Rectangle {
            x: x + padding,
            y: y + padding,
            width: scaled_size.width - 2.0 * padding,
            height: scaled_size.height - 2.0 * padding,
        }
    }
}