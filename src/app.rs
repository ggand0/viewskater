#![windows_subsystem = "windows"]

#[warn(unused_imports)]
#[cfg(target_os = "linux")]
mod other_os {
    //pub use iced;
    pub use iced_custom as iced;
}

#[cfg(not(target_os = "linux"))]
mod macos {
    pub use iced_custom as iced;
}

#[cfg(target_os = "linux")]
use other_os::*;

#[cfg(not(target_os = "linux"))]
use macos::*;

use std::path::PathBuf;
use std::borrow::Cow;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[allow(unused_imports)]
use log::{Level, debug, info, warn, error};
/*
use iced::{
    clipboard, Element, Length, Pixels, Settings, Subscription, Task, Theme,
    event::Event, keyboard::{self, Key, key::Named},
    widget::{self, text, button, container, column, row},
    font::{self, Font},
    window::{self, events},
};

use wgpu;
use pollster;

mod cache;
use crate::cache::img_cache::LoadOperation;
mod navigation;
use crate::navigation::{move_right_all, move_left_all, update_pos, load_remaining_images};
mod file_io;
mod menu;
use menu::PaneLayout;
mod widgets;
mod pane;
use crate::pane::Pane;
mod ui_builder;
mod loading_status;
mod loading;
mod config;
use crate::widgets::shader::scene::Scene;*/

//use wgpu;
use pollster;
use iced_wgpu::{wgpu, Engine, Renderer};
use iced_winit::core::Theme as WinitTheme;

//use iced_winit::core::{Color};
use iced_widget::{slider, text_input};

use crate::cache::img_cache::LoadOperation;
use crate::navigation_keyboard::{move_right_all, move_left_all};
use crate::navigation_slider::{update_pos, load_remaining_images};
use crate::menu::PaneLayout;
use crate::pane::Pane;
use crate::widgets::shader::scene::Scene;
use crate::pane;
use crate::loading_status;
use crate::file_io;
use crate::widgets;
use crate::ui_builder;
use crate::loading;
//use crate::widgets::modal;
//use iced_widget::modal as widget_modal;

use iced_winit::winit::keyboard::{KeyCode, PhysicalKey};
use crate::navigation_slider;
use crate::utils::timing::TimingStats;
use once_cell::sync::Lazy;
use std::sync::Mutex;
use crate::widgets::shader::cpu_scene::CpuScene;
use crate::widgets::shader::texture_scene::TextureScene;

static APP_UPDATE_STATS: Lazy<Mutex<TimingStats>> = Lazy::new(|| {
    Mutex::new(TimingStats::new("App Update"))
});


use iced::{
    clipboard, Pixels, Settings, Subscription,
    event::Event,// keyboard::{self, Key, key::Named},
    widget::{self, button},
    font::{self, Font},
    window::{self, events},
};
use iced_winit::runtime::{Program, Task};

use iced_widget::{center, shader, row, column, container, text};
use iced_winit::core::{Color, Element, Length, Length::*, Theme};
use iced_core::alignment::Horizontal;
use iced_core::keyboard::{self, Key, key::Named};
use crate::cache::img_cache::CachedData;

#[derive(Debug, Clone, Copy)]
pub enum MenuItem {
    Open,
    Close,
    Help
}

use crate::cache::img_cache::CacheStrategy;

pub struct DataViewer {
    pub background_color: Color,//debug
    pub title: String,
    pub directory_path: Option<String>,
    pub current_image_index: usize,
    pub slider_value: u16,                              // for master slider
    pub prev_slider_value: u16,                         // for master slider
    pub ver_divider_position: Option<u16>,
    pub hor_divider_position: Option<u16>,
    pub is_slider_dual: bool,
    pub show_footer: bool,
    pub pane_layout: PaneLayout,
    pub last_opened_pane: isize,
    pub panes: Vec<pane::Pane>,                         // Each pane has its own image cache
    pub loading_status: loading_status::LoadingStatus,  // global loading status for all panes
    pub skate_right: bool,
    pub skate_left: bool,
    pub update_counter: u32,
    pub show_about: bool,
    pub device: Arc<wgpu::Device>,                     // Shared ownership using Arc
    pub queue: Arc<wgpu::Queue>,                       // Shared ownership using Arc
    //pub device: Option<Arc<wgpu::Device>>,  // Now it's an Option
    //pub queue: Option<Arc<wgpu::Queue>>,    // Now it's an Option
    pub is_gpu_supported: bool,
    pub cache_strategy: CacheStrategy,
    pub last_slider_update: Instant,
    pub is_slider_moving: bool,
    pub backend: wgpu::Backend,
}


