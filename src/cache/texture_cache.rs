use iced_wgpu::wgpu;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use std::hash::{Hash, Hasher};

#[allow(unused_imports)]
use log::{debug, info, warn};
use image::GenericImageView;

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
        _dimensions: (u32, u32),
    ) -> Option<Arc<wgpu::Texture>> {
        // Safety check for empty data
        if image_bytes.is_empty() {
            warn!("TextureCache: Cannot create texture from empty image data");
            return None;
        }

        // Calculate a simple hash of the image data
        let hash_start = Instant::now();
        let hash = self.hash_image(image_bytes);
        let hash_time = hash_start.elapsed();
        debug!("TextureCache: Computed hash in {:?}", hash_time);

        if let Some(texture) = self.textures.get(&hash) {
            self.hits += 1;
            if self.hits.is_multiple_of(100) {
                debug!("TextureCache: {} hits, {} misses", self.hits, self.misses);
            }
            debug!("TextureCache: Cache hit for hash {}", hash);
            return Some(Arc::clone(texture));
        }

        // Cache miss - create new texture
        self.misses += 1;
        debug!("TextureCache: Creating new texture (hash: {})", hash);

        let load_start = Instant::now();
        match crate::exif_utils::decode_with_exif_orientation(image_bytes) {
            Ok(img) => {
                let load_time = load_start.elapsed();
                debug!("TextureCache: Loaded image in {:?}", load_time);

                let rgba_start = Instant::now();
                let rgba = img.to_rgba8();
                let rgba_time = rgba_start.elapsed();
                debug!("TextureCache: Converted to RGBA in {:?}", rgba_time);

                let dimensions = img.dimensions();

                if dimensions.0 == 0 || dimensions.1 == 0 {
                    warn!("TextureCache: Invalid image dimensions: {}x{}", dimensions.0, dimensions.1);
                    return None;
                }

                // Create the texture
                let texture_start = Instant::now();
                let texture = device.create_texture(
                    &wgpu::TextureDescriptor {
                        label: Some("CPU Image Texture"),
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
                let texture_create_time = texture_start.elapsed();
                debug!("TextureCache: Created texture in {:?}", texture_create_time);

                // Write the image data to the texture
                let upload_start = Instant::now();
                queue.write_texture(
                    wgpu::ImageCopyTexture {
                        texture: &texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    bytemuck::cast_slice(rgba.as_raw()),
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
                let upload_time = upload_start.elapsed();
                debug!("TextureCache: Uploaded texture data in {:?}", upload_time);

                let texture_arc = Arc::new(texture);
                self.textures.insert(hash, Arc::clone(&texture_arc));

                // Maybe clean up old textures
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
        if now.duration_since(self.last_cleared).as_secs() > 300
            && self.textures.len() > 100 {
                debug!("TextureCache: Clearing cache ({} entries)", self.textures.len());
                self.textures.clear();
                self.last_cleared = now;
            }
    }

    /// Get cache statistics
    pub fn _stats(&self) -> (usize, usize) {
        (self.hits, self.misses)
    }
}

impl Default for TextureCache {
    fn default() -> Self {
        Self::new()
    }
}