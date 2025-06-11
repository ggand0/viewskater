#[warn(unused_imports)]
#[cfg(target_os = "linux")]
mod other_os {
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
use std::sync::Arc;
use std::sync::Mutex;
use once_cell::sync::Lazy;

#[allow(unused_imports)]
use std::time::Instant;

#[allow(unused_imports)]
use log::{Level, debug, info, warn, error};

use iced::{
    clipboard, event::Event,
    widget::button,
    font::{self, Font},
};
use iced_core::keyboard::{self, Key, key::Named};
use iced::widget::image::Handle;
use iced_widget::{row, column, container, text};
use iced_wgpu::{wgpu, Renderer};
use iced_wgpu::engine::CompressionStrategy;
use iced_winit::core::Theme as WinitTheme;
use iced_winit::core::{Color, Element};
use iced_winit::runtime::Task;

use crate::navigation_keyboard::{move_right_all, move_left_all};
use crate::cache::img_cache::{CachedData, CacheStrategy, LoadOperation};
use crate::menu::PaneLayout;
use crate::pane::{self, Pane};
use crate::widgets::shader::{scene::Scene, cpu_scene::CpuScene};
use crate::ui;
use crate::widgets;
use crate::file_io;
use crate::loading_status;
use crate::loading_handler;
use crate::navigation_slider;
use crate::utils::timing::TimingStats;
use crate::pane::IMAGE_RENDER_TIMES;
use crate::pane::IMAGE_RENDER_FPS;
use crate::RendererRequest;
use crate::build_info::BuildInfo;

use std::sync::mpsc::{Sender, Receiver};

#[allow(dead_code)]
static APP_UPDATE_STATS: Lazy<Mutex<TimingStats>> = Lazy::new(|| {
    Mutex::new(TimingStats::new("App Update"))
});

#[allow(dead_code)]
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
    SliderImageWidgetLoaded(Result<(usize, usize, Handle), (usize, usize)>),
    Event(Event),
    ImagesLoaded(Result<(Vec<Option<CachedData>>, Option<LoadOperation>), std::io::ErrorKind>),
    OnSplitResize(u16),
    ResetSplit(u16),
    ToggleSliderType(bool),
    TogglePaneLayout(PaneLayout),
    ToggleFooter(bool),
    PaneSelected(usize, bool),
    CopyFilename(usize),
    CopyFilePath(usize),
    BackgroundColorChanged(Color),
    TimerTick,
    SetCacheStrategy(CacheStrategy),
    SetCompressionStrategy(CompressionStrategy),
    ToggleFpsDisplay(bool),
    ToggleSplitOrientation(bool),
    ToggleSyncedZoom(bool),
}

pub struct DataViewer {
    pub background_color: Color,//debug
    pub title: String,
    pub directory_path: Option<String>,
    pub current_image_index: usize,
    pub slider_value: u16,                              // for master slider
    pub prev_slider_value: u16,                         // for master slider
    pub divider_position: Option<u16>,
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
    pub is_gpu_supported: bool,
    pub cache_strategy: CacheStrategy,
    pub last_slider_update: Instant,
    pub is_slider_moving: bool,
    pub backend: wgpu::Backend,
    pub show_fps: bool,
    pub compression_strategy: CompressionStrategy,
    pub renderer_request_sender: Sender<RendererRequest>,
    pub is_horizontal_split: bool,
    pub file_receiver: Receiver<String>,
    pub synced_zoom: bool,
}

impl DataViewer {
    pub fn new(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        backend: wgpu::Backend,
        renderer_request_sender: Sender<RendererRequest>,
        file_receiver: Receiver<String>,
    ) -> Self {
        Self {
            title: String::from("ViewSkater"),
            directory_path: None,
            current_image_index: 0,
            slider_value: 0,
            prev_slider_value: 0,
            divider_position: None,
            is_slider_dual: false,
            show_footer: true,
            pane_layout: PaneLayout::SinglePane,
            last_opened_pane: -1,
            panes: vec![pane::Pane::new(Arc::clone(&device), Arc::clone(&queue), backend, 0, CompressionStrategy::Bc1)],
            loading_status: loading_status::LoadingStatus::default(),
            skate_right: false,
            skate_left: false,
            update_counter: 0,
            show_about: false,
            device,
            queue,
            is_gpu_supported: true,
            background_color: Color::WHITE,
            last_slider_update: Instant::now(),
            is_slider_moving: false,
            backend,
            cache_strategy: CacheStrategy::Gpu,
            show_fps: false,
            compression_strategy: CompressionStrategy::None,
            renderer_request_sender,
            is_horizontal_split: false,
            file_receiver,
            synced_zoom: true,
        }
    }

