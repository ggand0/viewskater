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
use crate::pane::{self, Pane, get_pane_with_largest_dir_size};
use crate::menu::PaneLayout;


#[derive(Debug, Clone)]
pub enum LoadOperation {
    LoadNext((usize, usize)),     // Includes the target index
    //ShiftNext((usize, usize)),
    ShiftNext((usize, isize)),
    LoadPrevious((usize, usize)), // Includes the target index
    ShiftPrevious((usize, isize)),
    LoadPos((usize, usize, usize)),        // Load an image at a specific position of the cache
    //LoadPosReload((usize, usize, usize)),  // Load an image at a specific position of the cache and then update the current_image handle
}

impl LoadOperation {
    //pub fn load_fn(&self) -> Box<dyn FnOnce(&mut ImageCache, Option<Vec<u8>>) -> Result<(), std::io::Error>> {
    pub fn load_fn(&self) -> Box<dyn FnOnce(&mut ImageCache, Option<Vec<u8>>) -> Result<(bool), std::io::Error>> {
        match self {
            LoadOperation::LoadNext(..) => Box::new(|cache, new_image| cache.move_next(new_image)),
            LoadOperation::ShiftNext(..) => Box::new(|cache, new_image| cache.move_next_edge(new_image)),
            LoadOperation::LoadPrevious(..) => Box::new(|cache, new_image| cache.move_prev(new_image)),
            LoadOperation::ShiftPrevious(..) => Box::new(|cache, new_image| cache.move_prev_edge(new_image)),
            LoadOperation::LoadPos(..) => {
                let pos = match self {
                    LoadOperation::LoadPos((_, _, pos)) => *pos,
                    _ => 0, // Default value if the variant pattern doesn't match
                };
                Box::new(move |cache, new_image| cache.load_pos(new_image, pos))
            },
            /*LoadOperation::LoadPosReload(..) => {
                let pos = match self {
                    LoadOperation::LoadPos((_, _, pos)) => *pos,
                    _ => 0, // Default value if the variant pattern doesn't match
                };
                Box::new(move |cache, new_image| cache.load_pos(new_image, pos))
            },*/
        }
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

    pub fn clear_cache(&mut self) {
        self.cached_images = vec![None; self.cache_count * 2 + 1];
        self.cache_states = vec![false; self.image_paths.len()];
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
            LoadOperation::ShiftNext((_c_index, img_index)) => img_index != &next_image_index,
            LoadOperation::ShiftPrevious((_c_index, img_index)) => img_index != &next_image_index,
            LoadOperation::LoadPos((_c_index, img_index, _pos)) => img_index != &next_index_usize,
        }) && self.being_loaded_queue.iter().all(|op| match op {
            LoadOperation::LoadNext((_c_index, img_index)) => img_index != &next_index_usize,
            LoadOperation::LoadPrevious((_c_index, img_index)) => img_index != &next_index_usize,
            LoadOperation::ShiftNext((_c_index, img_index)) => img_index != &next_image_index,
            LoadOperation::ShiftPrevious((_c_index, img_index)) => img_index != &next_image_index,
            LoadOperation::LoadPos((_c_index, img_index, _pos)) => img_index != &next_index_usize,
        })
    }

    fn is_some_at_index(&self, index: usize) -> bool {
        // Using pattern matching to check if element is None
        if let Some(image_data_option) = self.cached_images.get(index) {
            println!("is_some_at_index - index: {}, cached_images.len(): {}", index, self.cached_images.len());
            if let Some(image_data) = image_data_option {
                println!("is_some_at_index - image_data.len(): {}", image_data.len());
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
            println!("is_cache_index_within_bounds - index: {}, cached_images.len(): {}", index, self.cached_images.len());
            return false;
        }

        self.is_some_at_index(index)
    }

    pub fn is_next_cache_index_within_bounds(&self) -> bool {
        //let next_image_index_to_render = img_cache.cache_count as isize + img_cache.current_offset + 1;
        //let next_image_index_to_render = self.current_index + self.cache_count + 1;
        //self.print_cache();
        
        //self.cache_count as isize + self.current_offset + 1
        let next_image_index_to_render: usize = self.get_next_cache_index() as usize;
        if next_image_index_to_render >= self.image_paths.len() {
            return false;
        }
        assert!(next_image_index_to_render >= 0);
        self.is_cache_index_within_bounds(next_image_index_to_render as usize)
    }

    pub fn is_prev_cache_index_within_bounds(&self) -> bool {
        //self.print_cache();
        let prev_image_index_to_render: isize = self.cache_count as isize + self.current_offset - 1;
        if prev_image_index_to_render < 0 {
            return false;
        }
        println!("is_prev_cache_index_within_bounds - prev_image_index_to_render: {}", prev_image_index_to_render);
        self.print_cache();
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
        println!("self.current_offset_accumulated: {}", self.current_offset_accumulated);
        self.cache_count as isize + self.current_offset + 1
    }


    pub fn load_initial_images(&mut self) -> Result<(), io::Error> {
        let _cache_size = self.cache_count * 2 + 1;

        // Calculate the starting index of the cache array
        // let start_index: isize = self.current_index as isize - self.cache_count as isize
        /*let start_index: isize = if self.current_index <= self.cache_count {
            0
        } else if img_cache.current_index > (img_cache.image_paths.len()-1) - img_cache.cache_count -1 {
            (img_cache.image_paths.len()-1) - img_cache.cache_count -1
        } else {
            self.current_index as isize - self.cache_count as isize
        };*/
        let start_index: isize;
        let end_index: isize;
        if self.current_index <= self.cache_count {
            start_index = 0;
            end_index = (self.cache_count * 2 + 1) as isize;
            self.current_offset = -(self.cache_count as isize - self.current_index as isize);
        } else if self.current_index > (self.image_paths.len()-1) - self.cache_count {
            //start_index = (self.image_paths.len()-1) as isize - self.cache_count as isize ;
            start_index = self.image_paths.len() as isize - self.cache_count as isize * 2 - 1;
            end_index = (self.image_paths.len()) as isize;
            self.current_offset = self.cache_count  as isize - ((self.image_paths.len()-1) as isize - self.current_index as isize);
        } else {
            start_index = self.current_index as isize - self.cache_count as isize;
            end_index = self.current_index as isize + self.cache_count as isize + 1;
        }
        println!("start_index: {}, end_index: {}, current_offset: {}", start_index, end_index, self.current_offset);

        // Calculate the ending index of the cache array
        // let end_index = (start_index + cache_size).min(self.image_paths.len());
        // let end_index = start_index + cache_size as isize;
        ////let end_index: isize = self.current_index as isize + self.cache_count as isize + 1;

        
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

    pub fn get_initial_image(&self) -> Result<&Vec<u8>, io::Error> {
        let cache_index = (self.cache_count as isize + self.current_offset) as usize;
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

    pub fn move_next(&mut self, new_image: Option<Vec<u8>> ) -> Result<(bool), io::Error> {
        if self.current_index < self.image_paths.len() - 1 {
            // Move to the next image
            ////self.current_index += 1;
            self.shift_cache_left(new_image);
            Ok((false))
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "No more images to display"))
        }
    }

    pub fn move_next_edge(&mut self, new_image: Option<Vec<u8>>) -> Result<(bool), io::Error> {
        if self.current_index < self.image_paths.len() - 1 {
            // v1
            /*
            self.shift_cache_left(new_image);
            self.current_index += 1;
            // Since no more images will be loaded, update the current offset with the accumulated offset
            self.current_offset += self.current_offset_accumulated;
            self.current_offset_accumulated = 0;
            */

            // v2
            //self.current_offset += 1;
            //self.current_index += 1;
            println!("move_next_edge - current_index: {}, current_offset: {}", self.current_index, self.current_offset);
            Ok((false))
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "No more images to display"))
        }
    }

    pub fn move_prev(&mut self, new_image: Option<Vec<u8>>) -> Result<(bool), io::Error> {
        if self.current_index > 0 {
            //self.current_index -= 1; // shuold this be after the cache shift?
            self.shift_cache_right(new_image);
            ////self.current_index -= 1;
            Ok((false))
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "No previous images to display"))
        }
    }

    pub fn move_prev_edge(&mut self, new_image: Option<Vec<u8>>) -> Result<(bool), io::Error> {
        if self.current_index > 0 {
            // v1
            /*self.shift_cache_right(new_image);
            self.current_index -= 1;
            // Since no more images will be loaded, update the current offset with the accumulated offset
            self.current_offset += self.current_offset_accumulated;
            self.current_offset_accumulated = 0;*/

            // v2
            //self.current_offset -= 1;
            //self.current_index -= 1;

            println!("move_prev_edge - current_index: {}, current_offset: {}", self.current_index, self.current_offset);
            Ok((false))
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "No previous images to display"))
        }
    }

    fn shift_cache_right(&mut self, new_image: Option<Vec<u8>>) {
        // Shift the elements in cached_images to the right
        self.cached_images.pop(); // Remove the last (rightmost) element
        self.cached_images.insert(0, new_image);

        self.current_offset += 1;
        /*let prev_image_index_to_load = self.cache_count as isize - self.current_offset as isize + self.current_offset_accumulated - 1;
        if self.is_some_at_index(prev_image_index_to_load as usize) {
            self.current_offset += self.current_offset_accumulated + 1;
            self.current_offset_accumulated = 0; // need to evaluate if this is needed later
        } else {
            self.current_offset_accumulated += 1;
        }*/
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

        self.current_offset -= 1;
        /*
        // To address this, introduce a new variable, current_offset_accumulated
        //let next_image_index_to_render = self.cache_count as isize + self.current_offset + 1;
        let next_image_index_to_render = self.cache_count as isize + self.current_offset + self.current_offset_accumulated + 1;
        if self.is_some_at_index(next_image_index_to_render as usize) {
            self.current_offset += self.current_offset_accumulated - 1;
            self.current_offset_accumulated = 0; // need to evaluate if this is needed later
        } else {
            self.current_offset_accumulated -= 1;
        }*/
        
        //println!("shift_cache_left - current_offset: {}", self.current_offset);
        println!("shift_cache_left - current_offset: {}, current_offset_accumulated: {}", self.current_offset, self.current_offset_accumulated);
    }

    fn load_pos(&mut self, new_image: Option<Vec<u8>>, pos: usize) -> Result<(bool), io::Error> {
        // If `pos` is at the center of the cache return true to reload the current_image
        self.cached_images[pos] = new_image;
        self.print_cache();
        //Ok(())

        if pos == self.cache_count {
            Ok(true)
        } else {
            Ok(false)
        }
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
                LoadOperation::LoadPos((_cache_index, target_index, pos)) => {
                    load_image_by_index(img_cache, target_index, operation)
                }
            }
        } else {
            Command::none()
        }
    } else {
        Command::none()
    }
}

