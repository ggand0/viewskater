#[cfg(target_os = "linux")]
mod other_os {
    pub use iced;
    pub use iced_aw;
    pub use iced_widget;
}

#[cfg(not(target_os = "linux"))]
mod macos {
    pub use iced_custom as iced;
    pub use iced_aw_custom as iced_aw;
    pub use iced_widget_custom as iced_widget;
}

#[cfg(target_os = "linux")]
use other_os::*;

#[cfg(not(target_os = "linux"))]
use macos::*;

use std::fs;
use std::path::{Path, PathBuf};
use std::io;
// use tokio::fs::File;
use tokio::io::AsyncReadExt;
// use std::io::Read;
use std::collections::VecDeque;
use std::time::Instant;
use log::{debug, info, warn, error};


use crate::{DataViewer,Message};
use iced::Command;
//use crate::file_io::{async_load_image, empty_async_block, is_file, is_directory, get_file_paths, get_file_index, Error};
use crate::file_io::{async_load_image, empty_async_block};
use crate::pane;

/*#[derive(Debug, Clone)]
pub enum LoadOperation {
    LoadNext(usize),     // Includes the target index
    ShiftNext(usize),
    LoadPrevious(usize), // Includes the target index
    ShiftPrevious(isize),
}*/
#[derive(Debug, Clone)]
pub enum LoadOperation {
    LoadNext((usize, usize)),     // Includes the target index
    ShiftNext((usize, usize)),
    LoadPrevious((usize, usize)), // Includes the target index
    ShiftPrevious((usize, isize)),
}

impl LoadOperation {
    pub fn load_fn(&self) -> Box<dyn FnOnce(&mut ImageCache, Option<Vec<u8>>) -> Result<(), std::io::Error>> {
        match self {
            LoadOperation::LoadNext(..) => Box::new(|cache, new_image| cache.move_next(new_image)),
            LoadOperation::ShiftNext(..) => Box::new(|cache, new_image| cache.move_next(new_image)),
            LoadOperation::LoadPrevious(..) => Box::new(|cache, new_image| cache.move_prev(new_image)),
            LoadOperation::ShiftPrevious(..) => Box::new(|cache, new_image| cache.move_prev(new_image)),
        }
        /*// version that doesn't do anything
        match self {
            LoadOperation::LoadNext(..) => Box::new(|_cache, _new_image| Ok(())),
            LoadOperation::ShiftNext(..) => Box::new(|_cache, _new_image| Ok(())),
            LoadOperation::LoadPrevious(..) => Box::new(|_cache, _new_image| Ok(())),
            LoadOperation::ShiftPrevious(..) => Box::new(|_cache, _new_image| Ok(())),
        }*/
    }
}


// Define a struct to hold the image paths and the currently displayed image index
#[derive(Default, Clone)]
pub struct ImageCache {
    pub image_paths: Vec<PathBuf>,
    pub num_files: usize,
    pub current_index: usize,
    pub current_offset: isize,
    pub current_offset_accumulated: isize,
    // pub current_queued_index: isize, // 
    pub cache_count: usize, // Number of images to cache in advance
    cached_images: Vec<Option<Vec<u8>>>, // Changed cached_images to store Option<Vec<u8>> for better handling
    pub cached_image_indices: Vec<usize>, // Indices of cached images (index of the image_paths array)
    pub cache_states: Vec<bool>, // Cache states
    // pub loading_queue: VecDeque<usize>, // Queue of image indices to load
    pub loading_queue: VecDeque<LoadOperation>,
    pub being_loaded_queue: VecDeque<LoadOperation>, // Queue of image indices being loaded
    // max_concurrent_loading: usize, // Limit concurrent loading tasks
}

impl ImageCache {
    pub fn new(image_paths: Vec<PathBuf>, cache_count: usize, initial_index: usize) -> Result<Self, io::Error> {
        Ok(ImageCache {
            image_paths,
            num_files: 0,
            current_index: initial_index,
            current_offset: 0,
            cache_count,
            cached_images: vec![None; cache_count * 2 + 1], // Initialize cached_images with None
            loading_queue: VecDeque::new(),
            being_loaded_queue: VecDeque::new(),
            // max_concurrent_loading: 10,
            cache_states: Vec::new(),
            cached_image_indices: Vec::new(),
            current_offset_accumulated: 0,
        })
    }

    pub fn print_queue(&self) {
        println!("loading_queue: {:?}", self.loading_queue);
        println!("being_loaded_queue: {:?}", self.being_loaded_queue);
    }

