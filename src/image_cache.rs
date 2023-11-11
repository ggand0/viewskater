use std::fs;
use std::path::{Path, PathBuf};
use std::io;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use std::io::Read;
use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub enum LoadOperation {
    LoadNext(usize),     // Includes the target index
    LoadPrevious(usize), // Includes the target index
}


// Define a struct to hold the image paths and the currently displayed image index
#[derive(Default, Clone)]
pub struct ImageCache {
    pub image_paths: Vec<PathBuf>,
    pub current_index: usize,
    cache_count: usize, // Number of images to cache in advance
    cached_images: Vec<Option<Vec<u8>>>, // Changed cached_images to store Option<Vec<u8>> for better handling
    // pub loading_queue: VecDeque<usize>, // Queue of image indices to load
    pub loading_queue: VecDeque<LoadOperation>,
    max_concurrent_loading: usize, // Limit concurrent loading tasks
}

impl ImageCache {
    pub fn new(image_dir: &str, cache_count: usize) -> Result<Self, io::Error> {
        let mut image_paths: Vec<PathBuf> = fs::read_dir(image_dir)?
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let path = entry.path();
                if path.is_file() {
                    Some(path)
                } else {
                    None
                }
            })
            .collect();

        image_paths.sort_by(|a, b| {
            let a_str = a.to_str().unwrap_or("");
            let b_str = b.to_str().unwrap_or("");
            a_str.cmp(b_str)
        });
        

        Ok(ImageCache {
            image_paths,
            current_index: 0,
            cache_count,
            cached_images: vec![None; cache_count * 2 + 1], // Initialize cached_images with None
            loading_queue: VecDeque::new(),
            max_concurrent_loading: 10,
        })
    }

    pub fn enqueue_image_load(&mut self, operation: LoadOperation) {
        // Push the operation into the loading queue
        self.loading_queue.push_back(operation);
    }

    

    // Function to load initial images when current_index == 0
    pub fn load_initial_images(&mut self) -> Result<(), io::Error> {
        for i in self.cache_count..(self.cache_count*2 + 1) {
            
            if i >= 0 && i < self.image_paths.len() {
                let image = self.load_image(i)?;
                self.cached_images[i] = Some(image);
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
        println!("Current index: {}, Cache index: {}", self.current_index, cache_index);
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
    
    pub fn is_within_bounds(&self, index: usize) -> bool {
        (0..self.image_paths.len()).contains(&index)
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

    pub async fn on_slider_value_changed(&mut self, new_slider_value: usize) {
        // let new_index = self.slider_to_index(new_slider_value);
        /*let new_index = new_slider_value;
        println!("New slider value: {}, New index: {}", new_slider_value, new_index);
    
        // Calculate the new sliding window range
        let new_window_start = new_index.saturating_sub(self.cache_count);
        let new_window_end = new_index + self.cache_count;
    
        // Calculate the previous sliding window range
        let prev_window_start = self.current_index.saturating_sub(self.cache_count);
        let prev_window_end = self.current_index + self.cache_count;
    
        // Calculate overlapping elements
        let overlapping_start = std::cmp::max(new_window_start, prev_window_start);
        let overlapping_end = std::cmp::min(new_window_end, prev_window_end);
    
        // Load new images for non-overlapping elements in the new sliding window
        let mut new_cached_images: Vec<Option<Vec<u8>>> = Vec::new();
    
        for i in new_window_start..new_window_end {
            if i < overlapping_start || i >= overlapping_end {
                // Load new image for non-overlapping elements
                if let Some(image_path) = self.image_paths.get(i) {
                    let new_image = self.async_load_image(image_path).await;
                    new_cached_images.push(new_image.ok());
                }
            } else {
                // Reuse overlapping elements from the previous cache
                let cache_index = i - self.current_index + self.cache_count;
                new_cached_images.push(self.cached_images[cache_index].take());
            }
        }
    
        // Update the cache with the new images
        self.cached_images = new_cached_images;
    
        // Update the current_index
        // self.current_index = new_index;
        self.current_index = new_slider_value;
        println!("new_slider_value: {}", new_slider_value);
        println!("Current index: {}", self.current_index);
        */
    }

    fn shift_cache_right(&mut self, new_image: Option<Vec<u8>>) {
        // Shift the elements in cached_images to the right
        self.cached_images.pop(); // Remove the last (rightmost) element
        // self.cached_images.insert(0, None); // Add a None to the beginning (leftmost)

        // Load a new image to the beginning (leftmost)
        // sync
        // let new_image = self.load_image(self.current_index-self.cache_count);
        // self.cached_images.insert(0, new_image.ok());

        // load async loaded image synchronously
        self.cached_images.insert(0, new_image);
    }

    fn shift_cache_left(&mut self, new_image: Option<Vec<u8>>) {
        self.cached_images.remove(0);
        self.cached_images.push(new_image);
    }
}