impl DataViewer {
    pub fn new(device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>, backend: wgpu::Backend) -> Self {
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
            panes: vec![pane::Pane::new(Arc::clone(&device), Arc::clone(&queue), backend)],
            //panes: vec![pane::Pane::new()],
            loading_status: loading_status::LoadingStatus::default(),
            skate_right: false,
            skate_left: false,
            update_counter: 0,
            show_about: false,
            device,   // Store the Arc<device>
            queue,    // Store the Arc<queue>
            is_gpu_supported: true,
            background_color: Color::WHITE,
            last_slider_update: Instant::now(),
            is_slider_moving: false,
            backend: backend,
            //cache_strategy: CacheStrategy::Atlas,
            cache_strategy: CacheStrategy::Cpu,
        }
    }

    // moved here
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
        self.show_about = false;
        self.last_slider_update = Instant::now();
        self.is_slider_moving = false;
    }

    fn initialize_dir_path(&mut self, path: PathBuf, pane_index: usize) {
        debug!("last_opened_pane: {}", self.last_opened_pane);

        let pane_file_lengths = self.panes.iter().map(
            |pane| pane.img_cache.image_paths.len()).collect::<Vec<usize>>();
        let pane = &mut self.panes[pane_index];
        debug!("pane_file_lengths: {:?}", pane_file_lengths);

        pane.initialize_dir_path(
            Arc::clone(&self.device),
            Arc::clone(&self.queue),
            self.is_gpu_supported,
            &self.pane_layout,  // Pass the required &PaneLayout
            &pane_file_lengths, // Pass &[usize]
            pane_index,
            path,
            self.is_slider_dual,
            &mut self.slider_value,
        );
    
        debug!("pane_index: {}, self.panes.len(): {}", pane_index, self.panes.len());
        if pane_index >= self.panes.len() {
            self.panes.resize_with(pane_index + 1, || pane::Pane::default());
            debug!("resized pane_index: {}, self.panes.len(): {}", pane_index, self.panes.len());
        }

        self.last_opened_pane = pane_index as isize;
    }

    fn handle_key_pressed_event(&mut self, key: keyboard::Key, modifiers: keyboard::Modifiers) -> Vec<Task<Message>> {
        let mut tasks = Vec::new();
        match key.as_ref() {
            Key::Named(Named::Tab) => {
                debug!("Tab pressed");
                self.toggle_footer();
            }

            Key::Named(Named::Space) | Key::Character("b") => {
                debug!("Space pressed");
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

                if self.skate_left {
                    // will be handled at the end of update() to run move_left_all
                } else {
                    if modifiers.shift() {
                        self.skate_left = true;
                    } else {
                        self.skate_left = false;

                        debug!("move_left_all from handle_key_pressed_event()");
                        let task = move_left_all(
                            //Some(Arc::clone(&self.device)), Some(Arc::clone(&self.queue)), self.is_gpu_supported,
                            &self.device, &self.queue, self.cache_strategy,
                            &mut self.panes, &mut self.loading_status, &mut self.slider_value,
                            &self.pane_layout, self.is_slider_dual, self.last_opened_pane as usize);
                        tasks.push(task);
                    }
                }

            }
            Key::Named(Named::ArrowRight) | Key::Character("d") => {
                debug!("Right key or 'D' key pressed!");
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
                        //Some(Arc::clone(&self.device)), Some(Arc::clone(&self.queue)), self.is_gpu_supported,
                        &self.device, &self.queue, self.cache_strategy,
                        &mut self.panes, &mut self.loading_status, &mut self.slider_value,
                        &self.pane_layout, self.is_slider_dual, self.last_opened_pane as usize);
                    tasks.push(task);
                    debug!("handle_key_pressed_event() - tasks count: {}", tasks.len());
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
            self.slider_value = pane::get_master_slider_value(&mut panes_refs, &self.pane_layout, self.is_slider_dual, self.last_opened_pane as usize) as u16;
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
                //self.panes.resize(1, Default::default());
                Pane::resize_panes(&mut self.panes, 1);

                debug!("self.panes.len(): {}", self.panes.len());

                if self.pane_layout == PaneLayout::DualPane {
                    // Reset the slider value to the first pane's current index
                    let mut panes_refs: Vec<&mut pane::Pane> = self.panes.iter_mut().collect();
                    self.slider_value = pane::get_master_slider_value(&mut panes_refs, &pane_layout, self.is_slider_dual, self.last_opened_pane as usize) as u16;
                    self.panes[0].is_selected = true;
                }
            }
            PaneLayout::DualPane => {
                //self.panes.resize(2, Default::default()); // Resize to hold 2 image caches
                Pane::resize_panes(&mut self.panes, 2);
                debug!("self.panes.len(): {}", self.panes.len());
            }
        }
        self.pane_layout = pane_layout;
    }

    fn toggle_footer(&mut self) {
        self.show_footer = !self.show_footer;
    }

    /*fn subscription(&self) -> Subscription<Message> {
        Subscription::batch(vec![
            events().map(|(_id, event)| Message::Event(iced::Event::Window(event))),
            keyboard::on_key_press(|key, modifiers| {
                Some(Message::KeyPressed(key, modifiers))
            }),
            keyboard::on_key_release(|key, modifiers| {
                Some(Message::KeyReleased(key, modifiers))
            }),

        ])
    }*/

    fn theme(&self) -> Theme {
        iced::Theme::custom(
            "Custom Theme".to_string(),
            iced::theme::Palette {
                primary: iced::Color::from_rgba8(20, 148, 163, 1.0),
                ..iced::Theme::Dark.palette()
            },
        )
    }

    pub fn title(&self) -> String {
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
}

