#[allow(unused_imports)]
use std::time::Instant;

#[allow(unused_imports)]
use log::{debug, info, warn, error};

use std::path::PathBuf;
use std::io;
use std::collections::VecDeque;
use std::sync::Arc;
use iced_winit::runtime::Task;
use iced_wgpu::wgpu;

use crate::file_io::{empty_async_block_vec};
use crate::loading_status::LoadingStatus;
use crate::app::Message;
use crate::pane::Pane;
use crate::pane;
use crate::cache::cpu_img_cache::CpuImageCache;
use crate::cache::gpu_img_cache::GpuImageCache;
use crate::file_io;
use iced_wgpu::engine::CompressionStrategy;


#[derive(Debug, Clone, PartialEq)]
pub enum LoadOperation {
    LoadNext((Vec<usize>, Vec<Option<isize>>)),       // Includes the target index
    ShiftNext((Vec<usize>, Vec<Option<isize>>)),
    LoadPrevious((Vec<usize>, Vec<Option<isize>>)),   // Includes the target index
    ShiftPrevious((Vec<usize>, Vec<Option<isize>>)),
    LoadPos((usize, Vec<Option<(isize, usize)>>))   // // Load an images into specific cache positions
}

#[derive(PartialEq, Debug, Clone, Copy)]
pub enum LoadOperationType {
    LoadNext,
    ShiftNext,
    LoadPrevious,
    ShiftPrevious,
    LoadPos,
}

impl LoadOperation {
    pub fn operation_type(&self) -> LoadOperationType {
        match self {
            LoadOperation::LoadNext(..) => LoadOperationType::LoadNext,
            LoadOperation::ShiftNext(..) => LoadOperationType::ShiftNext,
            LoadOperation::LoadPrevious(..) => LoadOperationType::LoadPrevious,
            LoadOperation::ShiftPrevious(..) => LoadOperationType::ShiftPrevious,
            LoadOperation::LoadPos(..) => LoadOperationType::LoadPos,
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CacheStrategy {
    Cpu,         // Use CPU memory for image caching
    Gpu,         // Use individual GPU textures
}

impl CacheStrategy {
    pub fn is_gpu_based(&self) -> bool {
        match self {
            CacheStrategy::Cpu => false,
            CacheStrategy::Gpu => true,
        }
    }
}

/// Metadata for cached images to avoid repeated decoding
#[derive(Debug, Clone, Default)]
pub struct ImageMetadata {
    pub width: u32,
    pub height: u32,
    pub file_size: u64,
}

impl ImageMetadata {
    pub fn new(width: u32, height: u32, file_size: u64) -> Self {
        Self { width, height, file_size }
    }

    /// Format resolution as "WIDTHxHEIGHT" string
    pub fn resolution_string(&self) -> String {
        format!("{}x{}", self.width, self.height)
    }

    /// Format file size as human-readable string (e.g., "2.5 MB")
    pub fn file_size_string(&self) -> String {
        if self.file_size < 1024 {
            format!("{} B", self.file_size)
        } else if self.file_size < 1024 * 1024 {
            format!("{:.1} KB", self.file_size as f64 / 1024.0)
        } else {
            format!("{:.1} MB", self.file_size as f64 / (1024.0 * 1024.0))
        }
    }
}

#[derive(Debug, Clone)]
pub enum CachedData {
    Cpu(Vec<u8>),                // CPU: Raw image bytes
    Gpu(Arc<wgpu::Texture>),     // GPU: Uncompressed texture
    BC1(Arc<wgpu::Texture>),     // GPU: BC1 compressed texture
}

impl CachedData {
    pub fn take(self) -> Self {
        self
    }

    pub fn width(&self) -> u32 {
        match self {
            CachedData::Cpu(data) => {
                // Try to decode image and get width
                if let Ok(image) = image::load_from_memory(data) {
                    image.width()
                } else {
                    0
                }
            },
            CachedData::Gpu(texture) => texture.width(),
            CachedData::BC1(texture) => texture.width(),
        }
    }

    pub fn height(&self) -> u32 {
        match self {
            CachedData::Cpu(data) => {
                // Try to decode image and get height
                if let Ok(image) = image::load_from_memory(data) {
                    image.height()
                } else {
                    0
                }
            },
            CachedData::Gpu(texture) => texture.height(),
            CachedData::BC1(texture) => texture.height(),
        }
    }

    pub fn handle(&self) -> Option<iced_core::image::Handle> {
        match self {
            CachedData::Cpu(data) => {
                Some(iced_core::image::Handle::from_bytes(data.clone()))
            },
            CachedData::Gpu(_) => None, // No CPU handle for GPU-based textures
            CachedData::BC1(_) => None, // No CPU handle for BC1 compressed textures
        }
    }
}

impl CachedData {
    pub fn len(&self) -> usize {
        match self {
            CachedData::Cpu(data) => data.len(),
            CachedData::Gpu(texture) => {
                let width = texture.width();
                let height = texture.height();
                4 * (width as usize) * (height as usize) // 4 bytes per pixel (RGBA8)
            }
            CachedData::BC1(texture) => {
                // BC1 uses 8 bytes per 4x4 block, which is 0.5 bytes per pixel
                let width = texture.width();
                let height = texture.height();

                // Round up to nearest multiple of 4 if needed
                let block_width = width.div_ceil(4);
                let block_height = height.div_ceil(4);

                // Each 4x4 block is 8 bytes in BC1
                (block_width * block_height * 8) as usize
            }
        }
    }

    pub fn as_vec(&self) -> Result<Vec<u8>, io::Error> {
        match self {
            CachedData::Cpu(data) => Ok(data.clone()),
            CachedData::Gpu(_) => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "GPU data cannot be converted to a Vec<u8>",
            )),
            CachedData::BC1(_) => todo!()
        }
    }

    pub fn is_compressed(&self) -> bool {
        matches!(self, CachedData::BC1(_))
    }

    pub fn compression_format(&self) -> Option<&'static str> {
        match self {
            CachedData::BC1(_) => Some("BC1"),
            _ => None,
        }
    }
}
/// PathSource enum for type-safe image loading with performance optimization
#[derive(Clone, Debug)]
pub enum PathSource {
    /// Regular filesystem file - direct filesystem I/O
    Filesystem(PathBuf),
    /// Archive internal path - requires archive reading
    Archive(PathBuf),
    /// Preloaded archive content - available in ArchiveCache HashMap
    Preloaded(PathBuf),
}

