
#![windows_subsystem = "windows"]

#[warn(unused_imports)]
#[cfg(target_os = "linux")]
mod other_os {
    pub use iced;
    //pub use iced_aw;
}

#[cfg(not(target_os = "linux"))]
mod macos {
    pub use iced_custom as iced;
    //pub use iced_aw_custom as iced_aw;
}

#[cfg(target_os = "linux")]
use other_os::*;

#[cfg(not(target_os = "linux"))]
use macos::*;


use iced::event::Event;
use iced::subscription::{self, Subscription};
use iced::{keyboard, clipboard};
use iced::{Element, Length, Application, Theme, Settings, Command};
use iced::font::{self, Font};

use std::path::PathBuf;
#[allow(unused_imports)]
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
mod toggler {
    pub mod toggler;
    pub mod style;
}
//use dualslider::dualslider::DualSlider;

mod pane;
use crate::pane::get_master_slider_value;
mod ui_builder;
mod viewer;
//mod footer;


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
    show_footer: bool,
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
            title: String::from("ViewSkater"),
            directory_path: None,
            current_image_index: 0,
            slider_value: 0,
            prev_slider_value: 0,
            ver_divider_position: None,
            hor_divider_position: None,
            //pane_count: 2,
            is_slider_dual: false,
            show_footer: true,
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
    FontLoaded(Result<(), font::Error>),
    OpenFolder(usize),
    OpenFile(usize),
    FileDropped(isize, String),
    Close,
    Quit,
    FolderOpened(Result<String, Error>, usize),
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
    ToggleFooter(bool),
    PaneSelected(usize, bool),
    CopyFilename(usize),
    CopyFilePath(usize),
}