    pub fn print_cache(&self) {
        for (index, image_option) in self.cached_images.iter().enumerate() {
            match image_option {
                Some(image_bytes) => {
                    let image_info = format!("Image {} - Size: {} bytes", index, image_bytes.len());
                    println!("{}", image_info);
                }
                None => {
                    let no_image_info = format!("No image at index {}", index);
                    println!("{}", no_image_info);
                }
            }
        }
    }

    pub fn enqueue_image_load(&mut self, operation: LoadOperation) {
        // Push the operation into the loading queue
        self.loading_queue.push_back(operation);
    }

    pub fn reset_image_load_queue(&mut self) {
        self.loading_queue.clear();
    }

    pub fn enqueue_image_being_loaded(&mut self, operation: LoadOperation) {
        // Push the index into the being loaded queue
        self.being_loaded_queue.push_back(operation);
    }

    pub fn reset_image_being_loaded_queue(&mut self) {
        self.being_loaded_queue.clear();
    }

    pub fn is_next_image_loaded(&self, next_image_index: usize) -> bool {
        self.cache_states[next_image_index]
    }

    pub fn is_next_image_index_in_queue(&self, _cache_index: usize, next_image_index: isize) -> bool {
        let next_index_usize = next_image_index as usize;
        self.loading_queue.iter().all(|op| match op {
            LoadOperation::LoadNext((_c_index, img_index)) => img_index != &next_index_usize,
            LoadOperation::LoadPrevious((_c_index, img_index)) => img_index != &next_index_usize,
            LoadOperation::ShiftNext((_c_index, img_index)) => img_index != &next_index_usize,
            LoadOperation::ShiftPrevious((_c_index, img_index)) => img_index != &next_image_index,
        }) && self.being_loaded_queue.iter().all(|op| match op {
            LoadOperation::LoadNext((_c_index, img_index)) => img_index != &next_index_usize,
            LoadOperation::LoadPrevious((_c_index, img_index)) => img_index != &next_index_usize,
            LoadOperation::ShiftNext((_c_index, img_index)) => img_index != &next_index_usize,
            LoadOperation::ShiftPrevious((_c_index, img_index)) => img_index != &next_image_index,
        })
    }

    fn is_some_at_index(&self, index: usize) -> bool {
        // Using pattern matching to check if element is None
        /*if let Some(_) = self.cached_images.get(index) {
            true
        } else {
            false
        }*/
        if let Some(image_data_option) = self.cached_images.get(index) {
            if let Some(image_data) = image_data_option {
                true
            } else {
                false
            }
        } else {
            false
        }
        
    }

    pub fn is_cache_index_within_bounds(&self, index: usize) -> bool {
        //(0..self.cached_images.len()).contains(&index)
        if !(0..self.cached_images.len()).contains(&index) {
            return false;
        }

        self.is_some_at_index(index)
    }

    pub fn is_next_cache_index_within_bounds(&self) -> bool {
        //let next_image_index_to_render = img_cache.cache_count as isize + img_cache.current_offset + 1;
        //let next_image_index_to_render = self.current_index + self.cache_count + 1;
        let next_image_index_to_render = self.get_next_cache_index();
        assert!(next_image_index_to_render >= 0);
        self.is_cache_index_within_bounds(next_image_index_to_render as usize)
    }

    pub fn is_prev_cache_index_within_bounds(&self) -> bool {
        let prev_image_index_to_render = self.cache_count as isize + self.current_offset - 1;;
        self.is_cache_index_within_bounds(prev_image_index_to_render as usize)
    }

    pub fn is_image_index_within_bounds(&self, index: isize) -> bool {
        index < 0 && index >= -(self.cache_count as isize) ||
        index >= 0 && index < self.image_paths.len() as isize ||
        index >= self.image_paths.len() as isize && index < self.image_paths.len() as isize + self.cache_count as isize
    }

    pub fn is_current_index_within_bounds(&self) -> bool {
        (0..self.image_paths.len()).contains(&self.current_index)
    }

    pub fn get_next_cache_index(&self) -> isize {
        self.cache_count as isize + self.current_offset + 1
    }


