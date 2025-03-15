#[allow(unused_imports)]
use log::{debug, info, warn, error};

use std::path::PathBuf;
use std::io;
use std::sync::Arc;
use image::GenericImageView;
use iced_wgpu::wgpu;
use crate::cache::img_cache::{CachedData, ImageCacheBackend};


pub struct GpuImageCache {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
}

impl GpuImageCache {
    pub fn new(device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>) -> Self {
        Self { device, queue }
    }
}

impl ImageCacheBackend for GpuImageCache {
    fn load_image(&self, index: usize, image_paths: &[PathBuf]) -> Result<CachedData, io::Error> {
        if let Some(image_path) = image_paths.get(index) {
            let img = image::open(image_path).map_err(|e| {
                io::Error::new(io::ErrorKind::InvalidData, format!("Failed to open image: {}", e))
            })?;

            let rgba_image = img.to_rgba8();
            let (width, height) = img.dimensions();

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
                format: wgpu::TextureFormat::Rgba8UnormSrgb, // Use sRGB-aware format
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
                    rows_per_image: None, // None is correct because it's contiguous
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );

            Ok(CachedData::Gpu(texture.into()))
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
        let start_index: isize;
        let end_index: isize;
        if current_index <= cache_count {
            start_index = 0;
            end_index = (cache_count * 2 + 1) as isize;
            *current_offset = -(cache_count as isize - current_index as isize);
        } else if current_index > (image_paths.len() - 1) - cache_count {
            start_index = image_paths.len() as isize - cache_count as isize * 2 - 1;
            end_index = image_paths.len() as isize;
            *current_offset = cache_count as isize - ((image_paths.len() - 1) as isize - current_index as isize);
        } else {
            start_index = current_index as isize - cache_count as isize;
            end_index = current_index as isize + cache_count as isize + 1;
        }

        for (i, cache_index) in (start_index..end_index).enumerate() {
            if cache_index < 0 {
                continue;
            }
            if cache_index > image_paths.len() as isize - 1 {
                break;
            }
            let image = self.load_image(cache_index as usize, image_paths)?;
            cached_data[i] = Some(image);
            cached_image_indices[i] = cache_index;
        }

        // Display information about each image
        /*for (index, image_option) in cached_data.iter().enumerate() {
            match image_option {
                Some(image_bytes) => {
                    let image_info = format!("Image {} - Size: {} bytes", index, image_bytes.len());
                    debug!("{}", image_info);
                }
                None => {
                    let no_image_info = format!("No image at index {}", index);
                    debug!("{}", no_image_info);
                }
            }
        }

        // Display the indices
        for (index, cache_index) in cached_image_indices.iter().enumerate() {
            let index_info = format!("Index {} - Cache Index: {}", index, cache_index);
            debug!("{}", index_info);
        }*/

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
        println!("GpuCache: Setting image at position {}", pos);
    
        if pos >= cached_data.len() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Position out of bounds"));
        }
    
        // Store the new GPU texture in the cache
        cached_data[pos] = new_image;
        cached_image_indices[pos] = image_index;
    
        // Debugging output
        println!("Updated GPU cache at position {} with image index {}", pos, image_index);
    
        // If the position corresponds to the center of the cache, return true to trigger a reload
        let should_reload = pos == cache_count;
        Ok(should_reload)
    }
    

}
