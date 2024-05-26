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


use crate::image_cache;
use crate::ui_builder::get_footer;
use crate::Message;
use std::path::Path;
use std::path::PathBuf;

use crate::file_io;
use crate::file_io::{is_file, is_directory, get_file_index};

use iced::widget::{
    //container, row, column, slider, horizontal_space, text
    container, column, text
};
use iced::{Element, Length};
use crate::dualslider::dualslider::DualSlider;
use crate::menu::PaneLayout;

use crate::split::split::{Axis, Split};
use crate::viewer;

use crate::image_cache::LoadOperation;
use iced::Command;
use crate::image_cache::load_image_by_operation;
use std::time::Instant;
use crate::DataViewer;


// ref: https://github.com/iced-rs/iced/blob/master/examples/todos/src/main.rs
#[derive(Debug, Clone)]
pub enum PaneMessage {
}

#[derive(Clone)]
pub struct Pane {
    pub directory_path: Option<String>,
    pub dir_loaded: bool,
    pub img_cache: image_cache::ImageCache,
    pub current_image: iced::widget::image::Handle,
    pub is_next_image_loaded: bool, // whether the next image in cache is loaded
    pub is_prev_image_loaded: bool, // whether the previous image in cache is loaded
    pub slider_value: u16,
    pub prev_slider_value: u16,

    pub id: usize,
    pub is_selected: bool,
    pub is_selected_cache: bool,
}

impl Default for Pane {
    fn default() -> Self {
        Self {
            directory_path: None,
            dir_loaded: false,
            img_cache: image_cache::ImageCache::default(),
            current_image: iced::widget::image::Handle::from_memory(vec![]),
            is_next_image_loaded: true,
            is_prev_image_loaded: true,
            slider_value: 0,
            prev_slider_value: 0,
            id: 0,
            is_selected: true,
            is_selected_cache: true,
        }
    }
}