impl PathSource {
    /// Get the underlying PathBuf for any variant
    pub fn path(&self) -> &PathBuf {
        match self {
            PathSource::Filesystem(path) => path,
            PathSource::Archive(path) => path,
            PathSource::Preloaded(path) => path,
        }
    }
    /// Get filename for display/sorting purposes
    pub fn file_name(&self) -> std::borrow::Cow<'_, str> {
        match self {
            PathSource::Filesystem(_) => {
                self.path().file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            },
            _ => {
                std::borrow::Cow::from(self.path().display().to_string())
            }
        }
    }
}

pub trait ImageCacheBackend {
    fn load_image(
        &self,
        index: usize,
        image_paths: &[PathSource],
        compression_strategy: CompressionStrategy,
        archive_cache: Option<&mut crate::archive_cache::ArchiveCache>
    ) -> Result<CachedData, io::Error>;

    #[allow(clippy::too_many_arguments)]
    fn load_initial_images(
        &mut self,
        image_paths: &[PathSource],
        cache_count: usize,
        current_index: usize,
        cached_data: &mut Vec<Option<CachedData>>,
        cached_image_indices: &mut Vec<isize>,
        current_offset: &mut isize,
        compression_strategy: CompressionStrategy,
        archive_cache: Option<&mut crate::archive_cache::ArchiveCache>,
    ) -> Result<(), io::Error>;

    #[allow(dead_code)]
    #[allow(clippy::too_many_arguments)]
    fn load_pos(
        &mut self,
        new_image: Option<CachedData>,
        pos: usize,
        image_index: isize,
        cached_data: &mut Vec<Option<CachedData>>,
        cached_image_indices: &mut Vec<isize>,
        cache_count: usize,
        compression_strategy: CompressionStrategy,
        archive_cache: Option<&mut crate::archive_cache::ArchiveCache>,
    ) -> Result<bool, io::Error>;
}


pub struct ImageCache {
    pub image_paths: Vec<PathSource>,
    pub num_files: usize,
    pub current_index: usize,
    pub current_offset: isize,
    pub cache_count: usize,
    pub cached_image_indices: Vec<isize>,    // Indices of cached images
    pub cache_states: Vec<bool>,            // States of cache validity
    pub loading_queue: VecDeque<LoadOperation>,
    pub being_loaded_queue: VecDeque<LoadOperation>,    // Queue of image indices being loaded