impl DataViewer {
    fn reset_state(&mut self) {
        self.title = String::from("ViewSkater");
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

    /*fn handle_next_out_of_order_image(&mut self, c_index: usize,
        target_index: isize,
        _img_cache: &mut Option<&mut ImageCache>,
        image_data: Option<Vec<u8>>,
        //load_fn: Box<dyn FnOnce(&mut ImageCache, Option<Vec<u8>>) -> Result<(), std::io::Error>>,
        load_fn: Box<dyn FnOnce(&mut ImageCache, Option<Vec<u8>>) -> Result<bool, std::io::Error>>) {

        }*/

        

    fn handle_load_operation(
        &mut self,
        c_index: usize,
        target_index: isize,
        _img_cache: &mut Option<&mut ImageCache>,
        image_data: Option<Vec<u8>>,
        //load_fn: Box<dyn FnOnce(&mut ImageCache, Option<Vec<u8>>) -> Result<bool, std::io::Error>>,
        load_fn: Box<dyn FnOnce(&mut ImageCache, Option<Vec<u8>>, isize) -> Result<bool, std::io::Error>>,
        operation_type: image_cache::LoadOperationType,
    ) {
        //let mut pane = &mut self.panes[c_index];
        let pane = &mut self.panes[c_index];

        // TODO: Refactor this function
        // This looks better but I get borrow checker err later
        //let mut img_cache = Some(&mut self.panes[c_index].img_cache);
        let mut img_cache = None;
        img_cache.replace(&mut pane.img_cache);
        let cache_index = c_index;

        
        if let Some(cache) = img_cache.as_mut() {
            let _ = cache.being_loaded_queue.pop_front();

            if cache.is_operation_blocking(operation_type.clone()) {
                // If the operation is blocking, skip the operation
                return;
            }

            // Check if the next image that is supposed to be loaded matches target_index
            // If not, add image_data to out_of_order_images
            // If it does not match and if the matching image is in out_of_order_images, load it
            // If it matches, load `image_data`
            let target_image_to_load: isize = if operation_type == image_cache::LoadOperationType::LoadNext {
                cache.get_next_image_to_load() as isize
            } else if operation_type == image_cache::LoadOperationType::LoadPrevious {
                cache.get_prev_image_to_load() as isize
            } else {
                -99
            };
            let target_image_to_load_usize = target_image_to_load as usize;
            let is_matched = target_image_to_load == target_index;
            
            
            println!("IMAGE LOADED: target_image_to_load: {}, target_index: {}", target_image_to_load, target_index);
            println!("load_operation: {:?}", operation_type);
            // [1] LOADNEXT
            // 1. If it matches, load `image_data`
            ////if target_image_to_load == -1 || (target_image_to_load_usize == target_index as usize
            ////    || target_index > cache.num_files as isize - cache.cache_count as isize || target_index < cache.cache_count as isize
            let last_index = cache.cached_image_indices[cache.cached_image_indices.len() - 1];
            if target_image_to_load == -99 || is_matched {
                // If somehow the LoadNext is called when current_offset is at the right end, skip loading
                // The same goes for LoadPrevious
                if (operation_type == image_cache::LoadOperationType::LoadNext || operation_type == image_cache::LoadOperationType::ShiftNext) && cache.current_offset > cache.cache_count as isize ||
                (operation_type == image_cache::LoadOperationType::LoadPrevious || operation_type == image_cache::LoadOperationType::ShiftPrevious) && cache.current_offset < -(cache.cache_count as isize) {
                    return;
                }

                match load_fn(cache, image_data, target_index) {
                    Ok(reload_current_image) => {
                        if reload_current_image {
                            let loaded_image = cache.get_initial_image().unwrap().to_vec();
                            let handle = iced::widget::image::Handle::from_memory(loaded_image.clone());
                            pane.current_image = handle;
                        }
                    }
                    Err(error) => {
                        eprintln!("Error loading image: {}", error);
                    }
                }
            } else {

                /*println!("$$$$$$$$$$IMAGE LOADED: OUT OF ORDER$$$$$$$$$$");
                println!("target_image_to_load_usize: {}, target_index: {}", target_image_to_load_usize, target_index);
                println!("cache.current_index: {}, cache.current_offset: {}", cache.current_index, cache.current_offset);
                

                // 2-2. If it does not match and if the matching image is in out_of_order_images, load it
                //if cache.out_of_order_images.contains_key(&target_index) {
                //if let (image_index,image_data_buffered ) = cache.out_of_order_images.remove(next_image_to_load as usize) {
                //if let Some((image_index,image_data_buffered )) = cache.pop_out_of_order_image(target_index as usize) {
                if let Some(image_data_buffered ) = cache.pop_out_of_order_image(target_index as usize) {
                    println!("IMAGE LOADED: OUT OF ORDER: out_of_order_images.pop: target_index: {}, target_image_to_load_usize: {}",
                        target_index, target_image_to_load_usize);

                    // Load the image from out_of_order_images
                    //let image_data_buffered = cache.out_of_order_images.remove(target_index as usize);//.unwrap();
                    match load_fn(cache, Some(image_data_buffered), target_index) {
                        Ok(reload_current_image) => {
                            if reload_current_image {
                                //let mut pane = &mut self.panes[c_index];
                                let loaded_image = cache.get_initial_image().unwrap().to_vec();
                                let handle = iced::widget::image::Handle::from_memory(loaded_image.clone());
                                pane.current_image = handle;
                            }
                        }
                        Err(error) => {
                            eprintln!("Error loading image: {}", error);
                        }
                    }
                }

                // 2-1. If it does not match, store image_data into out_of_order_images
                if image_data.is_some() {
                    println!("out_of_order_images.push: target_index: {}, target_image_to_load_usize: {}",
                        target_index, target_image_to_load_usize);
                    cache.out_of_order_images.push((target_index as usize, image_data.unwrap()));
                    println!("out_of_order_images len: {}", cache.out_of_order_images.len());
                }
                println!("$$$$$$$$$$IMAGE LOADED: OUT OF ORDER$$$$$$$$$$");*/
            }

            println!("IMAGE LOADED: cache_index: {}, current_offset: {}",
                cache_index, cache.current_offset);
        }

    }


    fn handle_key_pressed_event(&mut self, key_code: keyboard::KeyCode, modifiers: keyboard::Modifiers) -> Vec<Command<Message>> {
        let mut commands = Vec::new();
        match key_code {
            keyboard::KeyCode::Tab => {
                println!("Tab pressed");
                // toggle footer
                self.toggle_footer();
            }

            keyboard::KeyCode::Space | keyboard::KeyCode::B => {
                println!("Space pressed");
                // Toggle slider type
                self.toggle_slider_type();
            }

            keyboard::KeyCode::Key1 => {
                println!("Key1 pressed");
                if self.pane_layout == PaneLayout::DualPane && self.is_slider_dual {
                    self.panes[0].is_selected = !self.panes[0].is_selected;
                }

                // If alt+ctrl is pressed, load a file into pane0
                if modifiers.alt() && modifiers.control() {
                    println!("Key1 Shift pressed");
                    commands.push(Command::perform(file_io::pick_file(), move |result| {
                        Message::FolderOpened(result, 0)
                    }));
                }

                // If alt is pressed, load a folder into pane0
                if modifiers.alt() {
                    println!("Key1 Alt pressed");
                    commands.push(Command::perform(file_io::pick_folder(), move |result| {
                        Message::FolderOpened(result, 0)
                    }));
                }

                // If ctrl is pressed, switch to single pane layout
                if modifiers.control() {
                    self.toggle_pane_layout(PaneLayout::SinglePane);
                }
            }
            keyboard::KeyCode::Key2 => {
                println!("Key2 pressed");
                if self.pane_layout == PaneLayout::DualPane {
                    if self.is_slider_dual {
                        self.panes[1].is_selected = !self.panes[1].is_selected;
                    }
                
                    // If alt+ctrl is pressed, load a file into pane1
                    if modifiers.alt() && modifiers.control() {
                        println!("Key2 Shift pressed");
                        commands.push(Command::perform(file_io::pick_file(), move |result| {
                            Message::FolderOpened(result, 1)
                        }));
                    }

                    // If alt is pressed, load a folder into pane1
                    if modifiers.alt() {
                        println!("Key2 Alt pressed");
                        commands.push(Command::perform(file_io::pick_folder(), move |result| {
                            Message::FolderOpened(result, 1)
                        }));
                    }
                }

                // If ctrl is pressed, switch to dual pane layout
                if modifiers.control() {
                    println!("Key2 Ctrl pressed");
                    self.toggle_pane_layout(PaneLayout::DualPane);
                    //commands.push(Command::perform(Message::TogglePaneLayout(PaneLayout::DualPane), |_| Message::Nothing));
                }
            }

            keyboard::KeyCode::C | keyboard::KeyCode::W => {
                // Close the selected panes
                if modifiers.control() {
                    for pane in self.panes.iter_mut() {
                        if pane.is_selected {
                            pane.reset_state();
                        }
                    }
                }
            }

            keyboard::KeyCode::Q => {
                // Terminate the app
                std::process::exit(0);
            }

            keyboard::KeyCode::Left | keyboard::KeyCode::A => {
                if self.skate_right {
                    println!("**********SKATE_LEFT: SWITCHED: skate_right was true**********");
                    self.skate_right = false;
                }

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

                    let command = move_left_all(
                        &mut self.panes, &mut self.slider_value,
                        &self.pane_layout, self.is_slider_dual, self.last_opened_pane as usize);
                    commands.push(command);
                }
                
            }
            keyboard::KeyCode::Right | keyboard::KeyCode::D => {
                debug!("ArrowRight pressed");
                if self.skate_left {
                    println!("**********SKATE_RIGHT: SWITCHED: skate_left was true**********");
                    self.skate_left = false;

                    // Discard all queue items that are LoadPrevious or ShiftPrevious
                    for pane in self.panes.iter_mut() {
                        pane.img_cache.reset_load_previous_queue_items();
                    }
                }


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

                    let command = move_right_all(
                        &mut self.panes, &mut self.slider_value,
                        &self.pane_layout, self.is_slider_dual, self.last_opened_pane as usize);
                    commands.push(command);
                }
            }

            _ => {}
        }

