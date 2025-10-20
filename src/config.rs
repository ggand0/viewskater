use once_cell::sync::Lazy;
use crate::settings::UserSettings;

// Default values for configuration
// These serve as fallback values and can be used for "reset to defaults" functionality
pub const DEFAULT_CACHE_SIZE: usize = 5;
pub const DEFAULT_MAX_LOADING_QUEUE_SIZE: usize = 3;
pub const DEFAULT_MAX_BEING_LOADED_QUEUE_SIZE: usize = 3;
pub const DEFAULT_WINDOW_WIDTH: u32 = 1200;
pub const DEFAULT_WINDOW_HEIGHT: u32 = 800;
pub const DEFAULT_ATLAS_SIZE: u32 = 2048;
pub const DEFAULT_DOUBLE_CLICK_THRESHOLD_MS: u16 = 250;
pub const DEFAULT_ARCHIVE_CACHE_SIZE: u64 = 209_715_200;    // 200MB
pub const DEFAULT_ARCHIVE_WARNING_THRESHOLD_MB: u64 = 500;  // 500MB threshold for warning dialog

pub struct Config {
    pub cache_size: usize,                  // Cache window size
    pub max_loading_queue_size: usize,      // Max size for the loading queue to prevent overloading
    pub max_being_loaded_queue_size: usize,
    pub window_width: u32,                  // Default window width
    pub window_height: u32,                 // Default window height
    pub atlas_size: u32,                    // Size of the square texture atlas used in iced_wgpu (affects slider performance)
    pub double_click_threshold_ms: u16,     // Double-click detection threshold in milliseconds
    pub archive_cache_size: u64,            // Max size for compressed file cache
    pub archive_warning_threshold_mb: u64   // Show warning dialog for solid archives larger than this (MB)
}

pub static CONFIG: Lazy<Config> = Lazy::new(|| {
    // Load settings from YAML file
    let settings = UserSettings::load(None);

    Config {
        cache_size: settings.cache_size,
        max_loading_queue_size: settings.max_loading_queue_size,
        max_being_loaded_queue_size: settings.max_being_loaded_queue_size,
        window_width: settings.window_width,
        window_height: settings.window_height,
        atlas_size: settings.atlas_size,
        double_click_threshold_ms: settings.double_click_threshold_ms,
        archive_cache_size: settings.archive_cache_size,
        archive_warning_threshold_mb: settings.archive_warning_threshold_mb,
    }
});
