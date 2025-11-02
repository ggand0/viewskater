// Submodules
mod message_handlers;

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
    event::Event,
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
#[cfg(feature = "ml")]
use crate::selection_manager::SelectionManager;
use crate::settings::UserSettings;

use std::sync::mpsc::{Sender, Receiver};
use std::collections::HashMap;

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
    ShowOptions,
    HideOptions,
    SaveSettings,
    ClearSettingsStatus,
    SettingsTabSelected(usize),
    ShowLogs,
    OpenSettingsDir,
    ExportDebugLogs,
    ExportAllLogs,
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
    SliderImageWidgetLoaded(Result<(usize, usize, Handle, (u32, u32)), (usize, usize)>),
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
    ToggleMouseWheelZoom(bool),
    ToggleCopyButtons(bool),
    ToggleFullScreen(bool),
    CursorOnTop(bool),
    CursorOnMenu(bool),
    CursorOnFooter(bool),
    #[cfg(feature = "ml")]
    MlAction(crate::ml_widget::MlMessage),
    #[cfg(feature = "coco")]
    CocoAction(crate::coco::widget::CocoMessage),
    // Advanced settings input
    AdvancedSettingChanged(String, String),  // (field_name, value)
    ResetAdvancedSettings,
}

/// Runtime-configurable settings that can be applied immediately without restart
pub struct RuntimeSettings {
    pub mouse_wheel_zoom: bool,                         // Flag to change mouse scroll wheel behavior
    pub show_copy_buttons: bool,                        // Show copy filename/filepath buttons in footer
    pub cache_size: usize,                              // Image cache window size (number of images to cache)
    pub archive_cache_size: u64,                        // Archive cache size in bytes (for preload decision)
    pub archive_warning_threshold_mb: u64,              // Warning threshold for large solid archives (MB)
    pub max_loading_queue_size: usize,                  // Max size for loading queue
    pub max_being_loaded_queue_size: usize,             // Max size for being loaded queue
    pub double_click_threshold_ms: u16,                 // Double-click threshold in milliseconds
}

impl RuntimeSettings {
    fn from_user_settings(settings: &UserSettings) -> Self {
        Self {
            mouse_wheel_zoom: settings.mouse_wheel_zoom,
            show_copy_buttons: settings.show_copy_buttons,
            cache_size: settings.cache_size,
            archive_cache_size: settings.archive_cache_size * 1_048_576,  // Convert MB to bytes
            archive_warning_threshold_mb: settings.archive_warning_threshold_mb,
            max_loading_queue_size: settings.max_loading_queue_size,
            max_being_loaded_queue_size: settings.max_being_loaded_queue_size,
            double_click_threshold_ms: settings.double_click_threshold_ms,
        }
    }
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
    pub show_options: bool,
    pub settings_save_status: Option<String>,
    pub active_settings_tab: usize,
    pub advanced_settings_input: HashMap<String, String>,  // Text input state for advanced settings
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
    pub is_fullscreen: bool,
    pub cursor_on_top: bool,
    pub cursor_on_menu: bool,                           // Flag to show menu when fullscreen
    pub cursor_on_footer: bool,                         // Flag to show footer when fullscreen
    pub runtime_settings: RuntimeSettings,              // Runtime-configurable settings
    pub(crate) ctrl_pressed: bool,                                 // Flag to save ctrl/cmd(macOS) press state
    #[cfg(feature = "ml")]
    pub selection_manager: SelectionManager,            // Manages image selections/exclusions
    #[cfg(feature = "coco")]
    pub annotation_manager: crate::coco::annotation_manager::AnnotationManager,  // Manages COCO annotations
}

// Implement Deref to expose RuntimeSettings fields directly on DataViewer
impl std::ops::Deref for DataViewer {
    type Target = RuntimeSettings;

    fn deref(&self) -> &Self::Target {
        &self.runtime_settings
    }
}

