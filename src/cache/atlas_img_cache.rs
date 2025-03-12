use std::path::PathBuf;
use std::io;
use std::sync::{Arc, RwLock};
use iced_wgpu::wgpu;
use crate::cache::img_cache::{CachedData, ImageCacheBackend};
use crate::atlas::atlas::Atlas;
use crate::atlas::entry;
use image::GenericImageView;

#[allow(unused_imports)]
use log::{debug, info, warn, error};

#[allow(dead_code)]
pub struct AtlasImageCache {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    backend: wgpu::Backend,
    atlas: Arc<RwLock<Atlas>>,
}

impl AtlasImageCache {
    pub fn new(device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>, backend: wgpu::Backend, atlas: Arc<RwLock<Atlas>>) -> Self {
        Self { device, queue, backend, atlas }
    }
}

impl ImageCacheBackend for AtlasImageCache {
    fn load_image(&self, index: usize, image_paths: &[PathBuf]) -> Result<CachedData, io::Error> {
        if let Some(image_path) = image_paths.get(index) {
            let img = image::open(image_path).map_err(|e| {
                io::Error::new(io::ErrorKind::InvalidData, format!("Failed to open image: {}", e))
            })?;

            let rgba_image = img.to_rgba8();
            let (width, height) = img.dimensions();

            // Create a command encoder for atlas upload
            let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Atlas Upload Encoder"),
            });

            // Use a block scope to ensure the guard is dropped
            let entry_result = {
                // Get a write lock to the Atlas
                let mut atlas_guard = self.atlas.write().map_err(|_| {
                    io::Error::new(io::ErrorKind::Other, "Failed to acquire write lock on atlas")
                })?;
                
                // Upload to the atlas
                atlas_guard.upload(
                    self.device.clone(),
                    &mut encoder,
                    width,
                    height,
                    &rgba_image
                )
            }; // atlas_guard is dropped here
            
            if let Some(entry) = entry_result {
                // Submit the upload command
                self.queue.submit(std::iter::once(encoder.finish()));
                
                Ok(CachedData::Atlas {
                    atlas: Arc::clone(&self.atlas),
                    entry,
                })
            } else {
                // Atlas upload failed, fall back to individual texture
                debug!("Atlas upload failed for {:?}, falling back to individual texture", image_path);
                
                // Create a GPU texture based on the image dimensions
                let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("CacheTexture"),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });

                // Upload the texture using `queue.write_texture()`
                self.queue.write_texture(
                    wgpu::ImageCopyTexture {
                        texture: &texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &rgba_image,
                    wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row: Some(4 * width),
                        rows_per_image: None,
                    },
                    wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                );

                Ok(CachedData::Gpu(texture.into()))
            }
        } else {
            Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid image index"))
        }
    }

    fn load_initial_images(
        &mut self,
        image_paths: &[PathBuf],
        cache_count: usize,
        current_index: usize,
        cached_data: &mut Vec<Option<CachedData>>,
        cached_image_indices: &mut Vec<isize>,
        current_offset: &mut isize,
    ) -> Result<(), io::Error> {
        // Calculate the range of images to load initially
        let start_index: isize;
        let end_index: isize;
        
        if current_index <= cache_count {
            // Near the beginning of the collection
            start_index = 0;
            end_index = (cache_count * 2 + 1) as isize;
            *current_offset = -(cache_count as isize - current_index as isize);
        } else if current_index > (image_paths.len() - 1) - cache_count {
            // Near the end of the collection
            start_index = image_paths.len() as isize - cache_count as isize * 2 - 1;
            end_index = image_paths.len() as isize;
            *current_offset = cache_count as isize - ((image_paths.len() - 1) as isize - current_index as isize);
        } else {
            // In the middle of the collection
            start_index = current_index as isize - cache_count as isize;
            end_index = current_index as isize + cache_count as isize + 1;
        }

        debug!("Atlas: Loading initial images from {} to {}", start_index, end_index);
        
        // Load images within the calculated range
        for (i, cache_index) in (start_index..end_index).enumerate() {
            if cache_index < 0 || cache_index >= image_paths.len() as isize {
                continue;
            }
            
            debug!("Atlas: Loading image at index {} into cache position {}", cache_index, i);
            let image = self.load_image(cache_index as usize, image_paths)?;
            cached_data[i] = Some(image);
            cached_image_indices[i] = cache_index;
        }

        debug!("Atlas: Initial images loaded. Current offset: {}", current_offset);
        Ok(())
    }

    fn load_pos(
        &mut self,
        new_image: Option<CachedData>,
        pos: usize,
        image_index: isize,
        cached_data: &mut Vec<Option<CachedData>>,
        cached_image_indices: &mut Vec<isize>,
        cache_count: usize,
    ) -> Result<bool, io::Error> {
        debug!("AtlasCache: Setting image at position {}", pos);
    
        if pos >= cached_data.len() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Position out of bounds"));
        }
        
        // Before replacing, deallocate any atlas resources if needed
        if let Some(Some(CachedData::Atlas { atlas, entry })) = cached_data.get(pos) {
            // Clone the arc outside the if-let block to extend its lifetime
            let atlas_clone = Arc::clone(atlas);
            
            // Create a new scope to ensure the guard is dropped
            {
                // Now try to get the write lock
                let write_result = atlas_clone.write();
                if let Ok(mut atlas_guard) = write_result {
                    // Depending on the entry type, deallocate resources
                    match entry {
                        entry::Entry::Contiguous(allocation) => {
                            // Deallocate using a reference to the allocation
                            atlas_guard.deallocate(allocation);
                        },
                        entry::Entry::Fragmented { fragments, .. } => {
                            // Deallocate each fragment
                            for fragment in fragments {
                                // Pass a reference to the allocation
                                atlas_guard.deallocate(&fragment.allocation);
                            }
                        }
                    }
                } // atlas_guard is dropped here
            } // explicit end of scope for write_result
        }
    
        // Store the new image in the cache
        cached_data[pos] = new_image;
        cached_image_indices[pos] = image_index;
    
        // If the position corresponds to the center of the cache, return true to trigger a reload
        let should_reload = pos == cache_count;
        Ok(should_reload)
    }
}