//pub fn load_all_images_in_queue(img_cache: &mut ImageCache) -> Command<<DataViewer as iced::Application>::Message> {
pub fn load_all_images_in_queue(img_cache: &mut ImageCache) -> Vec<Command<<DataViewer as iced::Application>::Message>>{
    if !img_cache.loading_queue.is_empty() {
        //let mut command = Command::none();
        let mut commands = Vec::new();
        for _ in 0..img_cache.loading_queue.len() {
            //command = command.then(load_image_by_operation(img_cache));
            commands.push(load_image_by_operation(img_cache));
        }
        //Command::batch(commands)
        commands
    } else {
        vec![Command::none()]
    }
}


fn get_loading_commands_slider(img_cache: &mut ImageCache, pane_index: usize, pos: usize) -> Vec<Command<<DataViewer as iced::Application>::Message>> {
    let mut commands = Vec::new();
    let cache_index = pane_index;
    
    if pos < img_cache.cache_count {
        let last_index = img_cache.cache_count*2 + 1;
        for i in 0..last_index {
            let target_cache_index = i;
            let image_index = i;
            img_cache.enqueue_image_load(
                LoadOperation::LoadPos((
                    cache_index, image_index, target_cache_index)));
        }
        img_cache.print_queue();
        let local_commands = load_all_images_in_queue(img_cache);
        commands.extend(local_commands);
        println!("load_remaining_images - current_offset: {}", img_cache.current_offset);
    } else if pos >= img_cache.image_paths.len() - img_cache.cache_count {
        let last_index = img_cache.cache_count*2 + 1;
        //let last_index = img_cache.cache_count*2 + 1 + 1;
        
        for i in 0..last_index {
            let target_cache_index = i;
            //let image_index = img_cache.image_paths.len() - last_index + i;
            let image_index = img_cache.image_paths.len() - last_index + i;
            println!("target_cache_index: {}, image_index: {}", target_cache_index, image_index);
            img_cache.enqueue_image_load(
                LoadOperation::LoadPos((
                    cache_index, image_index, target_cache_index)));
        }
        img_cache.print_queue();
        let local_commands = load_all_images_in_queue(img_cache);
        commands.extend(local_commands);

    } else if pos >= img_cache.image_paths.len() {
        let last_index = img_cache.image_paths.len() - 1;
        let last_pos = last_index - img_cache.cache_count;
        println!("pane_index: {}, load_remaining_images - out of bounds: pos: {}, last_pos: {}", pane_index, pos, last_pos);

        // Since it missed the last image, load the last imaeg into the current_index
        img_cache.enqueue_image_load(
            LoadOperation::LoadPos((
                cache_index, last_index, img_cache.cache_count)));
        img_cache.current_index = last_index;
        img_cache.current_offset = 0;
        img_cache.current_offset_accumulated = 0;

        if img_cache.image_paths.len() > img_cache.cache_count {
            // Load the last N images into the cache
            for i in 0..img_cache.cache_count {
                let target_cache_index = i;
                //let target_cache_index = img_cache.cache_count + i;
                let image_index = last_pos + i;
                img_cache.enqueue_image_load(
                    LoadOperation::LoadPos((
                        cache_index, image_index, target_cache_index)));
            }
        } else {
            // Load all images into the cache
            let start_index = img_cache.cache_count - img_cache.image_paths.len();
            for i in 0..img_cache.image_paths.len() {
                let target_cache_index = start_index + i;
                let image_index = i;
                img_cache.enqueue_image_load(
                    LoadOperation::LoadPos((
                        cache_index, image_index, target_cache_index)));
            }
        }
        
        println!("load_remaining_images - out of bounds: pos: {}, last_pos: {}", pos, last_pos);
        img_cache.print_queue();
        let local_commands = load_all_images_in_queue(img_cache);
        commands.extend(local_commands);
        
    } else {
        let center_index = img_cache.cache_count;
        for i in 0..img_cache.cache_count {
            let next_cache_index = center_index + i + 1;
            let prev_cache_index = center_index- i - 1;
            let next_image_index = pos + i + 1;
            let prev_image_index = pos as isize - i as isize - 1;
            println!("next_image_index: {}, prev_image_index: {}", next_image_index, prev_image_index);

            // Load images into cache indices with LoadPos
            if next_image_index < img_cache.image_paths.len() {
                img_cache.enqueue_image_load(
                    LoadOperation::LoadPos((
                        cache_index, next_image_index, next_cache_index)));
            }
            if prev_image_index >= 0 {
                img_cache.enqueue_image_load(
                    LoadOperation::LoadPos((
                        cache_index, prev_image_index as usize, prev_cache_index)));
            }
        }
        img_cache.print_queue();

        // Load the images in the loading queue
        let local_commands = load_all_images_in_queue(img_cache);
        commands.extend(local_commands);
    }

    commands
}

