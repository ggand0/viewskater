#[warn(unused_imports)]
#[cfg(target_os = "linux")]
mod other_os {
    //pub use iced;
    pub use iced_custom as iced;
}

#[cfg(not(target_os = "linux"))]
mod macos {
    pub use iced_custom as iced;
}

#[cfg(target_os = "linux")]
use other_os::*;

#[cfg(not(target_os = "linux"))]
use macos::*;

use std::fs;
use std::path::PathBuf;
use std::io;
use std::collections::VecDeque;
use std::sync::Arc;


#[allow(unused_imports)]
use std::time::Instant;

#[allow(unused_imports)]
use log::{debug, info, warn, error};

//use wgpu;
use iced_wgpu::{wgpu, Renderer};

use crate::app::Message;
//use iced::Task;
use iced_winit::runtime::Task;

use crate::file_io::{load_images_async, empty_async_block_vec};
use crate::loading_status::LoadingStatus;
use crate::pane::Pane;   
use crate::pane;


use crate::cache::cpu_img_cache::CpuImageCache;
use crate::cache::gpu_img_cache::GpuImageCache;
use crate::cache::cache_utils::{shift_cache_left, shift_cache_right, load_pos};
use std::path::Path;


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


#[derive(Debug, Clone)]
pub enum CachedData {
    Cpu(Vec<u8>),          // CPU: Raw image bytes
    Gpu(Arc<wgpu::Texture>),    // GPU: Use Arc to allow cloning
}

impl CachedData {
    pub fn take(self) -> Option<Self> {
        Some(self)
    }
}

impl CachedData {
    pub fn len(&self) -> usize {
        match self {
            CachedData::Cpu(data) => data.len(),
            //CachedData::Gpu(_) => 0, // Placeholder for GPU texture size
            CachedData::Gpu(texture) => {
                let width = texture.width();
                let height = texture.height();
                4 * (width as usize) * (height as usize) // 4 bytes per pixel (RGBA8)
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
        }
    }
}

pub trait ImageCacheBackend {
    fn load_image(&self, index: usize, image_paths: &[PathBuf]) -> Result<CachedData, io::Error>;
    fn load_initial_images(
        &mut self,
        image_paths: &[PathBuf],
        cache_count: usize,
        current_index: usize,
        cached_data: &mut Vec<Option<CachedData>>,
        cached_image_indices: &mut Vec<isize>,
        current_offset: &mut isize,
    ) -> Result<(), io::Error>;
    //fn load_pos(&mut self, new_image: Option<CachedData>, pos: usize, image_index: isize) -> Result<bool, io::Error>;
    fn load_pos(
        &mut self,
        new_image: Option<CachedData>,
        pos: usize,
        image_index: isize,
        cached_data: &mut Vec<Option<CachedData>>,
        cached_image_indices: &mut Vec<isize>,
        cache_count: usize,
    ) -> Result<bool, io::Error>;
}


pub struct ImageCache {
    pub image_paths: Vec<PathBuf>,
    pub num_files: usize,
    pub current_index: usize,
    pub current_offset: isize,
    pub cache_count: usize,
    pub cached_image_indices: Vec<isize>,    // Indices of cached images
    pub cache_states: Vec<bool>,            // States of cache validity
    pub loading_queue: VecDeque<LoadOperation>,
    pub being_loaded_queue: VecDeque<LoadOperation>,    // Queue of image indices being loaded
    pub loading_queue_slider: VecDeque<usize>,

    pub cached_data: Vec<Option<CachedData>>, // Caching mechanism
    pub backend: Box<dyn ImageCacheBackend>, // Backend determines caching type
    pub slider_texture: Option<Arc<wgpu::Texture>>,
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
            loading_queue_slider: VecDeque::new(),
            cached_data: Vec::new(),
            backend: Box::new(CpuImageCache {}),
            slider_texture: None,
        }
    }
}

