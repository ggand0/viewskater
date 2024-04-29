
#![windows_subsystem = "windows"]

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
use iced::{Element, Length, Application, Theme, Settings, Command};

use std::path::PathBuf;
use log::{debug, info, warn, error};

// #[macro_use]
extern crate log;

mod image_cache;
use crate::image_cache::ImageCache;
use image_cache::LoadOperation;
use image_cache::{move_right_all, move_left_all,
    update_pos, load_remaining_images};
mod file_io;
use file_io::Error;

mod menu;
use menu::PaneLayout;

mod split {
    pub mod split; // Import the module from split/split.rs
    pub mod style; // Import the module from split/style.rs
}
mod dualslider {
    pub mod dualslider;
    pub mod style;
}
//use dualslider::dualslider::DualSlider;

mod pane;
use crate::pane::get_master_slider_value;
mod ui_builder;
mod viewer;


#[derive(Debug, Clone, Copy)]
pub enum MenuItem {
    Open,
    Close,
    Help
}

/*enum SliderType {
    Single,
    Dual,
}*/


// Define the application state
// #[derive(Default)]
pub struct DataViewer {
    title: String,
    directory_path: Option<String>,
    current_image_index: usize,
    slider_value: u16,                  // for master slider
    prev_slider_value: u16,             // for master slider
    ver_divider_position: Option<u16>,
    hor_divider_position: Option<u16>,
    //pane_count: usize,
    is_slider_dual: bool,
    pane_layout: PaneLayout,
    last_opened_pane: isize,
    panes: Vec<pane::Pane>,             // Each pane has its own image cache
    skate_right: bool,
    skate_left: bool,
    update_counter: u32,
}