pub fn load_remaining_images(panes: &mut Vec<pane::Pane>, pane_index: isize, pos: usize) -> Command<<DataViewer as iced::Application>::Message> {
    // Load the rest of the images within the cache window asynchronously
    // Called from Message::SliderReleased

    // Since we've moved to a completely new position, clear the loading queues
    for (_cache_index, pane) in panes.iter_mut().enumerate() {
        let img_cache = &mut pane.img_cache;
        img_cache.reset_image_load_queue();
        img_cache.reset_image_being_loaded_queue();
    }

    if pane_index == -1 {
        // Perform dynamic loading:
        // Load the image at pos (center) synchronously,
        // and then load the rest of the images within the cache window asynchronously
        let mut commands = Vec::new();
        for (cache_index, pane) in panes.iter_mut().enumerate() {
            let img_cache = &mut pane.img_cache;

            if pane.dir_loaded {
                println!("load_remaining_images - cache_count: {}", img_cache.cache_count);

                // If the slider moved fast and current_index is out of bounds, load the last N images for that pane
                // NOTE: it needs to be like this:
                /*
                Image 0 - Size: 515028 bytes
                Image 1 - Size: 114875 bytes
                Image 2 - Size: 71109 bytes
                Image 3 - Size: 60409 bytes
                Image 4 - Size: 87190 bytes
                Image 5 - Size: 97990 bytes
                No image at index 6
                No image at index 7
                No image at index 8
                No image at index 9
                No image at index 10
                */

                
                /*if pos < img_cache.cache_count {
                    target_index = pos;
                    img_cache.current_offset = -(img_cache.cache_count as isize - pos as isize);
                } else if pos > img_cache.image_paths.len() - img_cache.cache_count {
                    //target_index = img_cache.image_paths.len() - pos;
                    target_index = img_cache.cache_count as isize - ((img_cache.image_paths.len()-1) as isize - pos as isize);
                    img_cache.current_offset = img_cache.cache_count as isize - ((img_cache.image_paths.len()-1) as isize - pos as isize);
                } else {
                    target_index = img_cache.cache_count;
                    img_cache.current_offset = 0;
                }
                */
            
                if pos < img_cache.cache_count {
                    let last_index = img_cache.cache_count*2 + 1;
                    for i in 0..last_index {
                        let target_cache_index = i;
                        let image_index = i;
                        img_cache.enqueue_image_load(
                            LoadOperation::LoadPos((
                                cache_index, image_index, target_cache_index)));
                    }
                    img_cache.print_queue();
                    let local_commands = load_all_images_in_queue(img_cache);
                    commands.extend(local_commands);
                    println!("load_remaining_images - current_offset: {}", img_cache.current_offset);
                } else if pos >= img_cache.image_paths.len() - img_cache.cache_count {
                    let last_index = img_cache.cache_count*2 + 1;
                    //let last_index = img_cache.cache_count*2 + 1 + 1;
                    
                    for i in 0..last_index {
                        let target_cache_index = i;
                        //let image_index = img_cache.image_paths.len() - last_index + i;
                        let image_index = img_cache.image_paths.len() - last_index + i;
                        println!("target_cache_index: {}, image_index: {}", target_cache_index, image_index);
                        img_cache.enqueue_image_load(
                            LoadOperation::LoadPos((
                                cache_index, image_index, target_cache_index)));
                    }
                    img_cache.print_queue();
                    let local_commands = load_all_images_in_queue(img_cache);
                    commands.extend(local_commands);

                } else if pos >= img_cache.image_paths.len() {
                    let last_index = img_cache.image_paths.len() - 1;
                    let last_pos = last_index - img_cache.cache_count;
                    println!("pane_index: {}, load_remaining_images - out of bounds: pos: {}, last_pos: {}", pane_index, pos, last_pos);

                    // Since it missed the last image, load the last imaeg into the current_index
                    img_cache.enqueue_image_load(
                        LoadOperation::LoadPos((
                            cache_index, last_index, img_cache.cache_count)));
                    img_cache.current_index = last_index;
                    img_cache.current_offset = 0;
                    img_cache.current_offset_accumulated = 0;

                    if img_cache.image_paths.len() > img_cache.cache_count {
                        // Load the last N images into the cache
                        for i in 0..img_cache.cache_count {
                            let target_cache_index = i;
                            //let target_cache_index = img_cache.cache_count + i;
                            let image_index = last_pos + i;
                            img_cache.enqueue_image_load(
                                LoadOperation::LoadPos((
                                    cache_index, image_index, target_cache_index)));
                        }
                    } else {
                        // Load all images into the cache
                        let start_index = img_cache.cache_count - img_cache.image_paths.len();
                        for i in 0..img_cache.image_paths.len() {
                            let target_cache_index = start_index + i;
                            let image_index = i;
                            img_cache.enqueue_image_load(
                                LoadOperation::LoadPos((
                                    cache_index, image_index, target_cache_index)));
                        }
                    }
                    
                    println!("load_remaining_images - out of bounds: pos: {}, last_pos: {}", pos, last_pos);
                    img_cache.print_queue();
                    let local_commands = load_all_images_in_queue(img_cache);
                    commands.extend(local_commands);
                    
                } else {
                    let center_index = img_cache.cache_count;
                    for i in 0..img_cache.cache_count {
                        let next_cache_index = center_index + i + 1;
                        let prev_cache_index = center_index- i - 1;
                        let next_image_index = pos + i + 1;
                        let prev_image_index = pos as isize - i as isize - 1;
                        println!("next_image_index: {}, prev_image_index: {}", next_image_index, prev_image_index);

                        // Load images into cache indices with LoadPos
                        if next_image_index < img_cache.image_paths.len() {
                            img_cache.enqueue_image_load(
                                LoadOperation::LoadPos((
                                    cache_index, next_image_index, next_cache_index)));
                        }
                        if prev_image_index >= 0 {
                            img_cache.enqueue_image_load(
                                LoadOperation::LoadPos((
                                    cache_index, prev_image_index as usize, prev_cache_index)));
                        }
                    }
                    img_cache.print_queue();

                    // Load the images in the loading queue
                    let local_commands = load_all_images_in_queue(img_cache);
                    commands.extend(local_commands);
                }
                /**/

                //let local_commands = get_loading_commands_slider(img_cache, cache_index, pos);
                //commands.extend(local_commands);
            } else {
                commands.push(Command::none());
            }
        }
        Command::batch(commands)
    } else{
        let mut commands = Vec::new();
        let pane = &mut panes[pane_index as usize];
        let img_cache = &mut pane.img_cache;

        if pane.dir_loaded {
            /*let center_index = img_cache.cache_count;
            for i in 0..img_cache.cache_count {
                let next_cache_index = center_index + i + 1;
                let prev_cache_index = center_index- i - 1;
                let next_image_index = pos + i + 1;
                let prev_image_index = pos as isize - i as isize - 1;

                // Load images into cache indices with LoadPos
                if next_cache_index < img_cache.image_paths.len() {
                    img_cache.enqueue_image_load(LoadOperation::LoadPos((pane_index as usize, next_image_index, next_cache_index)));
                }
                if prev_image_index >= 0 {
                    img_cache.enqueue_image_load(LoadOperation::LoadPos((pane_index as usize, prev_image_index as usize, prev_cache_index)));
                }
                
            }
            img_cache.print_queue();

            // Load the images in the loading queue
            let local_commands = load_all_images_in_queue(img_cache);
            commands.extend(local_commands);*/

            let local_commands = get_loading_commands_slider(img_cache, pane_index as usize, pos);
            commands.extend(local_commands);
        } else {
            commands.push(Command::none());
        }
        Command::batch(commands)
    }
}