    pub cached_data: Vec<Option<CachedData>>, // Caching mechanism
    pub cached_metadata: Vec<Option<ImageMetadata>>, // Metadata parallel to cached_data
    pub backend: Box<dyn ImageCacheBackend>, // Backend determines caching type
    pub slider_texture: Option<Arc<wgpu::Texture>>,
    pub compression_strategy: CompressionStrategy,
}

impl Default for ImageCache {
    fn default() -> Self {
        ImageCache {
            image_paths: Vec::new(),
            num_files: 0,
            current_index: 0,
            current_offset: 0,
            cache_count: 0,
            cached_image_indices: Vec::new(),
            cache_states: Vec::new(),
            loading_queue: VecDeque::new(),
            being_loaded_queue: VecDeque::new(),
            cached_data: Vec::new(),
            cached_metadata: Vec::new(),
            backend: Box::new(CpuImageCache {}),
            slider_texture: None,
            compression_strategy: CompressionStrategy::None,
        }
    }
}

// Constructor, cached_data getter / setter, and type specific methods
impl ImageCache {
    pub fn new(
        image_paths: &[PathSource],
        cache_count: usize,
        cache_strategy: CacheStrategy,
        compression_strategy: CompressionStrategy,
        initial_index: usize,
        device: Option<Arc<wgpu::Device>>,
        queue: Option<Arc<wgpu::Queue>>,
    ) -> Self {
        let cache_size = cache_count * 2 + 1;
        let mut cached_data = Vec::new();
        let mut cached_metadata = Vec::new();
        for _ in 0..cache_size {
            cached_data.push(None);
            cached_metadata.push(None);
        }

        // Initialize the image cache with the basic structure
        let mut image_cache = ImageCache {
            image_paths: image_paths.to_owned(),
            num_files: image_paths.len(),
            current_index: initial_index,
            current_offset: 0,
            cache_count,
            cached_data,
            cached_metadata,
            cached_image_indices: vec![-1; cache_size],
            cache_states: vec![false; cache_size],
            loading_queue: VecDeque::new(),
            being_loaded_queue: VecDeque::new(),
            slider_texture: None,
            backend: Box::new(CpuImageCache {}), // Temporary CPU backend, will be replaced
            compression_strategy,
        };

        // Initialize the slider texture if using GPU
        if cache_strategy.is_gpu_based() {
            if let Some(device) = device.clone() {
                image_cache.slider_texture = Some(Arc::new(device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("SliderTexture"),
                    size: wgpu::Extent3d {
                        width: 1280, // Fixed 720p resolution
                        height: 720,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                })));
            }
        }

        // Initialize the appropriate backend
        image_cache.init_cache(device, queue, cache_strategy, compression_strategy);