    pub fn load_initial_images(&mut self) -> Result<(), io::Error> {
        let _cache_size = self.cache_count * 2 + 1;

        // Calculate the starting index of the cache array
        // let start_index = self.current_index.saturating_sub(self.cache_count);
        // let start_index = self.cache_count.saturating_sub(self.current_index);
        let start_index: isize = self.current_index as isize - self.cache_count as isize;

        // Calculate the ending index of the cache array
        // let end_index = (start_index + cache_size).min(self.image_paths.len());
        // let end_index = start_index + cache_size as isize;
        let end_index: isize = self.current_index as isize + self.cache_count as isize + 1;

        
        // Fill in the cache array with image paths
        for (i, cache_index) in (start_index..end_index).enumerate() {
            println!("i: {}, cache_index: {}", i, cache_index);
            if cache_index < 0 {
                continue;
            }
            if cache_index > self.image_paths.len() as isize - 1 {
                break;
            }
            // cache[i] = file_paths.get(cache_index).cloned();
            let image = self.load_image(cache_index as usize)?;
            self.cached_images[i] = Some(image);
        }

        // Display information about each image
        for (index, image_option) in self.cached_images.iter().enumerate() {
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

        self.num_files = self.image_paths.len();

        // Set the cache states
        self.cache_states = vec![true; self.image_paths.len()];

        Ok(())
    }

    fn load_image(&self, index: usize) -> Result<Vec<u8>, io::Error> {
        if let Some(image_path) = self.image_paths.get(index) {
            fs::read(image_path) // Read the image bytes
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "Invalid image index",
            ))
        }
    }

    /*pub async fn async_load_image(path: &Path) -> Result<Option<Vec<u8>>, std::io::ErrorKind> {
        match tokio::fs::File::open(path).await {
            Ok(mut file) => {
                let mut buffer = Vec::new();
                if file.read_to_end(&mut buffer).await.is_ok() {
                    Ok(Some(buffer))
                } else {
                    Err(std::io::ErrorKind::InvalidData)
                }
            }
            Err(e) => Err(e.kind()),
        }
    }*/
    
    pub fn load_current_image(&mut self) -> Result<&Vec<u8>, io::Error> {
        // let cache_index = self.current_index + self.cache_count;
        let cache_index = self.cache_count;
        debug!(" Current index: {}, Cache index: {}", self.current_index, cache_index);
        if self.cached_images[cache_index].is_none() {
            debug!("Loading image");
            let current_image = self.load_image(self.current_index)?;
            self.cached_images[cache_index] = Some(current_image.clone());
        }
        Ok(self.cached_images[cache_index].as_ref().unwrap())
    }

    pub fn get_current_image(&self) -> Result<&Vec<u8>, io::Error> {
        let cache_index = self.cache_count; // center element of the cache
        debug!("    Current index: {}, Cache index: {}", self.current_index, cache_index);
        // Display information about each image
        /*for (index, image_option) in self.cached_images.iter().enumerate() {
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

        if let Some(image_data_option) = self.cached_images.get(cache_index) {
            if let Some(image_data) = image_data_option {
                Ok(image_data)
            } else {
                //println!()
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

    pub fn get_image_by_index(&self, index: usize) -> Result<&Vec<u8>, io::Error> {
        println!("current index: {}, cached_images.len(): {}", self.current_index, self.cached_images.len());
        if let Some(image_data_option) = self.cached_images.get(index) {
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

    
    pub fn move_next(&mut self, new_image: Option<Vec<u8>> ) -> Result<(), io::Error> {
        if self.current_index < self.image_paths.len() - 1 {
            // Move to the next image
            self.current_index += 1;
            let start_time = Instant::now();
            self.shift_cache_left(new_image);
            let elapsed_time = start_time.elapsed();
            debug!("move_next() & shift_cache_left() Elapsed time: {:?}", elapsed_time);
            Ok(())
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "No more images to display"))
        }
    }

    pub fn move_prev(&mut self, new_image: Option<Vec<u8>>) -> Result<(), io::Error> {
        if self.current_index > 0 {
            self.current_index -= 1;
            self.shift_cache_right(new_image);
            Ok(())
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "No previous images to display"))
        }
    }

    fn shift_cache_right(&mut self, new_image: Option<Vec<u8>>) {
        // Shift the elements in cached_images to the right
        self.cached_images.pop(); // Remove the last (rightmost) element
        self.cached_images.insert(0, new_image);

        
        self.current_offset += 1;
        println!("shift_cache_right - current_offset: {}", self.current_offset);
    }

    fn shift_cache_left(&mut self, new_image: Option<Vec<u8>>) {
        self.cached_images.remove(0);
        self.cached_images.push(new_image);

        //self.current_offset -= 1;
        // If we just decrement the offset, we can't address a case like this,
        // where the next image hasn't been loaded yet. 
        /*
        e.g. next_image_index_to_load: 702, next_image_index_to_render: 8 current_index: 694, current_offset: 2
        Image 0 - Size: 4736 bytes
        Image 1 - Size: 4650 bytes
        Image 2 - Size: 4690 bytes
        Image 3 - Size: 3885 bytes
        Image 4 - Size: 3803 bytes
        Image 5 - Size: 3741 bytes
        Image 6 - Size: 3625 bytes
        Image 7 - Size: 3555 bytes
        No image at index 8
        Image 9 - Size: 3538 bytes
        No image at index 10
        */

        // To address this, introduce a new variable, current_offset_accumulated
        let next_image_index_to_render = self.cache_count as isize + self.current_offset + 1;
        if self.is_some_at_index(next_image_index_to_render as usize) {
            self.current_offset += self.current_offset_accumulated - 1;
        } else {
            self.current_offset_accumulated -= 1;
        }
        
        //println!("shift_cache_left - current_offset: {}", self.current_offset);
        println!("shift_cache_left - current_offset: {}, current_offset_accumulated: {}", self.current_offset, self.current_offset_accumulated);
    }
}

