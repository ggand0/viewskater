use std::path::PathBuf;
use std::io;
use wgpu;
use crate::cache::img_cache::{ImageCache, CachedData, ImageCacheBackend};
use crate::cache::cache_utils::{shift_cache_left, shift_cache_right, load_pos};
use wgpu::Texture;
use std::path::Path;

#[allow(unused_imports)]
use log::{debug, info, warn, error};


use crate::loading_status::LoadingStatus;
use crate::cache::img_cache::{LoadOperation, LoadOperationType};

pub struct GpuImageCache;

impl ImageCacheBackend for GpuImageCache {
    fn load_image(&self, path: &Path) -> Result<CachedData, io::Error> {
        println!("GpuCache: Loading image into GPU from {:?}", path);
        // Placeholder logic for GPU image loading
        Err(io::Error::new(io::ErrorKind::Unsupported, "GPU image loading not implemented"))
    }

    fn load_pos(&mut self, new_image: Option<CachedData>, pos: usize, image_index: isize) -> Result<bool, io::Error> {
        println!("GpuCache: Setting image at position {}", pos);
        // Placeholder logic for setting position in GPU cache
        Err(io::Error::new(io::ErrorKind::Unsupported, "GPU load_pos not implemented"))
    }

    fn load_initial_images(&mut self, image_paths: &[PathBuf], cache_count: usize, current_index: usize, cached_data: &mut Vec<Option<CachedData>>, cached_image_indices: &mut Vec<isize>, current_offset: &mut isize) -> Result<(), io::Error> {
        println!("GpuCache: Initializing GPU cache");
        // Placeholder logic for initializing GPU cache
        Err(io::Error::new(io::ErrorKind::Unsupported, "GPU load_initial_images not implemented"))
    }

}


/*
// Manual Clone Implementation
impl Clone for GpuImageCache {
    fn clone(&self) -> Self {
        // Clone the `BaseImageCache` but leave `cached_data` empty
        Self {
            base: BaseImageCache {
                cached_data: vec![None; self.base.cache_count * 2 + 1], // Reset textures
                cached_image_indices: self.base.cached_image_indices.clone(),
                cache_count: self.base.cache_count,
                image_paths: self.base.image_paths.clone(),
                num_files: self.base.num_files,
                current_index: self.base.current_index,
                current_offset: self.base.current_offset,
                cache_states: self.base.cache_states.clone(),
                loading_queue: self.base.loading_queue.clone(),
                being_loaded_queue: self.base.being_loaded_queue.clone(),
            },
            device: self.device,
            queue: self.queue
        }
    }
}
*/