fn load_current_slider_image(pane: &mut pane::Pane, pos: usize ) -> Result<(), io::Error> {
    /*let img_cache = &mut pane.img_cache;
    let image = img_cache.load_current_image()?;
    pane.current_image = iced::widget::image::Handle::from_memory(image.to_vec());
    Ok(())*/

    // Load the image at pos synchronously into the center position of cache
    //let image = img_cache.load_image(pos as usize)?;
    let img_cache = &mut pane.img_cache;
    match img_cache.load_image(pos as usize) {
        Ok(image) => {
            // Handle successful image loading
            //let center_index = img_cache.cache_count;
            //img_cache.cached_images[center_index] = Some(image);

            let target_index: usize;
            if pos < img_cache.cache_count {
                target_index = pos;
                img_cache.current_offset = -(img_cache.cache_count as isize - pos as isize);
            } else if pos >= img_cache.image_paths.len() - img_cache.cache_count {
                //target_index = img_cache.image_paths.len() - pos;
                target_index = img_cache.cache_count + (img_cache.cache_count as isize - ((img_cache.image_paths.len()-1) as isize - pos as isize)) as usize;
                img_cache.current_offset = img_cache.cache_count as isize - ((img_cache.image_paths.len()-1) as isize - pos as isize);
            } else {
                target_index = img_cache.cache_count;
                img_cache.current_offset = 0;
            }
            img_cache.cached_images[target_index] = Some(image);

            img_cache.current_index = pos;
            //img_cache.current_offset = 0;
            img_cache.current_offset_accumulated = 0;
            let loaded_image = img_cache.get_initial_image().unwrap().to_vec();
            pane.current_image = iced::widget::image::Handle::from_memory(loaded_image);

            Ok(())
        }
        Err(err) => {
            // Handle error
            //println!("update_pos(): Error loading image: {}", err);
            Err(err)
        }
    }
}