// Helper function to load an image by index
// NOTE: This function returns a command object but does not execute it
pub fn load_image_by_index(img_cache: &mut ImageCache, target_index: usize, operation: LoadOperation) -> Command<<DataViewer as iced::Application>::Message> {
    let path = img_cache.image_paths.get(target_index);
    if let Some(path) = path {
        // debug!("target_index: {}, Loading Path: {}", path.clone().to_string_lossy(), target_index );
        let image_loading_task = async_load_image(path.clone(), operation);
        Command::perform(image_loading_task, Message::ImageLoaded)
    } else {
        Command::none()
    }
}


// for v3 (async multiple panes)
// NOTE: This function returns a command object but does not execute it
pub fn load_image_by_operation(img_cache: &mut ImageCache) -> Command<<DataViewer as iced::Application>::Message> {
    if !img_cache.loading_queue.is_empty() {
        if let Some(operation) = img_cache.loading_queue.pop_front() {
            img_cache.enqueue_image_being_loaded(operation.clone());
            match operation {
                LoadOperation::LoadNext((_cache_index, target_index)) => {
                    //DataViewer::load_image_by_index(img_cache, target_index, operation)
                    load_image_by_index(img_cache, target_index, operation)
                }
                LoadOperation::LoadPrevious((_cache_index, target_index)) => {
                    //DataViewer::load_image_by_index(img_cache, target_index, operation)
                    load_image_by_index(img_cache, target_index, operation)
                }
                LoadOperation::ShiftNext((_cache_index, _target_index)) => {
                    let empty_async_block = empty_async_block(operation);
                    Command::perform(empty_async_block, Message::ImageLoaded)
                }
                LoadOperation::ShiftPrevious((_cache_index, _target_index)) => {
                    let empty_async_block = empty_async_block(operation);
                    Command::perform(empty_async_block, Message::ImageLoaded)
                }
            }
        } else {
            Command::none()
        }
    } else {
        Command::none()
    }
}

pub fn update_pos(panes: &mut Vec<pane::Pane>, pane_index: isize, pos: usize) {
    // v2: multiple panes
    if pane_index == -1 {
        // Update all panes
        let mut updated_caches = Vec::with_capacity(panes.len());
        for (_cache_index, pane) in panes.iter_mut().enumerate() {
            let img_cache = &mut pane.img_cache;

            //if self.dir_loaded[cache_index] {
            if pane.dir_loaded {
                let file_paths = img_cache.image_paths.clone();
                debug!("file_paths.len() {:?}", file_paths.len());

                // NOTE: be careful with the `pos`; if pos is greater than img_cache.image_paths.len(), it will panic
                let position = pos.min(img_cache.image_paths.len() - 1);
                let mut img_cache =  ImageCache::new( // image_cache::ImageCache::new(
                    file_paths,
                    //2,
                    5,
                    position,
                ).unwrap();

                img_cache.load_initial_images().unwrap();
                updated_caches.push(img_cache);
            } else {
                let img_cache =  ImageCache::new(
                    Vec::new(),
                    //2,
                    5,
                    0,
                ).unwrap();
                updated_caches.push(img_cache);
            }
        }

        for (cache_index, new_cache) in updated_caches.into_iter().enumerate() {
            let pane = &mut panes[cache_index];
            debug!("new_cache.current_index: {}", new_cache.current_index);
            //self.img_caches[cache_index] = new_cache;
            pane.img_cache = new_cache;

            /*if self.dir_loaded[cache_index] {
                let loaded_image = self.img_caches[cache_index].get_current_image().unwrap().to_vec();
                self.current_images[cache_index] = iced::widget::image::Handle::from_memory(loaded_image);
            }*/
            if pane.dir_loaded {
                let loaded_image = pane.img_cache.get_current_image().unwrap().to_vec();
                pane.current_image = iced::widget::image::Handle::from_memory(loaded_image);
            }
        }
    } else {
        let pane_index = pane_index as usize;
        let pane = &mut panes[pane_index];
        // let file_paths = self.img_caches[pane_index].image_paths.clone();
        let file_paths = pane.img_cache.image_paths.clone();

        let mut img_cache =  ImageCache::new(
            file_paths,
            2,
            pos,
        ).unwrap();
        img_cache.load_initial_images().unwrap();
        //self.img_caches[pane_index] = img_cache;
        pane.img_cache = img_cache;

        // let loaded_image = self.img_caches[pane_index].get_current_image().unwrap().to_vec();
        let loaded_image = pane.img_cache.get_current_image().unwrap().to_vec();
        // self.current_images[pane_index] = iced::widget::image::Handle::from_memory(loaded_image);
        pane.current_image = iced::widget::image::Handle::from_memory(loaded_image);
    }

}

