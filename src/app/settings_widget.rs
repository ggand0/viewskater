//! Settings widget module
//! Manages all settings-related state and UI for the application

use std::collections::HashMap;
use crate::settings::UserSettings;

/// Runtime-configurable settings that can be applied immediately without restart
#[derive(Debug, Clone)]
pub struct RuntimeSettings {
    pub mouse_wheel_zoom: bool,                         // Flag to change mouse scroll wheel behavior
    pub show_copy_buttons: bool,                        // Show copy filename/filepath buttons in footer
    pub show_metadata: bool,                            // Show image metadata (resolution, file size) in footer
    pub cache_size: usize,                              // Image cache window size (number of images to cache)
    pub archive_cache_size: u64,                        // Archive cache size in bytes (for preload decision)
    pub archive_warning_threshold_mb: u64,              // Warning threshold for large solid archives (MB)
    pub max_loading_queue_size: usize,                  // Max size for loading queue
    pub max_being_loaded_queue_size: usize,             // Max size for being loaded queue
    pub double_click_threshold_ms: u16,                 // Double-click threshold in milliseconds
}

impl RuntimeSettings {
    pub fn from_user_settings(settings: &UserSettings) -> Self {
        Self {
            mouse_wheel_zoom: settings.mouse_wheel_zoom,
            show_copy_buttons: settings.show_copy_buttons,
            show_metadata: settings.show_metadata,
            cache_size: settings.cache_size,
            archive_cache_size: settings.archive_cache_size * 1_048_576,  // Convert MB to bytes
            archive_warning_threshold_mb: settings.archive_warning_threshold_mb,
            max_loading_queue_size: settings.max_loading_queue_size,
            max_being_loaded_queue_size: settings.max_being_loaded_queue_size,
            double_click_threshold_ms: settings.double_click_threshold_ms,
        }
    }
}

/// Settings widget state
pub struct SettingsWidget {
    pub show_options: bool,                             // Settings modal visibility
    pub save_status: Option<String>,                    // Save feedback message
    pub active_tab: usize,                              // Which tab is selected
    pub advanced_input: HashMap<String, String>,        // Text input state for advanced settings
    pub runtime_settings: RuntimeSettings,              // Runtime-configurable settings
}

impl SettingsWidget {
    pub fn new(settings: &UserSettings) -> Self {
        // Initialize advanced settings input with current values
        let mut advanced_input = HashMap::new();
        advanced_input.insert("cache_size".to_string(), settings.cache_size.to_string());
        advanced_input.insert("max_loading_queue_size".to_string(), settings.max_loading_queue_size.to_string());
        advanced_input.insert("max_being_loaded_queue_size".to_string(), settings.max_being_loaded_queue_size.to_string());
        advanced_input.insert("window_width".to_string(), settings.window_width.to_string());
        advanced_input.insert("window_height".to_string(), settings.window_height.to_string());
        advanced_input.insert("atlas_size".to_string(), settings.atlas_size.to_string());
        advanced_input.insert("double_click_threshold_ms".to_string(), settings.double_click_threshold_ms.to_string());
        advanced_input.insert("archive_cache_size".to_string(), settings.archive_cache_size.to_string());
        advanced_input.insert("archive_warning_threshold_mb".to_string(), settings.archive_warning_threshold_mb.to_string());

        Self {
            show_options: false,
            save_status: None,
            active_tab: 0,
            advanced_input,
            runtime_settings: RuntimeSettings::from_user_settings(settings),
        }
    }

    pub fn show(&mut self) {
        self.show_options = true;
    }

    pub fn hide(&mut self) {
        self.show_options = false;
    }

    pub fn set_save_status(&mut self, status: Option<String>) {
        self.save_status = status;
    }

    pub fn clear_save_status(&mut self) {
        self.save_status = None;
    }

    pub fn set_active_tab(&mut self, tab: usize) {
        self.active_tab = tab;
    }

    pub fn set_advanced_input(&mut self, key: String, value: String) {
        self.advanced_input.insert(key, value);
    }

    pub fn is_visible(&self) -> bool {
        self.show_options
    }
}
