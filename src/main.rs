
#![windows_subsystem = "windows"]

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


use iced::event::Event;
use iced::Subscription;
use iced::window::events;

use iced::{keyboard, clipboard};
use iced::keyboard::{Key, key::Named,};
use iced::{Element, Length, Theme, Settings, Pixels};
use iced::Task;
use iced::font::{self, Font};
use iced::window;

use std::path::PathBuf;
#[allow(unused_imports)]
use log::{Level, debug, info, warn, error};
use std::borrow::Cow;
use std::process::Command;


extern crate log;

mod image_cache;
use crate::image_cache::LoadOperation;
mod navigation;
use crate::navigation::{move_right_all, move_left_all, update_pos, load_remaining_images};
mod file_io;
use file_io::{Error};
mod menu;
use menu::PaneLayout;
mod widgets;
mod pane;
use crate::pane::get_master_slider_value;
mod ui_builder;
mod loading_status;
mod loading;
mod config;


#[derive(Debug, Clone, Copy)]
pub enum MenuItem {
    Open,
    Close,
    Help
}


pub struct DataViewer {
    title: String,
    directory_path: Option<String>,
    current_image_index: usize,
    slider_value: u16,                  // for master slider
    prev_slider_value: u16,             // for master slider
    ver_divider_position: Option<u16>,
    hor_divider_position: Option<u16>,
    is_slider_dual: bool,
    show_footer: bool,
    pane_layout: PaneLayout,
    last_opened_pane: isize,
    panes: Vec<pane::Pane>,             // Each pane has its own image cache
    loading_status: loading_status::LoadingStatus, // global loading status for all panes
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
            is_slider_dual: false,
            show_footer: true,
            pane_layout: PaneLayout::SinglePane,
            last_opened_pane: -1,
            panes: vec![pane::Pane::default()],
            loading_status: loading_status::LoadingStatus::default(),
            skate_right: false,
            skate_left: false,
            update_counter: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Debug(String),
    Nothing,
    ShowLogs,
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
    ImagesLoaded(Result<(Vec<Option<Vec<u8>>>, Option<LoadOperation>), std::io::ErrorKind>),
    OnVerResize(u16),
    OnHorResize(u16),
    ResetSplit(u16),
    ToggleSliderType(bool),
    TogglePaneLayout(PaneLayout),
    ToggleFooter(bool),
    PaneSelected(usize, bool),
    CopyFilename(usize),
    CopyFilePath(usize),
    KeyPressed(keyboard::Key, keyboard::Modifiers),
    KeyReleased(keyboard::Key, keyboard::Modifiers),
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
        self.loading_status = loading_status::LoadingStatus::default();
        self.skate_right = false;
        self.update_counter = 0;
    }

    fn initialize_dir_path(&mut self, path: PathBuf, pane_index: usize) {
        debug!("last_opened_pane: {}", self.last_opened_pane);

        let pane_file_lengths = self.panes.iter().map(|pane| pane.img_cache.image_paths.len()).collect::<Vec<usize>>();
        let pane = &mut self.panes[pane_index];
        debug!("pane_file_lengths: {:?}", pane_file_lengths);
        pane.initialize_dir_path(
            &self.pane_layout, &pane_file_lengths, pane_index, path, self.is_slider_dual, &mut self.slider_value);

        debug!("pane_index: {}, self.panes.len(): {}", pane_index, self.panes.len());
        if pane_index >= self.panes.len() {
            self.panes.resize_with(pane_index + 1, || pane::Pane::default());
            debug!("resized pane_index: {}, self.panes.len(): {}", pane_index, self.panes.len());
        }

        self.last_opened_pane = pane_index as isize;
    }


    /*
    fn handle_hotkey(key: keyboard::Key) -> Option<Message> {
    use keyboard::key::{self, Key};
    use pane_grid::{Axis, Direction};

    match key.as_ref() {
        Key::Character("v") => Some(Message::SplitFocused(Axis::Vertical)),
        Key::Character("h") => Some(Message::SplitFocused(Axis::Horizontal)),
        Key::Character("w") => Some(Message::CloseFocused),
        Key::Named(key) => {
            let direction = match key {
                    key::Named::ArrowUp => Some(Direction::Up),
                    key::Named::ArrowDown => Some(Direction::Down),
                    key::Named::ArrowLeft => Some(Direction::Left),
                    key::Named::ArrowRight => Some(Direction::Right),
                    _ => None,
                };
    */

    fn handle_key_pressed_event(&mut self, key: keyboard::Key, modifiers: keyboard::Modifiers) -> Vec<Task<Message>> {
        let mut tasks = Vec::new();
        match key.as_ref() {
            Key::Named(Named::Tab) => {
                debug!("Tab pressed");
                // toggle footer
                self.toggle_footer();
            }

            //Named::Space | Key::Character(c) if c == SmolStr::from("b") => {
            Key::Named(Named::Space) | Key::Character("b") => {
                debug!("Space pressed");
                // Toggle slider type
                self.toggle_slider_type();
            }

            Key::Character("1") => {
                debug!("Key1 pressed");
                if self.pane_layout == PaneLayout::DualPane && self.is_slider_dual {
                    self.panes[0].is_selected = !self.panes[0].is_selected;
                }

                // If alt+ctrl is pressed, load a file into pane0
                if modifiers.alt() && modifiers.control() {
                    debug!("Key1 Shift pressed");
                    tasks.push(Task::perform(file_io::pick_file(), move |result| {
                        Message::FolderOpened(result, 0)
                    }));
                }

                // If alt is pressed, load a folder into pane0
                if modifiers.alt() {
                    debug!("Key1 Alt pressed");
                    tasks.push(Task::perform(file_io::pick_folder(), move |result| {
                        Message::FolderOpened(result, 0)
                    }));
                }

                // If ctrl is pressed, switch to single pane layout
                if modifiers.control() {
                    self.toggle_pane_layout(PaneLayout::SinglePane);
                }
            }
            Key::Character("2") => {
                debug!("Key2 pressed");
                if self.pane_layout == PaneLayout::DualPane {
                    if self.is_slider_dual {
                        self.panes[1].is_selected = !self.panes[1].is_selected;
                    }
                
                    // If alt+ctrl is pressed, load a file into pane1
                    if modifiers.alt() && modifiers.control() {
                        debug!("Key2 Shift pressed");
                        tasks.push(Task::perform(file_io::pick_file(), move |result| {
                            Message::FolderOpened(result, 1)
                        }));
                    }

                    // If alt is pressed, load a folder into pane1
                    if modifiers.alt() {
                        debug!("Key2 Alt pressed");
                        tasks.push(Task::perform(file_io::pick_folder(), move |result| {
                            Message::FolderOpened(result, 1)
                        }));
                    }
                }

                // If ctrl is pressed, switch to dual pane layout
                if modifiers.control() {
                    debug!("Key2 Ctrl pressed");
                    self.toggle_pane_layout(PaneLayout::DualPane);
                }
            }

            Key::Character("c") |
            Key::Character("w") => {
                // Close the selected panes
                if modifiers.control() {
                    for pane in self.panes.iter_mut() {
                        if pane.is_selected {
                            pane.reset_state();
                        }
                    }
                }
            }

            Key::Character("q") => {
                // Terminate the app
                std::process::exit(0);
            }

            Key::Named(Named::ArrowLeft) | Key::Character("a") => {
                if self.skate_right {
                    self.skate_right = false;

                    // Discard all queue items that are LoadNext or ShiftNext
                    self.loading_status.reset_load_next_queue_items();
                }

                if self.pane_layout == PaneLayout::DualPane && self.is_slider_dual && !self.panes.iter().any(|pane| pane.is_selected) {
                    debug!("No panes selected");
                }

                if modifiers.shift() {
                    self.skate_left = true;
                } else {
                    self.skate_left = false;

                    let task = move_left_all(
                        &mut self.panes, &mut self.loading_status, &mut self.slider_value,
                        &self.pane_layout, self.is_slider_dual, self.last_opened_pane as usize);
                    tasks.push(task);
                }
                
            }
            Key::Named(Named::ArrowRight) | Key::Character("d") => {
                if self.skate_left {
                    self.skate_left = false;

                    // Discard all queue items that are LoadPrevious or ShiftPrevious
                    self.loading_status.reset_load_previous_queue_items();
                }

                if self.pane_layout == PaneLayout::DualPane && self.is_slider_dual && !self.panes.iter().any(|pane| pane.is_selected) {
                    debug!("No panes selected");
                }

                if modifiers.shift() {
                    self.skate_right = true;
                } else {
                    self.skate_right = false;

                    let task = move_right_all(
                        &mut self.panes, &mut self.loading_status, &mut self.slider_value,
                        &self.pane_layout, self.is_slider_dual, self.last_opened_pane as usize);
                    tasks.push(task);
                }
            }

            _ => {}
        }

        tasks
    }

    fn handle_key_released_event(&mut self, key_code: keyboard::Key, _modifiers: keyboard::Modifiers) -> Vec<Task<Message>> {
        #[allow(unused_mut)]
        let mut tasks = Vec::new();

        match key_code.as_ref() {
            Key::Named(Named::Tab) => {
                debug!("Tab released");
            }
            Key::Named(Named::Enter) | Key::Character("NumpadEnter")  => {
                debug!("Enter key released!");
                
            }
            Key::Named(Named::Escape) => {
                debug!("Escape key released!");
                
            }
            Key::Named(Named::ArrowLeft) | Key::Character("a") => {
                debug!("Left key or 'A' key released!");
                self.skate_left = false;
            }
            Key::Named(Named::ArrowRight) | Key::Character("d") => {
                debug!("Right key or 'D' key released!");
                self.skate_right = false;
            }
            _ => {},
        }

        tasks
    }

    fn toggle_slider_type(&mut self) {
        // When toggling from dual to single, reset pane.is_selected to true
        if self.is_slider_dual {
            for pane in self.panes.iter_mut() {
                pane.is_selected_cache = pane.is_selected;
                pane.is_selected = true;
                pane.is_next_image_loaded = false;
                pane.is_prev_image_loaded = false;
            }

            let mut panes_refs: Vec<&mut pane::Pane> = self.panes.iter_mut().collect();
            self.slider_value = get_master_slider_value(&mut panes_refs, &self.pane_layout, self.is_slider_dual, self.last_opened_pane as usize) as u16;
        } else {
            // Single to dual slider: give slider.value to each slider
            for pane in self.panes.iter_mut() {
                pane.slider_value = pane.img_cache.current_index as u16;
                pane.is_selected = pane.is_selected_cache;
            }
        }
        self.is_slider_dual = !self.is_slider_dual;
    }

    fn toggle_pane_layout(&mut self, pane_layout: PaneLayout) {
        match pane_layout {
            PaneLayout::SinglePane => {
                self.panes.resize(1, Default::default());
                debug!("self.panes.len(): {}", self.panes.len());

                if self.pane_layout == PaneLayout::DualPane {
                    // Reset the slider value to the first pane's current index
                    let mut panes_refs: Vec<&mut pane::Pane> = self.panes.iter_mut().collect();
                    self.slider_value = get_master_slider_value(&mut panes_refs, &pane_layout, self.is_slider_dual, self.last_opened_pane as usize) as u16;
                    self.panes[0].is_selected = true;
                }
            }
            PaneLayout::DualPane => {
                self.panes.resize(2, Default::default()); // Resize to hold 2 image caches
                debug!("self.panes.len(): {}", self.panes.len());
            }
        }
        self.pane_layout = pane_layout;
    }
    fn toggle_footer(&mut self) {
        self.show_footer = !self.show_footer;
    }

    
    fn open_in_file_explorer(&self, path: &str) {
        if cfg!(target_os = "windows") {
            // Windows: Use "explorer" to open the directory
            Command::new("explorer")
                .arg(path)
                .spawn()
                .expect("Failed to open directory in File Explorer");
        } else if cfg!(target_os = "macos") {
            // macOS: Use "open" to open the directory
            Command::new("open")
                .arg(path)
                .spawn()
                .expect("Failed to open directory in Finder");
        } else if cfg!(target_os = "linux") {
            // Linux: Use "xdg-open" to open the directory (works with most desktop environments)
            Command::new("xdg-open")
                .arg(path)
                .spawn()
                .expect("Failed to open directory in File Explorer");
        } else {
            eprintln!("Opening directories is not supported on this OS.");
        }
    }

    pub fn new() -> Self {
        Self::default()
    }

    fn title(&self) -> String {
        match self.pane_layout  {
            PaneLayout::SinglePane => {
                if self.panes[0].dir_loaded {
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

    
    //fn update(&mut self, message: Message) -> Task<Self::Message> {
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Nothing => {}
            Message::Debug(s) => {
                self.title = s;
            }
            Message::ShowLogs => {
                let app_name = "viewskater";
                let log_dir_path = file_io::get_log_directory(app_name);
                std::fs::create_dir_all(log_dir_path.clone());
                self.open_in_file_explorer(&log_dir_path.to_string_lossy().to_string());
            }
            Message::FontLoaded(_) => {}
            Message::OpenFolder(pane_index) => {
                return Task::perform(file_io::pick_folder(), move |result| {
                    Message::FolderOpened(result, pane_index)
                });
            }
            Message::OpenFile(pane_index) => {
                return Task::perform(file_io::pick_file(), move |result| {
                    Message::FolderOpened(result, pane_index)
                });
            }
            Message::FileDropped(pane_index, dropped_path) => {
                debug!("File dropped: {:?}, pane_index: {}", dropped_path, pane_index);
                debug!("self.dir_loaded, pane_index, last_opened_pane: {:?}, {}, {}", self.panes[pane_index as usize].dir_loaded, pane_index, self.last_opened_pane);
                self.initialize_dir_path( PathBuf::from(dropped_path), pane_index as usize);
            }
            Message::Close => {
                self.reset_state();
                debug!("directory_path: {:?}", self.directory_path);
                debug!("self.current_image_index: {}", self.current_image_index);
                for (_cache_index, pane) in self.panes.iter_mut().enumerate() {
                    let img_cache = &mut pane.img_cache;
                    debug!("img_cache.current_index: {}", img_cache.current_index);
                    debug!("img_cache.image_paths.len(): {}", img_cache.image_paths.len());
                }
            }
            Message::Quit => {
                std::process::exit(0);
            }
            Message::FolderOpened(result, pane_index) => {
                match result {
                    Ok(dir) => {
                        debug!("Folder opened: {}", dir);
                        self.initialize_dir_path(PathBuf::from(dir), pane_index);
                    }
                    Err(err) => {
                        debug!("Folder open failed: {:?}", err);
                    }
                }
            },
            Message::CopyFilename(pane_index) => {
                // Get the image path of the specified pane
                let img_path = self.panes[pane_index].img_cache
                    .image_paths[self.panes[pane_index].img_cache.current_index]
                    .file_name().map(|name| name.to_string_lossy().to_string());
                if let Some(img_path) = img_path {
                    if let Some(filename) = file_io::get_filename(&img_path) {
                        debug!("Filename: {}", filename);
                        return clipboard::write::<Message>(filename.to_string());
                    }
                }
            }
            Message::CopyFilePath(pane_index) => {
                // Get the image path of the specified pane
                let img_path = self.panes[pane_index].img_cache
                    .image_paths[self.panes[pane_index].img_cache.current_index]
                    .file_name().map(|name| name.to_string_lossy().to_string());
                if let Some(img_path) = img_path {
                    if let Some(dir_path) = self.panes[pane_index].directory_path.as_ref() {
                        let full_path = format!("{}/{}", dir_path, img_path);
                        debug!("Full Path: {}", full_path);
                        return clipboard::write::<Message>(full_path);
                    }
                }
            }
            Message::OnVerResize(position) => { self.ver_divider_position = Some(position); },
            Message::OnHorResize(position) => { self.hor_divider_position = Some(position); },
            Message::ResetSplit(_position) => { self.ver_divider_position = None; },
            Message::ToggleSliderType(_bool) => { self.toggle_slider_type(); },
            Message::TogglePaneLayout(pane_layout) => { self.toggle_pane_layout(pane_layout); },
            Message::ToggleFooter(_bool) => { self.toggle_footer(); },
            Message::PaneSelected(pane_index, is_selected) => {
                self.panes[pane_index].is_selected = is_selected;
                for (index, pane) in self.panes.iter_mut().enumerate() {
                    debug!("pane_index: {}, is_selected: {}", index, pane.is_selected);
                }
            }
            
            Message::ImagesLoaded(result) => {
                match result {
                    Ok((image_data, operation)) => {
                        if let Some(op) = operation {
                            let cloned_op = op.clone();
                            match op {
                                LoadOperation::LoadNext((ref pane_indices, ref target_indices))
                                | LoadOperation::LoadPrevious((ref pane_indices, ref target_indices))
                                | LoadOperation::ShiftNext((ref pane_indices, ref target_indices))
                                | LoadOperation::ShiftPrevious((ref pane_indices, ref target_indices)) => {
                                    let operation_type = cloned_op.operation_type();
            
                                    loading::handle_load_operation_all(
                                        &mut self.panes,
                                        &mut self.loading_status,
                                        pane_indices,
                                        target_indices.clone(),
                                        image_data,
                                        cloned_op,
                                        operation_type,
                                    );
                                }
                                LoadOperation::LoadPos((pane_index, target_indices_and_cache)) => {
                                    loading::handle_load_pos_operation(
                                        &mut self.panes,
                                        &mut self.loading_status,
                                        pane_index,
                                        target_indices_and_cache.clone(),
                                        image_data,
                                    );
                                }
                            }
                        }
                    }
                    Err(err) => {
                        debug!("Image load failed: {:?}", err);
                    }
                }
            }
            
            

            Message::SliderChanged(pane_index, value) => {
                debug!("pane_index {} slider value: {}", pane_index, value);
                // -1 means the master slider (broadcast operation to all panes)
                if pane_index == -1 {
                    self.prev_slider_value = self.slider_value;
                    self.slider_value = value;
                    debug!("slider - update_pos");
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
                debug!("slider released: pane_index: {}, value: {}", pane_index, value);
                if pane_index == -1 {
                    return load_remaining_images(
                        &mut self.panes, &mut self.loading_status, pane_index, value as usize);
                } else {
                    return load_remaining_images(&mut self.panes, &mut self.loading_status, pane_index as isize, value as usize);
                }
            }

            Message::KeyPressed(key, modifiers) => {
                let tasks = self.handle_key_pressed_event(key, modifiers);
                if tasks.is_empty() {
                    //Task::none()
                } else {
                    return Task::batch(tasks);
                }
            }
            Message::KeyReleased(key, modifiers) => {
                let tasks = self.handle_key_released_event(key, modifiers);
                if tasks.is_empty() {
                    //Task::none()
                } else {
                    return Task::batch(tasks);
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
                        },
                        PaneLayout::DualPane => {
                        }
                    }
                }
                #[cfg(target_os = "linux")]
                Event::Window(iced::window::Event::FileDropped(dropped_path)) => {
                    match self.pane_layout {
                        PaneLayout::SinglePane => {
                            debug!("File dropped: {:?}", dropped_path);
                            self.initialize_dir_path(dropped_path, 0);
                        },
                        PaneLayout::DualPane => {}
                    }
                }

                _ => return Task::none(),
            },
        }

        if self.skate_right {
            self.update_counter = 0;
            let task = move_right_all(
                &mut self.panes,
                &mut self.loading_status,
                &mut self.slider_value,
                &self.pane_layout,
                self.is_slider_dual,
                self.last_opened_pane as usize
            );
            task
        } else if self.skate_left {
            self.update_counter = 0;
            let task = move_left_all(
                &mut self.panes,
                &mut self.loading_status,
                &mut self.slider_value,
                &self.pane_layout,
                self.is_slider_dual,
                self.last_opened_pane as usize
            );
            task
        } else {
            debug!("no skate mode detected");
            let task = Task::none();
            task
        }
    }

    fn view(&self) -> Element<Message> {
        let container_all = ui_builder::build_ui(&self);
        container_all
        .height(Length::Fill)
        .width(Length::Fill)
        ////.center_x(Length::Fill)
        .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch(vec![
            events().map(|(_id, event)| Message::Event(iced::Event::Window(event))),
            
            keyboard::on_key_press(|key, modifiers| {
                Some(Message::KeyPressed(key, modifiers))
            }),
            keyboard::on_key_release(|key, modifiers| {
                Some(Message::KeyReleased(key, modifiers))
            }),

        ])
    }

    fn theme(&self) -> Theme {
        /*iced::Theme::custom(
            iced::theme::Palette {
                primary: iced::Color::from_rgba8(20, 148, 163, 1.0),
                ..iced::Theme::Dark.palette()
            }
        )*/
        iced::Theme::custom(
            "Custom Theme".to_string(),
            iced::theme::Palette {
                primary: iced::Color::from_rgba8(20, 148, 163, 1.0),
                ..iced::Theme::Dark.palette()
            },
        )
    }
}