// Constructor, cached_data getter / setter, and type specific methods
impl ImageCache {
    pub fn new(
        image_paths: Vec<PathBuf>,
        cache_count: usize,
        is_gpu_supported: bool,
        initial_inedx: usize,
        device: Option<Arc<wgpu::Device>>,
        queue: Option<Arc<wgpu::Queue>>,
    ) -> Result<Self, io::Error> {
        let device_ref = device.clone();
        let backend: Box<dyn ImageCacheBackend> = if is_gpu_supported {
            if let (Some(device), Some(queue)) = (device, queue) {
                Box::new(GpuImageCache::new(device, queue))

                
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "GPU support enabled but device/queue not provided",
                ));
            }

        } else {
            Box::new(CpuImageCache {})
        };

        let mut cached_data = Vec::new();
        for _ in 0..(cache_count * 2 + 1) {
            cached_data.push(None);
        }


        // 🔹 Create a fixed-size texture for slider previews if using GPU
        let slider_texture = if is_gpu_supported {
            if let Some(device) = device_ref {
                Some(Arc::new(device.create_texture(&wgpu::TextureDescriptor {
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
                })))
            } else {
                None
            }
        } else {
            None
        };

        Ok(ImageCache {
            image_paths: image_paths.clone(),
            num_files: image_paths.len(),
            current_index: initial_inedx,
            current_offset: 0,
            cache_count,
            cached_data: cached_data,
            cached_image_indices: vec![-1; cache_count * 2 + 1],
            cache_states: vec![false; cache_count * 2 + 1],
            loading_queue: VecDeque::new(),
            being_loaded_queue: VecDeque::new(),
            loading_queue_slider: VecDeque::new(),
            backend,
            slider_texture,
        })
    }

    pub fn get_cached_data(&self, index: usize) -> Option<&CachedData> {
        self.cached_data.get(index).and_then(|opt| opt.as_ref())
    }

    pub fn set_cached_data(&mut self, index: usize, data: CachedData) {
        if index < self.cached_data.len() {
            self.cached_data[index] = Some(data);
        }
    }

    pub fn load_image(&self, index: usize) -> Result<CachedData, io::Error> {
        self.backend.load_image(index, &self.image_paths)
    }

    pub fn load_pos(
        &mut self,
        new_data: Option<CachedData>,
        pos: usize,
        data_index: isize,
    ) -> Result<bool, io::Error> {
        //self.backend.load_pos(new_data, pos, data_index)

        self.backend.load_pos(
            new_data,
            pos,
            data_index,
            &mut self.cached_data,
            &mut self.cached_image_indices,
            self.cache_count,
        )
    }

    pub fn load_initial_images(&mut self) -> Result<(), io::Error> {
        self.backend.load_initial_images(
            &self.image_paths,
            self.cache_count,
            self.current_index,
            &mut self.cached_data,
            &mut self.cached_image_indices,
            &mut self.current_offset,
        )
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
        let mut cached_data = Vec::new();
        for _ in 0..(self.cache_count * 2 + 1) {
            cached_data.push(None);
        }
        self.cached_data = cached_data;

        self.cache_states = vec![false; self.image_paths.len()];
    }

    pub fn move_next(&mut self, new_image: Option<CachedData>, _image_index: isize) -> Result<bool, io::Error> {
        if self.current_index < self.image_paths.len() - 1 {
            
            //shift_cache_left(&mut self.cached_data, &mut self.cached_image_indices, new_image, &mut self.current_offset);
            self.shift_cache_left(new_image);
            Ok(false)
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "No more images to display"))
        }
    }

    pub fn move_prev(&mut self, new_image: Option<CachedData>, _image_index: isize) -> Result<bool, io::Error> {
        if self.current_index > 0 {
            
            //shift_cache_right(&mut self.cached_data, &mut self.cached_image_indices, new_image, &mut self.current_offset);
            self.shift_cache_right(new_image);
            Ok(false)
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "No previous images to display"))
        }
    }

    pub fn move_next_edge(&mut self, _new_image: Option<CachedData>, _image_index: isize) -> Result<bool, io::Error> {
        if self.current_index < self.image_paths.len() - 1 {
            Ok(false)
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "No more images to display"))
        }
    }

    pub fn move_prev_edge(&mut self, _new_image: Option<CachedData>, _image_index: isize) -> Result<bool, io::Error> {
        if self.current_index > 0 {
            Ok(false)
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "No previous images to display"))
        }
    }

    pub fn shift_cache_right(
        &mut self, new_item: Option<CachedData>,
    ) {
        // Shift the elements in cached_images to the right
        self.cached_data.pop(); // Remove the last (rightmost) element
        self.cached_data.insert(0, new_item);

        // Update indices
        self.cached_image_indices.pop();
        let prev_index = self.cached_image_indices[0] - 1;
        self.cached_image_indices.insert(0, prev_index);

        self.current_offset += 1;
        debug!("shift_cache_right - current_offset: {}", self.current_offset);
    }

    fn shift_cache_left(&mut self, new_item: Option<CachedData>) {
        self.cached_data.remove(0);
        self.cached_data.push(new_item);

        // Update indices
        self.cached_image_indices.remove(0);
        let next_index = self.cached_image_indices[self.cached_image_indices.len()-1] + 1;
        self.cached_image_indices.push(next_index);

        self.current_offset -= 1;
        debug!("shift_cache_left - current_offset: {}", self.current_offset);
    }

    pub fn get_initial_image(&self) -> Result<&CachedData, io::Error> {
        debug!("get_initial_image - current_index: {}", self.current_index);
        let cache_index = (self.cache_count as isize + self.current_offset) as usize;
        debug!("get_initial_image - cache_index: {}", cache_index);
        debug!("get_initial_image - cached_data.len(): {}", self.cached_data.len());
        
        if let Some(image_data_option) = self.cached_data.get(cache_index) {
            debug!("get_initial_image2");
            if let Some(image_data) = image_data_option {
                Ok(image_data)
            } else {
                Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Image data is not cached",
                ))
            }
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "Invalid cache index",
            ))
        }
    }/**/

    /*pub fn get_initial_image(&self) -> Result<&CachedData, io::Error> {
        debug!("get_initial_image - current_index: {}, cache_count: {}, current_offset: {}", 
            self.current_index, self.cache_count, self.current_offset);
    
        let cache_index = self.cached_image_indices.iter()
            .position(|&idx| idx == self.current_index as isize);
    
        if let Some(cache_index) = cache_index {
            debug!("get_initial_image - Found current_index at cache position: {}", cache_index);
            if let Some(image_data) = &self.cached_data[cache_index] {
                return Ok(image_data);
            } else {
                debug!("get_initial_image - Cache entry found but empty!");
                return Err(io::Error::new(io::ErrorKind::Other, "Image data is not cached"));
            }
        } else {
            debug!("get_initial_image - current_index not found in cached_image_indices!");
            return Err(io::Error::new(io::ErrorKind::Other, "Invalid cache index"));
        }
    }*/
    

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
                Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Image data is not cached",
                ))
            }
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "Invalid cache index",
            ))
        }
    }

    pub fn get_image_by_index(&self, index: usize) -> Result<&CachedData, io::Error> {
        debug!("current index: {}, cached_images.len(): {}", self.current_index, self.cached_data.len());
        if let Some(image_data_option) = self.cached_data.get(index) {
            if let Some(image_data) = image_data_option {
                Ok(image_data)
            } else {
                Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Image data is not cached",
                ))
            }
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "Invalid cache index",
            ))
        }
    }

    pub fn get_next_cache_index(&self) -> isize {
        self.cache_count as isize + self.current_offset + 1
    }

    pub fn get_next_image_to_load(&self) -> usize {
        let next_image_index = (self.current_index as isize + (self.cache_count as isize -  self.current_offset) as isize) as usize + 1;
        next_image_index
    }

    pub fn get_prev_image_to_load(&self) -> usize {
        let prev_image_index_to_load = (self.current_index as isize + (-(self.cache_count as isize) - self.current_offset) as isize) - 1;
        prev_image_index_to_load as usize
    }

    pub fn is_some_at_index(&self, index: usize) -> bool {
        // Using pattern matching to check if element is None
        if let Some(image_data_option) = self.cached_data.get(index) {
            if let Some(_image_data) = image_data_option {
                true
            } else {
                false
            }
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
        self.is_cache_index_within_bounds(next_image_index_to_render as usize)
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
                    if loading_status.being_loaded_queue.len() == 0 {
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


pub fn load_images_by_operation_slider(
    device: &Arc<wgpu::Device>,
    queue: &Arc<wgpu::Queue>,
    is_gpu_supported: bool,
    panes: &mut Vec<pane::Pane>,
    pane_index: usize,
    target_indices_and_cache: Vec<Option<(isize, usize)>>,
    operation: LoadOperation
) -> Task<Message> {
    let mut paths = Vec::new();

    // Ensure we access the correct pane by the pane_index
    if let Some(pane) = panes.get_mut(pane_index) {
        let img_cache = &mut pane.img_cache;

        // Loop over the target indices and cache positions
        for target in target_indices_and_cache.iter() {
            if let Some((target_index, cache_pos)) = target {
                if let Some(path) = img_cache.image_paths.get(*target_index as usize) {
                    if let Some(s) = path.to_str() {
                        paths.push(Some(s.to_string()));
                    } else {
                        paths.push(None);
                    }

                    // Store the target image at the specified cache position
                    img_cache.cached_image_indices[*cache_pos] = *target_index;
                } else {
                    paths.push(None);
                }
            } else {
                paths.push(None);
            }
        }

        // If we have valid paths, proceed to load the images asynchronously
        if !paths.is_empty() {
            let device_clone = Arc::clone(device);
            let queue_clone = Arc::clone(queue);
            debug!("Task::perform started for {:?}", operation.clone());

            let images_loading_task = async move {
                load_images_async(paths, is_gpu_supported, &device_clone, &queue_clone, operation).await
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
    is_gpu_supported: bool,
    panes: &mut Vec<&mut Pane>, 
    target_indices: Vec<Option<isize>>, 
    operation: LoadOperation
) -> Task<Message> {
    let mut paths = Vec::new();

    for (pane_index, pane) in panes.iter_mut().enumerate() {
        let img_cache = &mut pane.img_cache;

        if let Some(target_index) = target_indices[pane_index] {
            if let Some(path) = img_cache.image_paths.get(target_index as usize) {
                if let Some(s) = path.to_str() {
                    paths.push(Some(s.to_string()));
                } else {
                    paths.push(None);
                }
            } else {
                paths.push(None);
            }
        } else {
            paths.push(None);
        }
    }

    /*if !paths.is_empty() {
        debug!("load_images_by_indices - paths: {:?}", paths);
        let device_clone = Arc::clone(device);
        let queue_clone = Arc::clone(queue);
    
        /*let images_loading_task = async move {
            load_images_async(paths, is_gpu_supported, &device_clone, &queue_clone, operation).await
        };
        Task::perform(images_loading_task, Message::ImagesLoaded)*/
        debug!("Task is scheduled for execution");
        Task::perform(
            async move {
                let result = load_images_async(paths, is_gpu_supported, &device_clone, &queue_clone, operation).await;
                debug!("load_images_async actually executed with result: {:?}", result);
                result
            },
            Message::ImagesLoaded,
        )

    } else {
        Task::none()
    }*/

    if !paths.is_empty() {
        //debug!("load_images_by_indices - paths: {:?}", paths);
        let device_clone = Arc::clone(device);
        let queue_clone = Arc::clone(queue);
    
        //debug!("Task::perform about to be scheduled!");
    
        /*Task::perform(
            async move {
                debug!("Inside async move block! load_images_async will be called.");
                let result = load_images_async(paths, is_gpu_supported, &device_clone, &queue_clone, operation).await;
                debug!("load_images_async executed, result: {:?}", result);
                result
            },
            |res| {
                debug!("Task::perform completed, sending Message::ImagesLoaded");
                Message::ImagesLoaded(res)
            },
        )*/
        debug!("Task::perform started for {:?}", operation.clone());
        Task::perform(
            async move {
                let result = load_images_async(paths, is_gpu_supported, &device_clone, &queue_clone, operation).await;
                //debug!("load_images_async executed, result: {:?}", result);
                result
            },
            Message::ImagesLoaded, // Make sure this exactly matches the Message variant
        )
        
    } else {
        //debug!("load_images_by_indices - No paths to load.");
        Task::none()
    }
    

}


pub fn load_images_by_operation(
    device: &Arc<wgpu::Device>,
    queue: &Arc<wgpu::Queue>,
    is_gpu_supported: bool,
    panes: &mut Vec<&mut Pane>, loading_status: &mut LoadingStatus) -> Task<Message> {
    if !loading_status.loading_queue.is_empty() {
        debug!("load_images_by_operation - loading_status.loading_queue: {:?}", loading_status.loading_queue);
        if let Some(operation) = loading_status.loading_queue.pop_front() {
            loading_status.enqueue_image_being_loaded(operation.clone());
            debug!("load_images_by_operation - loading_status.being_loaded_queue: {:?}", loading_status.being_loaded_queue);
            match operation {
                LoadOperation::LoadNext((ref _pane_indices, ref target_indicies)) => {
                    load_images_by_indices(device, queue, is_gpu_supported,
                        panes, target_indicies.clone(), operation)
                }
                LoadOperation::LoadPrevious((ref _pane_indices, ref target_indicies)) => {
                    load_images_by_indices(device, queue, is_gpu_supported,
                        panes, target_indicies.clone(), operation)
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
    is_gpu_supported: bool,
    panes: &mut Vec<pane::Pane>,
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
        match operation {
            LoadOperation::LoadPos((ref pane_index, ref target_indices_and_cache)) => {
                // Handle LoadPos with the new structure of (image_index, cache_pos)
                let task = load_images_by_operation_slider(
                    device,
                    queue,
                    is_gpu_supported,
                    panes,
                    *pane_index,
                    target_indices_and_cache.clone(),
                    operation,
                );
                tasks.push(task);
            }
            _ => {
            }
        }
    }

    // Return the batch of tasks if any, otherwise return none
    if tasks.is_empty() {
        Task::none()
    } else {
        Task::batch(tasks)
    }
}
