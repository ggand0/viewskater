use iced_widget::shader::{self, Viewport};
use iced_winit::core::{Rectangle, mouse};
use iced_wgpu::wgpu;
use image::{GenericImageView, ImageFormat, DynamicImage};
use std::sync::Arc;
use std::time::Instant;
use log::{debug, info, warn, error};

use crate::cache::img_cache::CachedData;
use crate::utils::timing::TimingStats;
use crate::widgets::shader::texture_pipeline::TexturePipeline;
use crate::cache::texture_cache::TextureCache;
use once_cell::sync::Lazy;
use std::sync::Mutex;

static SHADER_UPDATE_STATS: Lazy<Mutex<TimingStats>> = Lazy::new(|| {
    Mutex::new(TimingStats::new("CPU Shader Update"))
});

// Global texture cache
static TEXTURE_CACHE: Lazy<Mutex<TextureCache>> = Lazy::new(|| {
    Mutex::new(TextureCache::new())
});

#[derive(Debug)]
pub struct CpuScene {
    pub image_bytes: Vec<u8>,               // Store CPU image bytes
    pub texture: Option<Arc<wgpu::Texture>>, // Lazily created GPU texture
    pub texture_size: (u32, u32),           // Image dimensions
    pub needs_update: bool,                 // Flag to indicate if texture needs updating
}

impl CpuScene {
    pub fn new(image_bytes: Vec<u8>) -> Self {
        // Attempt to load dimensions from image bytes
        let dimensions = match image::load_from_memory(&image_bytes) {
            Ok(img) => {
                let (width, height) = img.dimensions();
                debug!("CpuScene::new - loaded image with dimensions: {}x{}", width, height);
                (width, height)
            },
            Err(e) => {
                error!("CpuScene::new - Failed to load image dimensions: {:?}", e);
                (0, 0) // Default to 0,0 if we can't determine dimensions
            }
        };
        
        CpuScene {
            image_bytes,
            texture: None,
            texture_size: dimensions,
            needs_update: true,
        }
    }
    
    pub fn update_image(&mut self, new_image_bytes: Vec<u8>) {
        // Update image bytes and mark texture for recreation
        self.image_bytes = new_image_bytes;
        
        // Attempt to update dimensions from the new image bytes
        if let Ok(img) = image::load_from_memory(&self.image_bytes) {
            self.texture_size = img.dimensions();
        }
        
        self.needs_update = true;
        self.texture = None; // Force texture recreation
    }
    
    // Create GPU texture from CPU bytes - expose as public
    pub fn ensure_texture(&mut self, device: &Arc<wgpu::Device>, queue: &Arc<wgpu::Queue>) -> Option<Arc<wgpu::Texture>> {
        if self.needs_update || self.texture.is_none() {
            let start = Instant::now();
            debug!("CpuScene::ensure_texture - Using cached or creating texture from {} bytes", self.image_bytes.len());
            
            // Validate image data before attempting to create texture
            if self.image_bytes.is_empty() {
                error!("CpuScene::ensure_texture - Empty image data, cannot create texture");
                return None;
            }
            
            if let Ok(mut cache) = TEXTURE_CACHE.lock() {
                if let Some(texture) = cache.get_or_create_texture(
                    device, 
                    queue, 
                    &self.image_bytes, 
                    self.texture_size
                ) {
                    self.texture = Some(Arc::clone(&texture));
                    self.needs_update = false;
                    
                    // Timing statistics
                    let elapsed = start.elapsed();
                    if let Ok(mut stats) = SHADER_UPDATE_STATS.lock() {
                        stats.add_measurement(elapsed);
                    }
                    
                    debug!("CpuScene::ensure_texture - Retrieved texture in {:?}", elapsed);
                } else {
                    error!("CpuScene::ensure_texture - Failed to create or retrieve texture from cache");
                }
            } else {
                warn!("CpuScene::ensure_texture - Failed to acquire texture cache lock");
            }
        }
        
        if self.texture.is_none() {
            warn!("CpuScene::ensure_texture - No texture available after ensure_texture call");
        }
        
        self.texture.clone()
    }
}

#[derive(Debug)]
pub struct CpuPrimitive {
    image_bytes: Vec<u8>,
    texture: Option<Arc<wgpu::Texture>>,
    texture_size: (u32, u32),
    bounds: Rectangle,
    needs_update: bool,
}

impl CpuPrimitive {
    pub fn new(
        image_bytes: Vec<u8>,
        texture: Option<Arc<wgpu::Texture>>,
        texture_size: (u32, u32),
        bounds: Rectangle,
        needs_update: bool,
    ) -> Self {
        Self {
            image_bytes,
            texture,
            texture_size,
            bounds,
            needs_update,
        }
    }
}

impl shader::Primitive for CpuPrimitive {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        storage: &mut shader::Storage,
        bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        let scale_factor = viewport.scale_factor() as f32;
        let viewport_size = viewport.physical_size();

        let shader_size = (
            (bounds.width * scale_factor) as u32,
            (bounds.height * scale_factor) as u32,
        );

        let bounds_relative = (
            (bounds.x * scale_factor) / viewport_size.width as f32,
            (bounds.y * scale_factor) / viewport_size.height as f32,
            (bounds.width * scale_factor) / viewport_size.width as f32,
            (bounds.height * scale_factor) / viewport_size.height as f32,
        );

        debug!("CpuPrimitive prepare - bounds: {:?}, bounds_relative: {:?}", bounds, bounds_relative);
        debug!("CpuPrimitive prepare - viewport_size: {:?}, shader_size: {:?}", viewport_size, shader_size);

        // Only proceed if we have a valid texture
        if let Some(texture) = &self.texture {
            if !storage.has::<TexturePipeline>() {
                debug!("Creating new TexturePipeline for CPU image");
                storage.store(TexturePipeline::new(
                    device,
                    queue,
                    format,
                    texture.clone(),
                    shader_size,
                    self.texture_size,
                    bounds_relative,
                ));
            } else {
                debug!("Updating existing TexturePipeline for CPU image");
                let pipeline = storage.get_mut::<TexturePipeline>().unwrap();
                
                pipeline.update_vertices(device, bounds_relative);
                pipeline.update_texture(device, queue, texture.clone());
                pipeline.update_screen_uniforms(queue, self.texture_size, shader_size, bounds_relative);
            }
        } else {
            warn!("No texture available for rendering");
        }
    }

    fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        storage: &shader::Storage,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        if self.texture.is_some() {
            if let Some(pipeline) = storage.get::<TexturePipeline>() {
                debug!("Rendering CPU image with TexturePipeline");
                pipeline.render(target, encoder, clip_bounds);
            } else {
                warn!("TexturePipeline not found in storage");
            }
        } else {
            warn!("Cannot render - no texture available");
        }
    }
}

impl<Message> shader::Program<Message> for CpuScene {
    type State = ();
    type Primitive = CpuPrimitive;

    fn draw(
        &self,
        _state: &Self::State,
        _cursor: mouse::Cursor,
        bounds: Rectangle,
    ) -> Self::Primitive {
        CpuPrimitive::new(
            self.image_bytes.clone(),
            self.texture.clone(),
            self.texture_size,
            bounds,
            self.needs_update,
        )
    }
}