pub fn update_pos(panes: &mut Vec<pane::Pane>, pane_index: isize, pos: usize) -> Command<<DataViewer as iced::Application>::Message> {
    // Since we're moving to a completely new position, clear the loading queues
    for (_cache_index, pane) in panes.iter_mut().enumerate() {
        let img_cache = &mut pane.img_cache;
        img_cache.reset_image_load_queue();
        img_cache.reset_image_being_loaded_queue();
    }

    if pane_index == -1 {
        // Perform dynamic loading:
        // Load the image at pos (center) synchronously,
        // and then load the rest of the images within the cache window asynchronously
        let mut commands = Vec::new();
        for (cache_index, pane) in panes.iter_mut().enumerate() {
            //let img_cache = &mut pane.img_cache;

            if pane.dir_loaded {
                match load_current_slider_image(pane, pos) {
                    Ok(()) => {
                        // Handle success
                        println!("update_pos - Image loaded successfully for pane {}", cache_index);
                    }
                    Err(err) => {
                        // Handle error by logging
                        println!("update_pos - Error loading image for pane {}: {}", cache_index, err);
                    }
                }
            } else {
                commands.push(Command::none());
            }
        }
        Command::batch(commands)

    } else {
        let pane_index = pane_index as usize;
        let pane = &mut panes[pane_index];
        let img_cache = &mut pane.img_cache;

        if pane.dir_loaded {
            match load_current_slider_image(pane, pos) {
                Ok(()) => {
                    // Handle success
                    println!("update_pos - Image loaded successfully for pane {}", pane_index);
                }
                Err(err) => {
                    // Handle error by logging
                    println!("update_pos - Error loading image for pane {}: {}", pane_index, err);
                }
            }
        }

        Command::none()
    }
}