pub fn move_right_all_new(panes: &mut Vec<pane::Pane>, slider_value: &mut u16, is_slider_dual: bool) -> Command<Message> {
    let mut commands = Vec::new();
    for (cache_index, pane) in panes.iter_mut().enumerate() {
        // Skip panes that are not selected
        if !pane.is_selected {
            continue;
        }

        let img_cache = &mut pane.img_cache;
        //println!("img_cache.image_paths.len() > 0 && img_cache.current_index < img_cache.image_paths.len() - 1: {}", img_cache.image_paths.len() > 0 && img_cache.current_index < img_cache.image_paths.len() - 1);
        
        // If there are images to load and the current index is not the last index
        if img_cache.image_paths.len() > 0 && img_cache.current_index < img_cache.image_paths.len() - 1 {
            let next_image_index_to_load = img_cache.current_index as isize + img_cache.cache_count as isize + img_cache.current_offset + 1;
            assert!(next_image_index_to_load >= 0);
            let next_image_index_to_load_usize = next_image_index_to_load as usize;
            let next_image_index_to_render = img_cache.cache_count as isize + img_cache.current_offset + 1;
            
            println!("RENDERING NEXT: next_image_index_to_load: {}, next_image_index_to_render: {} current_index: {}, current_offset: {}",
                next_image_index_to_load, next_image_index_to_render, img_cache.current_index, img_cache.current_offset);
            //println!("image filename being rendered: {:?}", img_cache.image_paths[next_image_index_to_render]);

            if img_cache.is_next_image_index_in_queue(cache_index, next_image_index_to_load as isize)  &&
            img_cache.is_image_index_within_bounds(next_image_index_to_load) {
                if next_image_index_to_load_usize < img_cache.image_paths.len() {
                    img_cache.enqueue_image_load(LoadOperation::LoadNext((cache_index, next_image_index_to_load_usize)));
                } else {
                    img_cache.enqueue_image_load(LoadOperation::ShiftNext((cache_index, next_image_index_to_load_usize)));
                }
            }
            img_cache.print_cache();
            img_cache.print_queue();
            let command = load_image_by_operation(img_cache);
            commands.push(command);

            // Just load the next one (experimental)
            // Avoid loading around the edges
            if img_cache.current_index as isize + img_cache.current_offset < (img_cache.image_paths.len() - 1) as isize {
                let loaded_image = img_cache.get_image_by_index(next_image_index_to_render as usize).unwrap().to_vec();
                let handle = iced::widget::image::Handle::from_memory(loaded_image.clone());
                pane.current_image = handle;

                img_cache.current_offset += 1;
                //*slider_value = *slider_value + 1;
                if is_slider_dual {
                    //pane.slider_value = pane.img_cache.current_index as u16;
                    pane.slider_value = (pane.img_cache.current_index as isize + pane.img_cache.current_offset) as u16;
                }
            }

        } else {
            commands.push(Command::none())
        }
    }

    // Update master slider when !is_slider_dual
    if !is_slider_dual {
        let min_index = panes.iter().map(|pane| pane.img_cache.current_index as isize + pane.img_cache.current_offset).min().unwrap();
        *slider_value = min_index as u16;
    }
    Command::batch(commands)
}

