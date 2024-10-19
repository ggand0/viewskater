use once_cell::sync::Lazy;

pub struct Config {
    pub cache_size: usize,                  // Cache window size
    pub max_loading_queue_size: usize,      // Max size for the loading queue to prevent overloading
    pub max_being_loaded_queue_size: usize,
}

pub static CONFIG: Lazy<Config> = Lazy::new(|| Config {
    cache_size: 5,
    max_loading_queue_size: 3,
    max_being_loaded_queue_size: 3,
});