        image_cache
    }

    pub fn _get_cached_data(&self, index: usize) -> Option<&CachedData> {
        self.cached_data.get(index).and_then(|opt| opt.as_ref())
    }

    pub fn set_cached_data(&mut self, index: usize, data: CachedData) {
        if index < self.cached_data.len() {
            self.cached_data[index] = Some(data);
        }
    }

    pub fn set_cached_metadata(&mut self, index: usize, metadata: ImageMetadata) {
        if index < self.cached_metadata.len() {
            self.cached_metadata[index] = Some(metadata);
        }
    }

    pub fn load_image(&self, index: usize, archive_cache: Option<&mut crate::archive_cache::ArchiveCache>) -> Result<CachedData, io::Error> {
        self.backend.load_image(index, &self.image_paths, self.compression_strategy, archive_cache)
    }

    pub fn _load_pos(
        &mut self,
        new_data: Option<CachedData>,
        pos: usize,
        data_index: isize,
        archive_cache: Option<&mut crate::archive_cache::ArchiveCache>,
    ) -> Result<bool, io::Error> {
        //self.backend.load_pos(new_data, pos, data_index)

        self.backend.load_pos(
            new_data,
            pos,
            data_index,
            &mut self.cached_data,
            &mut self.cached_image_indices,
            self.cache_count,
            self.compression_strategy,
            archive_cache,
        )
    }

    pub fn load_initial_images(&mut self, archive_cache: Option<&mut crate::archive_cache::ArchiveCache>) -> Result<(), io::Error> {
        self.backend.load_initial_images(
            &self.image_paths,
            self.cache_count,
            self.current_index,
            &mut self.cached_data,
            &mut self.cached_image_indices,
            &mut self.current_offset,
            self.compression_strategy,
            archive_cache,
        )
    }

    pub fn shift_cache_left(&mut self, new_item: Option<CachedData>, new_metadata: Option<ImageMetadata>) {
        self.cached_data.remove(0);
        self.cached_data.push(new_item);

        // Shift metadata in parallel
        self.cached_metadata.remove(0);
        self.cached_metadata.push(new_metadata);

        // Update indices
        self.cached_image_indices.remove(0);
        if !self.cached_image_indices.is_empty() {
            let next_index = self.cached_image_indices[self.cached_image_indices.len()-1] + 1;
            self.cached_image_indices.push(next_index);
        } else {
            self.cached_image_indices.push(0);
        }

        self.current_offset -= 1;
        debug!("shift_cache_left - current_offset: {}", self.current_offset);
    }

    pub fn init_cache(
        &mut self,
        device: Option<Arc<wgpu::Device>>,
        queue: Option<Arc<wgpu::Queue>>,
        cache_strategy: CacheStrategy,
        compression_strategy: CompressionStrategy,
    ) {
        let backend: Box<dyn ImageCacheBackend> = match cache_strategy {
            CacheStrategy::Cpu => Box::new(CpuImageCache::new()),
            CacheStrategy::Gpu => {
                if let (Some(device), Some(queue)) = (device, queue) {
                    Box::new(GpuImageCache::new(device, queue))
                } else {
                    Box::new(CpuImageCache::new())
                }
            },
        };

        //self.backend = Some(backend);
        self.backend = backend;
        self.compression_strategy = compression_strategy;
    }

    pub fn _set_compression_strategy(&mut self, strategy: CompressionStrategy) {
        self.compression_strategy = strategy;
    }
}

// Methods independent of cache type
impl ImageCache {
    #[allow(dead_code)]
    pub fn print_cache(&self) {
        for (index, image_option) in self.cached_data.iter().enumerate() {
            match image_option {
                Some(image_bytes) => {
                    let image_info = format!("Image {} - Index {} - Size: {} bytes", index, self.cached_image_indices[index], image_bytes.len());
                    debug!("{}", image_info);
                }
                None => {
                    let no_image_info = format!("No image at index {}", index);
                    debug!("{}", no_image_info);
                }
            }
        }
    }

    #[allow(dead_code)]
    pub fn print_cache_index(&self) {
        for (index, cache_index) in self.cached_image_indices.iter().enumerate() {
            let index_info = format!("Index {} - Cache Index: {}", index, cache_index);
            debug!("{}", index_info);
        }
    }

    #[allow(dead_code)]
    pub fn clear_cache(&mut self) {
        // Clear all collections
        self.cached_data.clear();
        self.cached_metadata.clear();
        self.cached_image_indices.clear();
        self.cache_states.clear();
        self.image_paths.clear();
        self.num_files = 0;
        self.current_index = 0;
        self.current_offset = 0;
        self.cache_count = 0;
        self.slider_texture = None;

        // Clear the loading queues
        self.loading_queue.clear();
        self.being_loaded_queue.clear();

        // Reinitialize the cached_data and cached_metadata vectors
        let cache_size = self.cache_count * 2 + 1;
        let mut cached_data = Vec::new();
        let mut cached_metadata = Vec::new();
        for _ in 0..cache_size {
            cached_data.push(None);
            cached_metadata.push(None);
        }
        self.cached_data = cached_data;
        self.cached_metadata = cached_metadata;

        self.cache_states = vec![false; self.image_paths.len()];
    }

    pub fn move_next(&mut self, new_image: Option<CachedData>, new_metadata: Option<ImageMetadata>, _image_index: isize) -> Result<bool, io::Error> {
        if self.current_index < self.image_paths.len() - 1 {

            //shift_cache_left(&mut self.cached_data, &mut self.cached_image_indices, new_image, &mut self.current_offset);
            self.shift_cache_left(new_image, new_metadata);
            Ok(false)
        } else {
            Err(io::Error::other("No more images to display"))
        }
    }

