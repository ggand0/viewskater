#[allow(unused_imports)]
use log::{debug, info, warn, error};

use std::io;
use crate::cache::img_cache::{CachedData, ImageCacheBackend, ImageMetadata};
use iced_wgpu::engine::CompressionStrategy;


pub struct CpuImageCache;

impl CpuImageCache {
    pub fn new() -> Self {
        CpuImageCache
    }
}

impl ImageCacheBackend for CpuImageCache {
    fn load_image(
        &self,
        index: usize,
        image_paths: &[crate::cache::img_cache::PathSource],
        #[allow(unused_variables)] compression_strategy: CompressionStrategy,
        archive_cache: Option<&mut crate::archive_cache::ArchiveCache>
    ) -> Result<CachedData, io::Error> {
        if let Some(path_source) = image_paths.get(index) {
            debug!("CpuCache: Loading image from {:?}", path_source.file_name());
            Ok(CachedData::Cpu(crate::file_io::read_image_bytes(path_source, archive_cache)?))
        } else {
            Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid image index"))
        }
    }

    #[allow(clippy::needless_option_as_deref)]
    fn load_single_image(
        &mut self,
        image_paths: &[crate::cache::img_cache::PathSource],
        cache_count: usize,
        current_index: usize,
        cached_data: &mut Vec<Option<CachedData>>,
        cached_metadata: &mut Vec<Option<ImageMetadata>>,
        cached_image_indices: &mut Vec<isize>,
        current_offset: &mut isize,
        #[allow(unused_variables)] compression_strategy: CompressionStrategy,
        mut archive_cache: Option<&mut crate::archive_cache::ArchiveCache>,
    ) -> Result<(), io::Error> {
        // Calculate which cache slot to use for the current_index
        let cache_slot: usize;
        if current_index <= cache_count {
            cache_slot = current_index;
            *current_offset = -(cache_count as isize - current_index as isize);
        } else if current_index > (image_paths.len() - 1) - cache_count {
            cache_slot = cache_count + (cache_count as isize -
                          ((image_paths.len()-1) as isize - current_index as isize)) as usize;
            *current_offset = cache_count as isize -
                             ((image_paths.len()-1) as isize - current_index as isize);
        } else {
            cache_slot = cache_count;
            *current_offset = 0;
        }

        // Load only the single image at current_index
        if let Some(path_source) = image_paths.get(current_index) {
            match crate::file_io::read_image_bytes_with_size(path_source, archive_cache.as_deref_mut()) {
                Ok((bytes, file_size)) => {
                    // Get dimensions efficiently using header-only read
                    use std::io::Cursor;
                    use image::ImageReader;
                    let (width, height) = ImageReader::new(Cursor::new(&bytes))
                        .with_guessed_format()
                        .ok()
                        .and_then(|r| r.into_dimensions().ok())
                        .unwrap_or((0, 0));
                    cached_data[cache_slot] = Some(CachedData::Cpu(bytes));
                    cached_metadata[cache_slot] = Some(ImageMetadata::new(width, height, file_size));
                    cached_image_indices[cache_slot] = current_index as isize;
                    debug!("CpuCache: Loaded single image at index {} into cache slot {}", current_index, cache_slot);
                },
                Err(e) => {
                    warn!("Failed to load image at index {}: {}. Skipping...", current_index, e);
                    cached_data[cache_slot] = None;
                    cached_metadata[cache_slot] = None;
                    cached_image_indices[cache_slot] = -1;
                    return Err(e);
                }
            }
        } else {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid image index"));
        }

        Ok(())
    }

    #[allow(dead_code)]
    fn load_initial_images(
        &mut self,
        image_paths: &[crate::cache::img_cache::PathSource],
        cache_count: usize,
        current_index: usize,
        cached_data: &mut Vec<Option<CachedData>>,
        cached_metadata: &mut Vec<Option<ImageMetadata>>,
        cached_image_indices: &mut Vec<isize>,
        current_offset: &mut isize,
        #[allow(unused_variables)] compression_strategy: CompressionStrategy,
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
            // Load image bytes with metadata
            if let Some(path_source) = image_paths.get(cache_index as usize) {
                match crate::file_io::read_image_bytes_with_size(path_source, archive_cache.as_deref_mut()) {
                    Ok((bytes, file_size)) => {
                        // Get dimensions efficiently using header-only read
                        use std::io::Cursor;
                        use image::ImageReader;
                        let (width, height) = ImageReader::new(Cursor::new(&bytes))
                            .with_guessed_format()
                            .ok()
                            .and_then(|r| r.into_dimensions().ok())
                            .unwrap_or((0, 0));
                        cached_data[i] = Some(CachedData::Cpu(bytes));
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
        _cached_data: &mut Vec<Option<CachedData>>,
        _cached_image_indices: &mut Vec<isize>,
        _cache_count: usize,
        #[allow(unused_variables)] _compression_strategy: CompressionStrategy,
        _archive_cache: Option<&mut crate::archive_cache::ArchiveCache>,
    ) -> Result<bool, io::Error> {
        match new_image {
            Some(CachedData::Cpu(_)) => {
                debug!("CpuCache: Setting image at position {}", pos);
                Ok(pos == image_index as usize)
            }
            _ => Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid data for CPU cache")),
        }
    }
}