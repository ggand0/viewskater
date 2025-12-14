#[allow(unused_imports)]
use log::{debug, info, warn, error};


use std::io;
use std::sync::Arc;
use image::GenericImageView;
use iced_wgpu::wgpu;
use crate::cache::img_cache::{CachedData, ImageCacheBackend, ImageMetadata};
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
        image_paths: &[crate::cache::img_cache::PathSource],
        compression_strategy: CompressionStrategy,
        archive_cache: Option<&mut crate::archive_cache::ArchiveCache>
    ) -> Result<CachedData, io::Error> {
        if let Some(path_source) = image_paths.get(index) {
            // Use the safe load_original_image function to prevent crashes with oversized images
            let img = crate::cache::cache_utils::load_original_image(path_source, archive_cache).map_err(|e| {
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
        image_paths: &[crate::cache::img_cache::PathSource],
        cache_count: usize,
        current_index: usize,
        cached_data: &mut Vec<Option<CachedData>>,
        cached_metadata: &mut Vec<Option<ImageMetadata>>,
        cached_image_indices: &mut Vec<isize>,
        current_offset: &mut isize,
        compression_strategy: CompressionStrategy,
        mut archive_cache: Option<&mut crate::archive_cache::ArchiveCache>,
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
            // Load image and capture metadata
            if let Some(path_source) = image_paths.get(cache_index as usize) {
                // Get file size efficiently without reading file content
                let file_size = crate::file_io::get_file_size(path_source, archive_cache.as_deref_mut());

                // Load the image (this will read the file for actual decoding)
                match self.load_image(cache_index as usize, image_paths, compression_strategy, archive_cache.as_deref_mut()) {
                    Ok(image) => {
                        // Get dimensions from the loaded texture
                        let (width, height) = match &image {
                            CachedData::Gpu(texture) | CachedData::BC1(texture) => {
                                let size = texture.size();
                                (size.width, size.height)
                            },
                            CachedData::Cpu(_) => (0, 0), // Shouldn't happen in GPU cache
                        };
                        cached_data[i] = Some(image);
                        cached_metadata[i] = Some(ImageMetadata::new(width, height, file_size));
                        cached_image_indices[i] = cache_index;
                    },
                    Err(e) => {
                        warn!("Failed to load image at index {}: {}. Skipping...", cache_index, e);
                        cached_data[i] = None;
                        cached_metadata[i] = None;
                        cached_image_indices[i] = -1; // Mark as invalid
                    }
                }
            }
        }

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
        _archive_cache: Option<&mut crate::archive_cache::ArchiveCache>,
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