#[derive(Debug, Clone)]
pub enum Message {
    Debug(String),
    Nothing,
    ShowAbout,
    HideAbout,
    ShowLogs,
    OpenWebLink(String),
    FontLoaded(Result<(), font::Error>),
    OpenFolder(usize),
    OpenFile(usize),
    FileDropped(isize, String),
    Close,
    Quit,
    FolderOpened(Result<String, file_io::Error>, usize),
    SliderChanged(isize, u16),
    SliderReleased(isize, u16),
    SliderImageLoaded(Result<(usize, CachedData), usize>),
    Event(Event),
    //ImagesLoaded(Result<(Vec<Option<Vec<u8>>>, Option<LoadOperation>), std::io::ErrorKind>),
    ImagesLoaded(Result<(Vec<Option<CachedData>>, Option<LoadOperation>), std::io::ErrorKind>),

    OnVerResize(u16),
    OnHorResize(u16),
    ResetSplit(u16),
    ToggleSliderType(bool),
    TogglePaneLayout(PaneLayout),
    ToggleFooter(bool),
    PaneSelected(usize, bool),
    CopyFilename(usize),
    CopyFilePath(usize),
    //KeyPressed(keyboard::Key, keyboard::Modifiers),
    //KeyReleased(keyboard::Key, keyboard::Modifiers),
    BackgroundColorChanged(Color),
    TimerTick,
}

//impl DataViewer {
impl iced_winit::runtime::Program for DataViewer {
    type Theme = WinitTheme;
    type Message = Message;
    type Renderer = Renderer;