pub fn move_left_all_new(panes: &mut Vec<pane::Pane>, slider_value: &mut u16, is_slider_dual: bool) -> Command<Message> {
    let mut commands = Vec::new();
    for (cache_index, pane) in panes.iter_mut().enumerate() {
        // Skip panes that are not selected
        if !pane.is_selected {
            continue;
        }

        let img_cache = &mut pane.img_cache;
        if img_cache.current_index > 0 {
            //let next_image_index: isize = img_cache.current_index as isize - img_cache.cache_count as isize - 1;
            //let next_image_index_to_load: isize = img_cache.current_index as isize  - img_cache.cache_count as isize - img_cache.current_offset  as isize  - 1;
            let next_image_index_to_load: isize = img_cache.current_index as isize  - img_cache.cache_count as isize + img_cache.current_offset  as isize  - 1;
            let next_image_index_to_render = img_cache.cache_count as isize + (img_cache.current_offset - 1);
            println!("RENDERING PREV: next_image_index_to_load: {}, next_image_index_to_render: {} current_index: {}, current_offset: {}",
                next_image_index_to_load, next_image_index_to_render, img_cache.current_index, img_cache.current_offset);
            //println!("image filename being rendered: {:?}", img_cache.image_paths[next_image_index_to_render]);

            println!("img_cache.is_image_index_within_bounds(next_image_index_to_load)) {}", img_cache.is_image_index_within_bounds(next_image_index_to_load));
            if img_cache.is_next_image_index_in_queue(cache_index, next_image_index_to_load as isize) &&
                img_cache.is_image_index_within_bounds(next_image_index_to_load) {
                if next_image_index_to_load >= 0 {
                    img_cache.enqueue_image_load(LoadOperation::LoadPrevious((cache_index, next_image_index_to_load as usize)));
                } else {
                    img_cache.enqueue_image_load(LoadOperation::ShiftPrevious((cache_index, next_image_index_to_load)));
                }
            }
            img_cache.print_cache();
            img_cache.print_queue();
            let command = load_image_by_operation(img_cache);
            commands.push(command);

            // Just load the next one (experimental)
            // Avoid loading around the edges
            if img_cache.current_index as isize + img_cache.current_offset > 0 {
                let loaded_image = img_cache.get_image_by_index(next_image_index_to_render as usize).unwrap().to_vec();
                let handle = iced::widget::image::Handle::from_memory(loaded_image.clone());
                pane.current_image = handle;

                img_cache.current_offset -= 1;
                if is_slider_dual {
                    //pane.slider_value = pane.img_cache.current_index as u16;
                    let tmp = (pane.img_cache.current_index as isize + pane.img_cache.current_offset);
                    println!("tmp: {}", tmp);
                    pane.slider_value = tmp as u16;
                }
            }
        } else {
            commands.push(Command::none())
        }
    }

    // Update master slider when !is_slider_dual
    if !is_slider_dual {
        let min_index = panes.iter().map(|pane| pane.img_cache.current_index as isize + pane.img_cache.current_offset).min().unwrap();
        *slider_value = min_index as u16;
    }

    Command::batch(commands)
}

pub fn move_right_index_new(panes: &mut Vec<pane::Pane>, pane_index: usize) -> Command<Message> {
    let pane = &mut panes[pane_index];
    //let img_cache = &mut panes[pane_index].img_cache;
    let img_cache = &mut pane.img_cache;
    if !pane.is_selected {
        return Command::none();
    }
    
    // If there are images to load and the current index is not the last index
    if img_cache.image_paths.len() > 0 && img_cache.current_index < img_cache.image_paths.len() - 1 {
        let next_image_index_to_load = img_cache.current_index as isize + img_cache.cache_count as isize + img_cache.current_offset + 1;
        assert!(next_image_index_to_load >= 0);
        let next_image_index_to_load_usize = next_image_index_to_load as usize;
        let next_image_index_to_render = img_cache.cache_count as isize + img_cache.current_offset + 1;
        println!("RENDERING NEXT: next_image_index_to_load: {}, next_image_index_to_render: {} current_index: {}, current_offset: {}",
            next_image_index_to_load, next_image_index_to_render, img_cache.current_index, img_cache.current_offset);

        if img_cache.is_next_image_index_in_queue(pane_index, next_image_index_to_load as isize)  &&
        img_cache.is_image_index_within_bounds(next_image_index_to_load) {
            if next_image_index_to_load_usize < img_cache.image_paths.len() {
                img_cache.enqueue_image_load(LoadOperation::LoadNext((pane_index, next_image_index_to_load_usize)));
            } else {
                img_cache.enqueue_image_load(LoadOperation::ShiftNext((pane_index, next_image_index_to_load_usize)));
            }
        }
        img_cache.print_cache();
        img_cache.print_queue();
        let command = load_image_by_operation(img_cache);

        // Just load the next one (experimental)
        // Avoid loading around the edges
        if img_cache.current_index as isize + img_cache.current_offset < (img_cache.image_paths.len() - 1) as isize {
            let loaded_image = img_cache.get_image_by_index(next_image_index_to_render as usize).unwrap().to_vec();
            let handle = iced::widget::image::Handle::from_memory(loaded_image.clone());
            pane.current_image = handle;

            img_cache.current_offset += 1;
            pane.slider_value += 1;
        }

        command
    } else {
        Command::none()
    }
}