    pub fn clear_primitive_storage(&self) {
        if let Err(e) = self.renderer_request_sender.send(RendererRequest::ClearPrimitiveStorage) {
            error!("Failed to send ClearPrimitiveStorage request: {:?}", e);
        }
    }

    pub fn reset_state(&mut self, pane_index: isize) {
        // Reset loading status
        self.loading_status = loading_status::LoadingStatus::default();

        // Reset all panes
        if pane_index == -1 {
            for pane in &mut self.panes {
                pane.reset_state();
            }
        } else {
            self.panes[pane_index as usize].reset_state();
        }
        
        // Reset viewer state
        self.title = String::from("ViewSkater");
        self.directory_path = None;
        self.current_image_index = 0;
        self.slider_value = 0;
        self.prev_slider_value = 0;
        self.last_opened_pane = 0;
        
        self.skate_right = false;
        self.update_counter = 0;
        self.show_about = false;
        self.last_slider_update = Instant::now();
        self.is_slider_moving = false;

        crate::utils::mem::log_memory("DataViewer::reset_state: After reset_state");

        // Clear primitive storage
        self.clear_primitive_storage();
    }

    fn initialize_dir_path(&mut self, path: &PathBuf, pane_index: usize) {
        debug!("last_opened_pane: {}", self.last_opened_pane);

        // Make sure we have enough panes
        if pane_index >= self.panes.len() {
            // Create new panes with proper device and queue initialization
            while self.panes.len() <= pane_index {
                let new_pane_id = self.panes.len();
                debug!("Creating new pane at index {}", new_pane_id);
                self.panes.push(pane::Pane::new(    
                    Arc::clone(&self.device), 
                    Arc::clone(&self.queue),
                    self.backend,
                    new_pane_id, // Pass the pane_id matching its index
                    self.compression_strategy
                ));
            }
        }

        // Clear any cached slider images to prevent displaying stale images
        for pane in self.panes.iter_mut() {
            pane.slider_image = None;
            pane.slider_scene = None;
        }

        let pane_file_lengths = self.panes  .iter().map(
            |pane| pane.img_cache.image_paths.len()).collect::<Vec<usize>>();
        let pane = &mut self.panes[pane_index];
        debug!("pane_file_lengths: {:?}", pane_file_lengths);

        pane.initialize_dir_path(
            &Arc::clone(&self.device),
            &Arc::clone(&self.queue),
            self.is_gpu_supported,
            self.cache_strategy,
            self.compression_strategy,
            &self.pane_layout,
            &pane_file_lengths,
            pane_index,
            path,
            self.is_slider_dual,
            &mut self.slider_value,
        );

        self.last_opened_pane = pane_index as isize;
    }