impl Pane {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            directory_path: None,
            dir_loaded: false,
            img_cache: image_cache::ImageCache::default(),
            current_image: iced::widget::image::Handle::from_memory(vec![]),
            is_next_image_loaded: true,
            is_prev_image_loaded: true,
            slider_value: 0,
            prev_slider_value: 0,
            id: 0,
            is_selected: true,
            is_selected_cache: true,
        }
    }

    pub fn reset_state(&mut self) {
        self.directory_path = None;
        self.dir_loaded = false;
        self.img_cache = image_cache::ImageCache::default();
        self.current_image = iced::widget::image::Handle::from_memory(vec![]);
        self.is_next_image_loaded = true;
        self.slider_value = 0;
        self.prev_slider_value = 0;
    }

    pub fn is_cached_next(&self) -> bool {
        /*println!("pane.is_selected: {}, pane.dir_loaded: {}, pane.img_cache.is_next_cache_index_within_bounds(): {}, pane.img_cache.loading_queue.len(): {}, pane.img_cache.being_loaded_queue.len(): {}",
            pane.is_selected, pane.dir_loaded, pane.img_cache.is_next_cache_index_within_bounds(), pane.img_cache.loading_queue.len(), pane.img_cache.being_loaded_queue.len());

        pane.is_selected && pane.dir_loaded && pane.img_cache.is_next_cache_index_within_bounds() &&
            pane.img_cache.loading_queue.len() < 3 && pane.img_cache.being_loaded_queue.len() < 3*/
        
        println!("is_selected: {}, dir_loaded: {}, is_next_image_loaded: {}, img_cache.is_next_cache_index_within_bounds(): {}, img_cache.loading_queue.len(): {}, img_cache.being_loaded_queue.len(): {}",
            self.is_selected, self.dir_loaded, self.is_next_image_loaded, self.img_cache.is_next_cache_index_within_bounds(), self.img_cache.loading_queue.len(), self.img_cache.being_loaded_queue.len());

        self.is_selected && self.dir_loaded && self.img_cache.is_next_cache_index_within_bounds() &&
            self.img_cache.loading_queue.len() < 3 && self.img_cache.being_loaded_queue.len() < 3
    }

    //pub fn load_next_images(&mut self, cache_index: usize) -> Vec<Command<<DataViewer as iced::Application>::Message>>{
    pub fn load_next_images(&mut self, cache_index: usize) -> Vec<Command<Message>>{
        // NOTE: BEFORE the call of this method, current_index and current_offset got incremented in set_next_image()

        /*let mut commands = Vec::new();
        let img_cache = &mut self.img_cache;

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

        commands*/



        let mut commands = Vec::new();
        let img_cache = &mut self.img_cache;
        let current_index_before_render = img_cache.current_index - 1;

        // If there are images to load and the current index is not the last index
        if img_cache.image_paths.len() > 0 && current_index_before_render < img_cache.image_paths.len() - 1 {
            // Get the index of next image: consider the current_offset
            ////let next_image_index_to_load = current_index_before_render as isize + img_cache.cache_count as isize + 1;
            //let next_image_index_to_load = img_cache.current_index as isize + (img_cache.cache_count as isize -  img_cache.current_offset) as isize + 1;
            let next_image_index_to_load = img_cache.current_index as isize - img_cache.current_offset + img_cache.cache_count as isize + 1;

            assert!(next_image_index_to_load >= 0);
            let next_image_index_to_load_usize = next_image_index_to_load as usize;

            println!("LOADING NEXT: next_image_index_to_load: {}, current_index: {}, current_offset: {}",
                next_image_index_to_load, img_cache.current_index, img_cache.current_offset);
                
            println!("load_prev_images: is_blocking_loading_ops_in_queue: {}", img_cache.is_blocking_loading_ops_in_queue(LoadOperation::LoadNext((cache_index, next_image_index_to_load_usize))));

            if img_cache.is_image_index_within_bounds(next_image_index_to_load) &&
                img_cache.is_next_image_index_in_queue(cache_index, next_image_index_to_load) &&
                !img_cache.is_blocking_loading_ops_in_queue(LoadOperation::LoadNext((cache_index, next_image_index_to_load_usize)))
            {
                // TODO: BUGS HERE? need to consider offset
                /*if next_image_index_to_load_usize < img_cache.image_paths.len() &&
                ( current_index_before_render >= img_cache.cache_count &&
                    current_index_before_render <= (img_cache.image_paths.len()-1) - img_cache.cache_count) {
                    img_cache.enqueue_image_load(LoadOperation::LoadNext((cache_index, next_image_index_to_load_usize)));
                    
                } else if current_index_before_render < img_cache.cache_count {
                    let prev_image_index_to_load = current_index_before_render as isize - img_cache.cache_count as isize + 1;
                    img_cache.enqueue_image_load(LoadOperation::ShiftNext((cache_index, prev_image_index_to_load)));
                } else {
                    img_cache.enqueue_image_load(LoadOperation::ShiftNext((cache_index, next_image_index_to_load)));
                }*/
                
                if next_image_index_to_load_usize >= img_cache.num_files || img_cache.current_offset < 0 {
                    img_cache.enqueue_image_load(LoadOperation::ShiftNext((cache_index, next_image_index_to_load)));
                } else {
                    img_cache.enqueue_image_load(LoadOperation::LoadNext((cache_index, next_image_index_to_load_usize)));
                }

            }

            println!("LOADING QUEUED:");
            img_cache.print_queue();

            let command = load_image_by_operation(img_cache);
            commands.push(command);
        } else {
            commands.push(Command::none())
        }

        commands
    }

    pub fn set_next_image(&mut self, pane_layout: &PaneLayout, is_slider_dual: bool) -> bool {
        let img_cache = &mut self.img_cache;
        let mut did_render_happen = false;

        //if img_cache.is_some_at_index(img_cache.cache_count as usize + img_cache.current_offset as usize
        if img_cache.is_some_at_index(img_cache.cache_count as usize + img_cache.current_offset as usize + 1
        ) {
            let next_image_index_to_render = img_cache.cache_count as isize + img_cache.current_offset + 1;
            println!("BEGINE RENDERING NEXT: next_image_index_to_render: {} current_index: {}, current_offset: {}",
                next_image_index_to_render, img_cache.current_index, img_cache.current_offset);

            let loaded_image = img_cache.get_image_by_index(next_image_index_to_render as usize).unwrap().to_vec();
            let handle = iced::widget::image::Handle::from_memory(loaded_image.clone());

            self.current_image = handle;
            img_cache.current_offset += 1;

            // Since the next image is loaded and rendered, mark the is_next_image_loaded flag
            self.is_next_image_loaded = true;
            did_render_happen = true;

            // NEW: handle current_index here without performing LoadingOperation::ShiftPrevious
            //println!("(img_cache.image_paths.len()-1) - img_cache.cache_count -1 = {}", (img_cache.image_paths.len()-1) - img_cache.cache_count -1);
            if img_cache.current_index < img_cache.image_paths.len() - 1 {
                img_cache.current_index += 1;
            }
            //println!("RENDERED NEXT: current_index: {}, current_offset: {}", img_cache.current_index, img_cache.current_offset);
            
            if *pane_layout == PaneLayout::DualPane && is_slider_dual {
                //println!("dualpane && is_slider_dual slider update");
                self.slider_value = img_cache.current_index as u16;
            }
            println!("END RENDERING NEXT: current_index: {}, current_offset: {}", img_cache.current_index, img_cache.current_offset);
        }

        did_render_happen
    }

    pub fn load_prev_images(&mut self, cache_index: usize) -> Vec<Command<Message>> {
        let mut commands = Vec::new();
        let img_cache = &mut self.img_cache;

        /*if img_cache.current_index > 0 {
            let prev_image_index_to_load: isize = img_cache.current_index as isize  - img_cache.cache_count as isize  - 1;
            println!("LOADING PREV: next_image_index_to_load: {}, current_index: {}, current_offset: {}",
                prev_image_index_to_load, img_cache.current_index, img_cache.current_offset);

            if img_cache.is_image_index_within_bounds(prev_image_index_to_load) {
                // TODO: organize this better
                if prev_image_index_to_load >= 0 &&
                (img_cache.current_index >= img_cache.cache_count &&
                img_cache.current_index <= (img_cache.image_paths.len()-1) - img_cache.cache_count) {
                    img_cache.enqueue_image_load(LoadOperation::LoadPrevious((cache_index, prev_image_index_to_load as usize)));

                } else if img_cache.current_index > (img_cache.image_paths.len()-1) - img_cache.cache_count -1 {
                    let next_image_index_to_load = img_cache.current_index as isize - img_cache.cache_count as isize - 1;
                    img_cache.enqueue_image_load(LoadOperation::ShiftPrevious((cache_index, next_image_index_to_load)));
                } else {
                    img_cache.enqueue_image_load(LoadOperation::ShiftPrevious((cache_index, prev_image_index_to_load)));
                }
            }
            img_cache.print_queue();
            
            let command = load_image_by_operation(img_cache);
            commands.push(command);
        } else {
            commands.push(Command::none())
        }*/

        let current_index_before_render = img_cache.current_index + 1;
        if img_cache.current_index >= 0 {
            //let prev_image_index_to_load: isize = current_index_before_render as isize  - img_cache.cache_count as isize  - 1;

            //let next_image_index_to_load = img_cache.current_index as isize - img_cache.current_offset + img_cache.cache_count as isize + 1;
            //let prev_image_index_to_load = img_cache.current_index as isize - img_cache.current_offset - img_cache.cache_count as isize - 1;

            //let prev_image_index_to_load = (img_cache.current_index as isize + ((img_cache.cache_count as isize) + img_cache.current_offset) as isize) - 1;
            let prev_image_index_to_load = (img_cache.current_index as isize + (-(img_cache.cache_count as isize) - img_cache.current_offset) as isize) - 1;
            println!("LOADING PREV: prev_image_index_to_load: {}, current_index: {}, current_offset: {}",
                prev_image_index_to_load, img_cache.current_index, img_cache.current_offset);

            /*if img_cache.is_image_index_within_bounds(prev_image_index_to_load) {
                // TODO: organize this better
                if prev_image_index_to_load >= 0 &&
                (current_index_before_render >= img_cache.cache_count &&
                    current_index_before_render <= (img_cache.image_paths.len()-1) - img_cache.cache_count) {
                    img_cache.enqueue_image_load(LoadOperation::LoadPrevious((cache_index, prev_image_index_to_load as usize)));

                } else if current_index_before_render > (img_cache.image_paths.len()-1) - img_cache.cache_count -1 {
                    let next_image_index_to_load = current_index_before_render as isize - img_cache.cache_count as isize - 1;
                    img_cache.enqueue_image_load(LoadOperation::ShiftPrevious((cache_index, next_image_index_to_load)));
                } else {
                    img_cache.enqueue_image_load(LoadOperation::ShiftPrevious((cache_index, prev_image_index_to_load)));
                }
            }*/

            println!("load_prev_images: is_blocking_loading_ops_in_queue: {}", img_cache.is_blocking_loading_ops_in_queue(LoadOperation::LoadPrevious((cache_index, prev_image_index_to_load as usize))));

            if img_cache.is_image_index_within_bounds(prev_image_index_to_load) && 
                img_cache.is_next_image_index_in_queue(cache_index, prev_image_index_to_load) &&
                !img_cache.is_blocking_loading_ops_in_queue(LoadOperation::LoadPrevious((cache_index, prev_image_index_to_load as usize)))
            {
                //if next_image_index_to_load_usize >= img_cache.num_files {
                //if prev_image_index_to_load > img_cache.cache_count as isize {
                if prev_image_index_to_load >= 0 || img_cache.current_offset > 0 {
                    img_cache.enqueue_image_load(LoadOperation::LoadPrevious((cache_index, prev_image_index_to_load as usize)));
                } else {
                    img_cache.enqueue_image_load(LoadOperation::ShiftPrevious((cache_index, prev_image_index_to_load)));
                }
            }

            img_cache.print_queue();
            
            let command = load_image_by_operation(img_cache);
            commands.push(command);
        } else {
            commands.push(Command::none())
        }

        commands
    }

    pub fn set_prev_image(&mut self, pane_layout: &PaneLayout, is_slider_dual: bool) -> bool {
        let img_cache = &mut self.img_cache;
        let mut did_render_happen = false;

        // Render the previous one right away
        // Avoid loading around the edges
        if !self.is_prev_image_loaded && img_cache.cache_count as isize + img_cache.current_offset > 0 &&
            img_cache.is_some_at_index( (img_cache.cache_count as isize + img_cache.current_offset) as usize) {

            let next_image_index_to_render = img_cache.cache_count as isize + (img_cache.current_offset - 1);
            println!("RENDERING PREV: next_image_index_to_render: {} current_index: {}, current_offset: {}",
                next_image_index_to_render, img_cache.current_index, img_cache.current_offset);

            if img_cache.is_image_index_within_bounds(next_image_index_to_render) {
                let loaded_image = img_cache.get_image_by_index(next_image_index_to_render as usize).unwrap().to_vec();
                let handle = iced::widget::image::Handle::from_memory(loaded_image.clone());
                self.current_image = handle;
                img_cache.current_offset -= 1;

                assert!(img_cache.current_offset >= -5);

                // Since the prev image is loaded and rendered, mark the is_prev_image_loaded flag
                self.is_prev_image_loaded = true;

                //println!("(img_cache.image_paths.len()-1) - img_cache.cache_count -1 = {}", (img_cache.image_paths.len()-1) - img_cache.cache_count -1);
                //println!("img_cache.current_index <= img_cache.cache_count: {}", img_cache.current_index <= img_cache.cache_count);

                if img_cache.current_index > 0 {
                    img_cache.current_index -= 1;
                }
                println!("RENDERED PREV: current_index: {}, current_offset: {}",
                img_cache.current_index, img_cache.current_offset);

                if *pane_layout == PaneLayout::DualPane && is_slider_dual {
                    self.slider_value = img_cache.current_index as u16;
                }

                did_render_happen = true;
            }
        }

        did_render_happen
    }


    // Allowing for the sake of `is_dir_size_bigger`
    #[allow(unused_assignments)]
    pub fn initialize_dir_path(&mut self, pane_layout: &PaneLayout,
        pane_file_lengths: &[usize], _pane_index: usize, path: PathBuf,
        is_slider_dual: bool, slider_value: &mut u16) {
        let mut _file_paths: Vec<PathBuf> = Vec::new();
        let initial_index: usize;
        
        //let min_current_index_in_panes = panes.iter().map(|pane| pane.slider_value).min().unwrap_or(0);
        //let min_current_index_in_panes = pane_slider_values.iter().min().unwrap_or(&0);
        // min current slider value in panes except the current pane_index
        //let min_current_index_in_panes = pane_slider_values.iter().enumerate().filter(|(i, _)| *i != pane_index).map(|(_, v)| v).min().unwrap_or(&0);
        
        
        let mut is_dir_size_bigger: bool = false;
        if is_file(&path) {
            println!("Dropped path is a file");
            let directory = path.parent().unwrap_or(Path::new(""));
            let dir = directory.to_string_lossy().to_string();
            self.directory_path = Some(dir);

            _file_paths = file_io::get_image_paths(Path::new(&self.directory_path.clone().unwrap()));
            let file_index = get_file_index(&_file_paths, &path);

            let longest_file_length = pane_file_lengths.iter().max().unwrap_or(&0);
            is_dir_size_bigger = if *pane_layout == PaneLayout::SinglePane {
                true
            } else if *pane_layout == PaneLayout::DualPane && is_slider_dual {
                true
            } else {
                _file_paths.len() >= *longest_file_length
            };
            println!("longest_file_length: {:?}, is_dir_size_bigger: {:?}", longest_file_length, is_dir_size_bigger);

            if let Some(file_index) = file_index {
                println!("File index: {}", file_index);
                initial_index = file_index;
                // self.current_image_index = file_index;
                
                // self.slider_values[pane_index] = file_index as u16;
                // self.panes[pane_index].slider_value = file_index as u16;
                let current_slider_value = file_index as u16;
                println!("current_slider_value: {:?}", current_slider_value);
                if is_slider_dual {
                    *slider_value = current_slider_value;
                    self.slider_value = current_slider_value;
                } else {
                    if is_dir_size_bigger {
                        *slider_value = current_slider_value;
                    }
                }
                println!("slider_value: {:?}", *slider_value);
            } else {
                println!("File index not found");
                return;
            }

        } else if is_directory(&path) {
            println!("Dropped path is a directory");
            self.directory_path = Some(path.to_string_lossy().to_string());
            //_file_paths = get_file_paths(Path::new(&self.directory_path.clone().unwrap()));
            _file_paths = file_io::get_image_paths(Path::new(&self.directory_path.clone().unwrap()));
            
            initial_index = 0;
            // Display the first 100 paths
            /*for path in _file_paths.iter().take(100) {
                println!("{}", path.display());
            }*/

            
            let longest_file_length = pane_file_lengths.iter().max().unwrap_or(&0);
            is_dir_size_bigger = if *pane_layout == PaneLayout::SinglePane {
                true
            } else if *pane_layout == PaneLayout::DualPane && is_slider_dual {
                true
            } else {
                _file_paths.len() >= *longest_file_length
            };
            println!("longest_file_length: {:?}, is_dir_size_bigger: {:?}", longest_file_length, is_dir_size_bigger);
            let current_slider_value = 0;
            println!("current_slider_value: {:?}", current_slider_value);
            if is_slider_dual {
                *slider_value = current_slider_value;
                self.slider_value = current_slider_value;
            } else {
                if is_dir_size_bigger {
                    *slider_value = current_slider_value;
                }
            }
            println!("slider_value: {:?}", *slider_value);
        } else {
            println!("Dropped path does not exist or cannot be accessed");
            // Handle the case where the path does not exist or cannot be accessed
            return;
        }

        // Sort
        //alphanumeric_sort::sort_path_slice(&mut _file_paths);


        println!("File paths: {}", _file_paths.len());
        // self.dir_loaded[pane_index] = true;
        // self.panes[pane_index].dir_loaded = true;
        self.dir_loaded = true;

        // Instantiate a new image cache and load the initial images
        let mut img_cache =  image_cache::ImageCache::new(
            _file_paths,
            //2,
            5,
            //100,
            initial_index,
        ).unwrap();
        img_cache.load_initial_images().unwrap();
        img_cache.print_cache();
        

        let loaded_image = img_cache.get_initial_image().unwrap().to_vec();
        let handle = iced::widget::image::Handle::from_memory(loaded_image.clone());
        self.current_image = handle;

        let longest_file_length = pane_file_lengths.iter().max().unwrap_or(&0);
        
        println!("longest_file_length: {:?}, is_dir_size_bigger: {:?}", longest_file_length, is_dir_size_bigger);
        let current_slider_value = initial_index as u16;
        println!("current_slider_value: {:?}", current_slider_value);
        if is_slider_dual {
            //*slider_value = current_slider_value;
        } else {
            if is_dir_size_bigger {
                *slider_value = current_slider_value;
            }
        }
        println!("slider_value: {:?}", *slider_value);

        let file_paths = img_cache.image_paths.clone();
        println!("file_paths.len() {:?}", file_paths.len());
        
        self.img_cache = img_cache;
        println!("img_cache.cache_count {:?}", self.img_cache.cache_count);
        
        
    }

    #[allow(dead_code)]
    pub fn update(&mut self, message: PaneMessage) {
        match message {
        }
    }

    pub fn build_ui_dual_pane_slider1(&self) -> iced::widget::Container<Message> {
        let img: iced::widget::Container<Message>  = if self.dir_loaded {
            container(column![
                //Image::new(self.current_image.clone())
                viewer::Viewer::new(self.current_image.clone())
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
        img
    }
}

pub fn get_master_slider_value(panes: &[Pane], pane_layout: &PaneLayout, is_slider_dual: bool, last_opened_pane: usize) -> usize {
    let mut max_dir_size = 0;
    let mut max_dir_size_index = 0;
    //println!("get_master_slider_value - panes.len(): {:?}", panes.len());
    for (i, pane) in panes.iter().enumerate() {
        if pane.dir_loaded {
            if pane.img_cache.num_files > max_dir_size {
                max_dir_size = pane.img_cache.num_files;
                max_dir_size_index = i;
            }
        }
    }

    // If the directory size of the pane of max_dir_size_index and the pane of last_opened_pane is the same, 
    // select (prioritize) the last_opened_pane's current_index
    if pane_layout == &PaneLayout::DualPane && !is_slider_dual &&
        panes[max_dir_size_index].img_cache.num_files == panes[last_opened_pane].img_cache.num_files {
        return panes[last_opened_pane].img_cache.current_index as usize;
    }

    let pane = &panes[max_dir_size_index];
    ////(pane.img_cache.current_index as usize) + pane.img_cache.current_offset as usize
    ////(pane.img_cache.current_index as isize + pane.img_cache.current_offset) as usize
    pane.img_cache.current_index as usize
}

pub fn build_ui_dual_pane_slider1(panes: &[Pane], ver_divider_position: Option<u16>) -> Element<Message> {
    let first_img: iced::widget::Container<Message>  = panes[0].build_ui_dual_pane_slider1();
    let second_img: iced::widget::Container<Message> = panes[1].build_ui_dual_pane_slider1();

    let is_selected: Vec<bool> = panes.iter().map(|pane| pane.is_selected).collect();
    Split::new(
        false,
        first_img,
        second_img,
        is_selected,
        ver_divider_position,
        Axis::Vertical,
        Message::OnVerResize,
        Message::ResetSplit,
        Message::FileDropped,
        Message::PaneSelected
    )
    .into()
}

pub fn build_ui_dual_pane_slider2(panes: &[Pane], ver_divider_position: Option<u16>, show_footer: bool) -> Element<Message> {
    let footer_texts = vec![
        format!(
            "{}/{}",
            panes[0].img_cache.current_index + 1,
            panes[0].img_cache.num_files
        ),
        format!(
            "{}/{}",
            panes[1].img_cache.current_index + 1,
            panes[1].img_cache.num_files
        )
    ];

    let first_img: iced::widget::Container<Message> = if panes[0].dir_loaded {
        container(
            if show_footer { column![
                // NOTE: Wrapping the image in a container messes up the layout
                //Image::new(panes[0].current_image.clone())
                viewer::Viewer::new(panes[0].current_image.clone())
                .width(Length::Fill)
                .height(Length::Fill),
                DualSlider::new(
                    0..= (panes[0].img_cache.num_files - 1) as u16,
                    panes[0].slider_value,
                    0,
                    Message::SliderChanged,
                    Message::SliderReleased
                )
                .width(Length::Fill),
                get_footer(footer_texts[0].clone(), 0)
            ]} else { column![
                //Image::new(panes[0].current_image.clone())
                viewer::Viewer::new(panes[0].current_image.clone())
                .width(Length::Fill)
                .height(Length::Fill),
                DualSlider::new(
                    0..= (panes[0].img_cache.num_files - 1) as u16,
                    panes[0].slider_value,
                    0,
                    Message::SliderChanged,
                    Message::SliderReleased
                )
                .width(Length::Fill),
            ]}
        )
    } else {
        container(column![
            text(String::from(""))
                .width(Length::Fill)
                .height(Length::Fill),
        ])
    };

    let second_img: iced::widget::Container<Message> = if panes[1].dir_loaded {
        container(
            if show_footer { column![
                // NOTE: Wrapping the image in a container messes up the layout
                viewer::Viewer::new(panes[1].current_image.clone())
                .width(Length::Fill)
                .height(Length::Fill),
                DualSlider::new(
                    0..= (panes[1].img_cache.num_files - 1) as u16,
                    panes[1].slider_value,
                    1,
                    Message::SliderChanged,
                    Message::SliderReleased
                )
                .width(Length::Fill),
                get_footer(footer_texts[1].clone(), 1)
            ]} else { column![
                viewer::Viewer::new(panes[1].current_image.clone())
                .width(Length::Fill)
                .height(Length::Fill),
                DualSlider::new(
                    0..= (panes[1].img_cache.num_files - 1) as u16,
                    panes[1].slider_value,
                    1,
                    Message::SliderChanged,
                    Message::SliderReleased
                )
                .width(Length::Fill),
            ]}

        )
    } else {
        container(column![
            text(String::from(""))
                .width(Length::Fill)
                .height(Length::Fill),
        ])
    };

    let is_selected: Vec<bool> = panes.iter().map(|pane| pane.is_selected).collect();
    Split::new(
        true,
        first_img,
        second_img,
        is_selected,
        ver_divider_position,
        Axis::Vertical,
        Message::OnVerResize,
        Message::ResetSplit,
        Message::FileDropped,
        Message::PaneSelected
    )
    .into()
}