#[allow(unused_imports)]
use log::{debug, info, warn, error};

use std::sync::Arc;
use std::time::Instant;
use std::collections::HashMap;
use std::sync::Mutex;
use once_cell::sync::Lazy;
use image::GenericImageView;
use iced_widget::shader::{self, Viewport};
use iced_winit::core::{Rectangle, mouse};
use iced_wgpu::wgpu;
use crate::utils::timing::TimingStats;
use crate::widgets::shader::texture_pipeline::TexturePipeline;
use crate::cache::texture_cache::TextureCache;


static _SHADER_UPDATE_STATS: Lazy<Mutex<TimingStats>> = Lazy::new(|| {
    Mutex::new(TimingStats::new("CPU Shader Update"))
});

// Change from a single global cache to a map of pane-specific caches
static TEXTURE_CACHES: Lazy<Mutex<HashMap<String, TextureCache>>> = Lazy::new(|| {
    Mutex::new(HashMap::new())
});

#[derive(Debug, Default)]
pub struct CpuPipelineRegistry {
    pipelines: std::collections::HashMap<String, TexturePipeline>,
}

#[derive(Debug, Clone)]
pub struct CpuScene {
    pub image_bytes: Vec<u8>,               // Store CPU image bytes
    pub texture: Option<Arc<wgpu::Texture>>, // Lazily created GPU texture
    pub texture_size: (u32, u32),           // Image dimensions
    pub needs_update: bool,                 // Flag to indicate if texture needs updating
    pub use_cached_texture: bool,           // Flag to indicate if cached texture should be used
}

impl CpuScene {
    pub fn new(image_bytes: Vec<u8>, use_cached_texture: bool) -> Self {
        // Check if image_bytes is empty before attempting to load
        let dimensions = if !image_bytes.is_empty() {
            match crate::exif_utils::decode_with_exif_orientation(&image_bytes) {
                Ok(img) => {
                    let (width, height) = img.dimensions();
                    debug!("CpuScene::new - loaded image with dimensions: {width}x{height}");
                    (width, height)
                },
                Err(e) => {
                    error!("CpuScene::new - Failed to load image dimensions: {e:?}");
                    (0, 0) // Default to 0,0 if we can't determine dimensions
                }
            }
        } else {
            // No image data provided, use default dimensions
            debug!("CpuScene::new - No image data provided, using default dimensions");
            (0, 0)
        };

        CpuScene {
            image_bytes,
            texture: None,
            texture_size: dimensions,
            needs_update: true,
            use_cached_texture,
        }
    }

    pub fn update_image(&mut self, new_image_bytes: Vec<u8>) {
        // Update image bytes and mark texture for recreation
        self.image_bytes = new_image_bytes;

        // Attempt to update dimensions from the new image bytes
        if let Ok(img) = crate::exif_utils::decode_with_exif_orientation(&self.image_bytes) {
            self.texture_size = img.dimensions();
        }

        self.needs_update = true;
        self.texture = None; // Force texture recreation
    }

