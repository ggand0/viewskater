use iced_winit::core::{self, layout, mouse, renderer, widget::{self, tree::{self, Tree}}, Element, Length, Rectangle, Shell, Size};
use iced_wgpu::{wgpu, primitive};
use std::marker::PhantomData;
use std::sync::Arc;
use iced_widget::shader::{self, Viewport, Storage};
use iced_core::ContentFit;

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
    
    /// Calculate the layout bounds that preserve aspect ratio
    fn calculate_layout(&self, bounds: Rectangle) -> Rectangle {
        if let Some(scene) = &self.scene {
            if let Some(texture) = scene.get_texture() {
                debug!("ImageShader::calculate_layout - Got texture {}x{}", texture.width(), texture.height());
                
                let width = texture.width();
                let height = texture.height();
                
                if width == 0 || height == 0 {
                    debug!("ImageShader::calculate_layout - Zero texture size, using full bounds");
                    return bounds;
                }
                
                // Calculate the image size based on the content_fit
                let image_size = Size::new(width as f32, height as f32);
                let container_size = bounds.size();
                
                // Apply content_fit to maintain aspect ratio
                let fitted_size = self.content_fit.fit(image_size, container_size);
                
                // Calculate position (centered in the bounds)
                let x = bounds.x + (bounds.width - fitted_size.width) / 2.0;
                let y = bounds.y + (bounds.height - fitted_size.height) / 2.0;
                
                debug!("ImageShader::calculate_layout - Calculated content bounds: ({}, {}, {}, {})",
                      x, y, fitted_size.width, fitted_size.height);
                
                // Create content bounds that maintain aspect ratio
                return Rectangle::new(
                    core::Point::new(x, y),
                    fitted_size
                );
            } else {
                debug!("ImageShader::calculate_layout - Scene has NO texture!");
            }
        } else {
            debug!("ImageShader::calculate_layout - No scene available");
        }
        
        // Fallback to full bounds
        debug!("ImageShader::calculate_layout - Using full bounds: {:?}", bounds);
        bounds
    }
}

// This is our specialized primitive for image rendering
#[derive(Debug)]
pub struct ImagePrimitive {
    scene: Scene,
    bounds: Rectangle,
    content_bounds: Rectangle,
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
        struct Tag;
        tree::Tag::of::<Tag>()
    }
    
    fn state(&self) -> tree::State {
        tree::State::new(())
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
    
    fn draw(
        &self,
        _tree: &widget::Tree,
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
            
            // Calculate content bounds with proper aspect ratio
            let content_bounds = self.calculate_layout(bounds);
            debug!("ImageShader::draw - Content bounds: {:?}", content_bounds);
            
            if scene.get_texture().is_some() {
                debug!("ImageShader::draw - Scene has texture, creating primitive");
                
                let primitive = ImagePrimitive {
                    scene: scene.clone(),
                    bounds,
                    content_bounds,
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