pub fn move_left_index_new(panes: &mut Vec<pane::Pane>, pane_index: usize) -> Command<Message> {
    let pane = &mut panes[pane_index];
    //let img_cache = &mut panes[pane_index].img_cache;
    let img_cache = &mut pane.img_cache;
    if !pane.is_selected {
        return Command::none();
    }
    if img_cache.current_index > 0 {
        let next_image_index_to_load: isize = img_cache.current_index as isize  - img_cache.cache_count as isize + img_cache.current_offset  as isize  - 1;
        let next_image_index_to_render = img_cache.cache_count as isize + (img_cache.current_offset - 1);
        println!("RENDERING PREV: next_image_index_to_load: {}, next_image_index_to_render: {} current_index: {}, current_offset: {}",
            next_image_index_to_load, next_image_index_to_render, img_cache.current_index, img_cache.current_offset);
        println!("img_cache.is_image_index_within_bounds(next_image_index_to_load)) {}", img_cache.is_image_index_within_bounds(next_image_index_to_load));
        if img_cache.is_next_image_index_in_queue(pane_index, next_image_index_to_load as isize) &&
            img_cache.is_image_index_within_bounds(next_image_index_to_load) {
            if next_image_index_to_load >= 0 {
                img_cache.enqueue_image_load(LoadOperation::LoadPrevious((pane_index, next_image_index_to_load as usize)));
            } else {
                img_cache.enqueue_image_load(LoadOperation::ShiftPrevious((pane_index, next_image_index_to_load)));
            }
        }
        img_cache.print_cache();
        img_cache.print_queue();
        let command = load_image_by_operation(img_cache);

        // Just load the next one (experimental)
        // Avoid loading around the edges
        if img_cache.current_index as isize + img_cache.current_offset > 0 {
            let loaded_image = img_cache.get_image_by_index(next_image_index_to_render as usize).unwrap().to_vec();
            let handle = iced::widget::image::Handle::from_memory(loaded_image.clone());
            pane.current_image = handle;

            img_cache.current_offset -= 1;
            
            /*pane.slider_value -= 1;
            if pane.slider_value < 0 {
                pane.slider_value = 0;
            }*/
            let tmp = (pane.img_cache.current_index as isize + pane.img_cache.current_offset);
            println!("tmp: {}", tmp);
            pane.slider_value = tmp as u16;
            println!("pane_index: {}, slider_value: {}", pane_index, pane.slider_value);
        }

        command
    } else {
        Command::none()
    }
}


pub fn move_right_all(panes: &mut Vec<pane::Pane>) -> Command<Message> {
    // Returns a command object given a reference to the panes.
    // It needs to be a mutable reference as we need to enqueue image load operations into the image cache.

    // 3. Load from cache (async), multiple panes
    let mut commands = Vec::new();
    for (cache_index, pane) in panes.iter_mut().enumerate() {
        // Skip panes that are not selected
        if !pane.is_selected {
            continue;
        }

        let img_cache = &mut pane.img_cache;
        
        // If there are images to load and the current index is not the last index
        if img_cache.image_paths.len() > 0 && img_cache.current_index < img_cache.image_paths.len() - 1 {
                        
            // let next_image_index = img_cache.current_index + 1; // WRONG
            let next_image_index = img_cache.current_index + img_cache.cache_count + 1;
            debug!("NEXT_IMAGE_INDEX: {}", next_image_index);
            debug!("image load state: {:?}", pane.image_load_state);

            if img_cache.is_next_image_index_in_queue(cache_index, next_image_index as isize) {
                if next_image_index >= img_cache.image_paths.len() {
                    // No new images to load, but shift the cache
                    img_cache.enqueue_image_load(LoadOperation::ShiftNext((cache_index, next_image_index)));
                } else {
                    img_cache.enqueue_image_load(LoadOperation::LoadNext((cache_index, next_image_index)));
                }

            }
            img_cache.print_queue();

            
            let command = load_image_by_operation(img_cache);
            commands.push(command);
            //commands.push(Command::none());
            
            // ImageViewer::load_image_by_operation_with_cache(&mut self.img_cache)
        } else {
            commands.push(Command::none())
        }
    }

    Command::batch(commands)
}

