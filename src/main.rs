
/*#[cfg(target_os = "macos")]
use iced_custom as iced;
#[cfg(not(target_os = "macos"))]
use iced;

#[cfg(target_os = "macos")]
use iced_aw_custom as iced_aw;
#[cfg(not(target_os = "macos"))]
use iced_aw;

#[cfg(target_os = "macos")]
use iced_widget_custom as iced_widget;
#[cfg(not(target_os = "macos"))]
use iced_widget;*/

/*#[cfg(target_os = "macos")]
mod macos {
    pub use iced_custom as iced;
    pub use iced_aw_custom as iced_aw;
    pub use iced_widget_custom as iced_widget;
}

#[cfg(not(target_os = "macos"))]
mod other_os {
    pub use iced;
    pub use iced_aw;
    pub use iced_widget;
}

#[cfg(target_os = "macos")]
use macos::*;

#[cfg(not(target_os = "macos"))]
use other_os::*;*/

#[cfg(target_os = "linux")]
mod other_os {
    pub use iced;
    pub use iced_aw;
}

#[cfg(not(target_os = "linux"))]
mod macos {
    pub use iced_custom as iced;
    pub use iced_aw_custom as iced_aw;
}

#[cfg(target_os = "linux")]
use other_os::*;

#[cfg(not(target_os = "linux"))]
use macos::*;


use iced::event::Event;
use iced::subscription::{self, Subscription};
use iced::keyboard;
use iced::widget::{
    container, row, column, slider, horizontal_space, text
};
use iced::widget::Image;
// use iced::widget::pane_grid::{self, PaneGrid};
use iced::{Element, Length, Application, Theme, Settings, Command, Color, alignment};

use iced_aw::menu::{CloseCondition, ItemHeight, ItemWidth, PathHighlight};
use iced_aw::menu_bar;
use iced_native::image::Data;

use std::path::Path;
use std::path::PathBuf;

// #[macro_use]
extern crate log;

mod image_cache;
use image_cache::LoadOperation;
use image_cache::ImageLoadState;
mod utils;
use utils::{async_load_image, empty_async_block, is_file, is_directory, get_file_paths, get_file_index, Error};

mod ui;
use ui::{PaneLayout, Pane};

mod split {
    pub mod split; // Import the module from split/split.rs
    pub mod style; // Import the module from split/style.rs
}
use split::split::{Axis, Split};
mod dualslider {
    pub mod dualslider;
    pub mod style;
}
use dualslider::dualslider::DualSlider;

use crate::image_cache::ImageCache;
// use iced::{Space, Length};


#[derive(Debug, Clone, Copy)]
pub enum MenuItem {
    Open,
    Close,
    Help
}

enum SliderType {
    Single,
    Dual,
}

// Define the application state
// #[derive(Default)]
pub struct DataViewer {
    // image_path: String,
    // image_paths: Vec<String>,
    // error: Option<io::ErrorKind>,
    dir_loaded: Vec<bool>,
    directory_path: Option<String>,
    current_image_index: usize,
    // img_cache: image_cache::ImageCache,
    img_caches: Vec<image_cache::ImageCache>,
    // current_image: iced::widget::image::Handle,
    current_images: Vec<iced::widget::image::Handle>,
    slider_value: u16,      // for master slider
    prev_slider_value: u16, // for master slider
    slider_values: Vec<u16>,
    prev_slider_values: Vec<u16>,

    // num_files: usize,
    title: String,
    ver_divider_position: Option<u16>,
    hor_divider_position: Option<u16>,
    // image_load_state: Arc<Mutex<ImageLoadState>>,
    // image_load_state: ImageLoadState,
    image_load_state: Vec<bool>,
    pane_count: usize,
    // slider_type: SliderType,
    is_slider_dual: bool,
    pane_layout: PaneLayout,
    last_opened_pane: usize,
}

impl Default for DataViewer {
    fn default() -> Self {
        Self {
            // image_path: String::new(),
            // image_paths: Vec::new(),
            // error: None,
            dir_loaded: vec![false; 2],
            directory_path: None,
            current_image_index: 0,
            // img_cache: image_cache::ImageCache::default(),
            img_caches: vec![image_cache::ImageCache::default(), image_cache::ImageCache::default()],
            current_images: Vec::new(),
            slider_value: 0,
            prev_slider_value: 0,
            slider_values: vec![0; 2],
            prev_slider_values: vec![0; 2],
            // num_files: 0,
            title: String::from("View Skater"),
            ver_divider_position: None,
            hor_divider_position: None,
            image_load_state: vec![true; 2],
            pane_count: 2,
            is_slider_dual: false,
            pane_layout: PaneLayout::SinglePane,
            last_opened_pane: 0,
        }
    }
}

// Define application messages
#[derive(Debug, Clone)]
pub enum Message {
    Debug(String),
    Nothing,
    OpenFolder,
    OpenFile,
    FileDropped(isize, String),
    Close,
    FolderOpened(Result<String, Error>),
    SliderChanged(isize, u16),
    Event(Event),
    // ImageLoaded(Result<(), std::io::ErrorKind>),// std::io::Error doesn't seem to be clonable
    // ImageLoaded(Result<Option<Vec<u8>>, std::io::ErrorKind>),
    ImageLoaded(Result<(Option<Vec<u8>>, Option<LoadOperation>), std::io::ErrorKind>),
    // MenuItemClicked(MenuItem),
    OnVerResize(u16),
    OnHorResize(u16),
    ResetSplit(u16),
    ToggleSliderType(bool),
    TogglePaneLayout(PaneLayout),
}





impl DataViewer {
    fn reset_state(&mut self) {
        self.dir_loaded = vec![false; 2];
        self.image_load_state = vec![true; 2];
        self.directory_path = None;
        self.current_image_index = 0;
        //self.img_cache = image_cache::ImageCache::default();
        self.img_caches = vec![image_cache::ImageCache::default(), image_cache::ImageCache::default()];
        self.current_images = vec![iced::widget::image::Handle::from_memory(vec![]), iced::widget::image::Handle::from_memory(vec![])];
        // iced::widget::image::Handle::from_memory(vec![]);
        self.slider_value = 0;
        self.prev_slider_value = 0;
        self.slider_values = vec![0; 2];
        self.prev_slider_values = vec![0; 2];
        // self.num_files = 0;
        self.title = String::from("View Skater");
        self.last_opened_pane = 0;
    }