impl Default for DataViewer {
    fn default() -> Self {
        Self {
            title: String::from("View Skater"),
            directory_path: None,
            current_image_index: 0,
            slider_value: 0,
            prev_slider_value: 0,
            ver_divider_position: None,
            hor_divider_position: None,
            //pane_count: 2,
            is_slider_dual: false,
            pane_layout: PaneLayout::SinglePane,
            last_opened_pane: -1,
            panes: vec![pane::Pane::default()],
            skate_right: false,
            skate_left: false,
            update_counter: 0,
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
    SliderReleased(isize, u16),
    Event(Event),
    // ImageLoaded(Result<(), std::io::ErrorKind>),// std::io::Error doesn't seem to be clonable
    // ImageLoaded(Result<Option<Vec<u8>>, std::io::ErrorKind>),
    ImageLoaded(Result<(Option<Vec<u8>>, Option<LoadOperation>), std::io::ErrorKind>),
    OnVerResize(u16),
    OnHorResize(u16),
    ResetSplit(u16),
    ToggleSliderType(bool),
    TogglePaneLayout(PaneLayout),
    PaneSelected(usize, bool),
}

impl DataViewer {
    fn reset_state(&mut self) {
        self.title = String::from("View Skater");
        self.directory_path = None;
        self.current_image_index = 0;
        self.slider_value = 0;
        self.prev_slider_value = 0;
        self.last_opened_pane = 0;
        for pane in self.panes.iter_mut() {
            pane.reset_state();
        }
        self.skate_right = false;
        self.update_counter = 0;
    }

    // Function to initialize image_load_state for all panes
    fn init_image_loaded(&mut self) {
        for pane in self.panes.iter_mut() {
            if self.pane_layout == PaneLayout::DualPane && self.is_slider_dual {
                if pane.is_selected && pane.dir_loaded {
                    pane.image_load_state = false;
                }
            } else {
                pane.image_load_state = false;
            }
        }
    }

    // Function to mark an image as loaded for a specific pane
    fn mark_image_loaded(&mut self, pane_index: usize) {
        if let Some(pane) = self.panes.get_mut(pane_index) {
            pane.image_load_state = true;
        }
    }

    // Function to check if all images are loaded for all panes
    fn are_all_images_loaded(&self) -> bool {
        //self.panes.iter().all(|pane| !pane.dir_loaded || (pane.dir_loaded && pane.image_load_state))

        if self.is_slider_dual {
            self.panes
            .iter()
            .filter(|pane| pane.is_selected)  // Filter only selected panes
            .all(|pane| !pane.dir_loaded || (pane.dir_loaded && pane.image_load_state))
        } else {
            //self.panes.iter().all(|pane| !pane.dir_loaded || (pane.dir_loaded && pane.image_load_state))
            self.panes.iter().all(|pane| !pane.dir_loaded || (pane.dir_loaded ))
        }
    }

    fn are_all_images_cached(&self) -> bool {
        if self.is_slider_dual {
            self.panes
            .iter()
            .filter(|pane| pane.is_selected)  // Filter only selected panes
            .all(|pane| pane.dir_loaded && pane.img_cache.is_next_cache_index_within_bounds() &&
            pane.img_cache.loading_queue.len() < 3 && pane.img_cache.being_loaded_queue.len() < 3)
        } else {
            /*println!("self.panes[0].img_cache.get_next_cache_index(): {}", self.panes[0].img_cache.get_next_cache_index());
            println!("self.panes[0].img_cache.get_next_cache_index_within_bounds(): {}", self.panes[0].img_cache.is_next_cache_index_within_bounds());
            self.panes[0].img_cache.print_queue();
            self.panes[0].img_cache.print_cache();*/
            self.panes.iter().all(|pane|
                pane.dir_loaded && pane.img_cache.is_next_cache_index_within_bounds() &&
                pane.img_cache.loading_queue.len() < 3 && pane.img_cache.being_loaded_queue.len() < 3)
        }
    }

    fn are_all_images_cached_prev(&self) -> bool {
        if self.is_slider_dual {
            self.panes
            .iter()
            .filter(|pane| pane.is_selected)  // Filter only selected panes
            .all(|pane| pane.dir_loaded && pane.img_cache.is_prev_cache_index_within_bounds() &&
            pane.img_cache.loading_queue.len() < 3 && pane.img_cache.being_loaded_queue.len() < 3)
        } else {
            self.panes.iter().all(|pane|
                pane.dir_loaded && pane.img_cache.is_prev_cache_index_within_bounds() &&
                pane.img_cache.loading_queue.len() < 3 && pane.img_cache.being_loaded_queue.len() < 3)
        }
    }

    fn are_all_images_loaded_index(&self, next_index: usize) -> bool {
        if self.is_slider_dual {
            self.panes
            .iter()
            .filter(|pane| pane.is_selected)  // Filter only selected panes
            .all(|pane| !pane.dir_loaded || (pane.dir_loaded && pane.image_load_state))
        } else {
            //self.panes.iter().all(|pane| !pane.dir_loaded || (pane.dir_loaded && pane.image_load_state))
            self.panes.iter().all(|pane| !pane.dir_loaded || (pane.dir_loaded && pane.img_cache.is_next_image_loaded(next_index)))
        }
    }

    fn are_all_images_loaded_in_selected(&self) -> bool {
        self.panes
            .iter()
            .filter(|pane| pane.is_selected)  // Filter only selected panes
            .all(|pane| !pane.dir_loaded || (pane.dir_loaded && pane.image_load_state))
    }

    fn initialize_dir_path(&mut self, path: PathBuf, pane_index: usize) {
        debug!("last_opened_pane: {}", self.last_opened_pane);
        //self.panes[pane_index].initialize_dir_path(path);

        //let pane_slider_values = self.panes.iter().map(|pane| pane.slider_value).collect::<Vec<u16>>();
        let pane_file_lengths = self.panes.iter().map(|pane| pane.img_cache.image_paths.len()).collect::<Vec<usize>>();
        let pane = &mut self.panes[pane_index];
        //self.panes[pane_index].initialize_dir_path(&self.panes, pane_index, path, self.is_slider_dual);
        //pane.initialize_dir_path(panes, pane_index, path, self.is_slider_dual);
        println!("pane_file_lengths: {:?}", pane_file_lengths);
        pane.initialize_dir_path(
            &self.pane_layout, &pane_file_lengths, pane_index, path, self.is_slider_dual, &mut self.slider_value);


        // If the index is greater than or equal to the length of current_images,
        // fill the vector with default handles until the desired index
        // NOTE: I don't know if this is needed => turns out I do need it
        let _default_handle = iced::widget::image::Handle::from_memory(vec![]);
        /*if pane_index >= self.current_images.len() {
            self.current_images.resize_with(pane_index + 1, || default_handle.clone());
            self.img_caches.resize_with(pane_index + 1, || image_cache::ImageCache::default());
        }*/
        debug!("pane_index: {}, self.panes.len(): {}", pane_index, self.panes.len());
        if pane_index >= self.panes.len() {
            self.panes.resize_with(pane_index + 1, || pane::Pane::default());
            debug!("resized pane_index: {}, self.panes.len(): {}", pane_index, self.panes.len());
        }

        // Update the slider position
        if !self.is_slider_dual && self.last_opened_pane == -1 {
            //self.slider_value = self.panes[pane_index].img_cache.current_index as u16;
        }
        self.last_opened_pane = pane_index as isize;
    }


    fn handle_load_operation(
        &mut self,
        c_index: usize,
        _img_cache: &mut Option<&mut ImageCache>,
        image_data: Option<Vec<u8>>,
        //load_fn: Box<dyn FnOnce(&mut ImageCache, Option<Vec<u8>>) -> Result<(), std::io::Error>>,
        load_fn: Box<dyn FnOnce(&mut ImageCache, Option<Vec<u8>>) -> Result<bool, std::io::Error>>,
    ) {
        //let mut pane = &mut self.panes[c_index];
        let pane = &mut self.panes[c_index];

        // TODO: Refactor this function
        // This looks better but I get borrow checker err later
        //let mut img_cache = Some(&mut self.panes[c_index].img_cache);
        
        let mut img_cache = None;
        //let mut cache_index = 0;

        //println!("IMAGE LOADED: c_index: {}", c_index);
        ////self.mark_image_loaded(c_index);
        //img_cache.replace(&mut self.panes[c_index].img_cache);
        img_cache.replace(&mut pane.img_cache);
        let cache_index = c_index;
        
        if let Some(cache) = img_cache.as_mut() {
            let _ = cache.being_loaded_queue.pop_front();
            //let res = load_fn(cache, image_data);
            match load_fn(cache, image_data) {
                Ok(reload_current_image) => {
                    if reload_current_image {
                        //let mut pane = &mut self.panes[c_index];
                        
                        ////let loaded_image = cache.get_current_image().unwrap().to_vec();
                        let loaded_image = cache.get_initial_image().unwrap().to_vec();
                        let handle = iced::widget::image::Handle::from_memory(loaded_image.clone());
                        pane.current_image = handle;
                    }
                }
                Err(error) => {
                    eprintln!("Error loading image: {}", error);
                }
            }

            // TODO: move this line into load_fn
            //cache.current_offset -= 1;
            //println!("cache.current_offset: {}", cache.current_offset);

            println!("IMAGE LOADED: cache_index: {}, current_offset: {}",
                cache_index, cache.current_offset);
        }


        
    
        // TODO: run this block right after user interactions
        // ref: https://stackoverflow.com/questions/63643732/variable-does-not-need-to-be-mutable-but-it-does
        /*let mut pane = &mut self.panes[cache_index];
        let loaded_image = pane.img_cache.get_current_image().unwrap().to_vec();
        let handle = iced::widget::image::Handle::from_memory(loaded_image.clone());
        pane.current_image = handle;

    
        // Update slider values => 
        if self.is_slider_dual {
            pane.slider_value = pane.img_cache.current_index as u16;
        } else {
            //debug!("self.slider_value: {}", self.slider_value);
            if self.are_all_images_loaded() {
                // Set the smaller index for slider value
                let min_index = self.panes.iter().map(|pane| pane.img_cache.current_index).min().unwrap();
                self.slider_value = min_index as u16;
            }
            //debug!("self.slider_value: {}", self.slider_value);
        }*/
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
        //self.is_slider_dual = !self.is_slider_dual;

        // When toggling from dual to single, reset pane.is_selected to true
        if self.is_slider_dual {
            for pane in self.panes.iter_mut() {
                pane.is_selected_cache = pane.is_selected;
                pane.is_selected = true;
                pane.image_load_state = true;
            }

            // Set the slider value to the first pane's current index
            //self.slider_value = self.panes[0].img_cache.current_index as u16;

            self.slider_value = get_master_slider_value(&self.panes, &self.pane_layout, self.is_slider_dual, self.last_opened_pane as usize) as u16;
        } else {
            // Single to dual slider: give slider.value to each slider
            for pane in self.panes.iter_mut() {
                //pane.slider_value = self.slider_value;
                pane.slider_value = pane.img_cache.current_index as u16;
                pane.is_selected = pane.is_selected_cache;
            }
        }

        self.is_slider_dual = !self.is_slider_dual;
    }

    fn toggle_pane_layout(&mut self, pane_layout: PaneLayout) {
        
        match pane_layout {
            PaneLayout::SinglePane => {
                // self.img_caches.resize(1, Default::default()); // Resize to hold 1 image cache
                self.panes.resize(1, Default::default());
                debug!("self.panes.len(): {}", self.panes.len());
                // self.dir_loaded[1] = false;

                if self.pane_layout == PaneLayout::DualPane {
                    // Reset the slider value to the first pane's current index
                    //self.slider_value = self.panes[0].img_cache.current_index as u16;
                    self.slider_value = get_master_slider_value(
                        &self.panes, &pane_layout, self.is_slider_dual, self.last_opened_pane as usize) as u16;
                    self.panes[0].is_selected = true;
                }
            }
            PaneLayout::DualPane => {
                self.panes.resize(2, Default::default()); // Resize to hold 2 image caches
                debug!("self.panes.len(): {}", self.panes.len());
                //self.pane_layout = PaneLayout::SinglePane;
            }
        }
        
        self.pane_layout = pane_layout;
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
                title: String::from("View Skater"),
                directory_path: None,
                current_image_index: 0,
                slider_value: 0,
                prev_slider_value: 0,
                ver_divider_position: None,
                hor_divider_position: None,
                //pane_count: 2,
                is_slider_dual: false,
                pane_layout: PaneLayout::SinglePane,
                last_opened_pane: 0,
                panes: vec![pane::Pane::default()],
                skate_right: false,
                skate_left: false,
                update_counter: 0,
            },
            Command::none()
        )

    }
    
