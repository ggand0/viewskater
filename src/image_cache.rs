use std::fs;
use std::path::{Path, PathBuf};
use std::io;
// use tokio::fs::File;
use tokio::io::AsyncReadExt;
// use std::io::Read;
use std::collections::VecDeque;


#[derive(Debug, Clone)]
pub enum LoadOperation {
    LoadNext(usize),     // Includes the target index
    ShiftNext(usize),
    LoadPrevious(usize), // Includes the target index
    ShiftPrevious(isize),
}


// Define a struct to hold the image paths and the currently displayed image index
#[derive(Default, Clone)]
pub struct ImageCache {
    pub image_paths: Vec<PathBuf>,
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
            current_index: initial_index,
            cache_count,
            cached_images: vec![None; cache_count * 2 + 1], // Initialize cached_images with None
            loading_queue: VecDeque::new(),
            being_loaded_queue: VecDeque::new(),
            // max_concurrent_loading: 10,
        })
    }

    pub fn print_queue(&self) {
        println!("loading_queue: {:?}", self.loading_queue);
        println!("being_loaded_queue: {:?}", self.being_loaded_queue);

    }

    pub fn enqueue_image_load(&mut self, operation: LoadOperation) {
        // Push the operation into the loading queue
        self.loading_queue.push_back(operation);
    }

    pub fn enqueue_image_being_loaded(&mut self, operation: LoadOperation) {
        // Push the index into the being loaded queue
        self.being_loaded_queue.push_back(operation);
    }

    pub fn is_next_image_index_in_queue(&self, next_image_index: isize) -> bool {
        let next_index_usize = next_image_index as usize;
        self.loading_queue.iter().all(|op| match op {
            LoadOperation::LoadNext(index) => index != &next_index_usize,
            LoadOperation::LoadPrevious(index) => index != &next_index_usize,
            LoadOperation::ShiftNext(index) => index != &next_index_usize,
            LoadOperation::ShiftPrevious(index) => index != &next_image_index,
        }) && self.being_loaded_queue.iter().all(|op| match op {
            LoadOperation::LoadNext(index) => index != &next_index_usize,
            LoadOperation::LoadPrevious(index) => index != &next_index_usize,
            LoadOperation::ShiftNext(index) => index != &next_index_usize,
            LoadOperation::ShiftPrevious(index) => index != &next_image_index,
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
                    println!("{}", image_info);
                }
                None => {
                    let no_image_info = format!("No image at index {}", index);
                    println!("{}", no_image_info);
                }
            }
        }

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

    pub async fn async_load_image(path: &Path) -> Result<Option<Vec<u8>>, std::io::ErrorKind> {
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
    }
    
    pub fn load_current_image(&mut self) -> Result<&Vec<u8>, io::Error> {
        // let cache_index = self.current_index + self.cache_count;
        let cache_index = self.cache_count;
        println!(" Current index: {}, Cache index: {}", self.current_index, cache_index);
        if self.cached_images[cache_index].is_none() {
            println!("Loading image");
            let current_image = self.load_image(self.current_index)?;
            self.cached_images[cache_index] = Some(current_image.clone());
        }
        Ok(self.cached_images[cache_index].as_ref().unwrap())
    }

    pub fn get_current_image(&self) -> Result<&Vec<u8>, io::Error> {
        let cache_index = self.cache_count;
        println!("    Current index: {}, Cache index: {}", self.current_index, cache_index);
        // Display information about each image
        for (index, image_option) in self.cached_images.iter().enumerate() {
            match image_option {
                Some(image_bytes) => {
                    let image_info = format!("    Image {} - Size: {} bytes", index, image_bytes.len());
                    println!("{}", image_info);
                }
                None => {
                    let no_image_info = format!("    No image at index {}", index);
                    println!("{}", no_image_info);
                }
            }
        }

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
            self.shift_cache_left(new_image);
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