    fn load_image_by_index(img_cache: &mut image_cache::ImageCache, target_index: usize, operation: LoadOperation) -> Command<<DataViewer as iced::Application>::Message> {
        let path = img_cache.image_paths.get(target_index);
        if let Some(path) = path {
            // println!("target_index: {}, Loading Path: {}", path.clone().to_string_lossy(), target_index );
            let image_loading_task = async_load_image(path.clone(), operation);
            Command::perform(image_loading_task, Message::ImageLoaded)
        } else {
            Command::none()
        }
    }

    // for v2 (async single pane)
    /*fn load_image_by_operation(&mut self) -> Command<Message> {
        if !self.img_cache.loading_queue.is_empty() {
            if let Some(operation) = self.img_cache.loading_queue.pop_front() {
                self.img_cache.enqueue_image_being_loaded(operation.clone());
                match operation {
                    LoadOperation::LoadNext(target_index) => {
                        DataViewer::load_image_by_index(&mut self.img_cache, target_index, operation)
                    }
                    LoadOperation::LoadPrevious(target_index) => {
                        DataViewer::load_image_by_index(&mut self.img_cache, target_index, operation)
                    }
                    LoadOperation::ShiftNext(_target_index) => {
                        let empty_async_block = empty_async_block(operation);
                        Command::perform(empty_async_block, Message::ImageLoaded)
                    }
                    LoadOperation::ShiftPrevious(_target_index) => {
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
            
    }*/