    //fn update(&mut self, message: Message) -> iced_winit::runtime::Task<Message> {
    fn update(&mut self, message: Message) -> iced_winit::runtime::Task<Message> {
        //debug!("Received message: {:?}", message);
        let update_start = Instant::now();
        match message {
            Message::BackgroundColorChanged(color) => {
                self.background_color = color;
            }
            Message::Nothing => {}
            Message::Debug(s) => {
                self.title = s;
            }
            Message::ShowLogs => {
                let app_name = "viewskater";
                let log_dir_path = file_io::get_log_directory(app_name);
                let _ = std::fs::create_dir_all(log_dir_path.clone());
                file_io::open_in_file_explorer(&log_dir_path.to_string_lossy().to_string());
            }
            Message::ShowAbout => {
                self.show_about = true;
                return widget::focus_next()
            }
            Message::HideAbout => {
                self.show_about = false;
            }
            Message::OpenWebLink(url) => {
                if let Err(e) = webbrowser::open(&url) {
                    warn!("Failed to open link: {}, error: {:?}", url, e);
                }
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
                //debug!("ImagesLoaded result: {:?}", result);
                debug!("ImagesLoaded");
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
                                        image_data,  // Now using Vec<Option<CachedData>>
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

            Message::SliderImageLoaded(result) => {
                match result {
                    Ok((pos, cached_data)) => {
                        let pane = &mut self.panes[0]; // For single-pane slider
                        
                        // If slider is still moving, update the slider_scene
                        if self.is_slider_moving {
                            debug!("SLIDER_DEBUG: Updating slider_scene for pos {}", pos);
                            
                            if let CachedData::Cpu(bytes) = &cached_data {
                                // Create slider_scene if it doesn't exist - wrap CpuScene in the Scene enum
                                if pane.slider_scene.is_none() {
                                    let cpu_scene = CpuScene::new(bytes.clone());
                                    pane.slider_scene = Some(Scene::CpuScene(cpu_scene));
                                } else if let Some(Scene::CpuScene(scene)) = &mut pane.slider_scene {
                                    // Update existing CPU scene
                                    scene.update_image(bytes.clone());
                                } else {
                                    // Replace with a new CPU scene
                                    let cpu_scene = CpuScene::new(bytes.clone());
                                    pane.slider_scene = Some(Scene::CpuScene(cpu_scene));
                                }
                                
                                // Ensure texture is created
                                if let Some(scene) = &mut pane.slider_scene {
                                    // For Scene enum, we need to call ensure_texture through the enum's method
                                    if let Some(device) = &pane.device {
                                        if let Some(queue) = &pane.queue {
                                            scene.ensure_texture(Arc::clone(device), Arc::clone(queue));
                                        }
                                    }
                                }
                            }
                        } else {
                            // Slider is released, update the main scene
                            debug!("SLIDER_DEBUG: Updating main scene for pos {}", pos);
                            
                            match cached_data {
                                CachedData::Cpu(bytes) => {
                                    pane.current_image = CachedData::Cpu(bytes.clone());
                                    // Create a new CpuScene wrapped in the Scene enum
                                    let cpu_scene = CpuScene::new(bytes.clone());
                                    pane.scene = Some(Scene::CpuScene(cpu_scene));
                                    
                                    // Ensure texture is created
                                    if let Some(scene) = &mut pane.scene {
                                        if let Some(device) = &pane.device {
                                            if let Some(queue) = &pane.queue {
                                                scene.ensure_texture(Arc::clone(device), Arc::clone(queue));
                                            }
                                        }
                                    }
                                },
                                CachedData::Gpu(texture) => {
                                    pane.current_image = CachedData::Gpu(Arc::clone(&texture));
                                    if let Some(Scene::TextureScene(scene)) = &mut pane.scene {
                                        scene.update_texture(Arc::clone(&texture));
                                    } else {
                                        // Create a new TextureScene wrapped in the Scene enum
                                        let texture_scene = TextureScene::new(Some(&CachedData::Gpu(Arc::clone(&texture))));
                                        pane.scene = Some(Scene::TextureScene(texture_scene));
                                    }
                                },
                                _ => {}
                            }
                            
                            // Update cache indices only on slider release
                            pane.img_cache.current_index = pos;
                            debug!("SLIDER_DEBUG: Updated current_index for pos {}", pos);
                        }
                    },
                    Err(pos) => {
                        debug!("SLIDER_DEBUG: Failed to load image for pos {}", pos);
                    }
                }

            }
            
            
            Message::SliderChanged(pane_index, value) => {
                debug!("SLIDER_DEBUG: SliderChanged from {} to {} (delta: {})", 
                       self.slider_value, value, (value as i32 - self.slider_value as i32).abs());
                
                self.is_slider_moving = true;
                self.last_slider_update = Instant::now();
                //let use_sync = true;
                let use_sync = false;
                
                if pane_index == -1 {
                    self.prev_slider_value = self.slider_value;
                    self.slider_value = value;
                    debug!("SLIDER_DEBUG: Calling update_pos for master slider value {}", value);
                    
                    return navigation_slider::update_pos(
                        //&self.device, 
                        //&self.queue, 
                        &mut self.panes, 
                        pane_index, 
                        value as usize, 
                        self.cache_strategy,
                        use_sync
                    );
                } else {
                    let pane = &mut self.panes[pane_index as usize];
                    let pane_index_usize = pane_index as usize;
                    
                    pane.prev_slider_value = pane.slider_value;
                    pane.slider_value = value;
                    debug!("SLIDER_DEBUG: Calling update_pos for pane {} slider value {}", 
                           pane_index_usize, value);
                    
                    return navigation_slider::update_pos(
                        //&self.device, 
                        //&self.queue, 
                        &mut self.panes, 
                        pane_index, 
                        value as usize, 
                        self.cache_strategy,
                        use_sync
                    );
                }
            }
            

            /*Message::SliderChanged(pane_index, value) => {
                self.is_slider_moving = true;
                let now = Instant::now();

                // Ignore updates if the last update was too recent (throttle)
                //if now.duration_since(self.last_slider_update) < Duration::from_millis(50) {
                if now.duration_since(self.last_slider_update) < Duration::from_millis(100) {
                //if now.duration_since(self.last_slider_update) < Duration::from_millis(1000) {
                    return Task::none();
                }

                self.last_slider_update = now;


                debug!("pane_index {} slider value: {}", pane_index, value);

                // -1 means the master slider (broadcast operation to all panes)
                if pane_index == -1 {
                    self.prev_slider_value = self.slider_value;
                    self.slider_value = value;
                    debug!("slider - update_pos");
                    update_pos(
                        &self.device, &self.queue,
                        &mut self.panes, pane_index as isize, value as usize,
                        //self.is_slider_moving
                    );
                } else {
                    let pane = &mut self.panes[pane_index as usize];
                    let _pane_index_org = pane_index.clone();
                    let pane_index = pane_index as usize;

                    debug!("pane_index {} slider value: {}", pane_index, value);
                    pane.prev_slider_value = pane.slider_value;
                    pane.slider_value = value;
                    debug!("pane_index {} prev slider value: {}", pane_index, pane.prev_slider_value);
                    debug!("pane_index {} slider value: {}", pane_index, pane.slider_value);

                    update_pos(
                        &self.device, &self.queue,
                        &mut self.panes, pane_index as isize, value as usize,
                        //self.is_slider_moving
                    );
                }
            }*/
            Message::SliderReleased(pane_index, value) => {
                debug!("SLIDER_DEBUG: SliderReleased event received");
                let slider_move_duration = self.last_slider_update.elapsed();
                debug!("SLIDER_DEBUG: Slider was moving for {:?}", slider_move_duration);
                
                self.is_slider_moving = false;
                
                // Now we can do the more intensive cache index updates
                let pos = self.slider_value as usize;
                debug!("SLIDER_DEBUG: Final slider position: {}", pos);
                
                debug!("slider released: pane_index: {}, value: {}", pane_index, value);
                if pane_index == -1 {
                    return navigation_slider::load_remaining_images(
                        &self.device, &self.queue, self.is_gpu_supported,
                        &mut self.panes, &mut self.loading_status, pane_index, value as usize);
                } else {
                    return navigation_slider::load_remaining_images(
                        &self.device, &self.queue, self.is_gpu_supported,
                        &mut self.panes, &mut self.loading_status, pane_index as isize, value as usize);
                }

            }

            Message::Event(event) => match event {
                Event::Keyboard(iced_core::keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                    debug!("KeyPressed - Key pressed: {:?}, modifiers: {:?}", key, modifiers);
                    debug!("modifiers.shift(): {}", modifiers.shift());
                    let tasks = self.handle_key_pressed_event(key, modifiers);

                    if !tasks.is_empty() {
                        return Task::batch(tasks);
                    }
                }
            
                Event::Keyboard(iced_core::keyboard::Event::KeyReleased { key, modifiers, .. }) => {
                    let tasks = self.handle_key_released_event(key, modifiers);
                    if !tasks.is_empty() {
                        return Task::batch(tasks);
                    }
                }
                
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
                Event::Window(iced::window::Event::FileDropped(dropped_path, _)) => {
                    match self.pane_layout {
                        PaneLayout::SinglePane => {
                            debug!("File dropped: {:?}", dropped_path);
                            //self.initialize_dir_path(dropped_path, 0);
                            self.initialize_dir_path(dropped_path[0].clone(), 0);
                        },
                        PaneLayout::DualPane => {}
                    }
                }

                //_ => return iced_winit::runtime::Task::none()
                _ => {}
            },
            Message::TimerTick => {
                // Implementation of TimerTick message
                // This is a placeholder and should be replaced with the actual implementation
                debug!("TimerTick received");
            }
        }

        if self.skate_right {
            self.update_counter = 0;
            let task = move_right_all(
                &self.device, &self.queue, self.cache_strategy,
                &mut self.panes,
                &mut self.loading_status,
                &mut self.slider_value,
                &self.pane_layout,
                self.is_slider_dual,
                self.last_opened_pane as usize
            );
            let update_end = Instant::now();
            let update_duration = update_end.duration_since(update_start);
            APP_UPDATE_STATS.lock().unwrap().add_measurement(update_duration);
            task
        } else if self.skate_left {
            self.update_counter = 0;
            debug!("move_left_all from self.skate_left block");
            let task = move_left_all(
                &self.device, &self.queue, self.cache_strategy,
                &mut self.panes,
                &mut self.loading_status,
                &mut self.slider_value,
                &self.pane_layout,
                self.is_slider_dual,
                self.last_opened_pane as usize
            );
            let update_end = Instant::now();
            let update_duration = update_end.duration_since(update_start);
            APP_UPDATE_STATS.lock().unwrap().add_measurement(update_duration);
            task
        } else {
            // Log that there's no task to perform once
            if self.update_counter == 0 {
                debug!("No skate mode detected, update_counter: {}", self.update_counter);
                self.update_counter += 1;
            }
            let update_end = Instant::now();
            let update_duration = update_end.duration_since(update_start);
            APP_UPDATE_STATS.lock().unwrap().add_measurement(update_duration);

            iced_winit::runtime::Task::none()
        }
    }

    fn view(&self) -> Element<Message, WinitTheme, Renderer> {
        let content = ui_builder::build_ui(&self);
        content.into()
        /*let content = ui_builder::build_ui(&self);

        let background_color = self.background_color;

        let sliders = row![
            slider(0.0..=1.0, background_color.r, move |r| {
                Message::BackgroundColorChanged(Color {
                    r,
                    ..background_color
                })
            })
            .step(0.01),
        ].width(500)
        .spacing(20);

        column![
            content,     // Your existing UI
            sliders // Single slider control
        ]
        .spacing(20)
        .into()*/


        /*let container_all = ui_builder::build_ui(&self);
        let content = container_all
            .height(Length::Fill)
            .width(Length::Fill);

        //Element::<Message, WinitTheme, Renderer>::from(content)
        content.into()*/

        /*if self.show_about {
            let about_content = container(
                column![
                    text("ViewSkater").size(25)
                    .font(Font {
                        family: iced::font::Family::Name("Roboto"),
                        weight: iced::font::Weight::Bold,
                        stretch: iced::font::Stretch::Normal,
                        style: iced::font::Style::Normal,
                    }),
                    column![
                        text("Version 0.1.2").size(15),
                        row![
                            text("Author:  ").size(15),
                            text("Gota Gando").size(15)
                            .style(|theme: &Theme| {
                                text::Style {
                                    color: Some(theme.extended_palette().primary.strong.color),
                                }
                            })
                        ],
                        text("Learn more at:").size(15),
                            button(
                                text("https://github.com/ggand0/viewskater")
                                    .size(18)
                            )
                            .style(|theme: &Theme, _status| {
                                button::Style {
                                    background: Some(iced::Color::TRANSPARENT.into()),
                                    text_color: theme.extended_palette().primary.strong.color,
                                    border: iced::Border {
                                        color: iced::Color::TRANSPARENT,
                                        width: 1.0,
                                        radius: iced::border::Radius::new(0.0),
                                    },
                                    ..Default::default()
                                }
                            })
                            .on_press(Message::OpenWebLink(
                                "https://github.com/ggand0/viewskater".to_string(),
                            )),
                    ].spacing(4)
                ]
                .spacing(15)
                .align_x(iced::Alignment::Center),
                
            )
            .padding(20)
            .style(container::rounded_box);

            widgets::modal::modal(content, about_content, Message::HideAbout)
            //widget_modal::modal(content, about_content, Message::HideAbout)
        } else {
            //content.into()
            Element::<Message, WinitTheme, Renderer>::from(content)
        }*/
        

        // ref: working debug code
        /*let shader_widget = shader(&self.panes[0].scene)
            .width(Fill).height(Fill);
        
        let other_ui = column![
            text("Custom Shader Example").color(Color::WHITE),
            text("Use 'A' and 'D' to navigate images").size(32).color(Color::WHITE),
        ]
        .width(Length::Fill)
        .height(100)
        .spacing(10)
        .padding(20)
        .align_x(Horizontal::Center);

        center(
        //container(
            column![
                shader_widget,
                other_ui
            ]
            //.align_x(Horizontal::Center)
        ).into()*/
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

/*
fn main() -> iced::Result {
    // Set up panic hook to log to a file
    let app_name = "viewskater";
    let shared_log_buffer = file_io::setup_logger(app_name);
    file_io::setup_panic_hook(app_name, shared_log_buffer);

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
*/