    pub fn move_prev(&mut self, new_image: Option<CachedData>, new_metadata: Option<ImageMetadata>, _image_index: isize) -> Result<bool, io::Error> {
        if self.current_index > 0 {

            //shift_cache_right(&mut self.cached_data, &mut self.cached_image_indices, new_image, &mut self.current_offset);
            self.shift_cache_right(new_image, new_metadata);
            Ok(false)
        } else {
            Err(io::Error::other("No previous images to display"))
        }
    }

    pub fn move_next_edge(&self, _new_image: Option<CachedData>, _image_index: isize) -> Result<bool, io::Error> {
        if self.current_index < self.image_paths.len() - 1 {
            Ok(false)
        } else {
            Err(io::Error::other("No more images to display"))
        }
    }

    pub fn move_prev_edge(&self, _new_image: Option<CachedData>, _image_index: isize) -> Result<bool, io::Error> {
        if self.current_index > 0 {
            Ok(false)
        } else {
            Err(io::Error::other("No previous images to display"))
        }
    }

    pub fn shift_cache_right(
        &mut self, new_item: Option<CachedData>, new_metadata: Option<ImageMetadata>,
    ) {
        // Shift the elements in cached_images to the right
        self.cached_data.pop(); // Remove the last (rightmost) element
        self.cached_data.insert(0, new_item);

        // Shift metadata in parallel
        self.cached_metadata.pop();
        self.cached_metadata.insert(0, new_metadata);

        // Update indices
        self.cached_image_indices.pop();
        let prev_index = self.cached_image_indices[0] - 1;
        self.cached_image_indices.insert(0, prev_index);

        self.current_offset += 1;
        debug!("shift_cache_right - current_offset: {}", self.current_offset);
    }

    pub fn get_initial_image(&self) -> Result<&CachedData, io::Error> {
        let cache_index = (self.cache_count as isize + self.current_offset) as usize;

        if let Some(image_data_option) = self.cached_data.get(cache_index) {
            if let Some(image_data) = image_data_option {
                Ok(image_data)
            } else {
                Err(io::Error::other("Image data is not cached"))
            }
        } else {
            Err(io::Error::other("Invalid cache index"))
        }
    }

    /// Gets the initial image as CPU data, loading from file if necessary
    /// This is useful for slider images which need Vec<u8> data
    pub fn get_initial_image_as_cpu(&self, archive_cache: Option<&mut crate::archive_cache::ArchiveCache>) -> Result<Vec<u8>, io::Error> {
        // First try to get from cache
        match self.get_initial_image() {
            Ok(cached_data) => {
                // If it's already CPU data, return it
                match cached_data.as_vec() {
                    Ok(bytes) => Ok(bytes),
                    Err(_) => {
                        // If it's GPU data, we need to load from file instead
                        let cache_index = (self.cache_count as isize + self.current_offset) as usize;
                        let image_index = self.cached_image_indices[cache_index];

                        if image_index >= 0 && (image_index as usize) < self.image_paths.len() {
                            // Load directly from file
                            let img_path = &self.image_paths[image_index as usize];
                            crate::file_io::read_image_bytes(img_path, archive_cache)

                        } else {
                            Err(io::Error::other("Invalid image index"))
                        }
                    }
                }
            },
            Err(err) => Err(err)
        }
    }


    #[allow(dead_code)]
    pub fn get_current_image(&self) -> Result<&CachedData, io::Error> {
        let cache_index = self.cache_count; // center element of the cache
        debug!("    Current index: {}, Cache index: {}", self.current_index, cache_index);

        // Display information about each image
        /*for (index, image_option) in self.cached_data.iter().enumerate() {
            match image_option {
                Some(image_bytes) => {
                    let image_info = format!("    Image {} - Size: {} bytes", index, image_bytes.len());
                    debug!("{}", image_info);
                }
                None => {
                    let no_image_info = format!("    No image at index {}", index);
                    debug!("{}", no_image_info);
                }
            }
        }*/

        if let Some(image_data_option) = self.cached_data.get(cache_index) {
            if let Some(image_data) = image_data_option {
                Ok(image_data)
            } else {
                Err(io::Error::other("Image data is not cached"))
            }
        } else {
            Err(io::Error::other("Invalid cache index"))
        }
    }