    // for v3 (async multiple panes)
    fn load_image_by_operation(img_cache: &mut image_cache::ImageCache) -> Command<<DataViewer as iced::Application>::Message> {
        if !img_cache.loading_queue.is_empty() {
            if let Some(operation) = img_cache.loading_queue.pop_front() {
                img_cache.enqueue_image_being_loaded(operation.clone());
                match operation {
                    LoadOperation::LoadNext((_cache_index, target_index)) => {
                        // DataViewer::load_image_by_index(&mut img_cache, target_index, operation)
                        DataViewer::load_image_by_index(img_cache, target_index, operation)
                    }
                    LoadOperation::LoadPrevious((_cache_index, target_index)) => {
                        // DataViewer::load_image_by_index(&mut img_cache, target_index, operation)
                        DataViewer::load_image_by_index(img_cache, target_index, operation)
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

    fn init_image_loaded(&mut self) {
        for state in self.image_load_state.iter_mut() {
            *state = false;
        }
    }

    // Function to mark an image as loaded
    fn mark_image_loaded(&mut self, index: usize) {
        if let Some(state) = self.image_load_state.get_mut(index) {
            *state = true;
        }
    }

    // Function to check if all images are loaded
    fn are_all_images_loaded(&self) -> bool {
        // self.image_load_state.iter().all(|&loaded| loaded)
        self.image_load_state
        .iter()
        .zip(self.dir_loaded.iter())
        .all(|(&loaded, &dir)| !dir || (dir && loaded))
    }

    fn initialize_dir_path(&mut self, path: PathBuf, pane_index: usize) {
        self.last_opened_pane = pane_index;
        println!("last_opened_pane: {}", self.last_opened_pane);
        let mut _file_paths: Vec<PathBuf> = Vec::new();
        let initial_index: usize;
        if is_file(&path) {
            println!("Dropped path is a file");
            let directory = path.parent().unwrap_or(Path::new(""));
            let dir = directory.to_string_lossy().to_string();
            self.directory_path = Some(dir);

            // _file_paths = get_file_paths(Path::new(&self.directory_path.clone().unwrap()));
            _file_paths = utils::get_image_paths(Path::new(&self.directory_path.clone().unwrap()));
            let file_index = get_file_index(&_file_paths, &path);
            // let file_index = get_file_index(&self.image_paths.iter().map(PathBuf::from).collect::<Vec<_>>(), &path);
            if let Some(file_index) = file_index {
                println!("File index: {}", file_index);
                initial_index = file_index;
                self.current_image_index = file_index;
                // self.slider_value = file_index as u16;
                self.slider_values[pane_index] = file_index as u16;
            } else {
                println!("File index not found");
                return;
            }
        } else if is_directory(&path) {
            println!("Dropped path is a directory");
            self.directory_path = Some(path.to_string_lossy().to_string());
            _file_paths = get_file_paths(Path::new(&self.directory_path.clone().unwrap()));
            initial_index = 0;
            self.current_image_index = 0;
            // self.slider_value = 0;
            self.slider_values[pane_index] = 0;
        } else {
            println!("Dropped path does not exist or cannot be accessed");
            // Handle the case where the path does not exist or cannot be accessed
            return;
        }

        // Debug print the files
        for path in _file_paths.iter().take(20) {
            println!("{}", path.display());
        }

        // self.image_paths = file_paths.iter().map(|p| p.to_string_lossy().to_string()).collect();
        println!("File paths: {}", _file_paths.len());
        // self.num_files = _file_paths.len();
        // self.dir_loaded = true;
        self.dir_loaded[pane_index] = true;

        // Instantiate a new image cache and load the initial images
        let mut img_cache =  image_cache::ImageCache::new(
            _file_paths,
            2,
            initial_index,
        ).unwrap();
        img_cache.load_initial_images().unwrap();
        // self.img_cache = img_cache;
        

        // If the index is greater than or equal to the length of current_images,
        // fill the vector with default handles until the desired index
        let default_handle = iced::widget::image::Handle::from_memory(vec![]);
        if pane_index >= self.current_images.len() {
            self.current_images.resize_with(pane_index + 1, || default_handle.clone());
            self.img_caches.resize_with(pane_index + 1, || image_cache::ImageCache::default());
        }

        let loaded_image = img_cache.get_current_image().unwrap().to_vec();
        let handle = iced::widget::image::Handle::from_memory(loaded_image.clone());
        self.current_images[pane_index] = handle;
        println!("current images all: {:?}", self.current_images);

        self.img_caches[pane_index] = img_cache;
        
        
        for (cache_index, img_cache) in self.img_caches.iter_mut().enumerate() {
            let file_paths = img_cache.image_paths.clone();
            println!("file_paths.len() {:?}", file_paths.len());

        }
        
    }

    fn update_pos(&mut self, pane_index: isize, pos: usize) {
        // self.current_image_index = pos;
        // self.slider_value = pos as u16;
        // self.title = format!("{}", self.img_cache.image_paths[pos].display());

        // v1: single pane
        /*let file_paths = self.img_cache.image_paths.clone();

        let mut img_cache =  image_cache::ImageCache::new(
            file_paths,
            2,
            pos,
        ).unwrap();
        img_cache.load_initial_images().unwrap();
        // self.current_image = iced::widget::image::Handle::from_memory(img_cache.get_current_image().unwrap().to_vec());
        self.img_cache = img_cache;

        let loaded_image = self.img_cache.get_current_image().unwrap().to_vec();
        // self.current_image = iced::widget::image::Handle::from_memory(loaded_image);
        self.current_images = vec![
            iced::widget::image::Handle::from_memory(loaded_image.clone()),
            iced::widget::image::Handle::from_memory(loaded_image)
        ];*/

        // v2: multiple panes
        if pane_index == -1 {
            // Update all panes
            let mut updated_caches = Vec::with_capacity(self.img_caches.len());
            for (cache_index, img_cache) in self.img_caches.iter_mut().enumerate() {
                if self.dir_loaded[cache_index] {
                    let file_paths = img_cache.image_paths.clone();
                    println!("file_paths.len() {:?}", file_paths.len());

                    // NOTE: be careful with the `pos`; if pos is greater than img_cache.image_paths.len(), it will panic
                    let position = pos.min(img_cache.image_paths.len() - 1);
                    let mut img_cache =  image_cache::ImageCache::new(
                        file_paths,
                        2,
                        position,
                    ).unwrap();

                    /*let mut img_cache =  image_cache::ImageCache::new(
                        file_paths,
                        2,
                        pos,
                    ).unwrap();*/
                    img_cache.load_initial_images().unwrap();
                    // self.current_image = iced::widget::image::Handle::from_memory(img_cache.get_current_image().unwrap().to_vec());
                    updated_caches.push(img_cache);

                    // let loaded_image = self.img_caches[cache_index].get_current_image().unwrap().to_vec();
                    // self.current_images[cache_index] = iced::widget::image::Handle::from_memory(loaded_image);
                } else {
                    let mut img_cache =  image_cache::ImageCache::new(
                        Vec::new(),
                        2,
                        0,
                    ).unwrap();
                    updated_caches.push(img_cache);
                }
            }

            for (cache_index, new_cache) in updated_caches.into_iter().enumerate() {
                println!("new_cache.current_index: {}", new_cache.current_index);
                self.img_caches[cache_index] = new_cache;

                if self.dir_loaded[cache_index] {
                    let loaded_image = self.img_caches[cache_index].get_current_image().unwrap().to_vec();
                    self.current_images[cache_index] = iced::widget::image::Handle::from_memory(loaded_image);
                }
            }
        } else {
            let pane_index = pane_index as usize;
            let file_paths = self.img_caches[pane_index].image_paths.clone();

            let mut img_cache =  image_cache::ImageCache::new(
                file_paths,
                2,
                pos,
            ).unwrap();
            img_cache.load_initial_images().unwrap();
            // self.current_image = iced::widget::image::Handle::from_memory(img_cache.get_current_image().unwrap().to_vec());
            self.img_caches[pane_index] = img_cache;

            let loaded_image = self.img_caches[pane_index].get_current_image().unwrap().to_vec();
            self.current_images[pane_index] = iced::widget::image::Handle::from_memory(loaded_image);
        }


    }

    fn move_left_all(&mut self) -> Command<Message> {
        // v1
        // self.img_cache.move_prev();
        // self.current_image = self.img_cache.get_current_image().unwrap().clone();

        // v2
        /*let img_cache = &mut self.img_cache;
        if img_cache.current_index <=0 {
            Command::none()
        } else {
            // let next_image_index = img_cache.current_index - 1; // WRONG
            let next_image_index: isize = img_cache.current_index as isize - img_cache.cache_count as isize - 1;
            if img_cache.is_next_image_index_in_queue(next_image_index) {
                if next_image_index < 0 {
                    // No new images to load but shift the cache
                    img_cache.enqueue_image_load(LoadOperation::ShiftPrevious(next_image_index));
                } else {
                    img_cache.enqueue_image_load(LoadOperation::LoadPrevious(next_image_index as usize));
                }
            }
            img_cache.print_queue();
            self.load_image_by_operation()
        }*/
        
        // v3 (multiple panes)
        let mut commands = Vec::new();

        // Get the current index of the img_cache that has the largest file_paths
        /*let max_length = img_caches
        .iter() // Iterate over ImageCache elements
        .map(|cache| cache.file_paths.len()) // Map each ImageCache to the length of its file_paths
        .max(); // Find the maximum length*/

        let mut global_current_index = None;
        let mut index_of_max_length_cache = None;
        // Find the index of the ImageCache with the longest file_paths
        if let Some(index_of_max_length) = self.img_caches
            .iter()
            .enumerate()
            .max_by_key(|(_, cache)| cache.image_paths.len())
            .map(|(index, _)| index)
        {
            let current_index_of_max_length = self.img_caches[index_of_max_length].current_index;
            println!("Index of max length file_paths: {}", index_of_max_length);
            println!("Current index of max length file_paths: {}", current_index_of_max_length);

            global_current_index = Some(current_index_of_max_length);
            index_of_max_length_cache = Some(index_of_max_length);
        } else {
            println!("No ImageCache found in the array");
        }
        println!("global_current_index: {:?}", global_current_index);
        println!("index_of_max_length_cache: {:?}", index_of_max_length_cache);


        for (cache_index, img_cache) in self.img_caches.iter_mut().enumerate() {
            println!("current_index: {}, global_current_index: {:?}", img_cache.current_index, global_current_index);
            println!("cache_index, index_of_max_length_cache: {}, {}", cache_index, index_of_max_length_cache.unwrap());
            /*if img_cache.current_index <=0 || cache_index == index_of_max_length_cache.unwrap() ||
                img_cache.current_index < global_current_index.unwrap() {
                commands.push(Command::none())
            } else {*/
            if img_cache.current_index <=0 {
                commands.push(Command::none());
                continue;
            }

            if cache_index != index_of_max_length_cache.unwrap() && img_cache.current_index < global_current_index.unwrap() {
                self.image_load_state[cache_index] = true;
            }

            if cache_index == index_of_max_length_cache.unwrap() ||
                cache_index != index_of_max_length_cache.unwrap() && img_cache.current_index == global_current_index.unwrap() {
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
                // let command = DataViewer::load_image_by_operation(&mut img_cache);
                let command = DataViewer::load_image_by_operation(img_cache);
                commands.push(command);
            } else {
                commands.push(Command::none())
            }
        }

        Command::batch(commands)

    }

    fn move_right_all(&mut self) -> Command<Message> {
        // 1. Naive loading
        // self.image_path = "../data/landscape/".to_string() + &self.image_paths[self.current_image_index].clone();
        // println!("Image path: {}", self.image_path)

        // 2. Load from cache (async), single pane
        // load the image from cache now
        // STRATEGY: image at current_index: ALREADY LOADED in cache => set to self.current_image
        //      image at current_index + cache_count: NOT LOADED in cache => enqueue load operation
        // since it's a new image, update the cache
        /*if self.img_cache.image_paths.len() > 0 && self.img_cache.current_index < self.img_cache.image_paths.len() - 1 {
                        
            // let next_image_index = img_cache.current_index + 1; // WRONG
            let next_image_index = self.img_cache.current_index + self.img_cache.cache_count + 1;
            println!("NEXT_IMAGE_INDEX: {}", next_image_index);

            if self.img_cache.is_next_image_index_in_queue(next_image_index as isize) {
                if next_image_index >= self.img_cache.image_paths.len() {
                    // No new images to load, but shift the cache
                    self.img_cache.enqueue_image_load(LoadOperation::ShiftNext(next_image_index));
                } else {
                    self.img_cache.enqueue_image_load(LoadOperation::LoadNext(next_image_index));
                }

            }
            self.img_cache.print_queue();
            self.load_image_by_operation()
            // ImageViewer::load_image_by_operation_with_cache(&mut self.img_cache)
        } else {
            Command::none()
        }*/

        // 3. Load from cache (async), multiple panes
        let mut commands = Vec::new();

        for (cache_index, img_cache) in self.img_caches.iter_mut().enumerate() {
            // println!("move_right_all(): cache_index: {}", cache_index);
            if img_cache.image_paths.len() > 0 && img_cache.current_index < img_cache.image_paths.len() - 1 {
                            
                // let next_image_index = img_cache.current_index + 1; // WRONG
                let next_image_index = img_cache.current_index + img_cache.cache_count + 1;
                println!("NEXT_IMAGE_INDEX: {}", next_image_index);
                println!("image load state: {:?}", self.image_load_state);

                if img_cache.is_next_image_index_in_queue(cache_index, next_image_index as isize) {
                    if next_image_index >= img_cache.image_paths.len() {
                        // No new images to load, but shift the cache
                        img_cache.enqueue_image_load(LoadOperation::ShiftNext((cache_index, next_image_index)));
                    } else {
                        img_cache.enqueue_image_load(LoadOperation::LoadNext((cache_index, next_image_index)));
                    }

                }
                img_cache.print_queue();
                // let command = DataViewer::load_image_by_operation(&mut img_cache);
                let command = DataViewer::load_image_by_operation(img_cache);
                commands.push(command);
                
                // ImageViewer::load_image_by_operation_with_cache(&mut self.img_cache)
            } else {
                commands.push(Command::none())
            }
        }

        Command::batch(commands)
    }

    fn move_left_index(&mut self, pane_index: usize) -> Command<Message> {
        // NOTE: pane_index == cache_index
        let img_cache = &mut self.img_caches[pane_index];
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
            // let command = DataViewer::load_image_by_operation(&mut img_cache);
            DataViewer::load_image_by_operation(img_cache)
        }
    }

    fn move_right_index (&mut self, pane_index: usize) -> Command<Message> {
        // NOTE: pane_index == cache_index
        let img_cache = &mut self.img_caches[pane_index];
        // if img_cache.current_index <=0 {
        //     Command::none()
        // } else {
        if img_cache.image_paths.len() > 0 && img_cache.current_index < img_cache.image_paths.len() - 1 {
            // let next_image_index = img_cache.current_index - 1; // WRONG
            let next_image_index = img_cache.current_index + img_cache.cache_count + 1;
                println!("NEXT_IMAGE_INDEX: {}", next_image_index);
                println!("image load state: {:?}", self.image_load_state);

                if img_cache.is_next_image_index_in_queue(pane_index, next_image_index as isize) {
                    if next_image_index >= img_cache.image_paths.len() {
                        // No new images to load, but shift the cache
                        img_cache.enqueue_image_load(LoadOperation::ShiftNext((pane_index, next_image_index)));
                    } else {
                        img_cache.enqueue_image_load(LoadOperation::LoadNext((pane_index, next_image_index)));
                    }

                }
            img_cache.print_queue();
            // let command = DataViewer::load_image_by_operation(&mut img_cache);
            DataViewer::load_image_by_operation(img_cache)
        } else {
            Command::none()
        }
    }
    // UI
    fn toggle_slider_type(&mut self) {
        /*match self.slider_type {
            SliderType::Single => {
                self.slider_type = SliderType::Dual;
            },
            SliderType::Dual => self.slider_type = SliderType::Single,
        }*/
        
        // binary ver
        self.is_slider_dual = !self.is_slider_dual;
    }

    fn toggle_pane_layout(&mut self, pane_layout: PaneLayout) {
        self.pane_layout = pane_layout;
        match self.pane_layout {
            PaneLayout::SinglePane => {
                self.img_caches.resize(1, Default::default()); // Resize to hold 1 image cache
                println!("self.img_caches.len(): {}", self.img_caches.len());
                self.dir_loaded[1] = false;
                
            }
            PaneLayout::DualPane => {
                self.img_caches.resize(2, Default::default()); // Resize to hold 2 image caches
                println!("self.img_caches.len(): {}", self.img_caches.len());
                //self.pane_layout = PaneLayout::SinglePane;
            }
        }
        // Update other app state as needed...
    }

    fn build_ui_dual_pane_slider1(&self) -> Element<Message> {
        let first_img: iced::widget::Container<Message>  = if self.dir_loaded[0] {
            container(column![
                Image::new(self.current_images[0].clone())
                .width(Length::Fill)
                .height(Length::Fill),
            ])   
        } else {
            container(column![
            text(String::from(""))
            .width(Length::Fill)
            .height(Length::Fill)
            
            ])
        };
            
        let second_img: iced::widget::Container<Message>  = if self.dir_loaded[1] {
            container(column![
                Image::new(self.current_images[1].clone())
                .width(Length::Fill)
                .height(Length::Fill),
                ]
            )
        } else {
            container(column![
                text(String::from(""))
                .width(Length::Fill)
                .height(Length::Fill)
                
                ])
        };

        let panes: Element<Message> = Split::new(
            first_img,
            second_img,
            self.ver_divider_position,
            Axis::Vertical,
            Message::OnVerResize,
            Message::ResetSplit,
            Message::FileDropped
            //Message::FileDropped((1), (String::from("")).into()),
        )
        .into();

        panes
    }

    fn build_ui_dual_pane_slider2(&self) -> Element<Message> {
        let first_img: iced::widget::Container<Message>  = if self.dir_loaded[0] {
            container(column![
                Image::new(self.current_images[0].clone())
                .width(Length::Fill)
                .height(Length::Fill),
                //slider(0..= (self.img_caches[0].num_files-1) as u16, self.slider_values[0], Message::SliderChanged)
                /*slider(
                    0..= (self.img_caches[0].num_files - 1) as u16,
                    self.slider_values[0],
                    |value| {
                        let pane_index = 0; // Replace this with the desired pane index
                        Message::SliderChanged((pane_index, value))
                    }
                )*/
                DualSlider::new(
                    0..= (self.img_caches[0].num_files - 1) as u16,
                    self.slider_values[0],
                    0,
                    Message::SliderChanged
                )
                .width(Length::Fill)
                ]
            )
        } else {
            container(column![
            text(String::from(""))
            .width(Length::Fill)
            .height(Length::Fill)
            
            ])
        };
            
        let second_img: iced::widget::Container<Message>  = if self.dir_loaded[1] {
            container(column![
                Image::new(self.current_images[1].clone())
                .width(Length::Fill)
                .height(Length::Fill),
                DualSlider::new(
                    0..= (self.img_caches[1].num_files - 1) as u16, // panic
                    self.slider_values[1],
                    1,
                    Message::SliderChanged
                )
                ]
            )
        } else {
            container(column![
                text(String::from(""))
                .width(Length::Fill)
                .height(Length::Fill)
                
                ])
        };

        let panes: Element<Message> = Split::new(
            first_img,
            second_img,
            self.ver_divider_position,
            Axis::Vertical,
            Message::OnVerResize,
            Message::ResetSplit,
            Message::FileDropped
            //Message::FileDropped((1), (String::from("")).into()),
        )
        .into();

        panes
    }

}


impl Application for DataViewer {
    type Message = Message;
    type Theme = Theme;
    type Executor= iced::executor::Default;
    type Flags = ();

    fn new(_flags: Self::Flags) -> (Self, Command<Self::Message>) {
        (
            Self {
                // image_path: String::new(),
                // image_paths: Vec::new(),
                // error: None,
                dir_loaded: vec![false; 2],
                directory_path: None,
                current_image_index: 0,
                // img_cache: image_cache::ImageCache::default(),
                img_caches: vec![image_cache::ImageCache::default(), image_cache::ImageCache::default()],
                //current_image: iced::widget::image::Handle::from_memory(vec![]),
                current_images: vec![iced::widget::image::Handle::from_memory(vec![]), iced::widget::image::Handle::from_memory(vec![])],
                slider_value: 0,
                prev_slider_value: 0,
                slider_values: vec![0; 2],
                prev_slider_values: vec![0; 2],
                // num_files: 0,
                title: String::from("View Skater"),
                ver_divider_position: None,
                hor_divider_position: None,
                image_load_state: vec![true; 2],
                pane_count: 2,
                is_slider_dual: false,
                pane_layout: PaneLayout::SinglePane,
                last_opened_pane: 0,
            },
            Command::none()
        )

    }
    
    fn title(&self) -> String {
        match self.pane_layout  {
            PaneLayout::SinglePane => {
                if self.dir_loaded[0] {
                    // return string here
                    self.img_caches[0].image_paths[self.img_caches[0].current_index].display().to_string()

                } else {
                    self.title.clone()
                }
            }
            PaneLayout::DualPane => {
                let left_pane_filename = if self.dir_loaded[0] {
                    self.img_caches[0].image_paths[self.img_caches[0].current_index]
                        .file_name()
                        .map(|name| name.to_string_lossy().to_string())
                        .unwrap_or_else(|| String::from("Unknown"))
                } else {
                    String::from("No File")
                };
    
                let right_pane_filename = if self.dir_loaded[1] {
                    self.img_caches[1].image_paths[self.img_caches[1].current_index]
                        .file_name()
                        .map(|name| name.to_string_lossy().to_string())
                        .unwrap_or_else(|| String::from("Unknown"))
                } else {
                    String::from("No File")
                };
    
                format!("Left: {} | Right: {}", left_pane_filename, right_pane_filename)
            }
        }
    }

    fn update(&mut self, message: Message) -> Command<Self::Message> {
        match message {
            Message::Nothing => {
                Command::none()
            }
            Message::Debug(s) => {
                self.title = s;
                Command::none()
            }
            Message::OpenFolder => {
                Command::perform(utils::pick_folder(), |result| {
                    Message::FolderOpened(result)
                })
            }
            Message::OpenFile => {
                Command::perform(utils::pick_file(), |result| {
                    Message::FolderOpened(result)
                })
            }
            Message::FileDropped(pane_index, dropped_path) => {
                println!("File dropped: {:?}, pane_index: {}", dropped_path, pane_index);

                // Workaround: when the index is -2,
                // 1. when the both panes are empty, load it into the first pane
                // 2. when the first pane is loaded and second is empty, load it into the second pane
                // 3. when the first pane is empty and second is loaded, load it into the first pane
                // 4. when both panes are loaded, load it into the pane not in `self.last_opened_pane`
                println!("self.dir_loaded, pane_index, last_opened_pane: {:?}, {}, {}", self.dir_loaded, pane_index, self.last_opened_pane);
                /*if pane_index == -2 {
                    if !self.dir_loaded[0] && !self.dir_loaded[1] {
                        self.initialize_dir_path( PathBuf::from(dropped_path), 0);
                    } else if self.dir_loaded[0] && !self.dir_loaded[1] {
                        self.initialize_dir_path( PathBuf::from(dropped_path), 1);
                    } else if !self.dir_loaded[0] && self.dir_loaded[1] {
                        self.initialize_dir_path( PathBuf::from(dropped_path), 0);
                    } else if self.dir_loaded[0] && self.dir_loaded[1] {
                        if self.last_opened_pane == 0 {
                            self.initialize_dir_path( PathBuf::from(dropped_path), 1);
                        } else {
                            self.initialize_dir_path( PathBuf::from(dropped_path), 0);
                        }
                    }

                    return Command::none();
                }*/


                self.initialize_dir_path( PathBuf::from(dropped_path), pane_index as usize);
                
                Command::none()
            
            }
            Message::Close => {
                self.reset_state();
                // self.current_image = iced::widget::image::Handle::from_memory(vec![]);
                println!("directory_path: {:?}", self.directory_path);
                println!("self.current_image_index: {}", self.current_image_index);
                for img_cache in self.img_caches.iter_mut() {
                    println!("img_cache.current_index: {}", img_cache.current_index);
                    println!("img_cache.image_paths.len(): {}", img_cache.image_paths.len());
                }
                Command::none()
            }
            Message::FolderOpened(result) => {
                match result {
                    Ok(dir) => {
                        println!("Folder opened: {}", dir);
                        self.initialize_dir_path(PathBuf::from(dir), 0);

                        Command::none()
                    }
                    Err(err) => {
                        println!("Folder open failed: {:?}", err);
                        Command::none()
                    }
                }
            }
            Message::OnVerResize(position) => { self.ver_divider_position = Some(position); Command::none() },
            Message::OnHorResize(position) => { self.hor_divider_position = Some(position); Command::none() },
            Message::ResetSplit(position) => {
                self.ver_divider_position = None; Command::none()
            },
            Message::ToggleSliderType(bool) => {
                self.toggle_slider_type();
                Command::none()
            },
            Message::TogglePaneLayout(pane_layout) => {
                self.toggle_pane_layout(pane_layout);
                Command::none()
            },

            Message::ImageLoaded (result) => {
                // v1: single pane
                /*let img_cache = &mut self.img_cache;
                match result {
                    Ok((image_data, operation)) => {
                        let _ = img_cache.being_loaded_queue.pop_front();

                        // println!("Image loaded [before shift] img_cache.current_index: {:?}, operation: {:?}", img_cache.current_index, operation);
                        println!("    Image Loaded");
                        if let Some(op) = operation {
                            match op {
                                LoadOperation::LoadNext(_target_index) => {
                                    let _ = img_cache.move_next(image_data);
                                }
                                LoadOperation::LoadPrevious(_target_index) => {
                                    let _ = img_cache.move_prev(image_data);
                                }
                                LoadOperation::ShiftNext(_target_index) => {
                                    let _ = img_cache.move_next(None);
                                }
                                LoadOperation::ShiftPrevious(_target_index) => {
                                    let _ = img_cache.move_prev(None);
                                }
                            }
                        }
                        let loaded_image = img_cache.get_current_image().unwrap().to_vec();
                        self.current_images = vec![
                            iced::widget::image::Handle::from_memory(loaded_image.clone()),
                            iced::widget::image::Handle::from_memory(loaded_image)
                        ];
                        self.current_image_index = img_cache.current_index;
                        self.slider_value = img_cache.current_index as u16;
                        self.title = format!("{}", img_cache.image_paths[img_cache.current_index].display());
                        
                        // println!("loading_queue length: {}", img_cache.loading_queue.len());
                        let command = self.load_image_by_operation();
                        // println!("Current image index: {}", self.current_image_index);
                        command
                            
                    }
                    Err(err) => {
                        println!("Image load failed: {:?}", err);
                        Command::none()
                    }
                }*/

                // v2: multiple panes
                // for img_cache in self.img_caches.iter_mut() {
                let mut img_cache = None;
                let mut cache_index = 0;
                match result {
                    Ok((image_data, operation)) => {
                        //let _ = img_cache.being_loaded_queue.pop_front();

                        if let Some(op) = operation {
                            match op {
                                LoadOperation::LoadNext((c_index, _target_index)) => {
                                    self.mark_image_loaded(c_index);
                                    // let img_cache = &mut self.img_caches[c_index];
                                    img_cache = Some(&mut self.img_caches[c_index]);
                                    cache_index = c_index;
                                    if let Some(cache) = img_cache.as_mut() {
                                        let _ = cache.being_loaded_queue.pop_front();
                                        let _ = cache.move_next(image_data);
                                    }
                                }
                                LoadOperation::LoadPrevious((c_index, _target_index)) => {
                                    self.mark_image_loaded(c_index);
                                    img_cache = Some(&mut self.img_caches[c_index]);
                                    cache_index = c_index;
                                    if let Some(cache) = img_cache.as_mut() {
                                        let _ = cache.being_loaded_queue.pop_front();
                                        let _ = cache.move_prev(image_data);
                                    }
                                }
                                LoadOperation::ShiftNext((c_index, _target_index)) => {
                                    self.mark_image_loaded(c_index);
                                    img_cache = Some(&mut self.img_caches[c_index]);
                                    cache_index = c_index;
                                    if let Some(cache) = img_cache.as_mut() {
                                        let _ = cache.being_loaded_queue.pop_front();
                                        let _ = cache.move_next(None);
                                    }
                                }
                                LoadOperation::ShiftPrevious((c_index, _target_index)) => {
                                    self.mark_image_loaded(c_index);
                                    img_cache = Some(&mut self.img_caches[c_index]);
                                    cache_index = c_index;
                                    if let Some(cache) = img_cache.as_mut() {
                                        let _ = cache.being_loaded_queue.pop_front();
                                        let _ = cache.move_prev(None);
                                    }
                                }
                            }
                        }

                        if let Some(mut cache) = img_cache.take() {
                            let loaded_image = cache.get_current_image().unwrap().to_vec();
                            let handle = iced::widget::image::Handle::from_memory(loaded_image.clone());
                            self.current_images[cache_index] = handle;
                        
                            // Update slider values
                            
                            if self.is_slider_dual {
                                println!("self.slider_values: {:?}", self.slider_values);
                                self.slider_values[cache_index] = cache.current_index as u16;
                                println!("self.slider_values: {:?}", self.slider_values);
                            } else {
                                println!("self.slider_value: {}", self.slider_value);
                                self.slider_value = cache.current_index as u16;
                                println!("self.slider_value: {}", self.slider_value);
                            }

                            img_cache = Some(cache);
                        }

                        println!("image load state: {:?}", self.image_load_state);

                        Command::none()
                    }
                    Err(err) => {
                        println!("Image load failed: {:?}", err);
                        Command::none()
                    }
                }

            }

            Message::SliderChanged(pane_index, value) => {
                println!("pane_index {} slider value: {}", pane_index, value);
                // -1 means the master slider (broadcast operation to all panes)
                if pane_index == -1 {
                    self.prev_slider_value = self.slider_value;
                    self.slider_value = value;
                    if value == self.prev_slider_value + 1 {
                        // Value changed by +1
                        // Call a function or perform an action for this case
                        self.move_right_all()
    
                    } else if value == self.prev_slider_value.saturating_sub(1) {
                        // Value changed by -1
                        // Call a different function or perform an action for this case
                        self.move_left_all()
                    } else {
                        // Value changed by more than 1 or it's the initial change
                        // Call another function or handle this case differently
                        self.update_pos(pane_index, value as usize);
                        Command::none()
                    }

                } else {
                    let pane_index = pane_index as usize;
                    println!("pane_index {} slider value: {}", pane_index, value);
                    self.prev_slider_values[pane_index] = self.slider_values[pane_index];
                    self.slider_values[pane_index] = value;
                    println!("pane_index {} prev slider value: {}", pane_index, self.prev_slider_values[pane_index]);
                    println!("pane_index {} slider value: {}", pane_index, self.slider_values[pane_index]);
                    
                    if value == self.prev_slider_values[pane_index] + 1 {
                        println!("move_right_index");
                        // Value changed by +1
                        // Call a function or perform an action for this case
                        self.move_right_index(pane_index)

                    } else if value == self.prev_slider_values[pane_index].saturating_sub(1) {
                        // Value changed by -1
                        // Call a different function or perform an action for this case
                        println!("move_left_index");
                        self.move_left_index(pane_index)
                    } else {
                        // Value changed by more than 1 or it's the initial change
                        // Call another function or handle this case differently
                        println!("update_pos");
                        self.update_pos(pane_index as isize, value as usize);
                        Command::none()
                    }
                }
            }


            Message::Event(event) => match event {
                // Only using for single pane layout
                #[cfg(any(target_os = "macos", target_os = "windows"))]
                Event::Window(iced::window::Event::FileDropped(dropped_paths, _position)) => {
                    match self.pane_layout {
                        PaneLayout::SinglePane => {
                            println!("File dropped: {:?}", dropped_paths.clone());

                            self.initialize_dir_path(dropped_paths[0].clone(), 0);
                            
                            Command::none()
                        },
                        PaneLayout::DualPane => {
                            Command::none()
                        }
                    }
                }
                #[cfg(target_os = "linux")]
                Event::Window(iced::window::Event::FileDropped(dropped_path)) => {
                    match self.pane_layout {
                        PaneLayout::SinglePane => {
                            println!("File dropped: {:?}", dropped_path);

                            self.initialize_dir_path(dropped_path, 0);
                            
                            Command::none()
                        },
                        PaneLayout::DualPane => {
                            Command::none()
                        }
                    }
                }

                Event::Keyboard(keyboard::Event::KeyPressed {
                    key_code: keyboard::KeyCode::Tab,
                    modifiers: _,
                }) => {
                    println!("Tab pressed");
                    Command::none()
                }

                Event::Keyboard(keyboard::Event::KeyPressed {
                    key_code: keyboard::KeyCode::Right,
                    modifiers: _,
                }) => {
                    println!("ArrowRight pressed");
                    println!("image load state bf: {:?}", self.image_load_state);
                    println!("dir_loaded: {:?}", self.dir_loaded);
                    println!("are_all_images_loaded: {}", self.are_all_images_loaded());
                    if self.are_all_images_loaded() {
                        self.init_image_loaded(); // [false, false]
                        println!("image load state af: {:?}", self.image_load_state);

                        // if a pane has reached the directory boundary, mark as loaded
                        let finished_indices: Vec<usize> = self.img_caches.iter().enumerate().filter_map(|(index, img_cache)| {
                            if img_cache.image_paths.len() > 0 && img_cache.current_index >= img_cache.image_paths.len() - 1 {
                                Some(index)
                            } else {
                                None
                            }
                        }).collect();
                        for finished_index in finished_indices.clone() {
                            self.mark_image_loaded(finished_index);
                        }
                        println!("finished_indices: {:?}", finished_indices);

                        self.move_right_all()
                    } else {
                        Command::none()
                    }
                }


                Event::Keyboard(keyboard::Event::KeyPressed {
                    key_code: keyboard::KeyCode::Left,
                    modifiers: _,
                }) => {
                    println!("ArrowLeft pressed");
                    //self.move_left()

                    if self.are_all_images_loaded() {
                        self.init_image_loaded(); // [false, false]
                        // if a pane has reached the directory boundary, mark as loaded
                        let finished_indices: Vec<usize> = self.img_caches.iter().enumerate().filter_map(|(index, img_cache)| {
                            if img_cache.image_paths.len() > 0 && img_cache.current_index <= 0 {
                                Some(index)
                            } else {
                                None
                            }
                        }).collect();
                        for finished_index in finished_indices {
                            self.mark_image_loaded(finished_index);
                        }

                        self.move_left_all()
                    } else {
                        Command::none()
                    }
                }

                _ => Command::none(),
            },
        }
    }

    fn view(&self) -> Element<Message> {
        let mb =  { menu_bar!(ui::menu_1(self), ui::menu_3(self))
                    .item_width(ItemWidth::Uniform(180))
                    .item_height(ItemHeight::Uniform(27)) }
                    .spacing(4.0)
                    .bounds_expand(30)
                    .main_offset(13)
                    .cross_offset(16)
                    .path_highlight(Some(PathHighlight::MenuActive))
                    .close_condition(CloseCondition {
                        leave: true,
                        click_outside: false,
                        click_inside: false,
                    });
        let r = row!(mb, horizontal_space(Length::Fill))
            .padding([2, 8])
            .align_items(alignment::Alignment::Center);
        let top_bar_style: fn(&iced::Theme) -> container::Appearance =
            |_theme| container::Appearance {
                background: Some(Color::TRANSPARENT.into()),
                ..Default::default()
            };
        let top_bar = container(r).width(Length::Fill).style(top_bar_style);

        /*let h_slider: iced::widget::Slider<u16, Message>;
        if self.dir_loaded {
            h_slider =
                slider(0..= (self.num_files-1) as u16, self.slider_value, Message::SliderChanged)
                    .width(Length::Fill);
        } else {
            h_slider =
                slider(0..= 0 as u16, 0, Message::SliderChanged)
                    .width(Length::Fill);
        }
        
        let first_img: Element<Message> = if self.dir_loaded[0] {
            Image::new(self.current_images[0].clone())
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        } else {
            text(String::from(""))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        };*/

        let container_all;
        match self.pane_layout {
            PaneLayout::SinglePane => {
                let first_img: iced::widget::Container<Message>  = if self.dir_loaded[0] {
                    container(column![
                        Image::new(self.current_images[0].clone())
                        .width(Length::Fill)
                        .height(Length::Fill),
                        //slider(0..= (self.img_caches[0].num_files-1) as u16, self.slider_values[0], Message::SliderChanged)
                        /*slider(
                            0..= (self.img_caches[0].num_files - 1) as u16,
                            self.slider_values[0],
                            |value| {
                                let pane_index = 0; // Replace this with the desired pane index
                                Message::SliderChanged((pane_index, value))
                            }
                        )*/
                        DualSlider::new(
                            0..= (self.img_caches[0].num_files - 1) as u16,
                            // self.slider_values[0],
                            self.slider_value,
                            -1,
                            Message::SliderChanged
                        )
                        .width(Length::Fill)
                        ]
                    )
                } else {
                    container(column![
                    text(String::from(""))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    
                    ])
                };
    
                container_all = container(
                    column![
                        top_bar,
                        first_img,
                        // h_slider,
                    ]
                    // .spacing(25),
                )
                .center_y();
            }
            PaneLayout::DualPane => {
                if self.is_slider_dual {
                    let panes = self.build_ui_dual_pane_slider2();
                    container_all = container(
                        column![
                            top_bar,
                            panes,
                            // h_slider,
                        ]
                        // .spacing(25),
                    )
                    .center_y();
                } else {
                    let panes = self.build_ui_dual_pane_slider1();

                    let max_num_files = self.img_caches.iter().fold(0, |max, cache| {
                        if cache.num_files > max {
                            cache.num_files
                        } else {
                            max
                        }
                    });
                    if self.dir_loaded[0] || self.dir_loaded[1] {
                        println!("self.slider_value at draw: {}", self.slider_value);
                        let h_slider = DualSlider::new(
                            0..= (max_num_files - 1) as u16,
                            self.slider_value,
                            -1, // -1 means all panes
                            Message::SliderChanged
                        );
                        
                        container_all = container(
                            column![
                                top_bar,
                                panes,
                                h_slider,
                            ]
                            .spacing(25),
                        )
                        .center_y();
                    } else {
                        container_all = container(
                            column![
                                top_bar,
                                panes,
                            ]
                            .spacing(25),
                        )
                        .center_y();}
                        
                }
            }
        }
        
        
        container_all
        .height(Length::Fill)
        .width(Length::Fill)
        .center_x()
        // .title(format!("{}", current_image_path.display()))
        //.title(current_image_path.to_string_lossy().to_string())
        .into()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        // subscription::events().map(Message::Event)

        Subscription::batch(vec![
            subscription::events().map(Message::Event),
        ])
    }

    fn theme(&self) -> Self::Theme {
        Theme::Dark
        // Theme::default()
    }
}

fn main() -> iced::Result {
    env_logger::init();
    use iced::window;

    // let app_icon_data = include_bytes!("../icon_v0.png"); // Replace with your icon path

    // Load the image using the image crate
    // self.current_images = vec![iced::widget::image::Handle::from_memory(vec![])
    // let app_icon_image = iced::widget::image::Handle::from_memory(app_icon_data);
    // let icon = window::icon::Icon::from(app_icon_image);
    // let icon = window::Icon::from_rgba(app_icon_data.to_vec(), 64, 64);

    // let icon =  iced::window::icon::from_file("../icon_v0.png");
    
    // let icon =  iced::window::icon::from_file("v2.png");
    let icon =  iced::window::icon::from_file("icon.ico");
    match icon {
        Ok(icon) => {
            println!("Icon loaded successfully");
            let settings = Settings {
                window: window::Settings {
                    icon: Some(icon),
                    ..Default::default()
                },
                ..Settings::default()
            };
            DataViewer::run(settings)
        }
        Err(err) => {
            println!("Icon load failed: {:?}", err);
            DataViewer::run(Settings::default())
        }
    }


    // DataViewer::run(Settings::default())
    
}