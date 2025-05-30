#[allow(unused_imports)]
use log::{debug, info, warn, error};

use std::io;
use std::fs;
use std::path::PathBuf;
use crate::cache::img_cache::{CachedData, ImageCacheBackend};
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
        image_paths: &[PathBuf],
        #[allow(unused_variables)] compression_strategy: CompressionStrategy
    ) -> Result<CachedData, io::Error> {
        if let Some(path) = image_paths.get(index) {
            debug!("CpuCache: Loading image from {:?}", path);
            Ok(CachedData::Cpu(fs::read(path)?))
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
        #[allow(unused_variables)] compression_strategy: CompressionStrategy,
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
            let image = self.load_image(cache_index as usize, image_paths, compression_strategy)?;
            cached_data[i] = Some(image);
            cached_image_indices[i] = cache_index;
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