    pub fn get_image_by_index(&self, index: usize) -> Result<&CachedData, io::Error> {
        debug!("current index: {}, cached_images.len(): {}", self.current_index, self.cached_data.len());
        if let Some(image_data_option) = self.cached_data.get(index) {
            if let Some(image_data) = image_data_option {
                Ok(image_data)
            } else {
                Err(io::Error::other("Image data is not cached"))
            }
        } else {
            Err(io::Error::other("Invalid cache index"))
        }
    }

    /// Get metadata for the initial (current) image
    pub fn get_initial_metadata(&self) -> Option<&ImageMetadata> {
        let cache_index = (self.cache_count as isize + self.current_offset) as usize;
        self.cached_metadata.get(cache_index).and_then(|opt| opt.as_ref())
    }

    /// Get metadata by cache index
    pub fn get_metadata_by_index(&self, index: usize) -> Option<&ImageMetadata> {
        self.cached_metadata.get(index).and_then(|opt| opt.as_ref())
    }

    pub fn get_next_cache_index(&self) -> isize {
        self.cache_count as isize + self.current_offset + 1
    }

    #[allow(clippy::let_and_return)]
    pub fn get_next_image_to_load(&self) -> usize {
        let next_image_index = (self.current_index as isize + (self.cache_count as isize -  self.current_offset)) as usize + 1;
        next_image_index
    }

    pub fn get_prev_image_to_load(&self) -> usize {
        let prev_image_index_to_load = (self.current_index as isize + (-(self.cache_count as isize) - self.current_offset)) - 1;
        prev_image_index_to_load as usize
    }

    pub fn is_some_at_index(&self, index: usize) -> bool {
        // Using pattern matching to check if element is None
        if let Some(image_data_option) = self.cached_data.get(index) {
            matches!(image_data_option, Some(_image_data))
        } else {
            false
        }
    }

    pub fn is_cache_index_within_bounds(&self, index: usize) -> bool {
        if !(0..self.cached_data.len()).contains(&index) {
            debug!("is_cache_index_within_bounds - index: {}, cached_images.len(): {}", index, self.cached_data.len());
            return false;
        }
        self.is_some_at_index(index)
    }

    pub fn is_next_cache_index_within_bounds(&self) -> bool {
        let next_image_index_to_render: usize = self.get_next_cache_index() as usize;
        if next_image_index_to_render >= self.image_paths.len() {
            return false;
        }
        self.is_cache_index_within_bounds(next_image_index_to_render)
    }

    pub fn is_prev_cache_index_within_bounds(&self) -> bool {
        let prev_image_index_to_render: isize = self.cache_count as isize + self.current_offset - 1;
        if prev_image_index_to_render < 0 {
            return false;
        }
        debug!("is_prev_cache_index_within_bounds - prev_image_index_to_render: {}", prev_image_index_to_render);
        self.print_cache();
        self.is_cache_index_within_bounds(prev_image_index_to_render as usize)
    }

    pub fn is_image_index_within_bounds(&self, index: isize) -> bool {
        index < 0 && index >= -(self.cache_count as isize) ||
        index >= 0 && index < self.image_paths.len() as isize ||
        index >= self.image_paths.len() as isize && index < self.image_paths.len() as isize + self.cache_count as isize
    }

    pub fn _is_operation_in_queues(&self, operation: LoadOperationType) -> bool {
        debug!("img_cache.loading_queue: {:?}", self.loading_queue);
        debug!("img_cache.being_loaded_queue: {:?}", self.being_loaded_queue);
        self.loading_queue.iter().any(|op| op.operation_type() == operation) ||
        self.being_loaded_queue.iter().any(|op| op.operation_type() == operation)
    }

    pub fn is_operation_blocking(&self, operation: LoadOperationType) -> bool {
        match operation {
            LoadOperationType::LoadNext => {
                if self.current_offset == -(self.cache_count as isize) {
                    return true;
                }
            }
            LoadOperationType::LoadPrevious => {
                if self.current_offset == self.cache_count as isize {
                    return true;
                }
            }
            _ => {}
        }
        false
    }

