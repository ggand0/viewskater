use once_cell::sync::Lazy;

pub struct Config {
    pub cache_size: usize,                  // Cache window size
    pub max_loading_queue_size: usize,      // Max size for the loading queue to prevent overloading
    pub max_being_loaded_queue_size: usize,
    pub window_width: u32,                  // Default window width
    pub window_height: u32,                 // Default window height
    pub atlas_size: u32,                    // Size of the square texture atlas used in iced_wgpu (affects slider performance)
    pub double_click_threshold_ms: u16,     // Double-click detection threshold in milliseconds
    pub archive_cache_size: u64             // Max size for compressed file cache
}

pub static CONFIG: Lazy<Config> = Lazy::new(|| Config {
    cache_size: 5,
    max_loading_queue_size: 3,
    max_being_loaded_queue_size: 3,
    window_width: 1200,
    window_height: 800,
    atlas_size: 2048,
    double_click_threshold_ms: 250,
    archive_cache_size: 2_097_152_00,
});
