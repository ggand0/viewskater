#[warn(unused_imports)]
#[cfg(target_os = "linux")]
mod other_os {
    pub use iced;
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

#[allow(unused_imports)]
use std::time::Instant;

#[allow(unused_imports)]
use log::{debug, info, warn, error};


use crate::{DataViewer,Message};
use iced::Command;
use crate::file_io::{async_load_image, empty_async_block};
use crate::pane::{self, get_master_slider_value};
use crate::menu::PaneLayout;


#[derive(Debug, Clone)]
pub enum LoadOperation {
    /*LoadNext((usize, usize,)),       // Includes the target index
    ShiftNext((usize, isize)),
    LoadPrevious((usize, usize)),   // Includes the target index
    ShiftPrevious((usize, isize)),
    LoadPos((usize, usize, usize)), // Load an image at a specific position of the cache*/
    LoadNext((usize, usize,)),       // Includes the target index
    ShiftNext((usize, isize)),
    LoadPrevious((usize, usize)),   // Includes the target index
    ShiftPrevious((usize, isize)),
    LoadPos((usize, usize, usize)), // Load an image at a specific position of the cache
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
    pub fn load_fn(&self) -> Box<dyn FnOnce(&mut ImageCache, Option<Vec<u8>>, isize) -> Result<bool, std::io::Error>> {
        match self {
            /*LoadOperation::LoadNext(..) => Box::new(|cache, new_image| cache.move_next(new_image)),
            LoadOperation::ShiftNext(..) => Box::new(|cache, new_image| cache.move_next_edge(new_image)),
            LoadOperation::LoadPrevious(..) => Box::new(|cache, new_image| cache.move_prev(new_image)),
            LoadOperation::ShiftPrevious(..) => Box::new(|cache, new_image| cache.move_prev_edge(new_image)),*/
            LoadOperation::LoadNext(..) => {
                Box::new(|cache, new_image, image_index| cache.move_next(new_image, image_index))
            },
            LoadOperation::ShiftNext(..) => Box::new(|cache, new_image, image_index| cache.move_next_edge(new_image, image_index)),
            LoadOperation::LoadPrevious(..) => Box::new(|cache, new_image, image_index| cache.move_prev(new_image, image_index)),
            LoadOperation::ShiftPrevious(..) => Box::new(|cache, new_image, image_index| cache.move_prev_edge(new_image, image_index)),
            LoadOperation::LoadPos(..) => {
                /*let pos = match self {
                    LoadOperation::LoadPos((_, _, pos)) => *pos,
                    _ => 0, // Default value if the variant pattern doesn't match
                };
                Box::new(move |cache, new_image| cache.load_pos(new_image, pos))*/

                let pos = match self {
                    LoadOperation::LoadPos((_, _, pos)) => *pos,
                    _ => 0, // Default value if the variant pattern doesn't match
                };
                /*let image_index = match self {
                    LoadOperation::LoadPos((_, img_index, _)) => *img_index,
                    _ => 0, // Default value if the variant pattern doesn't match
                };*/
                Box::new(move |cache, new_image, image_index| cache.load_pos(new_image, pos, image_index))
            },
        }
    }
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


// Define a struct to hold the image paths and the currently displayed image index
#[derive(Default, Clone)]
pub struct ImageCache {
    pub image_paths: Vec<PathBuf>,
    pub num_files: usize,
    pub current_index: usize,
    pub current_offset: isize,
    pub cache_count: usize,                             // Number of images to cache in advance
    cached_images: Vec<Option<Vec<u8>>>,                // Changed cached_images to store Option<Vec<u8>> for better handling
    pub cached_image_indices: Vec<isize>,               // Indices of cached images (index of the image_paths array)
    pub cache_states: Vec<bool>,                        // Cache states
    pub loading_queue: VecDeque<LoadOperation>,
    pub being_loaded_queue: VecDeque<LoadOperation>,    // Queue of image indices being loaded
    pub out_of_order_images: Vec<(usize, Vec<u8>)>,     // Store out-of-order images (used in Message::ImageLoaded)
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
            cache_states: Vec::new(),
            cached_image_indices: vec![-1; cache_count * 2 + 1],
            out_of_order_images: Vec::new(),
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
                    let image_info = format!("Image {} - Index {} - Size: {} bytes", index, self.cached_image_indices[index], image_bytes.len());
                    println!("{}", image_info);
                }
                None => {
                    let no_image_info = format!("No image at index {}", index);
                    println!("{}", no_image_info);
                }
            }
        }
    }
    pub fn print_cache_index(&self) {
        for (index, cache_index) in self.cached_image_indices.iter().enumerate() {
            let index_info = format!("Index {} - Cache Index: {}", index, cache_index);
            println!("{}", index_info);
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

    pub fn reset_load_next_queue_items(&mut self) {
        // Discard all queue items that are LoadNext or ShiftNext
        self.loading_queue.retain(|op| match op {
            LoadOperation::LoadNext(..) => false,
            LoadOperation::ShiftNext(..) => false,
            _ => true,
        });
    }
    pub fn reset_load_previous_queue_items(&mut self) {
        // Discard all queue items that are LoadPrevious or ShiftPrevious
        self.loading_queue.retain(|op| match op {
            LoadOperation::LoadPrevious(..) => false,
            LoadOperation::ShiftPrevious(..) => false,
            _ => true,
        });
    }

    pub fn is_load_next_items_in_queue(&self) -> bool {
        self.loading_queue.iter().any(|op| match op {
            LoadOperation::LoadNext(..) => true,
            LoadOperation::ShiftNext(..) => true,
            _ => false,
        })
    }
    pub fn is_load_previous_items_in_queue(&self) -> bool {
        self.loading_queue.iter().any(|op| match op {
            LoadOperation::LoadPrevious(..) => true,
            LoadOperation::ShiftPrevious(..) => true,
            _ => false,
        })
    }

    // Search for and remove the specific image from the out_of_order_images Vec
    pub fn pop_out_of_order_image(&mut self, target_index: usize) -> Option<Vec<u8>> {
        if let Some(pos) = self.out_of_order_images.iter().position(|&(index, _)| index == target_index) {
            Some(self.out_of_order_images.remove(pos).1)
        } else {
            None
        }
    }

    pub fn get_next_image_to_load(&self) -> usize {
        //let next_image_index = (self.current_index + self.cache_count ) as usize;
        let next_image_index = (self.current_index as isize + (self.cache_count as isize -  self.current_offset) as isize) as usize + 1;

        /*if next_image_index < self.image_paths.len() {
            next_image_index
        } else {
            self.image_paths.len()
        }*/
        next_image_index
    }
    pub fn get_prev_image_to_load(&self) -> usize {
        //let prev_image_index = (self.current_index + self.cache_count ) as usize;

        //let prev_image_index_to_load = img_cache.current_index as isize - img_cache.current_offset - img_cache.cache_count as isize - 1;
        //let prev_image_index = (self.current_index as isize + (self.cache_count as isize +  self.current_offset) as isize) as usize - 1;

        //let prev_image_index_to_load = (self.current_index as isize + ((self.cache_count as isize) + self.current_offset) as isize) - 1;
        let prev_image_index_to_load = (self.current_index as isize + (-(self.cache_count as isize) - self.current_offset) as isize) - 1;
        prev_image_index_to_load as usize
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
    pub fn is_blocking_loading_ops_in_queue(&self, loading_operation: LoadOperation) -> bool {
        match loading_operation {
            LoadOperation::LoadNext((_cache_index, _target_index)) => {
                if self.current_offset == -(self.cache_count as isize) {
                    return true;
                }
                if self.current_offset == self.cache_count as isize {
                    if let Some(op) = self.being_loaded_queue.front() {
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

    pub fn is_some_at_index(&self, index: usize) -> bool {
        // Using pattern matching to check if element is None
        if let Some(image_data_option) = self.cached_images.get(index) {
            //println!("is_some_at_index - index: {}, cached_images.len(): {}", index, self.cached_images.len());
            if let Some(image_data) = image_data_option {
                //println!("is_some_at_index - image_data.len(): {}", image_data.len());
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
        self.cache_count as isize + self.current_offset + 1
    }

    pub fn load_initial_images(&mut self) -> Result<(), io::Error> {
        let _cache_size = self.cache_count * 2 + 1;

        // Calculate the starting & ending indices for the cache array
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
            self.cached_image_indices[i] = cache_index;
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

        // Display the indices
        for (index, cache_index) in self.cached_image_indices.iter().enumerate() {
            let index_info = format!("Index {} - Cache Index: {}", index, cache_index);
            debug!("{}", index_info);
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

    pub fn move_next(&mut self, new_image: Option<Vec<u8>>, _image_index: isize) -> Result<bool, io::Error> {
        if self.current_index < self.image_paths.len() - 1 {
            // Move to the next image
            ////self.current_index += 1;
            self.shift_cache_left(new_image);
            Ok(false)
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "No more images to display"))
        }
    }

    pub fn move_next_edge(&mut self, _new_image: Option<Vec<u8>>, _image_index: isize) -> Result<bool, io::Error> {
        if self.current_index < self.image_paths.len() - 1 {
            // v2
            //self.current_offset += 1;
            //self.current_index += 1;
            println!("move_next_edge - current_index: {}, current_offset: {}", self.current_index, self.current_offset);
            Ok(false)
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "No more images to display"))
        }
    }

    pub fn move_prev(&mut self, new_image: Option<Vec<u8>>, _image_index: isize) -> Result<bool, io::Error> {
        if self.current_index > 0 {
            //self.current_index -= 1; // shuold this be after the cache shift?
            self.shift_cache_right(new_image);
            ////self.current_index -= 1;
            Ok(false)
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "No previous images to display"))
        }
    }

    pub fn move_prev_edge(&mut self, _new_image: Option<Vec<u8>>, _image_index: isize) -> Result<bool, io::Error> {
        if self.current_index > 0 {
            // v2
            //self.current_offset -= 1;
            //self.current_index -= 1;

            println!("move_prev_edge - current_index: {}, current_offset: {}", self.current_index, self.current_offset);
            Ok(false)
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "No previous images to display"))
        }
    }

    fn shift_cache_right(&mut self, new_image: Option<Vec<u8>>) {
        // Shift the elements in cached_images to the right
        self.cached_images.pop(); // Remove the last (rightmost) element
        self.cached_images.insert(0, new_image);

        // also update indices
        self.cached_image_indices.pop();
        let prev_index = self.cached_image_indices[0] - 1;
        self.cached_image_indices.insert(0, prev_index);

        self.current_offset += 1;
        println!("shift_cache_right - current_offset: {}", self.current_offset);
    }

    fn shift_cache_left(&mut self, new_image: Option<Vec<u8>>) {
        self.cached_images.remove(0);
        self.cached_images.push(new_image);

        // also update indices
        self.cached_image_indices.remove(0);
        let next_index = self.cached_image_indices[self.cached_image_indices.len()-1] + 1;
        self.cached_image_indices.push(next_index);

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
        println!("shift_cache_left - current_offset: {}", self.current_offset);
    }

    fn load_pos(&mut self, new_image: Option<Vec<u8>>, pos: usize, image_index: isize) -> Result<bool, io::Error> {
        // If `pos` is at the center of the cache return true to reload the current_image
        self.cached_images[pos] = new_image;
        self.cached_image_indices[pos] = image_index as isize;
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
                LoadOperation::LoadPos((_cache_index, target_index, _pos)) => {
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


//fn get_loading_commands_slider(pane: &mut pane::Pane, img_cache: &mut ImageCache, pane_index: usize, pos: usize) -> Vec<Command<<DataViewer as iced::Application>::Message>> {
fn get_loading_commands_slider(pane: &mut pane::Pane, pane_index: usize, pos: usize) -> Vec<Command<<DataViewer as iced::Application>::Message>> {
    #[allow(unused_mut)]
    let mut img_cache = &mut pane.img_cache;
    let mut commands = Vec::new();
    let cache_index = pane_index;
    println!("get_loading_commands_slider - cache_count: {}, pos: {}, img_paths.len(): {}",
                    img_cache.cache_count, pos, img_cache.image_paths.len());
    
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
        println!("get_loading_commands_slider - `pos < img_cache.cache_count`: current_offset: {}", img_cache.current_offset);
    } else if pos >= img_cache.image_paths.len() - img_cache.cache_count {
        let last_index = img_cache.cache_count*2 + 1;
        //let last_index = img_cache.cache_count*2 + 1 + 1;
        
        for i in 0..last_index {
            let target_cache_index = i;
            //let image_index = img_cache.image_paths.len() - last_index + i;
            let image_index = img_cache.image_paths.len() - last_index + i;
            ////println!("target_cache_index: {}, image_index: {}", target_cache_index, image_index);
            img_cache.enqueue_image_load(
                LoadOperation::LoadPos((
                    cache_index, image_index, target_cache_index)));
        }
        img_cache.print_queue();

        // If it missed the last image, load the last image into the current_index
        if pos >= img_cache.image_paths.len() {
            /*let last_pos = img_cache.image_paths.len() - 1;
            //img_cache.enqueue_image_load(
            //    LoadOperation::LoadPos((cache_index, last_index, img_cache.cache_count)));
            img_cache.current_index = last_index;
            img_cache.current_offset = 0;
            img_cache.current_index = last_index;*/

            img_cache.current_index = img_cache.image_paths.len() - 1;
            //img_cache.current_offset = img_cache.cache_count as isize - 1;
            img_cache.current_offset = img_cache.cache_count as isize;

            // I get runtime error here. Maybe handle the case where the image is not cached
            /*if img_cache.cached_images[img_cache.cache_count].is_none() {
                let current_image = img_cache.load_image(img_cache.current_index)?;
                img_cache.cached_images[img_cache.cache_count] = Some(current_image);
            }*/

            pane.current_image = iced::widget::image::Handle::from_memory(
                img_cache.get_initial_image().unwrap().to_vec());
        }

        let local_commands = load_all_images_in_queue(img_cache);
        commands.extend(local_commands);
        println!("get_loading_commands_slider - `pos >= img_cache.image_paths.len() - img_cache.cache_count`: current_offset: {}", img_cache.current_offset);
    } else if pos >= img_cache.image_paths.len() {
        let last_index = img_cache.image_paths.len() - 1;
        let last_pos = last_index - img_cache.cache_count;
        println!("get_loading_commands_slider - out of bounds (pos >= img_cache.image_paths.len()): pane_index: {}, pos: {}, last_pos: {}",
            pane_index, pos, last_pos);

        // Since it missed the last image, load the last image into the current_index
        img_cache.enqueue_image_load(
            LoadOperation::LoadPos((
                cache_index, last_index, img_cache.cache_count)));
        img_cache.current_index = last_index;
        img_cache.current_offset = 0;

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
        
        //println!("get_loading_commands_slider - out of bounds: pos: {}, last_pos: {}", pos, last_pos);
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
            println!("get_loading_commands_slider - else: next_image_index: {}, prev_image_index: {}", next_image_index, prev_image_index);

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
            //let img_cache = &mut pane.img_cache;

            if pane.dir_loaded {
                let local_commands = get_loading_commands_slider(pane, cache_index as usize, pos);
                commands.extend(local_commands);
            } else {
                commands.push(Command::none());
            }
        }
        Command::batch(commands)
    } else{
        let mut commands = Vec::new();
        let pane = &mut panes[pane_index as usize];
        let _img_cache = &mut pane.img_cache;

        if pane.dir_loaded {
            let local_commands = get_loading_commands_slider(pane, pane_index as usize, pos);
            commands.extend(local_commands);
        } else {
            commands.push(Command::none());
        }
        Command::batch(commands)
    }
}

fn load_current_slider_image(pane: &mut pane::Pane, pos: usize ) -> Result<(), io::Error> {
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
            img_cache.cached_image_indices[target_index] = pos as isize;

            img_cache.current_index = pos;
            //img_cache.current_offset = 0;
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
        //let img_cache = &mut pane.img_cache;

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

/*fn is_pane_cached_next(pane: pane::Pane, _index: usize, _is_slider_dual: bool) -> bool {
    println!("pane.is_selected: {}, pane.dir_loaded: {}, pane.img_cache.is_next_cache_index_within_bounds(): {}, pane.img_cache.loading_queue.len(): {}, pane.img_cache.being_loaded_queue.len(): {}",
        pane.is_selected, pane.dir_loaded, pane.img_cache.is_next_cache_index_within_bounds(), pane.img_cache.loading_queue.len(), pane.img_cache.being_loaded_queue.len());
    pane.is_selected && pane.dir_loaded && pane.img_cache.is_next_cache_index_within_bounds() &&
        pane.img_cache.loading_queue.len() < 3 && pane.img_cache.being_loaded_queue.len() < 3
}*/

fn is_pane_cached_prev(pane: pane::Pane, _index: usize, _is_slider_dual: bool) -> bool {
    //println!("pane.is_selected: {}, pane.dir_loaded: {}, pane.img_cache.is_prev_cache_index_within_bounds(): {}, pane.img_cache.loading_queue.len(): {}, pane.img_cache.being_loaded_queue.len(): {}",
    //    pane.is_selected, pane.dir_loaded, pane.img_cache.is_prev_cache_index_within_bounds(), pane.img_cache.loading_queue.len(), pane.img_cache.being_loaded_queue.len());
    pane.is_selected && pane.dir_loaded && pane.img_cache.is_prev_cache_index_within_bounds() &&
        pane.img_cache.loading_queue.len() < 3 && pane.img_cache.being_loaded_queue.len() < 3
}

// Function to initialize image_load_state for all panes
fn init_is_next_image_loaded(panes: &mut Vec<pane::Pane>, _pane_layout: &PaneLayout, _is_slider_dual: bool) {
    for pane in panes.iter_mut() {
        pane.is_next_image_loaded = false;
        pane.is_prev_image_loaded = false;
    }
}
fn init_is_prev_image_loaded(panes: &mut Vec<pane::Pane>, _pane_layout: &PaneLayout, _is_slider_dual: bool) {
    for pane in panes.iter_mut() {
        pane.is_prev_image_loaded = false;
        pane.is_next_image_loaded = false;
    }
}


// Function to check if all images are loaded for all panes
fn are_all_next_images_loaded(panes: &mut Vec<pane::Pane>, is_slider_dual: bool) -> bool {
    if is_slider_dual {
        panes
        .iter()
        .filter(|pane| pane.is_selected)  // Filter only selected panes
        .all(|pane| !pane.dir_loaded || (pane.dir_loaded && pane.is_next_image_loaded))
    } else {
        panes.iter().all(|pane| !pane.dir_loaded || (pane.dir_loaded && pane.is_next_image_loaded))
    }
}
fn are_all_prev_images_loaded(panes: &mut Vec<pane::Pane>, is_slider_dual: bool) -> bool {
    if is_slider_dual {
        panes
        .iter()
        .filter(|pane| pane.is_selected)  // Filter only selected panes
        .all(|pane| !pane.dir_loaded || (pane.dir_loaded && pane.is_prev_image_loaded))
    } else {
        panes.iter().all(|pane| !pane.dir_loaded || (pane.dir_loaded && pane.is_prev_image_loaded))
    }
}

pub fn move_right_all(panes: &mut Vec<pane::Pane>, slider_value: &mut u16,
    pane_layout: &PaneLayout, is_slider_dual: bool, last_opened_pane: usize) -> Command<Message> {
    println!("##########MOVE_RIGHT_ALL() CALL##########");

    // If all panes have been rendered, start rendering the next image; reset is_next_image_loaded
    if are_all_next_images_loaded(panes, is_slider_dual) {
        init_is_next_image_loaded(panes, pane_layout, is_slider_dual);
    }

    
    let mut commands = Vec::new();
    for (cache_index, pane) in panes.iter_mut().enumerate() {
        println!("move_right_all() - PROCESSING pane_index: {}, current_index: {}, current_offset: {}", cache_index, pane.img_cache.current_index, pane.img_cache.current_offset);
        pane.img_cache.print_cache();
        //pane.img_cache.print_cache_index();
        pane.img_cache.print_queue();
        println!("==============================");

        /*if !pane.is_cached_next() {
            // If this pane already reaches the edge, mark is_next_image_loaded as true
            if pane.img_cache.current_index == pane.img_cache.image_paths.len() - 1 {
                pane.is_next_image_loaded = true;
            }
            continue;
        }

        if !pane.is_next_image_loaded {
            commands.extend(pane.load_next_images(cache_index));

            // Render the next one right away by setting a new handle (next image) to the current_image
            // Avoid loading around the edges
            // Only render if it's not been rendered yet
            pane.set_next_image(pane_layout, is_slider_dual);
            // current_index gets incremented here, offset gets incremented as well
        }*/
        if pane.img_cache.out_of_order_images.len() > 0 {
            println!("move_right_all() - pane_index: {}, out_of_order_images.len(): {:?}", cache_index, pane.img_cache.out_of_order_images.len());
            continue;
        }

        if !pane.is_cached_next() {
            println!("move_right_all() - pane_index: {}, not cached next, skipping...", cache_index);
            // If this pane already reaches the edge, mark is_next_image_loaded as true
            if pane.img_cache.current_index == pane.img_cache.image_paths.len() - 1 {
                pane.is_next_image_loaded = true;
            }
            continue;
        }

        if !pane.is_next_image_loaded {
            println!("move_right_all() - pane_index: {}, setting next image...", cache_index);
            let did_render_happen: bool = pane.set_next_image(pane_layout, is_slider_dual);

            /*//if did_render_happen && pane.img_cache.current_offset < 0 {
            if did_render_happen {
                // Update the current_image
                //pane.current_image = iced::widget::image::Handle::from_memory(pane.img_cache.get_initial_image().unwrap().to_vec());
                let loaded_image = pane.img_cache.get_initial_image().unwrap().to_vec();
                let handle = iced::widget::image::Handle::from_memory(loaded_image.clone());
                pane.current_image = handle;

                // Just call shiftnext instead
                //pane.img_cache.enqueue_image_load(LoadOperation::ShiftNext((cache_index, next_image_index_to_load)));
                //let command = load_image_by_operation(pane.img_cache);
                //commands.push(command)
            }*/

            //if did_render_happen && pane.img_cache.current_offset >= 0 {
            if did_render_happen {
                println!("move_right_all() - pane_index: {}, render happened, current_index: {}, current_offset: {}",
                    cache_index, pane.img_cache.current_index, pane.img_cache.current_offset);
                println!("move_right_all() - pane_index: {}, loading next images...", cache_index);
                commands.extend(pane.load_next_images(cache_index));
            }
        }

    }
    
    let did_new_render_happen = are_all_next_images_loaded(panes, is_slider_dual);

    // Update master slider when !is_slider_dual
    if did_new_render_happen && !is_slider_dual || *pane_layout == PaneLayout::SinglePane {
        // v2: use the current_index of the pane with largest dir size
        //*slider_value = get_master_slider_value(panes) as u16;
        *slider_value = (get_master_slider_value(panes, pane_layout, is_slider_dual, last_opened_pane)) as u16;
    }

    Command::batch(commands)
}

pub fn move_left_all(panes: &mut Vec<pane::Pane>, slider_value: &mut u16, pane_layout: &PaneLayout, is_slider_dual: bool, last_opened_pane: usize
) -> Command<Message> {
    let mut commands = Vec::new();

    // If all panes have been rendered, start rendering the next(prev) image; reset is_next_image_loaded
    if are_all_prev_images_loaded(panes, is_slider_dual) {
        init_is_prev_image_loaded(panes, pane_layout, is_slider_dual);
    }

    for (cache_index, pane) in panes.iter_mut().enumerate() {
        println!("move_left_all_new - cache_index: {}, is_pane_cached_prev: {}", cache_index, is_pane_cached_prev(pane.clone(), cache_index, is_slider_dual));
        println!("current_index: {}, current_offset: {}", pane.img_cache.current_index, pane.img_cache.current_offset);
        pane.img_cache.print_cache();

        if pane.img_cache.out_of_order_images.len() > 0 {
            println!("move_left_all() - pane_index: {}, out_of_order_images.len(): {:?}", cache_index, pane.img_cache.out_of_order_images.len());
            continue;
        }
        
        if !is_pane_cached_prev(pane.clone(), cache_index, is_slider_dual) {
            // If this pane already reaches the edge, mark is_next_image_loaded as true
            if pane.img_cache.current_index == 0 {
                pane.is_prev_image_loaded = true;
            }
            continue;
        }
        
        if !pane.is_prev_image_loaded {
            let did_render_happen: bool = pane.set_prev_image(pane_layout, is_slider_dual);

            //if did_render_happen && pane.img_cache.current_offset <= 0 {
            if did_render_happen {
                commands.extend(pane.load_prev_images(cache_index));
            }
        }
    }

    let did_new_render_happen = are_all_prev_images_loaded(panes, is_slider_dual);

    // Update master slider when !is_slider_dual
    if did_new_render_happen && !is_slider_dual || *pane_layout == PaneLayout::SinglePane {
        *slider_value = (get_master_slider_value(panes, pane_layout, is_slider_dual, last_opened_pane) ) as u16;
    }

    Command::batch(commands)
}

#[allow(dead_code)]
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
            debug!("is_next_image_loaded state: {:?}", pane.is_next_image_loaded);

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

#[allow(dead_code)]
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
