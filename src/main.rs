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

// #[macro_use]
extern crate log;

mod image_cache;
use crate::image_cache::ImageCache;
use image_cache::LoadOperation;
use image_cache::{move_right_all, move_left_all,
    move_right_index, move_left_index, update_pos};
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
mod ui_builder;
mod viewer;


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
    last_opened_pane: usize,
    panes: Vec<pane::Pane>,             // Each pane has its own image cache
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
            last_opened_pane: 0,
            panes: vec![pane::Pane::default()],
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
    }

    // Function to initialize image_load_state for all panes
    fn init_image_loaded(&mut self) {
        for pane in self.panes.iter_mut() {
            if self.pane_layout == PaneLayout::DualPane && self.is_slider_dual {
                if pane.is_selected {
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
            self.panes.iter().all(|pane| !pane.dir_loaded || (pane.dir_loaded && pane.image_load_state))
        }
    }
    fn are_all_images_loaded_in_selected(&self) -> bool {
        self.panes
            .iter()
            .filter(|pane| pane.is_selected)  // Filter only selected panes
            .all(|pane| !pane.dir_loaded || (pane.dir_loaded && pane.image_load_state))
    }

    fn initialize_dir_path(&mut self, path: PathBuf, pane_index: usize) {
        self.last_opened_pane = pane_index;
        println!("last_opened_pane: {}", self.last_opened_pane);
        self.panes[pane_index].initialize_dir_path(path);


        // If the index is greater than or equal to the length of current_images,
        // fill the vector with default handles until the desired index
        // NOTE: I don't know if this is needed => turns out I do need it
        let _default_handle = iced::widget::image::Handle::from_memory(vec![]);
        /*if pane_index >= self.current_images.len() {
            self.current_images.resize_with(pane_index + 1, || default_handle.clone());
            self.img_caches.resize_with(pane_index + 1, || image_cache::ImageCache::default());
        }*/
        println!("pane_index: {}, self.panes.len(): {}", pane_index, self.panes.len());
        if pane_index >= self.panes.len() {
            self.panes.resize_with(pane_index + 1, || pane::Pane::default());
            println!("resized pane_index: {}, self.panes.len(): {}", pane_index, self.panes.len());
        }

        // Update the slider position
        if !self.is_slider_dual {
            self.slider_value = self.panes[pane_index].img_cache.current_index as u16;
        }
    }


    fn handle_load_operation(
        &mut self,
        c_index: usize,
        _img_cache: &mut Option<&mut ImageCache>,
        image_data: Option<Vec<u8>>,
        load_fn: Box<dyn FnOnce(&mut ImageCache, Option<Vec<u8>>) -> Result<(), std::io::Error>>,
    ) {
        // TODO: Refactor this function
        // This looks better but I get borrow checker err later
        //let mut img_cache = Some(&mut self.panes[c_index].img_cache);
        
        let mut img_cache = None;
        //let mut cache_index = 0;

        self.mark_image_loaded(c_index);
        img_cache.replace(&mut self.panes[c_index].img_cache);
        let cache_index = c_index;
        
        if let Some(cache) = img_cache.as_mut() {
            let _ = cache.being_loaded_queue.pop_front();
            let _ = load_fn(cache, image_data);
        }
    
        // ref: https://stackoverflow.com/questions/63643732/variable-does-not-need-to-be-mutable-but-it-does
        let mut pane = &mut self.panes[cache_index];
        let loaded_image = pane.img_cache.get_current_image().unwrap().to_vec();
        let handle = iced::widget::image::Handle::from_memory(loaded_image.clone());
        pane.current_image = handle;
    
        // Update slider values
        if self.is_slider_dual {
            pane.slider_value = pane.img_cache.current_index as u16;
        } else {
            println!("self.slider_value: {}", self.slider_value);


            //self.slider_value = pane.img_cache.current_index as u16;
            if self.are_all_images_loaded() {
            //if self.are_all_images_loaded_in_selected() {
                // Set the smaller index for slider value
                let min_index = self.panes.iter().map(|pane| pane.img_cache.current_index).min().unwrap();
                self.slider_value = min_index as u16;
            }
            println!("self.slider_value: {}", self.slider_value);
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
        //self.is_slider_dual = !self.is_slider_dual;

        // When toggling from dual to single, reset pane.is_selected to true
        if self.is_slider_dual {
            for pane in self.panes.iter_mut() {
                pane.is_selected_cache = pane.is_selected;
                pane.is_selected = true;
                pane.image_load_state = true;
            }

            // Set the slider value to the first pane's current index
            self.slider_value = self.panes[0].img_cache.current_index as u16;
        } else {
            // Single to dual slider: give slider.value to each slider
            for pane in self.panes.iter_mut() {
                pane.slider_value = self.slider_value;
                pane.is_selected = pane.is_selected_cache;
            }
        }

        self.is_slider_dual = !self.is_slider_dual;
    }

    fn toggle_pane_layout(&mut self, pane_layout: PaneLayout) {
        self.pane_layout = pane_layout;
        match self.pane_layout {
            PaneLayout::SinglePane => {
                // self.img_caches.resize(1, Default::default()); // Resize to hold 1 image cache
                self.panes.resize(1, Default::default());
                println!("self.panes.len(): {}", self.panes.len());
                // self.dir_loaded[1] = false;
            }
            PaneLayout::DualPane => {
                self.panes.resize(2, Default::default()); // Resize to hold 2 image caches
                println!("self.panes.len(): {}", self.panes.len());
                //self.pane_layout = PaneLayout::SinglePane;
            }
        }
        // Update other app state as needed...
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
                Command::none()
            }
            Message::Debug(s) => {
                self.title = s;
                Command::none()
            }
            Message::OpenFolder => {
                Command::perform(file_io::pick_folder(), |result| {
                    Message::FolderOpened(result)
                })
            }
            Message::OpenFile => {
                Command::perform(file_io::pick_file(), |result| {
                    Message::FolderOpened(result)
                })
            }
            Message::FileDropped(pane_index, dropped_path) => {
                println!("File dropped: {:?}, pane_index: {}", dropped_path, pane_index);
                println!("self.dir_loaded, pane_index, last_opened_pane: {:?}, {}, {}", self.panes[pane_index as usize].dir_loaded, pane_index, self.last_opened_pane);
                self.initialize_dir_path( PathBuf::from(dropped_path), pane_index as usize);
                
                Command::none()
            }
            Message::Close => {
                self.reset_state();
                // self.current_image = iced::widget::image::Handle::from_memory(vec![]);
                println!("directory_path: {:?}", self.directory_path);
                println!("self.current_image_index: {}", self.current_image_index);
                for (_cache_index, pane) in self.panes.iter_mut().enumerate() {
                    let img_cache = &mut pane.img_cache;
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
            Message::ResetSplit(_position) => {
                self.ver_divider_position = None; Command::none()
            },
            Message::ToggleSliderType(_bool) => {
                self.toggle_slider_type();
                Command::none()
            },
            Message::TogglePaneLayout(pane_layout) => {
                self.toggle_pane_layout(pane_layout);
                Command::none()
            },
            Message::PaneSelected(pane_index, is_selected) => {
                self.panes[pane_index].is_selected = is_selected;
                /*if self.panes[pane_index].is_selected {
                    self.panes[pane_index].is_selected = false;
                } else {
                    self.panes[pane_index].is_selected = true;
                }*/
                
                Command::none()
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
                            }
                        }

                        // println!("image load state: {:?}", self.image_load_state);
                        for pane in self.panes.iter() {
                            println!("pane.image_load_state: {:?}", pane.image_load_state);
                        }

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
                        //self.move_right_all()
                        let command = move_right_all(&mut self.panes);
                        command
    
                    } else if value == self.prev_slider_value.saturating_sub(1) {
                        // Value changed by -1
                        // Call a different function or perform an action for this case
                        move_left_all(&mut self.panes)
                    } else {
                        // Value changed by more than 1 or it's the initial change
                        // Call another function or handle this case differently
                        //self.update_pos(pane_index, value as usize);
                        update_pos(&mut self.panes, pane_index, value as usize);
                        Command::none()
                    }

                } else {
                    let pane = &mut self.panes[pane_index as usize];

                    let pane_index_org = pane_index.clone();
                    let pane_index = pane_index as usize;

                    println!("pane_index {} slider value: {}", pane_index, value);
                    // self.prev_slider_values[pane_index] = self.slider_values[pane_index];
                    pane.prev_slider_value = pane.slider_value;
                    // self.slider_values[pane_index] = value;
                    pane.slider_value = value;
                    println!("pane_index {} prev slider value: {}", pane_index, pane.prev_slider_value);
                    println!("pane_index {} slider value: {}", pane_index, pane.slider_value);
                    
                    // if value == self.prev_slider_values[pane_index] + 1 {
                    if value == pane.prev_slider_value + 1 {
                        println!("move_right_index");
                        // Value changed by +1
                        // Call a function or perform an action for this case
                        move_right_index(&mut self.panes, pane_index)

                    // } else if value == self.prev_slider_values[pane_index].saturating_sub(1) {
                    } else if value == pane.prev_slider_value.saturating_sub(1) {
                        // Value changed by -1
                        // Call a different function or perform an action for this case
                        println!("move_left_index");
                        move_left_index(&mut self.panes, pane_index)
                    } else {
                        // Value changed by more than 1 or it's the initial change
                        // Call another function or handle this case differently
                        println!("update_pos");
                        update_pos(&mut self.panes, pane_index_org, value as usize);
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
                    if self.pane_layout == PaneLayout::DualPane && self.is_slider_dual && !self.panes.iter().any(|pane| pane.is_selected) {
                        println!("No panes selected");
                        return Command::none();
                    }


                    /*println!("image load state bf: {:?}", self.image_load_state);
                    println!("dir_loaded: {:?}", self.dir_loaded);
                    println!("are_all_images_loaded: {}", self.are_all_images_loaded());*/
                    for pane in self.panes.iter() {
                        println!("pane.image_load_state: {:?}", pane.image_load_state);
                    }
                    println!("are_all_images_loaded: {}", self.are_all_images_loaded());
                    println!("are_all_images_loaded_in_selected: {}", self.are_all_images_loaded_in_selected());
                    if self.are_all_images_loaded() {
                    //if self.are_all_images_loaded_in_selected() {
                        self.init_image_loaded(); // [false, false]
                        // println!("image load state af: {:?}", self.image_load_state);

                        // if a pane has reached the directory boundary, mark as loaded
                        let finished_indices: Vec<usize> = self.panes.iter_mut().enumerate().filter_map(|(index, pane)| {
                            let img_cache = &mut pane.img_cache;
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

                        //self.move_right_all()
                        let command = move_right_all(&mut self.panes);
                        command
                    } else {
                        Command::none()
                    }
                }
                Event::Keyboard(keyboard::Event::KeyPressed {
                    key_code: keyboard::KeyCode::Left,
                    modifiers: _,
                }) => {
                    println!("ArrowLeft pressed");
                    // Return if it's dual pane, dual slider and no panes are selected
                    if self.pane_layout == PaneLayout::DualPane && self.is_slider_dual && !self.panes.iter().any(|pane| pane.is_selected) {
                        println!("No panes selected");
                        return Command::none();
                    }

                    for pane in self.panes.iter() {
                        println!("pane.image_load_state: {:?}", pane.image_load_state);
                    }
                    println!("are_all_images_loaded: {}", self.are_all_images_loaded());
                    println!("are_all_images_loaded_in_selected: {}", self.are_all_images_loaded_in_selected());
                    if self.are_all_images_loaded() {
                    //if self.are_all_images_loaded_in_selected() {
                        self.init_image_loaded(); // [false, false]
                        // if a pane has reached the directory boundary, mark as loaded
                        let finished_indices: Vec<usize> = self.panes.iter_mut().enumerate().filter_map(|(index, pane)| {
                            let img_cache = &mut pane.img_cache;
                            if img_cache.image_paths.len() > 0 && img_cache.current_index <= 0 {
                                Some(index)
                            } else {
                                None
                            }
                        }).collect();
                        for finished_index in finished_indices {
                            self.mark_image_loaded(finished_index);
                        }

                        move_left_all(&mut self.panes)
                    } else {
                        Command::none()
                    }
                    
                }

                _ => Command::none(),
            },
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