    fn title(&self) -> String {
        match self.pane_layout  {
            PaneLayout::SinglePane => {
                if self.panes[0].dir_loaded {
                    // return string here
                    self.panes[0].img_cache.image_paths[self.panes[0].img_cache.current_index].display().to_string()
                    //self.title.clone()
                } else {
                    self.title.clone()
                }
            }
            PaneLayout::DualPane => {
                let left_pane_filename = if self.panes[0].dir_loaded {
                    self.panes[0].img_cache.image_paths[self.panes[0].img_cache.current_index]
                        .file_name()
                        .map(|name| name.to_string_lossy().to_string())
                        .unwrap_or_else(|| String::from("Unknown"))
                } else {
                    String::from("No File")
                };
    
                let right_pane_filename = if self.panes[1].dir_loaded {
                    self.panes[1].img_cache.image_paths[self.panes[1].img_cache.current_index]
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
                //Command::none()
            }
            Message::Debug(s) => {
                self.title = s;
                //Command::none()
            }
            Message::OpenFolder => {
                return Command::perform(file_io::pick_folder(), |result| {
                    Message::FolderOpened(result)
                });
            }
            Message::OpenFile => {
                return Command::perform(file_io::pick_file(), |result| {
                    Message::FolderOpened(result)
                });
            }
            Message::FileDropped(pane_index, dropped_path) => {
                debug!("File dropped: {:?}, pane_index: {}", dropped_path, pane_index);
                debug!("self.dir_loaded, pane_index, last_opened_pane: {:?}, {}, {}", self.panes[pane_index as usize].dir_loaded, pane_index, self.last_opened_pane);
                self.initialize_dir_path( PathBuf::from(dropped_path), pane_index as usize);
                
                //Command::none()
            }
            Message::Close => {
                self.reset_state();
                // self.current_image = iced::widget::image::Handle::from_memory(vec![]);
                debug!("directory_path: {:?}", self.directory_path);
                debug!("self.current_image_index: {}", self.current_image_index);
                for (_cache_index, pane) in self.panes.iter_mut().enumerate() {
                    let img_cache = &mut pane.img_cache;
                    debug!("img_cache.current_index: {}", img_cache.current_index);
                    debug!("img_cache.image_paths.len(): {}", img_cache.image_paths.len());
                }
                //Command::none()
            }
            Message::FolderOpened(result) => {
                match result {
                    Ok(dir) => {
                        debug!("Folder opened: {}", dir);
                        self.initialize_dir_path(PathBuf::from(dir), 0);

                        //Command::none()
                    }
                    Err(err) => {
                        debug!("Folder open failed: {:?}", err);
                        //Command::none()
                    }
                }
            }
            Message::OnVerResize(position) => { self.ver_divider_position = Some(position); },//Command::none() },
            Message::OnHorResize(position) => { self.hor_divider_position = Some(position); },//Command::none() },
            Message::ResetSplit(_position) => {
                self.ver_divider_position = None; //Command::none()
            },
            Message::ToggleSliderType(_bool) => {
                self.toggle_slider_type();
                //Command::none()
            },
            Message::TogglePaneLayout(pane_layout) => {
                self.toggle_pane_layout(pane_layout);
                //Command::none()
            },
            Message::PaneSelected(pane_index, is_selected) => {
                self.panes[pane_index].is_selected = is_selected;
                /*if self.panes[pane_index].is_selected {
                    self.panes[pane_index].is_selected = false;
                } else {
                    self.panes[pane_index].is_selected = true;
                }*/

                for (index, pane) in self.panes.iter_mut().enumerate() {
                    debug!("pane_index: {}, is_selected: {}", index, pane.is_selected);
                }
                
                //Command::none()
            }

            Message::ImageLoaded (result) => {
                // v2: multiple panes
                let mut img_cache = None;
                //let cache_index = 0;

                match result {
                    Ok((image_data, operation)) => {
                        if let Some(op) = operation {
                            match op {
                                LoadOperation::LoadNext((c_index, _target_index)) => {

                                    self.handle_load_operation(c_index, &mut img_cache, image_data, op.load_fn());
                                }
                                LoadOperation::LoadPrevious((c_index, _target_index)) => {
                                    self.handle_load_operation(c_index, &mut img_cache, image_data, op.load_fn());
                                }
                                LoadOperation::ShiftNext((c_index, _target_index)) => {
                                    self.handle_load_operation(c_index, &mut img_cache, image_data, op.load_fn());
                                }
                                LoadOperation::ShiftPrevious((c_index, _target_index)) => {
                                    self.handle_load_operation(c_index, &mut img_cache, image_data, op.load_fn());
                                }
                                LoadOperation::LoadPos((c_index, _target_index, _pos)) => {
                                    self.handle_load_operation(c_index, &mut img_cache, image_data, op.load_fn());
                                }
                            }
                        }

                        // debug!("image load state: {:?}", self.image_load_state);
                        for pane in self.panes.iter() {
                            debug!("pane.image_load_state: {:?}", pane.image_load_state);
                        }

                        //Command::none()
                    }
                    Err(err) => {
                        debug!("Image load failed: {:?}", err);
                        //Command::none()
                    }
                }

            }

            Message::SliderChanged(pane_index, value) => {
                debug!("pane_index {} slider value: {}", pane_index, value);
                // -1 means the master slider (broadcast operation to all panes)
                if pane_index == -1 {
                    self.prev_slider_value = self.slider_value;
                    self.slider_value = value;
                    /*if value == self.prev_slider_value + 1 {
                        // Value changed by +1
                        // Call a function or perform an action for this case
                        //self.move_right_all()
                        println!("slider - move_right_all");
                        //let command = move_right_all(&mut self.panes);
                        let command = move_right_all_new(&mut self.panes, &mut self.slider_value, self.is_slider_dual);
                        return command;
    
                    } else if value == self.prev_slider_value.saturating_sub(1) {
                        // Value changed by -1
                        // Call a different function or perform an action for this case
                        println!("slider - move_left_all");
                        //move_left_all(&mut self.panes)
                        let command = move_left_all_new(&mut self.panes, &mut self.slider_value, self.is_slider_dual);
                        return command;
                    } else {
                        // Value changed by more than 1 or it's the initial change
                        // Call another function or handle this case differently
                        //self.update_pos(pane_index, value as usize);
                        println!("slider - update_pos");
                        update_pos(&mut self.panes, pane_index, value as usize);
                        //Command::none()
                    }*/
                    println!("slider - update_pos");
                    //update_pos(&mut self.panes, pane_index, value as usize);
                    return update_pos(&mut self.panes, pane_index as isize, value as usize);

                } else {
                    let pane = &mut self.panes[pane_index as usize];

                    let _pane_index_org = pane_index.clone();
                    let pane_index = pane_index as usize;

                    debug!("pane_index {} slider value: {}", pane_index, value);
                    // self.prev_slider_values[pane_index] = self.slider_values[pane_index];
                    pane.prev_slider_value = pane.slider_value;
                    // self.slider_values[pane_index] = value;
                    pane.slider_value = value;
                    debug!("pane_index {} prev slider value: {}", pane_index, pane.prev_slider_value);
                    debug!("pane_index {} slider value: {}", pane_index, pane.slider_value);
                    
                    // if value == self.prev_slider_values[pane_index] + 1 {
                    /*if value == pane.prev_slider_value + 1 {
                        debug!("move_right_index");
                        // Value changed by +1
                        // Call a function or perform an action for this case
                        //move_right_index(&mut self.panes, pane_index)
                        return move_right_index_new(&mut self.panes, pane_index);

                    // } else if value == self.prev_slider_values[pane_index].saturating_sub(1) {
                    } else if value == pane.prev_slider_value.saturating_sub(1) {
                        // Value changed by -1
                        // Call a different function or perform an action for this case
                        debug!("move_left_index");
                        return move_left_index_new(&mut self.panes, pane_index);
                    } else {
                        // Value changed by more than 1 or it's the initial change
                        // Call another function or handle this case differently
                        debug!("update_pos");
                        update_pos(&mut self.panes, pane_index_org, value as usize);
                        //Command::none()
                    }*/

                    return update_pos(&mut self.panes, pane_index as isize, value as usize);
                }
            }

            Message::SliderReleased(pane_index, value) => {
                println!("slider released: pane_index: {}, value: {}", pane_index, value);
                if pane_index == -1 {
                    return load_remaining_images(
                        &mut self.panes, pane_index, value as usize);
                } else {
                    return load_remaining_images(&mut self.panes, pane_index as isize, value as usize);
                }
            }


            Message::Event(event) => match event {
                // Only using for single pane layout
                #[cfg(any(target_os = "macos", target_os = "windows"))]
                Event::Window(iced::window::Event::FileDropped(dropped_paths, _position)) => {
                    match self.pane_layout {
                        PaneLayout::SinglePane => {
                            debug!("File dropped: {:?}", dropped_paths.clone());

                            self.initialize_dir_path(dropped_paths[0].clone(), 0);
                            
                            //Command::none()
                        },
                        PaneLayout::DualPane => {
                            //Command::none()
                        }
                    }
                }
                #[cfg(target_os = "linux")]
                Event::Window(iced::window::Event::FileDropped(dropped_path)) => {
                    match self.pane_layout {
                        PaneLayout::SinglePane => {
                            debug!("File dropped: {:?}", dropped_path);

                            self.initialize_dir_path(dropped_path, 0);
                            
                            //Command::none()
                        },
                        PaneLayout::DualPane => {
                            //Command::none()
                        }
                    }
                }

                Event::Keyboard(keyboard::Event::KeyPressed {
                    key_code: keyboard::KeyCode::Tab,
                    modifiers: _,
                }) => {
                    debug!("Tab pressed");
                    //Command::none()
                }

                Event::Keyboard(keyboard::Event::KeyPressed {
                    key_code: keyboard::KeyCode::Right,
                    //modifiers: _,
                    modifiers,
                }) => {

                    debug!("ArrowRight pressed");
                    if self.pane_layout == PaneLayout::DualPane && self.is_slider_dual && !self.panes.iter().any(|pane| pane.is_selected) {
                        debug!("No panes selected");
                        //Command::none();
                    }

                    if modifiers.shift() {
                        println!("SKATE_RIGHT: true");
                        self.skate_right = true;
                    } else {
                        println!("SKATE_RIGHT: false");
                        self.skate_right = false;

                        /*if self.are_all_images_cached() {
                            self.init_image_loaded(); // [false, false]
                            let command = move_right_all_new(&mut self.panes, &mut self.slider_value, self.is_slider_dual);
                            return command;
                        } else {
                            println!("not are_all_images_cached()");
                            //Command::none()
                        }*/
                        self.init_image_loaded(); // [false, false]
                        let command = move_right_all(
                            &mut self.panes, &mut self.slider_value,
                            &self.pane_layout, self.is_slider_dual, self.last_opened_pane as usize);
                        return command;
                    }
                }
                Event::Keyboard(keyboard::Event::KeyReleased {
                    key_code: keyboard::KeyCode::Right,
                    modifiers: _,
                }) => {
                    println!("ArrowRight released, SKATE_RIGHT: false");
                    self.skate_right = false;

                    // Reset panes' image loading queues
                    for pane in self.panes.iter_mut() {
                        pane.img_cache.reset_image_load_queue();
                        pane.img_cache.reset_image_being_loaded_queue();

                        //pane.img_cache.current_offset += pane.img_cache.current_offset_accumulated;
                        //pane.img_cache.current_offset_accumulated = 0;
                    }
                    //Command::none()
                }
                Event::Keyboard(keyboard::Event::KeyPressed {
                    key_code: keyboard::KeyCode::Left,
                    modifiers,
                }) => {
                    if self.pane_layout == PaneLayout::DualPane && self.is_slider_dual && !self.panes.iter().any(|pane| pane.is_selected) {
                        debug!("No panes selected");
                        //Command::none();
                    }

                    if modifiers.shift() {
                        println!("SKATE_LEFT: true");
                        self.skate_left = true;
                    } else {
                        println!("SKATE_LEFT: false");
                        self.skate_left = false;
                        /*if self.are_all_images_cached_prev() {
                            self.init_image_loaded(); // [false, false]
                            let command = move_left_all_new(&mut self.panes, &mut self.slider_value, self.is_slider_dual);
                            return command;
                        } else {
                            println!("not are_all_images_cached()");
                            //Command::none()
                        }*/
                        self.init_image_loaded(); // [false, false]
                        let command = move_left_all(
                            &mut self.panes, &mut self.slider_value,
                            &self.pane_layout, self.is_slider_dual, self.last_opened_pane as usize);
                        return command;
                    }
                    
                }
                Event::Keyboard(keyboard::Event::KeyReleased {
                    key_code: keyboard::KeyCode::Left,
                    modifiers: _,
                }) => {
                    debug!("ArrowLeft released, SKATE_LEFT: false");
                    self.skate_left = false;

                    // Reset panes' image loading queues
                    for pane in self.panes.iter_mut() {
                        pane.img_cache.reset_image_load_queue();
                        pane.img_cache.reset_image_being_loaded_queue();

                        //pane.img_cache.current_offset += pane.img_cache.current_offset_accumulated;
                        //pane.img_cache.current_offset_accumulated = 0;
                    }
                    //Command::none()
                }

                _ => return Command::none(),
                //_ => command,

            
            },
            
        }

        //self.update_counter += 1;
        //if self.skate_right && self.are_all_images_cached() {
            if self.skate_right {
            println!("skae_right: {}", self.skate_right);
            println!("update_counter: {}", self.update_counter);
            self.update_counter = 0;
            self.init_image_loaded(); // [false, false]
            let command = move_right_all(
                &mut self.panes, &mut self.slider_value, &self.pane_layout, self.is_slider_dual, self.last_opened_pane as usize);
            command
        } else if self.skate_left {
            println!("skae_left: {}", self.skate_left);
            println!("update_counter: {}", self.update_counter);
            self.update_counter = 0;
            self.init_image_loaded(); // [false, false]
            let command = move_left_all(&mut self.panes, &mut self.slider_value, &self.pane_layout, self.is_slider_dual, self.last_opened_pane as usize);
            println!("command: {:?}", command);
            command
        } else {
            println!("no skate mode detected");
            let command = Command::none();
            //self.panes[0].img_cache.print_cache();
            command
        }

        
    }

    fn view(&self) -> Element<Message> {
        let container_all = ui_builder::build_ui(&self);

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

use image::io::Reader as ImageReader;
static ICON: &[u8] = include_bytes!("../assets/icon_128.png");
//static ICON: &[u8] = include_bytes!("../assets/icon_48.png");
//const ICON_HEIGHT: u32 = 512;
//const ICON_WIDTH: u32 = 512;

fn main() -> iced::Result {
    env_logger::init();
    use iced::window;

    
    //let icon =  iced::window::icon::from_file("icon.ico");
    //let icon =  iced::window::icon::from_file("icon.png");

    //let icon_data = iced::widget::image::Handle::from_memory(ICON).data;
    let icon_data = ImageReader::new(std::io::Cursor::new(ICON))
        .with_guessed_format()
        .unwrap()
        .decode()
        .unwrap()
        .to_rgba8();

    //let icon = iced::window::icon::from_rgba(icon_data.to_vec(), icon_data.width(), icon_data.height());
    //let icon = iced::window::icon::from_rgba(ICON.to_owned().into(), 512, 512);
    //let icon = iced::window::icon::from_rgba(include_bytes!("../assets/icon_512.png").to_owned().into(), 512, 512);
    //let icon = iced::window::icon::from_file_data(include_bytes!("../assets/icon_512.png"), Some(image::image::ImageFormat::Png));

    //let icon = iced::window::icon::from_file_data(include_bytes!("../assets/icon_512.png"), Option::None);
    let icon = iced::window::icon::from_file_data(ICON, Option::None);

    //let icon = iced::window::icon::from_rgba(ICON.to_owned().into(), 64, 64);
    //let icon = iced::window::icon::from_rgba(include_bytes!("../assets/icon_512.png").as_bytes().to_vec(), icon_data.width(), icon_data.height());
    match icon {
        Ok(icon) => {
            info!("Icon loaded successfully");
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
            info!("Icon load failed: {:?}", err);
            DataViewer::run(Settings::default())
        }
    }


    // DataViewer::run(Settings::default())
    
}