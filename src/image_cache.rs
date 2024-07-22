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
use crate::file_io::{async_load_image, load_images_async, empty_async_block_vec};
use crate::loading_status::LoadingStatus;
use crate::pane::Pane;   


#[derive(Debug, Clone, PartialEq)]
pub enum LoadOperation {
    LoadNext((Vec<usize>, Vec<isize>)),       // Includes the target index
    ShiftNext((Vec<usize>, Vec<isize>)),
    LoadPrevious((Vec<usize>, Vec<isize>)),   // Includes the target index
    ShiftPrevious((Vec<usize>, Vec<isize>)),
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
    //pub fn load_fn(&self) -> Box<dyn FnOnce(&mut ImageCache, Option<Vec<u8>>, isize) -> Result<bool, std::io::Error>> {
    pub fn load_fn(&self) -> Box<dyn FnMut(&mut ImageCache, Option<Vec<u8>>, isize) -> Result<bool, std::io::Error>> {
        match self {
            LoadOperation::LoadNext(..) => {
                Box::new(|cache, new_image, image_index| cache.move_next(new_image, image_index))
            },
            LoadOperation::ShiftNext(..) => Box::new(|cache, new_image, image_index| cache.move_next_edge(new_image, image_index)),
            LoadOperation::LoadPrevious(..) => Box::new(|cache, new_image, image_index| cache.move_prev(new_image, image_index)),
            LoadOperation::ShiftPrevious(..) => Box::new(|cache, new_image, image_index| cache.move_prev_edge(new_image, image_index)),
            LoadOperation::LoadPos(..) => {
                let pos = match self {
                    LoadOperation::LoadPos((_, _, pos)) => *pos,
                    _ => 0, // Default value if the variant pattern doesn't match
                };
                Box::new(move |cache, new_image, image_index| cache.load_pos(new_image, pos, image_index))
            }
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
    pub cached_images: Vec<Option<Vec<u8>>>,                // Changed cached_images to store Option<Vec<u8>> for better handling
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

    pub fn print_state(&self) {
        println!("current_index: {}, current_offset: {}", self.current_index, self.current_offset);
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
        let next_image_index = (self.current_index as isize + (self.cache_count as isize -  self.current_offset) as isize) as usize + 1;
        next_image_index
    }
    pub fn get_prev_image_to_load(&self) -> usize {
        let prev_image_index_to_load = (self.current_index as isize + (-(self.cache_count as isize) - self.current_offset) as isize) - 1;
        prev_image_index_to_load as usize
    }

    pub fn is_next_image_loaded(&self, next_image_index: usize) -> bool {
        self.cache_states[next_image_index]
    }

    pub fn is_next_image_index_in_queue(&self, _cache_index: usize, next_image_index: isize) -> bool {
        let next_index_usize = next_image_index as usize;
        self.loading_queue.iter().all(|op| match op {
            LoadOperation::LoadNext((_c_index, _img_indices)) => { false },
            LoadOperation::LoadPrevious((_c_index, _img_index)) => { false },
            LoadOperation::ShiftNext((_c_index, _img_indices)) => { false },
            LoadOperation::ShiftPrevious((_c_index, _img_index)) => { false },
            LoadOperation::LoadPos((_c_index, img_index, _pos)) => img_index != &next_index_usize,
        }) && self.being_loaded_queue.iter().all(|op| match op {
            LoadOperation::LoadNext((_c_index, _img_indices)) => { false },
            LoadOperation::LoadPrevious((_c_index, _img_index)) => { false },
            LoadOperation::ShiftNext((_c_index, _img_indices)) => { false },
            LoadOperation::ShiftPrevious((_c_index, _img_index)) => { false },
            LoadOperation::LoadPos((_c_index, img_index, _pos)) => img_index != &next_index_usize,
        })
    }
    pub fn are_next_image_indices_in_queue(&self, next_image_indices: Vec<isize>) -> bool {
        let flag = self.loading_queue.iter().all(|op| match op {
            LoadOperation::LoadNext((_c_index, img_indices)) => img_indices != &next_image_indices,
            LoadOperation::ShiftNext((_c_index, img_indices)) => img_indices != &next_image_indices,
            LoadOperation::LoadPrevious((_c_index, img_indices)) => img_indices != &next_image_indices,
            LoadOperation::ShiftPrevious((_c_index, img_indices)) => img_indices != &next_image_indices,
            LoadOperation::LoadPos((_c_index, _img_index, _pos)) => { false },
        }) && self.being_loaded_queue.iter().all(|op| match op {
            LoadOperation::LoadNext((_c_index, img_indices)) => img_indices != &next_image_indices,
            LoadOperation::ShiftNext((_c_index, img_indices)) => img_indices != &next_image_indices,
            LoadOperation::LoadPrevious((_c_index, img_indices)) => img_indices != &next_image_indices,
            LoadOperation::ShiftPrevious((_c_index, img_indices)) => img_indices != &next_image_indices,
            LoadOperation::LoadPos((_c_index, _img_index, _pos)) => { false },
        });
        flag
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
    pub fn is_blocking_loading_ops_in_queue(&self, loading_operation: LoadOperation, loading_status: &LoadingStatus) -> bool {
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
                        println!("is_blocking_loading_ops_in_queue - op: {:?}", op);
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

    pub fn is_operation_in_queues(&self, operation: LoadOperationType) -> bool {
        self.loading_queue.iter().any(|op| op.operation_type() == operation) ||
        self.being_loaded_queue.iter().any(|op| op.operation_type() == operation)
    }

    pub fn is_some_at_index(&self, index: usize) -> bool {
        // Using pattern matching to check if element is None
        if let Some(image_data_option) = self.cached_images.get(index) {
            //println!("is_some_at_index - index: {}, cached_images.len(): {}", index, self.cached_images.len());
            if let Some(_image_data) = image_data_option {
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

    pub fn load_image(&self, index: usize) -> Result<Vec<u8>, io::Error> {
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
                // LoadNext => covewred in load_images_by_operation()
                LoadOperation::LoadNext((_cache_index, _target_index)) => {
                    //load_image_by_index(img_cache, target_index, operation)
                    Command::none()
                }
                LoadOperation::LoadPrevious((_cache_index, _target_index)) => {
                    //load_image_by_index(img_cache, target_index, operation)
                    Command::none()
                }
                LoadOperation::ShiftNext((_cache_index, _target_index)) => {
                    //let empty_async_block = empty_async_block(operation);
                    //Command::perform(empty_async_block, Message::ImageLoaded)
                    Command::none()
                }
                LoadOperation::ShiftPrevious((_cache_index, _target_index)) => {
                    //let empty_async_block = empty_async_block(operation);
                    //Command::perform(empty_async_block, Message::ImageLoaded)
                    Command::none()
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

//pub fn load_images_by_indices(panes: &mut Vec<Pane>, target_indices: Vec<isize>, operation: LoadOperation) -> Command<<DataViewer as iced::Application>::Message> {
pub fn load_images_by_indices(panes: &mut Vec<&mut Pane>, target_indices: Vec<isize>, operation: LoadOperation) -> Command<<DataViewer as iced::Application>::Message> {
    println!("load_images_by_indices");
    let mut paths = Vec::new();

    for (pane_index, pane) in panes.iter_mut().enumerate() {
        let img_cache = &mut pane.img_cache;
        let target_index = target_indices[pane_index];
        
        if target_index < 0 {
            paths.push(None);
        } else {
            if let Some(path) = img_cache.image_paths.get(target_index as usize) {
                if let Some(s) = path.to_str() {
                    paths.push(Some(s.to_string()));
                } else {
                    paths.push(None);
                }
            } else {
                paths.push(None);
            }
        }
    }
    // show all paths
    for (i, path) in paths.iter().enumerate() {
        println!("path[{}]: {:?}", i, path);
    }

    if !paths.is_empty() {
        let images_loading_task = load_images_async(paths, operation);
        Command::perform(images_loading_task, Message::ImagesLoaded)
    } else {
        Command::none()
    }
}

pub fn load_images_by_operation(panes: &mut Vec<&mut Pane>, loading_status: &mut LoadingStatus) -> Command<<DataViewer as iced::Application>::Message> {
    println!("load_images_by_operation");
    if !loading_status.loading_queue.is_empty() {
        if let Some(operation) = loading_status.loading_queue.pop_front() {
            println!("load_images_by_operation - operation: {:?}", operation);
            loading_status.enqueue_image_being_loaded(operation.clone());
            match operation {
                LoadOperation::LoadNext((ref _pane_indices, ref target_indicies)) => {
                    load_images_by_indices(panes, target_indicies.clone(), operation)
                }
                LoadOperation::LoadPrevious((ref _pane_indices, ref target_indicies)) => {
                    load_images_by_indices(panes, target_indicies.clone(), operation)
                }
                LoadOperation::ShiftNext((ref _pane_indices, ref _target_indicies)) => {
                    //let empty_async_block = empty_async_block(operation);
                    let empty_async_block = empty_async_block_vec(operation, panes.len());
                    Command::perform(empty_async_block, Message::ImagesLoaded)
                }
                LoadOperation::ShiftPrevious((ref _pane_indices,  ref _target_indicies)) => {
                    let empty_async_block = empty_async_block_vec(operation, panes.len());
                    Command::perform(empty_async_block, Message::ImagesLoaded)
                    //Command::none()
                }
                LoadOperation::LoadPos((ref _pane_indices, _target_index, _pos)) => {
                    //load_images_by_indices(target_index, operation)
                    Command::none()
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