        commands
    }

    fn handle_key_released_event(&mut self, key_code: keyboard::KeyCode, _modifiers: keyboard::Modifiers) -> Vec<Command<Message>> {
        #[allow(unused_mut)]
        let mut commands = Vec::new();

        match key_code {
            keyboard::KeyCode::Tab => {
                println!("Tab released");
                //Command::perform(async {}, |_| Message::TabReleased)
                
            }
            keyboard::KeyCode::Enter | keyboard::KeyCode::NumpadEnter => {
                println!("Enter key released!");
                
            }
            keyboard::KeyCode::Escape => {
                println!("Escape key released!");
                
            }
            keyboard::KeyCode::Left | keyboard::KeyCode::A => {
                println!("Left key or 'A' key released!");
                debug!("ArrowLeft released, SKATE_LEFT: false");
                self.skate_left = false;

                // Reset panes' image loading queues
                for pane in self.panes.iter_mut() {
                    pane.img_cache.reset_image_load_queue();
                    pane.img_cache.reset_image_being_loaded_queue();
                }
                
            }
            keyboard::KeyCode::Right | keyboard::KeyCode::D => {
                println!("Right key or 'D' key released!");
                //Command::perform(async {}, |_| Message::RightReleased)

                self.skate_right = false;
                // Reset panes' image loading queues
                for pane in self.panes.iter_mut() {
                    pane.img_cache.reset_image_load_queue();
                    pane.img_cache.reset_image_being_loaded_queue();
                }
                
            }
            _ => {},
        }

        commands
    }



    // UI
    fn toggle_slider_type(&mut self) {
        // When toggling from dual to single, reset pane.is_selected to true
        if self.is_slider_dual {
            for pane in self.panes.iter_mut() {
                pane.is_selected_cache = pane.is_selected;
                pane.is_selected = true;
                //pane.image_load_state = true;
                pane.is_next_image_loaded = false;
                pane.is_prev_image_loaded = false;
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

    fn toggle_footer(&mut self) {
        self.show_footer = !self.show_footer;
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
                title: String::from("ViewSkater"),
                directory_path: None,
                current_image_index: 0,
                slider_value: 0,
                prev_slider_value: 0,
                ver_divider_position: None,
                hor_divider_position: None,
                //pane_count: 2,
                is_slider_dual: false,
                show_footer: true,
                pane_layout: PaneLayout::SinglePane,
                last_opened_pane: 0,
                panes: vec![pane::Pane::default()],
                skate_right: false,
                skate_left: false,
                update_counter: 0,
            },
            Command::batch(vec![
                font::load(include_bytes!("../assets/fonts/viewskater-fonts.ttf").as_slice()).map(Message::FontLoaded), // icon font
                //font::load(include_bytes!("../assets/fonts/Iosevka-Regular.ttc").as_slice()).map(Message::FontLoaded),  // footer digit font
                font::load(include_bytes!("../assets/fonts/Iosevka-Regular-ascii.ttf").as_slice()).map(Message::FontLoaded),  // footer digit font
                font::load(include_bytes!("../assets/fonts/Roboto-Regular.ttf").as_slice()).map(Message::FontLoaded),   // UI font
            ])
        )

    }
    
    fn title(&self) -> String {
        match self.pane_layout  {
            PaneLayout::SinglePane => {
                if self.panes[0].dir_loaded {
                    // return string here
                    //self.title.clone()
                    //self.panes[0].img_cache.image_paths[self.panes[0].img_cache.current_index].display().to_string()
                    self.panes[0].img_cache.image_paths[self.panes[0].img_cache.current_index].file_name().map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_else(|| String::from("Unknown"))
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
            Message::FontLoaded(_) => {
                //Command::none()
            }
            Message::OpenFolder(pane_index) => {
                return Command::perform(file_io::pick_folder(), move |result| {
                    Message::FolderOpened(result, pane_index)
                });
            }
            Message::OpenFile(pane_index) => {
                return Command::perform(file_io::pick_file(), move |result| {
                    Message::FolderOpened(result, pane_index)
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
            Message::Quit => {
                std::process::exit(0);
            }
            Message::FolderOpened(result, pane_index) => {
                match result {
                    Ok(dir) => {
                        debug!("Folder opened: {}", dir);
                        self.initialize_dir_path(PathBuf::from(dir), pane_index);

                        //Command::none()
                    }
                    Err(err) => {
                        debug!("Folder open failed: {:?}", err);
                        //Command::none()
                    }
                }
            },
            Message::CopyFilename(pane_index) => {
                // Get the image path of the specified pane
                //let img_path = self.panes[pane_index].img_cache.image_paths[self.panes[pane_index].img_cache.current_index].file_name().map(|name| name.to_string_lossy().to_string());
                let img_path = self.panes[pane_index].img_cache.image_paths[self.panes[pane_index].img_cache.current_index].file_name().map(|name| name.to_string_lossy().to_string());

                /*if let Some(filename) = file_io::get_filename(img_path) {
                    println!("Filename: {}", filename);

                    // to_owned vs to_string
                    return clipboard::write::<Message>(filename.to_string());
                }*/
                if let Some(img_path) = img_path {
                    if let Some(filename) = file_io::get_filename(&img_path) {
                        println!("Filename: {}", filename);
                        return clipboard::write::<Message>(filename.to_string());
                    }
                }
                
                // works
                //return clipboard::write::<Message>("debug debug".to_string());
            }
            Message::CopyFilePath(pane_index) => {
                // Get the image path of the specified pane
                let img_path = self.panes[pane_index].img_cache.image_paths[self.panes[pane_index].img_cache.current_index].file_name().map(|name| name.to_string_lossy().to_string());

                /*if let Some(path) = img_path {
                    println!("Path: {}", path);
                    return clipboard::write::<Message>(path.to_string());
                }*/
                if let Some(img_path) = img_path {
                    //println!("Path: {}", img_path);
                    //return clipboard::write::<Message>(img_path.to_string());
                    if let Some(dir_path) = self.panes[pane_index].directory_path.as_ref() {
                        let full_path = format!("{}/{}", dir_path, img_path);
                        println!("Full Path: {}", full_path);
                        return clipboard::write::<Message>(full_path);
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
            Message::ToggleFooter(_bool) => {
                self.toggle_footer();
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
                                // I've realized that _target_index is not used but we do need to check the index
                                LoadOperation::LoadNext((c_index, target_index)) => {

                                    self.handle_load_operation(c_index, target_index as isize, &mut img_cache, image_data, op.load_fn(), op.operation_type());
                                }
                                LoadOperation::LoadPrevious((c_index, target_index)) => {
                                    self.handle_load_operation(c_index, target_index as isize, &mut img_cache, image_data, op.load_fn(), op.operation_type());
                                }
                                LoadOperation::ShiftNext((c_index, target_index)) => {
                                    self.handle_load_operation(c_index, target_index, &mut img_cache, image_data, op.load_fn(), op.operation_type());
                                }
                                LoadOperation::ShiftPrevious((c_index, target_index)) => {
                                    self.handle_load_operation(c_index, target_index, &mut img_cache, image_data, op.load_fn(), op.operation_type());
                                }
                                LoadOperation::LoadPos((c_index, target_index, _pos)) => {
                                    self.handle_load_operation(c_index, target_index as isize, &mut img_cache, image_data, op.load_fn(), op.operation_type());
                                }
                            }
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
                    println!("slider - update_pos");
                    return update_pos(&mut self.panes, pane_index as isize, value as usize);

                } else {
                    let pane = &mut self.panes[pane_index as usize];

                    let _pane_index_org = pane_index.clone();
                    let pane_index = pane_index as usize;

                    debug!("pane_index {} slider value: {}", pane_index, value);
                    pane.prev_slider_value = pane.slider_value;
                    pane.slider_value = value;
                    debug!("pane_index {} prev slider value: {}", pane_index, pane.prev_slider_value);
                    debug!("pane_index {} slider value: {}", pane_index, pane.slider_value);

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

                Event::Keyboard(keyboard::Event::KeyPressed { key_code, modifiers, .. }) => {
                    let commands = self.handle_key_pressed_event(key_code, modifiers);
                    if commands.is_empty() {
                        
                    } else {
                        return Command::batch(commands);
                    }
                }

                Event::Keyboard(keyboard::Event::KeyReleased { key_code, modifiers, .. }) => {
                    let commands = self.handle_key_released_event(key_code, modifiers);
                    if commands.is_empty() {
                        
                    } else {
                        return Command::batch(commands);
                    }
                }


                _ => return Command::none(),
                //_ => command,

            
            },
            
        }

        //self.update_counter += 1;
        //if self.skate_right && self.are_all_images_cached() {
        if self.skate_right {
            println!("SKATE_RIGHT CONTINUOUS: {}", self.skate_right);
            println!("update_counter: {}", self.update_counter);
            self.update_counter = 0;
            //self.init_image_loaded(); // [false, false]
            let command = move_right_all(
                &mut self.panes, &mut self.slider_value, &self.pane_layout, self.is_slider_dual, self.last_opened_pane as usize);
            command
        } else if self.skate_left {
            println!("skae_left: {}", self.skate_left);
            println!("update_counter: {}", self.update_counter);
            self.update_counter = 0;
            //self.init_image_loaded(); // [false, false]
            let command = move_left_all(&mut self.panes, &mut self.slider_value, &self.pane_layout, self.is_slider_dual, self.last_opened_pane as usize);
            println!("command: {:?}", command);
            command
        } else {
            println!("no skate mode detected");
            let command = Command::none();
            command
        }

        
    }

    fn view(&self) -> Element<Message> {
        let container_all = ui_builder::build_ui(&self);
        container_all
        .height(Length::Fill)
        .width(Length::Fill)
        .center_x()
        .into()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        Subscription::batch(vec![
            subscription::events().map(Message::Event),
        ])
    }

    fn theme(&self) -> Self::Theme {
        //Theme::Dark
        iced::Theme::custom(
            //"Custom Theme".into(),
            iced::theme::Palette {
                primary: iced::Color::from_rgba8(20, 148, 163, 1.0),
                ..iced::Theme::Dark.palette()
            }
        )
    }
}


// Include the icon image data at compile time
static ICON: &[u8] = if cfg!(target_os = "windows") {
    include_bytes!("../assets/icon_512.png")
} else if cfg!(target_os = "macos") {
    include_bytes!("../assets/icon_512.png")
} else {
    include_bytes!("../assets/icon_48.png")
};


fn main() -> iced::Result {
    env_logger::init();
    use iced::window;

    let icon = iced::window::icon::from_file_data(ICON, Option::None);

    match icon {
        Ok(icon) => {
            info!("Icon loaded successfully");
            let settings = Settings {
                window: window::Settings {
                    icon: Some(icon),
                    ..Default::default()
                },
                default_font: Font {
                    family:  iced::font::Family::Name("Roboto"),
                    weight: iced::font::Weight::Normal,
                    stretch: iced::font::Stretch::Normal,
                    monospaced: true,
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
}