pub fn move_left_all(panes: &mut Vec<pane::Pane>) -> Command<Message> {        
    // v3 (multiple panes)
    let mut commands = Vec::new();
    for (cache_index, pane) in panes.iter_mut().enumerate() {
        // Skip panes that are not selected
        if !pane.is_selected {
            continue;
        }

        let img_cache = &mut pane.img_cache;
        // debug!("current_index: {}, global_current_index: {:?}", img_cache.current_index, global_current_index);
        // debug!("cache_index, index_of_max_length_cache: {}, {}", cache_index, index_of_max_length_cache.unwrap());
        if img_cache.current_index <=0 {
            commands.push(Command::none());
            continue;
        }

        if img_cache.image_paths.len() > 0 && img_cache.current_index > 0 {
            // let next_image_index = img_cache.current_index - 1; // WRONG
            let next_image_index: isize = img_cache.current_index as isize - img_cache.cache_count as isize - 1;
            if img_cache.is_next_image_index_in_queue(cache_index, next_image_index) {
                if next_image_index < 0 {
                    // No new images to load but shift the cache
                    img_cache.enqueue_image_load(LoadOperation::ShiftPrevious((cache_index, next_image_index)));
                } else {
                    img_cache.enqueue_image_load(LoadOperation::LoadPrevious((cache_index, next_image_index as usize)));
                }
            }
            img_cache.print_queue();
            let command = load_image_by_operation(img_cache);
            commands.push(command);
        } else {
            commands.push(Command::none())
        }
    }

    Command::batch(commands)

}

pub fn move_left_index(panes: &mut Vec<pane::Pane>, pane_index: usize) -> Command<Message> {
    // NOTE: pane_index == cache_index
    let img_cache = &mut panes[pane_index].img_cache;
    if img_cache.current_index <=0 {
        Command::none()
    } else {
        // let next_image_index = img_cache.current_index - 1; // WRONG
        let next_image_index: isize = img_cache.current_index as isize - img_cache.cache_count as isize - 1;
        if img_cache.is_next_image_index_in_queue(pane_index, next_image_index) {
            if next_image_index < 0 {
                // No new images to load but shift the cache
                img_cache.enqueue_image_load(LoadOperation::ShiftPrevious((pane_index, next_image_index)));
            } else {
                img_cache.enqueue_image_load(LoadOperation::LoadPrevious((pane_index, next_image_index as usize)));
            }
        }
        img_cache.print_queue();
        // let command = load_image_by_operation(&mut img_cache);
        load_image_by_operation(img_cache)
    }
}

pub fn move_right_index(panes: &mut Vec<pane::Pane>, pane_index: usize) -> Command<Message> {
    // NOTE: pane_index == cache_index
    let img_cache = &mut panes[pane_index].img_cache;
    if img_cache.image_paths.len() > 0 && img_cache.current_index < img_cache.image_paths.len() - 1 {
        // let next_image_index = img_cache.current_index - 1; // WRONG
        let next_image_index = img_cache.current_index + img_cache.cache_count + 1;
            debug!("NEXT_IMAGE_INDEX: {}", next_image_index);
            // debug!("image load state: {:?}", self.image_load_state);
           // debug!("image load state: {:?}", self.panes[pane_index].image_load_state);

            if img_cache.is_next_image_index_in_queue(pane_index, next_image_index as isize) {
                if next_image_index >= img_cache.image_paths.len() {
                    // No new images to load, but shift the cache
                    img_cache.enqueue_image_load(LoadOperation::ShiftNext((pane_index, next_image_index)));
                } else {
                    img_cache.enqueue_image_load(LoadOperation::LoadNext((pane_index, next_image_index)));
                }

            }
        img_cache.print_queue();
        // let command = load_image_by_operation(&mut img_cache);
        load_image_by_operation(img_cache)
    } else {
        Command::none()
    }
}