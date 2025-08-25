#[allow(unused_imports)]
use log::{debug, info, warn, error};


use std::io;
use std::sync::Arc;
use image::GenericImageView;
use iced_wgpu::wgpu;
use crate::cache::img_cache::{CachedData, ImageCacheBackend, PathType};
use iced_wgpu::engine::CompressionStrategy;


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
    fn load_image(
        &self,
        index: usize,
        image_paths: &[PathType],
        compression_strategy: CompressionStrategy
    ) -> Result<CachedData, io::Error> {
        if let Some(image_path) = image_paths.get(index) {
            // Use the safe load_original_image function to prevent crashes with oversized images
            let img = crate::cache::cache_utils::load_original_image(image_path).map_err(|e| {
                io::Error::new(io::ErrorKind::InvalidData, format!("Failed to open image: {}", e))
            })?;

            let rgba_image = img.to_rgba8();
            let (width, height) = img.dimensions();
            let rgba_data = rgba_image.into_raw();

            // Use our utility function to determine if compression should be used
            let use_compression = crate::cache::cache_utils::should_use_compression(
                width, height, compression_strategy
            );

            // Create the texture with the appropriate format
            let texture = crate::cache::cache_utils::create_gpu_texture(
                &self.device, width, height, compression_strategy
            );

            if use_compression {
                // Use the utility to compress and upload
                let (compressed_data, row_bytes) = crate::cache::cache_utils::compress_image_data(
                    &rgba_data, width, height
                );

                // Upload using the utility function
                crate::cache::cache_utils::upload_compressed_texture(
                    &self.queue, &texture, &compressed_data, width, height, row_bytes
                );

                Ok(CachedData::BC1(texture.into()))
            } else {
                // Upload uncompressed using the utility function
                crate::cache::cache_utils::upload_uncompressed_texture(
                    &self.queue, &texture, &rgba_data, width, height
                );

                Ok(CachedData::Gpu(texture.into()))
            }
        } else {
            Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid image index"))
        }
    }

    fn load_initial_images(
        &mut self,
        image_paths: &[PathType],
        cache_count: usize,
        current_index: usize,
        cached_data: &mut Vec<Option<CachedData>>,
        cached_image_indices: &mut Vec<isize>,
        current_offset: &mut isize,
        compression_strategy: CompressionStrategy,
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
            match self.load_image(cache_index as usize, image_paths, compression_strategy) {
                Ok(image) => {
                    cached_data[i] = Some(image);
                    cached_image_indices[i] = cache_index;
                },
                Err(e) => {
                    warn!("Failed to load image at index {}: {}. Skipping...", cache_index, e);
                    cached_data[i] = None;
                    cached_image_indices[i] = -1; // Mark as invalid
                }
            }
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
        _compression_strategy: CompressionStrategy,
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