// Implement DerefMut to allow mutable access to RuntimeSettings fields
impl std::ops::DerefMut for DataViewer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.runtime_settings
    }
}

impl DataViewer {
    pub fn new(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        backend: wgpu::Backend,
        renderer_request_sender: Sender<RendererRequest>,
        file_receiver: Receiver<String>,
        settings_path: Option<&str>,
    ) -> Self {
        // Load user settings from YAML file
        let settings = UserSettings::load(settings_path);
        let cache_strategy = settings.get_cache_strategy();
        let compression_strategy = settings.get_compression_strategy();

        info!("Initializing DataViewer with settings:");
        info!("  show_fps: {}", settings.show_fps);
        info!("  show_footer: {}", settings.show_footer);
        info!("  is_horizontal_split: {}", settings.is_horizontal_split);
        info!("  synced_zoom: {}", settings.synced_zoom);
        info!("  mouse_wheel_zoom: {}", settings.mouse_wheel_zoom);
        info!("  show_copy_buttons: {}", settings.show_copy_buttons);
        info!("  cache_strategy: {:?}", cache_strategy);
        info!("  compression_strategy: {:?}", compression_strategy);
        info!("  is_slider_dual: {}", settings.is_slider_dual);

        // Initialize advanced settings input with current values
        let mut advanced_settings_input = HashMap::new();
        advanced_settings_input.insert("cache_size".to_string(), settings.cache_size.to_string());
        advanced_settings_input.insert("max_loading_queue_size".to_string(), settings.max_loading_queue_size.to_string());
        advanced_settings_input.insert("max_being_loaded_queue_size".to_string(), settings.max_being_loaded_queue_size.to_string());
        advanced_settings_input.insert("window_width".to_string(), settings.window_width.to_string());
        advanced_settings_input.insert("window_height".to_string(), settings.window_height.to_string());
        advanced_settings_input.insert("atlas_size".to_string(), settings.atlas_size.to_string());
        advanced_settings_input.insert("double_click_threshold_ms".to_string(), settings.double_click_threshold_ms.to_string());
        advanced_settings_input.insert("archive_cache_size".to_string(), settings.archive_cache_size.to_string());
        advanced_settings_input.insert("archive_warning_threshold_mb".to_string(), settings.archive_warning_threshold_mb.to_string());

        Self {
            title: String::from("ViewSkater"),
            directory_path: None,
            current_image_index: 0,
            slider_value: 0,
            prev_slider_value: 0,
            divider_position: None,
            is_slider_dual: settings.is_slider_dual,
            show_footer: settings.show_footer,
            pane_layout: PaneLayout::SinglePane,
            last_opened_pane: -1,
            panes: vec![pane::Pane::new(Arc::clone(&device), Arc::clone(&queue), backend, 0, compression_strategy)],
            loading_status: loading_status::LoadingStatus::default(),
            skate_right: false,
            skate_left: false,
            update_counter: 0,
            show_about: false,
            show_options: false,
            settings_save_status: None,
            active_settings_tab: 0,
            advanced_settings_input,
            device,
            queue,
            is_gpu_supported: true,
            background_color: Color::WHITE,
            last_slider_update: Instant::now(),
            is_slider_moving: false,
            backend,
            cache_strategy,
            show_fps: settings.show_fps,
            compression_strategy,
            renderer_request_sender,
            is_horizontal_split: settings.is_horizontal_split,
            file_receiver,
            synced_zoom: settings.synced_zoom,
            is_fullscreen: false,
            cursor_on_top: false,
            cursor_on_menu: false,
            cursor_on_footer: false,
            runtime_settings: RuntimeSettings::from_user_settings(&settings),
            ctrl_pressed: false,
            #[cfg(feature = "ml")]
            selection_manager: SelectionManager::new(),
            #[cfg(feature = "coco")]
            annotation_manager: crate::coco::annotation_manager::AnnotationManager::new(),
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

    pub(crate) fn initialize_dir_path(&mut self, path: &PathBuf, pane_index: usize) {
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

        // Capture runtime settings before mutable borrow
        let cache_size = self.cache_size;
        let archive_cache_size = self.archive_cache_size;
        let archive_warning_threshold_mb = self.archive_warning_threshold_mb;

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
            cache_size,
            archive_cache_size,
            archive_warning_threshold_mb,
        );

        self.last_opened_pane = pane_index as isize;

        // Load selection state for the directory (ML tools only)
        #[cfg(feature = "ml")]
        if let Some(dir_path) = &pane.directory_path {
            if let Err(e) = self.selection_manager.load_for_directory(dir_path) {
                warn!("Failed to load selection state for {}: {}", dir_path, e);
            }
        }
    }

    fn set_ctrl_pressed(&mut self, enabled: bool) {
        self.ctrl_pressed = enabled;
        for pane in self.panes.iter_mut() {
            pane.ctrl_pressed = enabled;
        }
    }

    pub(crate) fn handle_key_pressed_event(&mut self, key: &keyboard::Key, modifiers: keyboard::Modifiers) -> Vec<Task<Message>> {
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
                } else if modifiers.shift() {
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

            Key::Named(Named::Super) => {
                #[cfg(target_os = "macos")] {
                    self.set_ctrl_pressed(true);
                }
            }
            Key::Named(Named::Control) => {
                #[cfg(not(target_os = "macos"))] {
                    self.set_ctrl_pressed(true);
                }
            }
            _ => {
                // Check if ML module wants to handle this key
                #[cfg(feature = "ml")]
                if let Some(task) = crate::ml_widget::handle_keyboard_event(
                    key,
                    modifiers,
                    &self.pane_layout,
                    self.last_opened_pane,
                ) {
                    tasks.push(task);
                }

                // Check if COCO module wants to handle this key
                #[cfg(feature = "coco")]
                if let Some(task) = crate::coco::widget::handle_keyboard_event(
                    key,
                    modifiers,
                    &self.pane_layout,
                    self.last_opened_pane,
                ) {
                    tasks.push(task);
                }
            }
        }

        tasks
    }

    pub(crate) fn handle_key_released_event(&mut self, key_code: &keyboard::Key, _modifiers: keyboard::Modifiers) -> Vec<Task<Message>> {
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
            Key::Named(Named::Super) => {
                #[cfg(target_os = "macos")] {
                    self.set_ctrl_pressed(false);
                }
            }
            Key::Named(Named::Control) => {
                #[cfg(not(target_os = "macos"))] {
                    self.set_ctrl_pressed(false);
                }
            }
            _ => {},
        }

        tasks
    }

    pub(crate) fn toggle_slider_type(&mut self) {
        // When toggling from dual to single, reset pane.is_selected to true
        if self.is_slider_dual {
            for pane in self.panes.iter_mut() {
                pane.is_selected_cache = pane.is_selected;
                pane.is_selected = true;
                pane.is_next_image_loaded = false;
                pane.is_prev_image_loaded = false;
            }

            let panes_refs: Vec<&mut pane::Pane> = self.panes.iter_mut().collect();
            self.slider_value = pane::get_master_slider_value(&panes_refs, &self.pane_layout, self.is_slider_dual, self.last_opened_pane as usize) as u16;
        } else {
            // Single to dual slider: give slider.value to each slider
            for pane in self.panes.iter_mut() {
                pane.slider_value = pane.img_cache.current_index as u16;
                pane.is_selected = pane.is_selected_cache;
            }
        }
        self.is_slider_dual = !self.is_slider_dual;
    }

    pub(crate) fn toggle_pane_layout(&mut self, pane_layout: PaneLayout) {
        match pane_layout {
            PaneLayout::SinglePane => {
                Pane::resize_panes(&mut self.panes, 1);

                debug!("self.panes.len(): {}", self.panes.len());

                if self.pane_layout == PaneLayout::DualPane {
                    // Reset the slider value to the first pane's current index
                    let panes_refs: Vec<&mut pane::Pane> = self.panes.iter_mut().collect();
                    self.slider_value = pane::get_master_slider_value(&panes_refs, &pane_layout, self.is_slider_dual, self.last_opened_pane as usize) as u16;
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

    pub(crate) fn toggle_footer(&mut self) {
        self.show_footer = !self.show_footer;
    }

    pub fn title(&self) -> String {
        match self.pane_layout  {
            PaneLayout::SinglePane => {
                if self.panes[0].dir_loaded {
                    let path = &self.panes[0].img_cache.image_paths[self.panes[0].img_cache.current_index];
                    path.file_name().to_string()
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
                    let path = &self.panes[0].img_cache.image_paths[self.panes[0].img_cache.current_index];
                    path.file_name().to_string()
                } else {
                    String::from("No File")
                };

                let second_pane_filename = if self.panes[1].dir_loaded {
                    let path = &self.panes[1].img_cache.image_paths[self.panes[1].img_cache.current_index];
                    path.file_name().to_string()
                } else {
                    String::from("No File")
                };

                format!("{}: {} | {}: {}", first_label, first_pane_filename, second_label, second_pane_filename)
            }
        }
    }


    pub(crate) fn update_cache_strategy(&mut self, strategy: CacheStrategy) {
        debug!("Changing cache strategy from {:?} to {:?}", self.cache_strategy, strategy);
        self.cache_strategy = strategy;

        // Get current pane file lengths
        let pane_file_lengths: Vec<usize> = self.panes.iter()
            .map(|p| p.img_cache.num_files)
            .collect();

        // Capture runtime settings before mutable borrow
        let cache_size = self.cache_size;
        let archive_cache_size = self.archive_cache_size;
        let archive_warning_threshold_mb = self.archive_warning_threshold_mb;

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
                        cache_size,
                        archive_cache_size,
                        archive_warning_threshold_mb,
                    );
                }
            }
        }
    }

    pub(crate) fn update_compression_strategy(&mut self, strategy: CompressionStrategy) {
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

                // Capture runtime settings before mutable borrow
                let cache_size = self.cache_size;
                let archive_cache_size = self.archive_cache_size;
                let archive_warning_threshold_mb = self.archive_warning_threshold_mb;

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
                                cache_size,
                                archive_cache_size,
                                archive_warning_threshold_mb,
                            );
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn toggle_split_orientation(&mut self) {
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

        // Route messages to appropriate handler functions
        let task = match message {
            // Misc messages (simple ones handled inline)
            Message::Nothing => Task::none(),
            Message::Debug(s) => {
                self.title = s;
                Task::none()
            }
            Message::BackgroundColorChanged(color) => {
                self.background_color = color;
                Task::none()
            }
            Message::FontLoaded(_) => Task::none(),
            Message::TimerTick => {
                debug!("TimerTick received");
                Task::none()
            }
            Message::Quit => {
                std::process::exit(0);
            }

            // UI state messages (About, Options, Logs)
            Message::ShowLogs | Message::OpenSettingsDir | Message::ExportDebugLogs |
            Message::ExportAllLogs | Message::ShowAbout | Message::HideAbout |
            Message::ShowOptions | Message::HideOptions | Message::OpenWebLink(_) => {
                crate::app::message_handlers::handle_ui_messages(self, message)
            }

            // Settings messages
            Message::SaveSettings | Message::ClearSettingsStatus | Message::SettingsTabSelected(_) |
            Message::AdvancedSettingChanged(_, _) | Message::ResetAdvancedSettings => {
                crate::app::message_handlers::handle_settings_messages(self, message)
            }

            // File operation messages
            Message::OpenFolder(_) | Message::OpenFile(_) | Message::FileDropped(_, _) |
            Message::Close | Message::FolderOpened(_, _) | Message::CopyFilename(_) | Message::CopyFilePath(_) => {
                crate::app::message_handlers::handle_file_messages(self, message)
            }

            // Image loading messages
            Message::ImagesLoaded(_) | Message::SliderImageWidgetLoaded(_) | Message::SliderImageLoaded(_) => {
                crate::app::message_handlers::handle_image_loading_messages(self, message)
            }

            // Slider and navigation messages
            Message::SliderChanged(_, _) | Message::SliderReleased(_, _) => {
                crate::app::message_handlers::handle_slider_messages(self, message)
            }

            // Toggle and UI control messages
            Message::OnSplitResize(_) | Message::ResetSplit(_) | Message::ToggleSliderType(_) |
            Message::TogglePaneLayout(_) | Message::ToggleFooter(_) | Message::ToggleSyncedZoom(_) |
            Message::ToggleMouseWheelZoom(_) | Message::ToggleCopyButtons(_) | Message::ToggleFullScreen(_) |
            Message::ToggleFpsDisplay(_) | Message::ToggleSplitOrientation(_) |
            Message::CursorOnTop(_) | Message::CursorOnMenu(_) | Message::CursorOnFooter(_) |
            Message::PaneSelected(_, _) | Message::SetCacheStrategy(_) | Message::SetCompressionStrategy(_) => {
                crate::app::message_handlers::handle_toggle_messages(self, message)
            }

            // Event messages (mouse, keyboard, file drops)
            Message::Event(event) => {
                crate::app::message_handlers::handle_event_messages(self, event)
            }

            // Feature-specific messages
            #[cfg(feature = "ml")]
            Message::MlAction(ml_msg) => {
                return crate::ml_widget::handle_ml_message(
                    ml_msg,
                    &self.panes,
                    &mut self.selection_manager,
                );
            }

            #[cfg(feature = "coco")]
            Message::CocoAction(coco_msg) => {
                return crate::coco::widget::handle_coco_message(
                    coco_msg,
                    &mut self.panes,
                    &mut self.annotation_manager,
                );
            }
        };

        // Return the task if it's not skate mode
        // Skate mode overrides normal task handling for continuous navigation
        if self.skate_right {
            self.update_counter = 0;
            move_right_all(
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
            )
        } else if self.skate_left {
            self.update_counter = 0;
            debug!("move_left_all from self.skate_left block");
            move_left_all(
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
            )
        } else if self.skate_left {
            self.update_counter = 0;
            debug!("move_left_all from self.skate_left block");
            move_left_all(
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
            )
        } else {
            // No skate mode, return the task from message handler
            if self.update_counter == 0 {
                debug!("No skate mode detected, update_counter: {}", self.update_counter);
                self.update_counter += 1;
            }
            task
        }
    }


    fn view(&self) -> Element<'_, Message, WinitTheme, Renderer> {
        let content = ui::build_ui(self);

        if self.show_options {
            let options_content = crate::settings_modal::view_settings_modal(self);
            widgets::modal::modal(content, options_content, Message::HideOptions)
        } else if self.show_about {
            // Build the info column dynamically to avoid empty text widgets
            let mut info_column = column![
                text(format!("Version {}", BuildInfo::display_version())).size(15),
                text(format!("Build: {} ({})", BuildInfo::build_string(), BuildInfo::build_profile())).size(12)
                .style(|theme: &WinitTheme| {
                    iced_widget::text::Style {
                        color: Some(theme.extended_palette().background.weak.color)
                    }
                }),
                text(format!("Commit: {}", BuildInfo::git_hash_short())).size(12)
                .style(|theme: &WinitTheme| {
                    iced_widget::text::Style {
                        color: Some(theme.extended_palette().background.weak.color)
                    }
                }),
                text(format!("Platform: {}", BuildInfo::target_platform())).size(12)
                .style(|theme: &WinitTheme| {
                    iced_widget::text::Style {
                        color: Some(theme.extended_palette().background.weak.color),
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