    // Create GPU texture from CPU bytes - expose as public
    pub fn ensure_texture(&mut self, device: &Arc<wgpu::Device>, queue: &Arc<wgpu::Queue>, pane_id: &str) -> Option<Arc<wgpu::Texture>> {
        if self.needs_update || self.texture.is_none() {
            let start = Instant::now();
            debug!("CpuScene::ensure_texture - Using cached or creating texture from {} bytes for pane {}",
                   self.image_bytes.len(), pane_id);

            // Validate image data before attempting to create texture
            if self.image_bytes.is_empty() {
                error!("CpuScene::ensure_texture - Empty image data, cannot create texture");
                return None;
            }

            if self.use_cached_texture {
                let cache_start = Instant::now();
                if let Ok(mut caches) = TEXTURE_CACHES.lock() {
                    let cache_lock_time = cache_start.elapsed();
                    debug!("CpuScene::ensure_texture - Acquired texture caches lock in {cache_lock_time:?}");

                    // Get or create the cache for this specific pane
                    let cache = caches.entry(pane_id.to_string())
                                     .or_insert_with(TextureCache::new);

                    let texture_start = Instant::now();
                    if let Some(texture) = cache.get_or_create_texture(
                        device,
                        queue,
                        &self.image_bytes,
                        self.texture_size
                    ) {
                        let texture_time = texture_start.elapsed();
                        debug!("CpuScene::ensure_texture - get_or_create_texture took {texture_time:?} for pane {pane_id}");

                        self.texture = Some(Arc::clone(&texture));
                        self.needs_update = false;

                        let total_time = start.elapsed();
                        debug!("CpuScene::ensure_texture - Total time: {total_time:?} for pane {pane_id}");

                        return Some(Arc::clone(&texture));
                    }
                }

                // If we failed to get/create a texture from the cache, fallback to direct creation
                error!("Failed to get/create texture from cache for pane {pane_id}");
            }

            // Direct texture creation (fallback or when cache is disabled)
            let texture_start = Instant::now();
            match crate::exif_utils::decode_with_exif_orientation(&self.image_bytes) {
                Ok(img) => {
                    let rgba = img.to_rgba8();
                    let dimensions = img.dimensions();

                    if dimensions.0 == 0 || dimensions.1 == 0 {
                        error!("CpuScene::ensure_texture - Invalid image dimensions: {}x{}", dimensions.0, dimensions.1);
                        return None;
                    }

                    debug!("CpuScene::ensure_texture - Creating texture with dimensions {}x{}", dimensions.0, dimensions.1);

                    let texture = device.create_texture(
                        &wgpu::TextureDescriptor {
                            label: Some("CpuScene Texture"),
                            size: wgpu::Extent3d {
                                width: dimensions.0,
                                height: dimensions.1,
                                depth_or_array_layers: 1,
                            },
                            mip_level_count: 1,
                            sample_count: 1,
                            dimension: wgpu::TextureDimension::D2,
                            format: wgpu::TextureFormat::Rgba8UnormSrgb,
                            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                            view_formats: &[],
                        }
                    );

                    queue.write_texture(
                        wgpu::ImageCopyTexture {
                            texture: &texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        &rgba,
                        wgpu::ImageDataLayout {
                            offset: 0,
                            bytes_per_row: Some(4 * dimensions.0),
                            rows_per_image: Some(dimensions.1),
                        },
                        wgpu::Extent3d {
                            width: dimensions.0,
                            height: dimensions.1,
                            depth_or_array_layers: 1,
                        },
                    );

                    let texture_arc = Arc::new(texture);
                    self.texture = Some(Arc::clone(&texture_arc));
                    self.needs_update = false;

                    let creation_time = texture_start.elapsed();
                    debug!("Created texture directly in {creation_time:?}");

                    return Some(texture_arc);
                },
                Err(e) => {
                    error!("CpuScene::ensure_texture - Failed to load image: {e:?}");
                    return None;
                }
            }
        }

        if self.texture.is_none() {
            warn!("CpuScene::ensure_texture - No texture available after ensure_texture call");
        }

        self.texture.clone()
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
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
        let debug = false;
        let prepare_start = Instant::now();
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

        // Create a unique key for this pipeline based on position
        let pipeline_key = format!("cpu_pipeline_{}_{}_{}_{}",
                                  bounds.x, bounds.y, bounds.width, bounds.height);

        // Only proceed if we have a valid texture
        if let Some(texture) = &self.texture {
            // Ensure we have a registry
            if !storage.has::<CpuPipelineRegistry>() {
                storage.store(CpuPipelineRegistry::default());
            }

            // Get the registry
            let registry = storage.get_mut::<CpuPipelineRegistry>().unwrap();

            // Check if we need to create a new pipeline for this position
            if !registry.pipelines.contains_key(&pipeline_key) {
                let pipeline = TexturePipeline::new(
                    device,
                    queue,
                    format,
                    texture.clone(),
                    shader_size,
                    self.texture_size,
                    bounds_relative,
                    false, // Default to Linear filter for CPU scene renderer
                );

                registry.pipelines.insert(pipeline_key.clone(), pipeline);
            } else {
                let pipeline = registry.pipelines.get_mut(&pipeline_key).unwrap();

                let vertices_start = Instant::now();
                pipeline.update_vertices(device, bounds_relative);
                let _vertices_time = vertices_start.elapsed();

                let texture_update_start = Instant::now();
                pipeline.update_texture(device, queue, texture.clone(), false);
                let _texture_update_time = texture_update_start.elapsed();


                let uniforms_start = Instant::now();
                pipeline.update_screen_uniforms(queue, self.texture_size, shader_size, bounds_relative);
                let _uniforms_time = uniforms_start.elapsed();
            }
        } else {
            warn!("No texture available for rendering");
        }

        let prepare_time = prepare_start.elapsed();
        if debug {
            debug!("CpuPrimitive prepare - bounds: {bounds:?}, bounds_relative: {bounds_relative:?}");
            debug!("CpuPrimitive prepare - viewport_size: {viewport_size:?}, shader_size: {shader_size:?}");
            debug!("CpuPrimitive prepare completed in {prepare_time:?}");
        }
    }

    fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        storage: &shader::Storage,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        let render_start = Instant::now();

        if self.texture.is_some() {
            // Get the pipeline key for this position
            let pipeline_key = format!("cpu_pipeline_{}_{}_{}_{}",
                                     self.bounds.x, self.bounds.y, self.bounds.width, self.bounds.height);

            // Find our pipeline in the registry
            if let Some(registry) = storage.get::<CpuPipelineRegistry>() {
                if let Some(pipeline) = registry.pipelines.get(&pipeline_key) {
                    debug!("Rendering CPU image with TexturePipeline for key {pipeline_key}");
                    pipeline.render(target, encoder, clip_bounds);
                    let render_time = render_start.elapsed();
                    debug!("Rendered CPU image in {render_time:?}");
                } else {
                    warn!("TexturePipeline not found in registry with key {pipeline_key}");
                }
            } else {
                warn!("CpuPipelineRegistry not found in storage");
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