    fn handle_key_pressed_event(&mut self, key: &keyboard::Key, modifiers: keyboard::Modifiers) -> Vec<Task<Message>> {
        let mut tasks = Vec::new();
        
        // Helper function to check for the platform-appropriate modifier key
        let is_platform_modifier = |modifiers: &keyboard::Modifiers| -> bool {
            #[cfg(target_os = "macos")]
            return modifiers.logo(); // Use Command key on macOS
            
            #[cfg(not(target_os = "macos"))]
            return modifiers.control(); // Use Control key on other platforms
        };
        
        match key.as_ref() {
            Key::Named(Named::Tab) => {
                debug!("Tab pressed");
                self.toggle_footer();
            }

            Key::Named(Named::Space) | Key::Character("b") => {
                debug!("Space pressed");
                self.toggle_slider_type();
            }

            Key::Character("h") | Key::Character("H") => {
                debug!("H key pressed");
                // Only toggle split orientation in dual pane mode
                if self.pane_layout == PaneLayout::DualPane {
                    self.toggle_split_orientation();
                }
            }

            Key::Character("1") => {
                debug!("Key1 pressed");
                if self.pane_layout == PaneLayout::DualPane && self.is_slider_dual {
                    self.panes[0].is_selected = !self.panes[0].is_selected;
                }

                // If shift+alt is pressed, load a file into pane0
                if modifiers.shift() && modifiers.alt() {
                    debug!("Key1 Shift+Alt pressed");
                    tasks.push(Task::perform(file_io::pick_file(), move |result| {
                        Message::FolderOpened(result, 0)
                    }));
                }

                // If alt is pressed, load a folder into pane0
                else if modifiers.alt() {
                    debug!("Key1 Alt pressed");
                    tasks.push(Task::perform(file_io::pick_folder(), move |result| {
                        Message::FolderOpened(result, 0)
                    }));
                }

                // If platform_modifier is pressed, switch to single pane layout
                else if is_platform_modifier(&modifiers) {
                    self.toggle_pane_layout(PaneLayout::SinglePane);
                }
            }
            Key::Character("2") => {
                debug!("Key2 pressed");
                if self.pane_layout == PaneLayout::DualPane {
                    if self.is_slider_dual {
                        self.panes[1].is_selected = !self.panes[1].is_selected;
                    }
                
                    // If shift+alt is pressed, load a file into pane1
                    if modifiers.shift() && modifiers.alt() {
                        debug!("Key2 Shift+Alt pressed");
                        tasks.push(Task::perform(file_io::pick_file(), move |result| {
                            Message::FolderOpened(result, 1)
                        }));
                    }

                    // If alt is pressed, load a folder into pane1
                    else if modifiers.alt() {
                        debug!("Key2 Alt pressed");
                        tasks.push(Task::perform(file_io::pick_folder(), move |result| {
                            Message::FolderOpened(result, 1)
                        }));
                    }
                }

                // If platform_modifier is pressed, switch to dual pane layout
                else if is_platform_modifier(&modifiers) {
                    debug!("Key2 Ctrl pressed");
                    self.toggle_pane_layout(PaneLayout::DualPane);
                }
            }

            Key::Character("c") |
            Key::Character("w") => {
                // Close the selected panes
                if is_platform_modifier(&modifiers) {
                    self.reset_state(-1);
                }
            }

            Key::Character("q") => {
                // Terminate the app
                if is_platform_modifier(&modifiers) {
                    std::process::exit(0);
                }
            }

            Key::Character("o") => {
                // If platform_modifier is pressed, open a file or folder
                if is_platform_modifier(&modifiers) {
                    let pane_index = if self.pane_layout == PaneLayout::SinglePane {
                        0 // Use first pane in single-pane mode
                    } else {
                        self.last_opened_pane as usize // Use last opened pane in dual-pane mode
                    };
                    debug!("o key pressed pane_index: {}", pane_index);

                    // If shift is pressed or we have uppercase O, open folder
                    if modifiers.shift() {
                        debug!("Opening folder with platform_modifier+shift+o");
                        tasks.push(Task::perform(file_io::pick_folder(), move |result| {
                            Message::FolderOpened(result, pane_index)
                        }));
                    } else {
                        // Otherwise open file
                        debug!("Opening file with platform_modifier+o");
                        tasks.push(Task::perform(file_io::pick_file(), move |result| {
                            Message::FolderOpened(result, pane_index)
                        }));
                    }
                }
            }

            Key::Named(Named::ArrowLeft) | Key::Character("a") => {
                // Check for first image navigation with platform modifier or Fn key
                if is_platform_modifier(&modifiers) {
                    debug!("Navigating to first image");
                    
                    // Find which panes need to be updated
                    let mut operations = Vec::new();
                    
                    for (idx, pane) in self.panes.iter_mut().enumerate() {
                        if pane.dir_loaded && (pane.is_selected || self.is_slider_dual) {
                            // Navigate to the first image (index 0)
                            if pane.img_cache.current_index > 0 {
                                let new_pos = 0;
                                pane.slider_value = new_pos as u16;
                                self.slider_value = new_pos as u16;
                                
                                // Save the operation for later execution
                                operations.push((idx as isize, new_pos));
                            }
                        }
                    }
                    
                    // Now execute all operations after the loop is complete
                    for (pane_idx, new_pos) in operations {
                        tasks.push(crate::navigation_slider::load_remaining_images(
                            &self.device,
                            &self.queue,
                            self.is_gpu_supported,
                            self.cache_strategy,
                            self.compression_strategy,
                            &mut self.panes,
                            &mut self.loading_status,
                            pane_idx,
                            new_pos,
                        ));
                    }
                    
                    return tasks;
                }
                
                // Existing left-arrow logic
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
                            &self.device,
                            &self.queue,
                            self.cache_strategy,
                            self.compression_strategy,
                            &mut self.panes,
                            &mut self.loading_status,
                            &mut self.slider_value,
                            &self.pane_layout,
                            self.is_slider_dual,
                            self.last_opened_pane as usize);
                        tasks.push(task);
                    }
                }
            }
            Key::Named(Named::ArrowRight) | Key::Character("d") => {
                // Check for last image navigation with platform modifier or Fn key
                if is_platform_modifier(&modifiers) {
                    debug!("Navigating to last image");
                    
                    // Find which panes need to be updated
                    let mut operations = Vec::new();
                    
                    for (idx, pane) in self.panes.iter_mut().enumerate() {
                        if pane.dir_loaded && (pane.is_selected || self.is_slider_dual) {
                            // Get the last valid index
                            if let Some(last_index) = pane.img_cache.image_paths.len().checked_sub(1) {
                                if pane.img_cache.current_index < last_index {
                                    let new_pos = last_index;
                                    pane.slider_value = new_pos as u16;
                                    self.slider_value = new_pos as u16;
                                    
                                    // Save the operation for later execution
                                    operations.push((idx as isize, new_pos));
                                }
                            }
                        }
                    }
                    
                    // Now execute all operations after the loop is complete
                    for (pane_idx, new_pos) in operations {
                        tasks.push(crate::navigation_slider::load_remaining_images(
                            &self.device,
                            &self.queue,
                            self.is_gpu_supported,
                            self.cache_strategy,
                            self.compression_strategy,
                            &mut self.panes,
                            &mut self.loading_status,
                            pane_idx,
                            new_pos,
                        ));
                    }
                    
                    return tasks;
                }
                
                // Existing right-arrow logic
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
                        &self.device,
                        &self.queue,
                        self.cache_strategy,
                        self.compression_strategy,
                        &mut self.panes,
                        &mut self.loading_status,
                        &mut self.slider_value,
                        &self.pane_layout,
                        self.is_slider_dual,
                        self.last_opened_pane as usize);
                    tasks.push(task);
                    debug!("handle_key_pressed_event() - tasks count: {}", tasks.len());
                }
            }

            Key::Named(Named::F3)  => {
                self.show_fps = !self.show_fps;
                debug!("Toggled debug FPS display: {}", self.show_fps);
            }

            _ => {}
        }

        tasks
    }

    fn handle_key_released_event(&mut self, key_code: &keyboard::Key, _modifiers: keyboard::Modifiers) -> Vec<Task<Message>> {
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
                Pane::resize_panes(&mut self.panes, 2);
                debug!("self.panes.len(): {}", self.panes.len());
            }
        }
        self.pane_layout = pane_layout;
    }

    fn toggle_footer(&mut self) {
        self.show_footer = !self.show_footer;
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
                // Select labels based on split orientation
                let (first_label, second_label) = if self.is_horizontal_split {
                    ("Top", "Bottom")
                } else {
                    ("Left", "Right")
                };

                let first_pane_filename = if self.panes[0].dir_loaded {
                    self.panes[0].img_cache.image_paths[self.panes[0].img_cache.current_index]
                        .file_name()
                        .map(|name| name.to_string_lossy().to_string())
                        .unwrap_or_else(|| String::from("Unknown"))
                } else {
                    String::from("No File")
                };

                let second_pane_filename = if self.panes[1].dir_loaded {
                    self.panes[1].img_cache.image_paths[self.panes[1].img_cache.current_index]
                        .file_name()
                        .map(|name| name.to_string_lossy().to_string())
                        .unwrap_or_else(|| String::from("Unknown"))
                } else {
                    String::from("No File")
                };

                format!("{}: {} | {}: {}", first_label, first_pane_filename, second_label, second_pane_filename)
            }
        }
    }


    fn update_cache_strategy(&mut self, strategy: CacheStrategy) {
        debug!("Changing cache strategy from {:?} to {:?}", self.cache_strategy, strategy);
        self.cache_strategy = strategy;
        
        // Get current pane file lengths
        let pane_file_lengths: Vec<usize> = self.panes.iter()
            .map(|p| p.img_cache.num_files)
            .collect();
        
        // Reinitialize all loaded panes with the new cache strategy
        for (i, pane) in self.panes.iter_mut().enumerate() {
            if let Some(dir_path) = &pane.directory_path.clone() {
                if pane.dir_loaded {
                    let path = PathBuf::from(dir_path);
                    
                    // Reinitialize the pane with the current directory
                    pane.initialize_dir_path(
                        &Arc::clone(&self.device),
                        &Arc::clone(&self.queue),
                        self.is_gpu_supported,
                        self.cache_strategy,
                        self.compression_strategy,
                        &self.pane_layout,
                        &pane_file_lengths,
                        i,
                        &path,
                        self.is_slider_dual,
                        &mut self.slider_value,
                    );
                }
            }
        }
    }

    fn update_compression_strategy(&mut self, strategy: CompressionStrategy) {
        if self.compression_strategy != strategy {
            self.compression_strategy = strategy;
            
            debug!("Queuing compression strategy change to {:?}", strategy);
            
            // Instead of trying to lock renderer directly, send a request to the main thread
            if let Err(e) = self.renderer_request_sender.send(
                RendererRequest::UpdateCompressionStrategy(strategy)
            ) {
                error!("Failed to queue compression strategy change: {:?}", e);
            } else {
                debug!("Compression strategy change request sent successfully");

                // Get current pane file lengths
                let pane_file_lengths: Vec<usize> = self.panes.iter()
                .map(|p| p.img_cache.num_files)
                .collect();

                // Recreate image cache
                for (i, pane) in self.panes.iter_mut().enumerate() {
                    if let Some(dir_path) = &pane.directory_path.clone() {
                        if pane.dir_loaded {
                            let path = PathBuf::from(dir_path);
                            
                            // Reinitialize the pane with the current directory
                            pane.initialize_dir_path(
                                &Arc::clone(&self.device),
                                &Arc::clone(&self.queue),
                                self.is_gpu_supported,
                                self.cache_strategy,
                                self.compression_strategy,
                                &self.pane_layout,
                                &pane_file_lengths,
                                i,
                                &path,
                                self.is_slider_dual,
                                &mut self.slider_value,
                                
                            );
                        }
                    }
                }
            }
        }
    }

    fn toggle_split_orientation(&mut self) {
        self.is_horizontal_split = !self.is_horizontal_split;
    }
}


