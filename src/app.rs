// Submodules
mod message;
mod message_handlers;
mod keyboard_handlers;
mod settings_widget;

// Re-exports
pub use message::Message;
pub use settings_widget::{RuntimeSettings, SettingsWidget};

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
    widget::button,
    font::Font,
    Task,
};
use iced_widget::{row, column, container, text};
use iced_wgpu::{wgpu, Renderer};
use iced_wgpu::engine::CompressionStrategy;
use iced_winit::core::Theme as WinitTheme;
use iced_winit::core::{Color, Element};


use crate::navigation_keyboard::{move_right_all, move_left_all};
use crate::cache::img_cache::CacheStrategy;
use crate::menu::PaneLayout;
use crate::pane::{self, Pane};
use crate::ui;
use crate::widgets;
use crate::loading_status;
use crate::utils::timing::TimingStats;
use crate::RendererRequest;
use crate::build_info::BuildInfo;
#[cfg(feature = "selection")]
use crate::selection_manager::SelectionManager;
use crate::settings::UserSettings;

use std::sync::mpsc::{Sender, Receiver};

#[allow(dead_code)]
static APP_UPDATE_STATS: Lazy<Mutex<TimingStats>> = Lazy::new(|| {
    Mutex::new(TimingStats::new("App Update"))
});

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
    pub settings: SettingsWidget,                       // Settings widget (modal, tabs, runtime settings)
    pub device: Arc<wgpu::Device>,                     // Shared ownership using Arc
    pub queue: Arc<wgpu::Queue>,                       // Shared ownership using Arc
    pub is_gpu_supported: bool,
    pub cache_strategy: CacheStrategy,
    pub last_slider_update: Instant,
    pub is_slider_moving: bool,
    pub use_slider_image_for_render: bool,             // Keep using Viewer widget after slider release until keyboard nav
    pub backend: wgpu::Backend,
    pub show_fps: bool,
    pub compression_strategy: CompressionStrategy,
    pub renderer_request_sender: Sender<RendererRequest>,
    pub is_horizontal_split: bool,
    pub file_receiver: Receiver<String>,
    pub synced_zoom: bool,
    pub nearest_neighbor_filter: bool,
    pub is_fullscreen: bool,
    pub cursor_on_top: bool,
    pub cursor_on_menu: bool,                           // Flag to show menu when fullscreen
    pub cursor_on_footer: bool,                         // Flag to show footer when fullscreen
    pub(crate) ctrl_pressed: bool,                                 // Flag to save ctrl/cmd(macOS) press state
    pub use_binary_size: bool,                          // Use binary (KiB/MiB) vs decimal (KB/MB) for file sizes
    pub window_width: f32,                              // Current window width for responsive layout
    #[cfg(feature = "selection")]
    pub selection_manager: SelectionManager,            // Manages image selections/exclusions
    #[cfg(feature = "coco")]
    pub annotation_manager: crate::coco::annotation_manager::AnnotationManager,  // Manages COCO annotations
    #[cfg(feature = "coco")]
    pub coco_disable_simplification: bool,              // COCO: Disable polygon simplification for RLE masks
    #[cfg(feature = "coco")]
    pub coco_mask_render_mode: crate::settings::CocoMaskRenderMode,  // COCO: Mask rendering mode (Polygon or Pixel)
}

// Implement Deref to expose RuntimeSettings fields directly on DataViewer
impl std::ops::Deref for DataViewer {
    type Target = RuntimeSettings;

    fn deref(&self) -> &Self::Target {
        &self.settings.runtime_settings
    }
}

// Implement DerefMut to allow mutable access to RuntimeSettings fields
impl std::ops::DerefMut for DataViewer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.settings.runtime_settings
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
        info!("  nearest_neighbor_filter: {}", settings.nearest_neighbor_filter);
        info!("  cache_strategy: {:?}", cache_strategy);
        info!("  compression_strategy: {:?}", compression_strategy);
        info!("  is_slider_dual: {}", settings.is_slider_dual);

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
            settings: SettingsWidget::new(&settings),
            device,
            queue,
            is_gpu_supported: true,
            background_color: Color::WHITE,
            last_slider_update: Instant::now(),
            is_slider_moving: false,
            use_slider_image_for_render: false,
            backend,
            cache_strategy,
            show_fps: settings.show_fps,
            compression_strategy,
            renderer_request_sender,
            is_horizontal_split: settings.is_horizontal_split,
            file_receiver,
            synced_zoom: settings.synced_zoom,
            nearest_neighbor_filter: settings.nearest_neighbor_filter,
            is_fullscreen: false,
            cursor_on_top: false,
            cursor_on_menu: false,
            cursor_on_footer: false,
            ctrl_pressed: false,
            use_binary_size: settings.use_binary_size,
            window_width: settings.window_width as f32,
            #[cfg(feature = "selection")]
            selection_manager: SelectionManager::new(),
            #[cfg(feature = "coco")]
            annotation_manager: crate::coco::annotation_manager::AnnotationManager::new(),
            #[cfg(feature = "coco")]
            coco_disable_simplification: settings.coco_disable_simplification,
            #[cfg(feature = "coco")]
            coco_mask_render_mode: settings.coco_mask_render_mode,
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
        self.use_slider_image_for_render = false;

        crate::utils::mem::log_memory("DataViewer::reset_state: After reset_state");

        // Clear primitive storage
        self.clear_primitive_storage();
    }

    pub(crate) fn initialize_dir_path(&mut self, path: &PathBuf, pane_index: usize) -> Task<Message> {
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
            pane.slider_image_position = None;
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
        #[cfg(feature = "selection")]
        if let Some(dir_path) = &pane.directory_path {
            if let Err(e) = self.selection_manager.load_for_directory(dir_path) {
                warn!("Failed to load selection state for {}: {}", dir_path, e);
            }
        }

        // After loading the first image, load remaining images asynchronously
        let current_index = self.panes[pane_index].img_cache.current_index;
        crate::navigation_slider::load_initial_neighbors(
            &self.device,
            &self.queue,
            self.is_gpu_supported,
            self.cache_strategy,
            self.compression_strategy,
            &mut self.panes,
            &mut self.loading_status,
            pane_index,
            current_index,
        )
    }

    fn set_ctrl_pressed(&mut self, enabled: bool) {
        self.ctrl_pressed = enabled;
        for pane in self.panes.iter_mut() {
            pane.ctrl_pressed = enabled;
        }
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
                    let _ = pane.initialize_dir_path(
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
                            let _ = pane.initialize_dir_path(
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
            let _ = self.initialize_dir_path(&PathBuf::from(path), 0);
            println!("Directory path initialization complete");
        }

        let _update_start = Instant::now();

        // Route message to handler
        let task = message_handlers::handle_message(self, message);

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

        if self.settings.is_visible() {
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