// Include the icon image data at compile time
static ICON: &[u8] = if cfg!(target_os = "windows") {
    include_bytes!("../assets/icon.ico")
} else if cfg!(target_os = "macos") {
    include_bytes!("../assets/icon_512.png")
} else {
    include_bytes!("../assets/icon_48.png")
};


pub fn load_fonts() -> Vec<Cow<'static, [u8]>> {
    vec![
        include_bytes!("../assets/fonts/viewskater-fonts.ttf")          // icon font
            .as_slice()
            .into(),
        include_bytes!("../assets/fonts/Iosevka-Regular-ascii.ttf")     // footer digit font
            .as_slice()
            .into(),
        include_bytes!("../assets/fonts/Roboto-Regular.ttf")            // UI font
            .as_slice()
            .into(),
    ]
}


fn main() -> iced::Result {
    // Set up panic hook to log to a file
    let app_name = "viewskater";
    //file_io::setup_logger(app_name);
    //file_io::setup_panic_hook(app_name);

    // Initialize the logger and retrieve the shared log buffer
    let shared_log_buffer = file_io::setup_logger(app_name);

    // Set up the panic hook with the shared log buffer
    file_io::setup_panic_hook(app_name, shared_log_buffer);
    

    // 0.10.0
    /*let icon = iced::window::icon::from_file_data(ICON, Option::None);
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
    }*/

    // 0.13.1 (WIP)
    let settings = Settings {
        id: None,
        fonts: load_fonts(),
        default_font: Font::with_name("Roboto"),
        default_text_size: Pixels(20.0),
        antialiasing: true,
        ..Settings::default()
    };

    // Run the application with custom settings
    iced::application(
        DataViewer::title,
        DataViewer::update,
        DataViewer::view,
    )
    .window(window::Settings {
        icon: Some(
            window::icon::from_file_data(
                ICON,
                None,
            )
            .expect("Icon load failed")
        ),
        ..Default::default()
    })
    .theme(DataViewer::theme)
    .subscription(DataViewer::subscription)
    .settings(settings)
    .run_with(|| (DataViewer::new(), Task::none()))
    .inspect_err(|err| error!("Runtime error: {}", err))?;

    info!("Application exited");

    Ok(())
}