impl iced_winit::runtime::Program for DataViewer {
    type Theme = WinitTheme;
    type Message = Message;
    type Renderer = Renderer;

    fn update(&mut self, message: Message) -> iced_winit::runtime::Task<Message> {
        // Check for any file paths received from the background thread
        while let Ok(path) = self.file_receiver.try_recv() {
            println!("Processing file path in main thread: {}", path);
            // Reset state and initialize the directory path
            self.reset_state(-1);
            println!("State reset complete, initializing directory path");
            self.initialize_dir_path(&PathBuf::from(path), 0);
            println!("Directory path initialization complete");
        }

        let _update_start = Instant::now();
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
                
                // Schedule a follow-up redraw in the next frame
                return Task::perform(async {
                    // Small delay to ensure state has been updated
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }, |_| Message::Nothing);
            },
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
                // Reset state first
                debug!("Message::FileDropped - Resetting state");
                self.reset_state(pane_index);

                // Loads the dropped file/directory
                debug!("File dropped: {:?}, pane_index: {}", dropped_path, pane_index);
                debug!("self.dir_loaded, pane_index, last_opened_pane: {:?}, {}, {}", self.panes[pane_index as usize].dir_loaded, pane_index, self.last_opened_pane);
                self.initialize_dir_path(&PathBuf::from(dropped_path), pane_index as usize);
            }
            Message::Close => {
                self.reset_state(-1);
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
                        // Only allow opening in pane_index > 0 if we're in dual pane mode
                        if pane_index > 0 && self.pane_layout == PaneLayout::SinglePane {
                            debug!("Ignoring request to open folder in pane {} while in single-pane mode", pane_index);
                        } else {
                            self.initialize_dir_path(&PathBuf::from(dir), pane_index);
                        }
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
                if let Some(filename) = img_path {
                    if let Some(filename) = file_io::get_filename(&filename) {
                        debug!("Filename: {}", filename);
                        return clipboard::write::<Message>(filename);
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
            Message::OnSplitResize(position) => { self.divider_position = Some(position); },
            Message::ResetSplit(_position) => { self.divider_position = None; },
            Message::ToggleSliderType(_bool) => { self.toggle_slider_type(); },
            Message::TogglePaneLayout(pane_layout) => { self.toggle_pane_layout(pane_layout); },
            Message::ToggleFooter(_bool) => { self.toggle_footer(); },
            Message::ToggleSyncedZoom(enabled) => {
                self.synced_zoom = enabled;
            }
            Message::PaneSelected(pane_index, is_selected) => {
                self.panes[pane_index].is_selected = is_selected;
                for (index, pane) in self.panes.iter_mut().enumerate() {
                    debug!("pane_index: {}, is_selected: {}", index, pane.is_selected);
                }
            }

            Message::ImagesLoaded(result) => {
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
                                    
                                    loading_handler::handle_load_operation_all(
                                        &mut self.panes,
                                        &mut self.loading_status,
                                        pane_indices,
                                        target_indices,
                                        &image_data,  // Now using Vec<Option<CachedData>>
                                        &cloned_op,
                                        operation_type,
                                    );
                                }
                                LoadOperation::LoadPos((pane_index, target_indices_and_cache)) => {
                                    loading_handler::handle_load_pos_operation(
                                        &mut self.panes,
                                        &mut self.loading_status,
                                        pane_index,
                                        &target_indices_and_cache,
                                        &image_data,
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

            Message::SliderImageWidgetLoaded(result) => {
                match result {
                    Ok((pane_idx, pos, handle)) => {
                        // Track each async image delivery
                        crate::track_async_delivery();

                        // Use the specified pane index instead of hardcoded 0
                        if let Some(pane) = self.panes.get_mut(pane_idx) {
                            // Update the image widget handle directly
                            pane.slider_image = Some(handle);
                            
                            // Also update the cache state to keep everything in sync
                            pane.img_cache.current_index = pos;
                            
                            debug!("Slider image loaded for pane {} at position {}", pane_idx, pos);
                        } else {
                            warn!("SliderImageWidgetLoaded: Invalid pane index {}", pane_idx);
                        }
                    },
                    Err((pane_idx, pos)) => {
                        warn!("SLIDER: Failed to load image widget for pane {} at position {}", pane_idx, pos);
                    }
                }
            },

            Message::SliderImageLoaded(result) => {
                match result {
                    Ok((_pos, cached_data)) => {
                        let pane = &mut self.panes[0]; // For single-pane slider
                        
                        // Update the scene based on data type
                        if let CachedData::Cpu(bytes) = &cached_data {
                            debug!("SliderImageLoaded: loaded data: {:?}", bytes.len());

                            // Create or update the slider scene
                            pane.current_image = CachedData::Cpu(bytes.clone());
                            pane.slider_scene = Some(Scene::CpuScene(CpuScene::new(
                                bytes.clone(), true)));

                            // Ensure texture is created for CPU images
                            if let Some(device) = &pane.device {
                                if let Some(queue) = &pane.queue {
                                    if let Some(scene) = &mut pane.slider_scene {
                                        scene.ensure_texture(&device, &queue, pane.pane_id);
                                    }
                                }
                            }
                        }
                    },
                    Err(pos) => {
                        warn!("SLIDER: Failed to load image for position {}", pos);
                    }
                }
            }
            
            
            Message::SliderChanged(pane_index, value) => {
                self.is_slider_moving = true;
                self.last_slider_update = Instant::now();

                // Always use async on Linux for better responsiveness
                let use_async = true;

                // Use throttle for Linux
                #[cfg(target_os = "linux")]
                let use_throttle = true;

                #[cfg(not(target_os = "linux"))]
                let use_throttle = false;

                
                if pane_index == -1 {
                    // Master slider - only relevant when is_slider_dual is false
                    self.prev_slider_value = self.slider_value;
                    self.slider_value = value;
                    
                    // Clear any stale slider image if this is the first slider movement after loading a new directory
                    if self.panes[0].slider_image.is_none() {
                        for pane in self.panes.iter_mut() {
                            pane.slider_scene = None;
                        }
                    }
                } else {
                    let pane_index_usize = pane_index as usize;
                    
                    // In dual slider mode, clear the slider image for the other pane
                    // to ensure it keeps showing its normal scene
                    if self.is_slider_dual && self.pane_layout == PaneLayout::DualPane {
                        // Clear slider images for all panes except the active one
                        for idx in 0..self.panes.len() {
                            if idx != pane_index_usize {
                                self.panes[idx].slider_image = None;
                            }
                        }
                    }
                    
                    // Now update the slider value for the active pane
                    let pane = &mut self.panes[pane_index_usize];
                    pane.prev_slider_value = pane.slider_value;
                    pane.slider_value = value;
                    
                    // Clear any stale slider image if this is the first slider movement after loading a new directory
                    if pane.slider_image.is_none() {
                        pane.slider_scene = None;
                    }
                }
                
                return navigation_slider::update_pos(
                    &mut self.panes, 
                    pane_index, 
                    value as usize, 
                    use_async,
                    use_throttle,
                );
            }
            
            Message::SliderReleased(pane_index, value) => {
                debug!("SLIDER_DEBUG: SliderReleased event received");
                self.is_slider_moving = false;
                
                // Get the final image FPS AND the timestamps
                let final_image_fps = iced_wgpu::get_image_fps();
                let upload_timestamps = iced_wgpu::get_image_upload_timestamps();
                
                // Sync our application's image render times with the ones from iced_wgpu
                if !upload_timestamps.is_empty() {
                    if let Ok(mut render_times) = IMAGE_RENDER_TIMES.lock() {
                        // Convert VecDeque to Vec for our storage
                        *render_times = upload_timestamps.into_iter().collect();
                        
                        // Update FPS based on the final calculated value
                        if let Ok(mut fps) = IMAGE_RENDER_FPS.lock() {
                            *fps = final_image_fps as f32;
                            debug!("SLIDER_DEBUG: Synced image fps tracking, final FPS: {:.1}", final_image_fps);
                        }
                    }
                }
                
                // Continue with loading remaining images
                if pane_index == -1 {
                    return navigation_slider::load_remaining_images(
                        &self.device,
                        &self.queue,
                        self.is_gpu_supported,
                        self.cache_strategy,
                        self.compression_strategy,
                        &mut self.panes,
                        &mut self.loading_status,
                        pane_index,
                        value as usize);
                } else {
                    return navigation_slider::load_remaining_images(
                        &self.device,
                        &self.queue,
                        self.is_gpu_supported,
                        self.cache_strategy,
                        self.compression_strategy,
                        &mut self.panes,
                        &mut self.loading_status,
                        pane_index as isize,
                        value as usize);
                }
            }

            Message::Event(event) => match event {
                Event::Keyboard(iced_core::keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                    debug!("KeyPressed - Key pressed: {:?}, modifiers: {:?}", key, modifiers);
                    debug!("modifiers.shift(): {}", modifiers.shift());
                    let tasks = self.handle_key_pressed_event(&key, modifiers);

                    if !tasks.is_empty() {
                        return Task::batch(tasks);
                    }
                }
            
                Event::Keyboard(iced_core::keyboard::Event::KeyReleased { key, modifiers, .. }) => {
                    let tasks = self.handle_key_released_event(&key, modifiers);
                    if !tasks.is_empty() {
                        return Task::batch(tasks);
                    }
                }
                
                // Only using for single pane layout
                #[cfg(any(target_os = "macos", target_os = "windows"))]
                Event::Window(iced::window::Event::FileDropped(dropped_paths, _position)) => {
                    match self.pane_layout {
                        PaneLayout::SinglePane => {
                            // Reset state first
                            self.reset_state(-1);

                            debug!("File dropped: {:?}", dropped_paths);
                            self.initialize_dir_path(&dropped_paths[0].clone(), 0);
                        },
                        PaneLayout::DualPane => {
                        }
                    }
                }
                #[cfg(target_os = "linux")]
                Event::Window(iced::window::Event::FileDropped(dropped_path, _)) => {
                    match self.pane_layout {
                        PaneLayout::SinglePane => {
                            // Reset state first
                            debug!("window::Event::FileDropped - Resetting state");
                            self.reset_state(-1);

                            debug!("File dropped: {:?}", dropped_path);
                            self.initialize_dir_path(&dropped_path[0], 0);
                        },
                        PaneLayout::DualPane => {}
                    }
                }

                _ => {}
            },
            Message::TimerTick => {
                // Implementation of TimerTick message
                // This is a placeholder and should be replaced with the actual implementation
                debug!("TimerTick received");
            }
            Message::SetCacheStrategy(strategy) => {
                self.update_cache_strategy(strategy);
            }
            Message::SetCompressionStrategy(strategy) => {
                self.update_compression_strategy(strategy);
            }
            Message::ToggleFpsDisplay(value) => {
                self.show_fps = value;
            }
            Message::ToggleSplitOrientation(_bool) => { self.toggle_split_orientation(); },
        }

        if self.skate_right {
            self.update_counter = 0;
            let task = move_right_all(
                &self.device,
                &self.queue,
                self.cache_strategy,
                self.compression_strategy,
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
            debug!("move_left_all from self.skate_left block");
            let task = move_left_all(
                &self.device,
                &self.queue,
                self.cache_strategy,
                self.compression_strategy,
                &mut self.panes,
                &mut self.loading_status,
                &mut self.slider_value,
                &self.pane_layout,
                self.is_slider_dual,
                self.last_opened_pane as usize
            );
            task
        } else {
            // Log that there's no task to perform once
            if self.update_counter == 0 {
                debug!("No skate mode detected, update_counter: {}", self.update_counter);
                self.update_counter += 1;
            }

            iced_winit::runtime::Task::none()
        }
    }

    fn view(&self) -> Element<Message, WinitTheme, Renderer> {
        let content = ui::build_ui(&self);

        if self.show_about {
            // Build the info column dynamically to avoid empty text widgets
            let mut info_column = column![
                text(format!("Version {}", BuildInfo::display_version())).size(15),
                text(format!("Build: {} ({})", BuildInfo::build_string(), BuildInfo::build_profile())).size(12)
                .style(|theme: &WinitTheme| {
                    iced_widget::text::Style {
                        color: Some(theme.extended_palette().background.weak.color),
                        ..Default::default()
                    }
                }),
                text(format!("Commit: {}", BuildInfo::git_hash_short())).size(12)
                .style(|theme: &WinitTheme| {
                    iced_widget::text::Style {
                        color: Some(theme.extended_palette().background.weak.color),
                        ..Default::default()
                    }
                }),
                text(format!("Platform: {}", BuildInfo::target_platform())).size(12)
                .style(|theme: &WinitTheme| {
                    iced_widget::text::Style {
                        color: Some(theme.extended_palette().background.weak.color),
                        ..Default::default()
                    }
                }),
            ];

            // Add bundle version only on macOS to avoid empty widgets
            let bundle_info = BuildInfo::bundle_version_display();
            if !bundle_info.is_empty() {
                info_column = info_column.push(
                    text(format!("Bundle: {}", bundle_info)).size(12)
                    .style(|theme: &WinitTheme| {
                        iced_widget::text::Style {
                            color: Some(theme.extended_palette().background.weak.color),
                            ..Default::default()
                        }
                    })
                );
            }

            info_column = info_column.push(row![
                text("Author:  ").size(15),
                text("Gota Gando").size(15)
                .style(|theme: &WinitTheme| {
                    iced_widget::text::Style {
                        color: Some(theme.extended_palette().primary.strong.color),
                        ..Default::default()
                    }
                })
            ]);

            info_column = info_column.push(text("Learn more at:").size(15));

            info_column = info_column.push(button(
                text("https://github.com/ggand0/viewskater")
                    .size(18)
            )
            .style(|theme: &WinitTheme, _status| {
                iced_widget::button::Style {
                    background: Some(iced_winit::core::Color::TRANSPARENT.into()),
                    text_color: theme.extended_palette().primary.strong.color,
                    border: iced_winit::core::Border {
                        color: iced_winit::core::Color::TRANSPARENT,
                        width: 1.0,
                        radius: iced_winit::core::border::Radius::new(0.0),
                    },
                    ..Default::default()
                }
            })
            .on_press(Message::OpenWebLink(
                "https://github.com/ggand0/viewskater".to_string(),
            )));

            info_column = info_column.spacing(4);

            let about_content = container(
                column![
                    text("ViewSkater").size(25)
                    .font(Font {
                        family: iced_winit::core::font::Family::Name("Roboto"),
                        weight: iced_winit::core::font::Weight::Bold,
                        stretch: iced_winit::core::font::Stretch::Normal,
                        style: iced_winit::core::font::Style::Normal,
                    }),
                    info_column
                ]
                .spacing(15)
                .align_x(iced_winit::core::alignment::Horizontal::Center),
                
            )
            .padding(20)
            .style(|theme: &WinitTheme| {
                iced_widget::container::Style {
                    background: Some(theme.extended_palette().background.base.color.into()),
                    text_color: Some(theme.extended_palette().primary.weak.text),
                    border: iced_winit::core::Border {
                        color: theme.extended_palette().background.strong.color,
                        width: 1.0,
                        radius: iced_winit::core::border::Radius::from(8.0),
                    },
                    ..Default::default()
                }
            });

            widgets::modal::modal(content, about_content, Message::HideAbout)
        } else {
            content.into()
        }
    }
}