    /// If there are certain loading operations in the queue and the new loading op would cause bugs, return true
    /// e.g. When current_offset==5 and LoadPrevious op is at the head of the queue(queue.front()),
    /// the new op is LoadNext: this would make current_offset==6 and cache would be out of bounds
    pub fn is_blocking_loading_ops_in_queue(
        &self, loading_operation: LoadOperation, loading_status: &LoadingStatus
    ) -> bool {
        match loading_operation {
            LoadOperation::LoadNext((_cache_index, _target_index)) => {
                if self.current_offset == -(self.cache_count as isize) {
                    return true;
                }
                if self.current_offset == self.cache_count as isize {
                    if loading_status.being_loaded_queue.is_empty() {
                        return false;
                    }

                    if let Some(op) = loading_status.being_loaded_queue.front() {
                        debug!("is_blocking_loading_ops_in_queue - op: {:?}", op);
                        match op {
                            LoadOperation::LoadPrevious((_c_index, _img_index)) => {
                                return true;
                            }
                            LoadOperation::ShiftPrevious((_c_index, _img_index)) => {
                                return true;
                            }
                            _ => {}
                        }
                    }
                }
            }
            LoadOperation::LoadPrevious((_cache_index, _target_index)) => {
                if self.current_offset == self.cache_count as isize {
                    return true;
                }
                if self.current_offset == -(self.cache_count as isize) {
                    if let Some(op) = self.being_loaded_queue.front() {
                        match op {
                            LoadOperation::LoadNext((_c_index, _img_index)) => {
                                return true;
                            }
                            LoadOperation::ShiftNext((_c_index, _img_index)) => {
                                return true;
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
        false
    }

}

#[allow(clippy::too_many_arguments)]
pub fn load_images_by_operation_slider(
    device: &Arc<wgpu::Device>,
    queue: &Arc<wgpu::Queue>,
    cache_strategy: CacheStrategy,
    compression_strategy: CompressionStrategy,
    panes: &mut [pane::Pane],
    pane_index: usize,
    target_indices_and_cache: &[Option<(isize, usize)>],
    operation: LoadOperation
) -> Task<Message> {
    let mut paths = Vec::new();
    let mut archive_caches = Vec::new();

    // Ensure we access the correct pane by the pane_index
    if let Some(pane) = panes.get_mut(pane_index) {
        let img_cache = &mut pane.img_cache;

        // Loop over the target indices and cache positions
        for target in target_indices_and_cache.iter() {
            if let Some((target_index, cache_pos)) = target {
                if let Some(path) = img_cache.image_paths.get(*target_index as usize) {
                    paths.push(Some(path.clone()));
                    if pane.has_compressed_file {
                        archive_caches.push(Some(Arc::clone(&pane.archive_cache)));
                    } else {
                        archive_caches.push(None);
                    }
                    // Store the target image at the specified cache position
                    img_cache.cached_image_indices[*cache_pos] = *target_index;
                } else {
                    paths.push(None);
                    archive_caches.push(None);
                }
            } else {
                paths.push(None);
                archive_caches.push(None);
            }
        }

        // If we have valid paths, proceed to load the images asynchronously
        if !paths.is_empty() {
            let device_clone = Arc::clone(device);
            let queue_clone = Arc::clone(queue);

            // Check if the pane has compressed files and get the archive cache
            let _archive_cache = if pane.has_compressed_file {
                Some(Arc::clone(&pane.archive_cache))
            } else {
                None
            };

            debug!("Task::perform started for {:?}", operation);

            let images_loading_task = async move {
                file_io::load_images_async(
                    paths,
                    cache_strategy,
                    &device_clone,
                    &queue_clone,
                    compression_strategy,
                    operation,
                    archive_caches
                ).await
            };

            Task::perform(images_loading_task, Message::ImagesLoaded)
        } else {
            Task::none()
        }
    } else {
        debug!("Pane not found for pane_index: {}", pane_index);
        Task::none()
    }
}


pub fn load_images_by_indices(
    device: &Arc<wgpu::Device>,
    queue: &Arc<wgpu::Queue>,
    cache_strategy: CacheStrategy,
    compression_strategy: CompressionStrategy,
    panes: &mut Vec<&mut Pane>,
    target_indices: &[Option<isize>],
    operation: LoadOperation
) -> Task<Message> {
    let mut paths = Vec::new();

    let mut archive_caches = Vec::new();

    for (pane_index, pane) in panes.iter_mut().enumerate() {
        let img_cache = &mut pane.img_cache;

        if let Some(target_index) = target_indices[pane_index] {
            if let Some(path) = img_cache.image_paths.get(target_index as usize) {
                paths.push(Some(path.clone()));

                // Add archive cache if this pane has compressed files
                if pane.has_compressed_file {
                    archive_caches.push(Some(Arc::clone(&pane.archive_cache)));
                } else {
                    archive_caches.push(None);
                }
            } else {
                paths.push(None);
                archive_caches.push(None);
            }
        } else {
            paths.push(None);
            archive_caches.push(None);
        }
    }

    if !paths.is_empty() {
        let device_clone = Arc::clone(device);
        let queue_clone = Arc::clone(queue);

        debug!("Task::perform started for {:?}", operation);
        Task::perform(
            async move {
                let result = file_io::load_images_async(
                    paths,
                    cache_strategy,
                    &device_clone,
                    &queue_clone,
                    compression_strategy,
                    operation,
                    archive_caches
                ).await;
                result
            },
            Message::ImagesLoaded,
        )

    } else {
        Task::none()
    }
}


pub fn load_images_by_operation(
    device: &Arc<wgpu::Device>,
    queue: &Arc<wgpu::Queue>,
    cache_strategy: CacheStrategy,
    compression_strategy: CompressionStrategy,
    panes: &mut Vec<&mut Pane>,
    loading_status: &mut LoadingStatus
) -> Task<Message> {
    if !loading_status.loading_queue.is_empty() {
        debug!("load_images_by_operation - loading_status.loading_queue: {:?}", loading_status.loading_queue);
        if let Some(operation) = loading_status.loading_queue.pop_front() {
            loading_status.enqueue_image_being_loaded(operation.clone());
            debug!("load_images_by_operation - loading_status.being_loaded_queue: {:?}", loading_status.being_loaded_queue);
            match operation {
                LoadOperation::LoadNext((ref _pane_indices, ref target_indicies)) => {
                    load_images_by_indices(
                        device,
                        queue,
                        cache_strategy,
                        compression_strategy,
                        panes,
                        target_indicies,
                        operation.clone()
                    )
                }
                LoadOperation::LoadPrevious((ref _pane_indices, ref target_indicies)) => {
                    load_images_by_indices(
                        device,
                        queue,
                        cache_strategy,
                        compression_strategy,
                        panes,
                        target_indicies,
                        operation.clone()
                    )
                }
                LoadOperation::ShiftNext((ref _pane_indices, ref _target_indicies)) => {
                    let empty_async_block = empty_async_block_vec(operation, panes.len());
                    Task::perform(empty_async_block, Message::ImagesLoaded)
                }
                LoadOperation::ShiftPrevious((ref _pane_indices,  ref _target_indicies)) => {
                    let empty_async_block = empty_async_block_vec(operation, panes.len());
                    Task::perform(empty_async_block, Message::ImagesLoaded)
                }
                LoadOperation::LoadPos((ref _pane_indices, _target_indices_and_cache)) => {
                    Task::none()
                }
            }
        } else {
            Task::none()
        }
    } else {
        Task::none()
    }
}

pub fn load_all_images_in_queue(
    device: &Arc<wgpu::Device>,
    queue: &Arc<wgpu::Queue>,
    cache_strategy: CacheStrategy,
    compression_strategy: CompressionStrategy,
    panes: &mut [pane::Pane],
    loading_status: &mut LoadingStatus,
) -> Task<Message> {
    let mut tasks = Vec::new();
    let mut pane_refs: Vec<&mut pane::Pane> = vec![];

    // Collect references to panes
    for pane in panes.iter_mut() {
        pane_refs.push(pane);
    }

    debug!(
        "##load_all_images_in_queue - loading_status.loading_queue: {:?}",
        loading_status.loading_queue
    );
    loading_status.print_queue();

    // Process each operation in the loading queue
    while let Some(operation) = loading_status.loading_queue.pop_front() {
        loading_status.enqueue_image_being_loaded(operation.clone());
        if let LoadOperation::LoadPos((ref pane_index, ref target_indices_and_cache)) = operation {
            let task = load_images_by_operation_slider(
                device,
                queue,
                cache_strategy,
                compression_strategy,
                panes,
                *pane_index,
                target_indices_and_cache,
                operation.clone(),
            );
            tasks.push(task);
        }
    }

    // Return the batch of tasks if any, otherwise return none
    if tasks.is_empty() {
        Task::none()
    } else {
        Task::batch(tasks)
    }
}