fn is_pane_cached_next(pane: pane::Pane, index: usize, is_slider_dual: bool) -> bool {
    pane.is_selected && pane.dir_loaded && pane.img_cache.is_next_cache_index_within_bounds() &&
        pane.img_cache.loading_queue.len() < 3 && pane.img_cache.being_loaded_queue.len() < 3
}

fn is_pane_cached_prev(pane: pane::Pane, index: usize, is_slider_dual: bool) -> bool {
    println!("pane.is_selected: {}, pane.dir_loaded: {}, pane.img_cache.is_prev_cache_index_within_bounds(): {}, pane.img_cache.loading_queue.len(): {}, pane.img_cache.being_loaded_queue.len(): {}",
        pane.is_selected, pane.dir_loaded, pane.img_cache.is_prev_cache_index_within_bounds(), pane.img_cache.loading_queue.len(), pane.img_cache.being_loaded_queue.len());
    pane.is_selected && pane.dir_loaded && pane.img_cache.is_prev_cache_index_within_bounds() &&
        pane.img_cache.loading_queue.len() < 3 && pane.img_cache.being_loaded_queue.len() < 3
}

pub fn move_right_all(panes: &mut Vec<pane::Pane>, slider_value: &mut u16, pane_layout: &PaneLayout, is_slider_dual: bool) -> Command<Message> {
    let mut commands = Vec::new();
    for (cache_index, pane) in panes.iter_mut().enumerate() {
        println!("move_right_all_new - cache_index: {}, is_pane_cached_next: {}", cache_index, is_pane_cached_next(pane.clone(), cache_index, is_slider_dual));
        println!("current_index: {}, current_offset, current_offset_accumulated: {}, {}", pane.img_cache.current_index, pane.img_cache.current_offset, pane.img_cache.current_offset_accumulated);
        pane.img_cache.print_cache();
        pane.img_cache.print_queue();
        if !is_pane_cached_next(pane.clone(), cache_index, is_slider_dual) {
            continue;
        }
        let img_cache = &mut pane.img_cache;

        // If there are images to load and the current index is not the last index
        if img_cache.image_paths.len() > 0 && img_cache.current_index < img_cache.image_paths.len() - 1 {
            let next_image_index_to_load = img_cache.current_index as isize + img_cache.cache_count as isize + 1;
            assert!(next_image_index_to_load >= 0);
            let next_image_index_to_load_usize = next_image_index_to_load as usize;

            println!("LOADING NEXT: next_image_index_to_load: {}, current_index: {}, current_offset: {}",
                next_image_index_to_load, img_cache.current_index, img_cache.current_offset);

            if img_cache.is_image_index_within_bounds(next_image_index_to_load) {
                // TODO: organize this better
                if next_image_index_to_load_usize < img_cache.image_paths.len() &&
                ( img_cache.current_index >= img_cache.cache_count &&
                img_cache.current_index <= (img_cache.image_paths.len()-1) - img_cache.cache_count) {
                    img_cache.enqueue_image_load(LoadOperation::LoadNext((cache_index, next_image_index_to_load_usize)));
                } else if img_cache.current_index < img_cache.cache_count {
                    let prev_image_index_to_load = img_cache.current_index as isize - img_cache.cache_count as isize + 1;
                    img_cache.enqueue_image_load(LoadOperation::ShiftNext((cache_index, prev_image_index_to_load)));
                } else {
                    img_cache.enqueue_image_load(LoadOperation::ShiftNext((cache_index, next_image_index_to_load)));
                }
            }
            img_cache.print_queue();

            let command = load_image_by_operation(img_cache);
            commands.push(command);
        } else {
            commands.push(Command::none())
        }

        // Render the next one right away
        // Avoid loading around the edges
        //if img_cache.current_index as isize + img_cache.current_offset < (img_cache.image_paths.len() - 1) as isize {
        if img_cache.is_some_at_index(img_cache.cache_count as usize + img_cache.current_offset as usize) {
            let next_image_index_to_render = img_cache.cache_count as isize + img_cache.current_offset + 1;
            println!("RENDERING NEXT: next_image_index_to_render: {} current_index: {}, current_offset: {}",
                next_image_index_to_render, img_cache.current_index, img_cache.current_offset);

            let loaded_image = img_cache.get_image_by_index(next_image_index_to_render as usize).unwrap().to_vec();
            let handle = iced::widget::image::Handle::from_memory(loaded_image.clone());
            pane.current_image = handle;

            img_cache.current_offset += 1;

            // NEW: handle current_index here without performing LoadingOperation::ShiftPrevious
            println!("(img_cache.image_paths.len()-1) - img_cache.cache_count -1 = {}", (img_cache.image_paths.len()-1) - img_cache.cache_count -1);
            if img_cache.current_index < img_cache.image_paths.len() - 1 {
                img_cache.current_index += 1;
            }
            println!("RENDERED NEXT: current_index: {}, current_offset: {}",
                img_cache.current_index, img_cache.current_offset);
            
            if *pane_layout == PaneLayout::DualPane && is_slider_dual {
                println!("dualpane && is_slider_dual slider update");
                pane.slider_value = img_cache.current_index as u16;
            }
        }

    }

    // Update master slider when !is_slider_dual
    if !is_slider_dual || *pane_layout == PaneLayout::SinglePane {
        // v2: use the current_index of the pane with largest dir size
        //*slider_value = get_pane_with_largest_dir_size(panes) as u16;
        *slider_value = (get_pane_with_largest_dir_size(panes)) as u16;
    }
    Command::batch(commands)
}

