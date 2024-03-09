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
    }
}


// Shared state to track completion of image loading tasks
#[derive(Default)]
pub struct ImageLoadState {
    pane1_loaded: bool,
    pane2_loaded: bool,
}


// Define a struct to hold the image paths and the currently displayed image index
#[derive(Default, Clone)]
pub struct ImageCache {
    pub image_paths: Vec<PathBuf>,
    pub num_files: usize,
    pub current_index: usize,
    // pub current_queued_index: isize, // 
    pub cache_count: usize, // Number of images to cache in advance
    cached_images: Vec<Option<Vec<u8>>>, // Changed cached_images to store Option<Vec<u8>> for better handling
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
            cache_count,
            cached_images: vec![None; cache_count * 2 + 1], // Initialize cached_images with None
            loading_queue: VecDeque::new(),
            being_loaded_queue: VecDeque::new(),
            // max_concurrent_loading: 10,
        })
    }

    pub fn print_queue(&self) {
        debug!("loading_queue: {:?}", self.loading_queue);
        debug!("being_loaded_queue: {:?}", self.being_loaded_queue);
    }

    pub fn enqueue_image_load(&mut self, operation: LoadOperation) {
        // Push the operation into the loading queue
        self.loading_queue.push_back(operation);
    }

    pub fn enqueue_image_being_loaded(&mut self, operation: LoadOperation) {
        // Push the index into the being loaded queue
        self.being_loaded_queue.push_back(operation);
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
            debug!("i: {}, cache_index: {}", i, cache_index);
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
        /*for (index, image_option) in self.cached_images.iter().enumerate() {
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
        }*/

        self.num_files = self.image_paths.len();

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
        let cache_index = self.cache_count;
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
    
    pub fn is_index_within_bounds(&self, index: usize) -> bool {
        (0..self.image_paths.len()).contains(&index)
    }

    pub fn is_within_bounds(&self) -> bool {
        (0..self.image_paths.len()).contains(&self.current_index)
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
    }

    fn shift_cache_left(&mut self, new_image: Option<Vec<u8>>) {
        self.cached_images.remove(0);
        self.cached_images.push(new_image);
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
                    2,
                    position,
                ).unwrap();

                img_cache.load_initial_images().unwrap();
                updated_caches.push(img_cache);
            } else {
                let img_cache =  ImageCache::new(
                    Vec::new(),
                    2,
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
            // let command = load_image_by_operation(&mut img_cache);
            let command = load_image_by_operation(img_cache);
            commands.push(command);
            
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