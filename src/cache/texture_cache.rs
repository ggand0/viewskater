use iced_wgpu::wgpu;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use std::hash::{Hash, Hasher};
use log::{debug, info, warn};

/// A simple cache for GPU textures created from CPU images.
/// This avoids recreating textures for the same image data.
#[derive(Debug)]
pub struct TextureCache {
    /// Map from image content hash to texture
    textures: HashMap<u64, Arc<wgpu::Texture>>,
    /// Statistics
    hits: usize,
    misses: usize,
    last_cleared: Instant,
}

impl TextureCache {
    pub fn new() -> Self {
        Self {
            textures: HashMap::new(),
            hits: 0,
            misses: 0,
            last_cleared: Instant::now(),
        }
    }

    /// Get a cached texture or create a new one
    pub fn get_or_create_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        image_bytes: &[u8],
        dimensions: (u32, u32),
    ) -> Option<Arc<wgpu::Texture>> {
        // Calculate a simple hash of the image data
        let hash = self.hash_image(image_bytes);

        if let Some(texture) = self.textures.get(&hash) {
            self.hits += 1;
            if self.hits % 100 == 0 {
                debug!("TextureCache: {} hits, {} misses", self.hits, self.misses);
            }
            return Some(Arc::clone(texture));
        }

        // Cache miss - create new texture
        self.misses += 1;
        debug!("TextureCache: Creating new texture (hash: {})", hash);

        match image::load_from_memory(image_bytes) {
            Ok(img) => {
                let rgba = img.to_rgba8();
                
                // Create new texture
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("Cached CPU Image Texture"),
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
                });
                
                // Write image data to texture
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
                
                // Cache the texture
                let texture_arc = Arc::new(texture);
                self.textures.insert(hash, Arc::clone(&texture_arc));
                
                // Periodically clean up old textures
                self.maybe_cleanup();
                
                Some(texture_arc)
            },
            Err(e) => {
                warn!("TextureCache: Failed to load image: {:?}", e);
                None
            }
        }
    }

    /// Generate a simple hash for the image data
    fn hash_image(&self, bytes: &[u8]) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        
        let mut hasher = DefaultHasher::new();
        // For large images, hash just a sample of bytes to improve performance
        if bytes.len() > 1024 {
            // Hash image length
            bytes.len().hash(&mut hasher);
            
            // Hash the first 512 bytes
            if bytes.len() >= 512 {
                bytes[..512].hash(&mut hasher);
            }
            
            // Hash the last 512 bytes
            if bytes.len() >= 1024 {
                bytes[bytes.len() - 512..].hash(&mut hasher);
            }
            
            // Hash some bytes from the middle
            if bytes.len() >= 1536 {
                let mid = bytes.len() / 2;
                bytes[mid - 256..mid + 256].hash(&mut hasher);
            }
        } else {
            // For small images, hash everything
            bytes.hash(&mut hasher);
        }
        
        hasher.finish()
    }

    /// Clean up the cache periodically to avoid memory leaks
    fn maybe_cleanup(&mut self) {
        let now = Instant::now();
        // Clear the cache every 5 minutes
        if now.duration_since(self.last_cleared).as_secs() > 300 {
            if self.textures.len() > 100 {
                debug!("TextureCache: Clearing cache ({} entries)", self.textures.len());
                self.textures.clear();
                self.last_cleared = now;
            }
        }
    }

    /// Get cache statistics
    pub fn stats(&self) -> (usize, usize) {
        (self.hits, self.misses)
    }
}

impl Default for TextureCache {
    fn default() -> Self {
        Self::new()
    }
}