pub fn move_left_all(panes: &mut Vec<pane::Pane>, slider_value: &mut u16, pane_layout: &PaneLayout, is_slider_dual: bool) -> Command<Message> {
    let mut commands = Vec::new();
    let mut did_new_render_happen = false;
    for (cache_index, pane) in panes.iter_mut().enumerate() {
        println!("move_left_all_new - cache_index: {}, is_pane_cached_prev: {}", cache_index, is_pane_cached_prev(pane.clone(), cache_index, is_slider_dual));
        println!("current_index: {}, current_offset, current_offset_accumulated: {}, {}", pane.img_cache.current_index, pane.img_cache.current_offset, pane.img_cache.current_offset_accumulated);
        pane.img_cache.print_cache();
        
        if !is_pane_cached_prev(pane.clone(), cache_index, is_slider_dual) {
            continue;
        }
        let img_cache = &mut pane.img_cache;

        if img_cache.current_index > 0 {
            let next_image_index_to_load: isize = img_cache.current_index as isize  - img_cache.cache_count as isize  - 1;
            let next_image_index_to_load_usize = next_image_index_to_load as usize;
            println!("LOADING PREV: next_image_index_to_load: {}, current_index: {}, current_offset: {}",
                next_image_index_to_load, img_cache.current_index, img_cache.current_offset);

            if img_cache.is_image_index_within_bounds(next_image_index_to_load) {
                // TODO: organize this better
                if next_image_index_to_load >= 0 &&
                (img_cache.current_index >= img_cache.cache_count &&
                img_cache.current_index <= (img_cache.image_paths.len()-1) - img_cache.cache_count) {
                    img_cache.enqueue_image_load(LoadOperation::LoadPrevious((cache_index, next_image_index_to_load as usize)));

                } else if img_cache.current_index > (img_cache.image_paths.len()-1) - img_cache.cache_count -1 {
                    let next_image_index_to_load = img_cache.current_index as isize - img_cache.cache_count as isize - 1;
                    img_cache.enqueue_image_load(LoadOperation::ShiftPrevious((cache_index, next_image_index_to_load)));
                } else {
                    img_cache.enqueue_image_load(LoadOperation::ShiftPrevious((cache_index, next_image_index_to_load)));
                }
            }
            img_cache.print_queue();
            
            let command = load_image_by_operation(img_cache);
            commands.push(command);
        } else {
            commands.push(Command::none())
        }


        // Render the previous one right away
        // Avoid loading around the edges
        if img_cache.cache_count as isize + img_cache.current_offset > 0 &&
            img_cache.is_some_at_index( (img_cache.cache_count as isize + img_cache.current_offset) as usize) {

            let next_image_index_to_render = img_cache.cache_count as isize + (img_cache.current_offset - 1);
            println!("RENDERING PREV: next_image_index_to_render: {} current_index: {}, current_offset: {}",
                next_image_index_to_render, img_cache.current_index, img_cache.current_offset);

            if img_cache.is_image_index_within_bounds(next_image_index_to_render) {
                let loaded_image = img_cache.get_image_by_index(next_image_index_to_render as usize).unwrap().to_vec();
                let handle = iced::widget::image::Handle::from_memory(loaded_image.clone());
                pane.current_image = handle;
                img_cache.current_offset -= 1;

                println!("(img_cache.image_paths.len()-1) - img_cache.cache_count -1 = {}", (img_cache.image_paths.len()-1) - img_cache.cache_count -1);
                println!("img_cache.current_index <= img_cache.cache_count: {}", img_cache.current_index <= img_cache.cache_count);

                if img_cache.current_index > 0 {
                    img_cache.current_index -= 1;
                }
                println!("RENDERED PREV: current_index: {}, current_offset: {}",
                img_cache.current_index, img_cache.current_offset);

                if *pane_layout == PaneLayout::DualPane && is_slider_dual {
                    pane.slider_value = img_cache.current_index as u16;
                }
                did_new_render_happen = true;
            }
        }
    }

    // Update master slider when !is_slider_dual
    if did_new_render_happen && (!is_slider_dual || *pane_layout == PaneLayout::SinglePane) {
        *slider_value = (get_pane_with_largest_dir_size(panes) ) as u16;
    }

    Command::batch(commands)
}

pub fn move_right_all_unused(panes: &mut Vec<pane::Pane>) -> Command<Message> {
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
                    img_cache.enqueue_image_load(LoadOperation::ShiftNext((cache_index, next_image_index as isize)));
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

pub fn move_left_all_unused(panes: &mut Vec<pane::Pane>) -> Command<Message> {        
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
