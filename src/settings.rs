use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use log::{debug, info, warn, error};
use iced_wgpu::engine::CompressionStrategy;
use crate::cache::img_cache::CacheStrategy;
use crate::config;

/// User-specific settings that persist across app sessions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSettings {
    /// Toggle display of FPS counter
    #[serde(default)]
    pub show_fps: bool,

    /// Toggle footer visibility
    #[serde(default = "default_show_footer")]
    pub show_footer: bool,

    /// Use horizontal split for dual panes
    #[serde(default)]
    pub is_horizontal_split: bool,

    /// Sync zoom and pan between panes
    #[serde(default = "default_synced_zoom")]
    pub synced_zoom: bool,

    /// Enable mouse wheel zoom
    #[serde(default)]
    pub mouse_wheel_zoom: bool,

    /// Cache strategy: "cpu" or "gpu"
    #[serde(default = "default_cache_strategy")]
    pub cache_strategy: String,

    /// Compression strategy: "none" or "bc1"
    #[serde(default = "default_compression_strategy")]
    pub compression_strategy: String,

    /// Slider type: dual (true) or single (false)
    #[serde(default)]
    pub is_slider_dual: bool,

    /// Show copy filename/filepath buttons in footer
    #[serde(default = "default_show_copy_buttons")]
    pub show_copy_buttons: bool,

    /// Show image metadata (resolution, file size) in footer
    #[serde(default = "default_show_metadata")]
    pub show_metadata: bool,

    /// Use nearest-neighbor filtering for pixel-perfect image scaling
    #[serde(default)]
    pub nearest_neighbor_filter: bool,

    // Advanced settings (from config.rs)
    /// Cache window size
    #[serde(default = "default_cache_size")]
    pub cache_size: usize,

    /// Max size for the loading queue
    #[serde(default = "default_max_loading_queue_size")]
    pub max_loading_queue_size: usize,

    /// Max size for being loaded queue
    #[serde(default = "default_max_being_loaded_queue_size")]
    pub max_being_loaded_queue_size: usize,

    /// Default window width
    #[serde(default = "default_window_width")]
    pub window_width: u32,

    /// Default window height
    #[serde(default = "default_window_height")]
    pub window_height: u32,

    /// Texture atlas size (affects slider performance)
    #[serde(default = "default_atlas_size")]
    pub atlas_size: u32,

    /// Double-click detection threshold in milliseconds
    #[serde(default = "default_double_click_threshold_ms")]
    pub double_click_threshold_ms: u16,

    /// Max size for compressed file cache (MB)
    #[serde(default = "default_archive_cache_size")]
    pub archive_cache_size: u64,

    /// Warning threshold for solid archives (MB)
    #[serde(default = "default_archive_warning_threshold_mb")]
    pub archive_warning_threshold_mb: u64,

    /// COCO: Disable polygon simplification for segmentation masks
    #[serde(default)]
    pub coco_disable_simplification: bool,

    /// COCO: Mask rendering mode
    #[serde(default)]
    pub coco_mask_render_mode: CocoMaskRenderMode,

    /// Use binary file size units (KiB/MiB with 1024 divisor) instead of decimal (KB/MB with 1000)
    /// - true: Binary units like `ls -lh` (1 KiB = 1024 bytes)
    /// - false: Decimal units like GNOME/macOS/Windows (1 KB = 1000 bytes)
    #[serde(default)]
    pub use_binary_size: bool,

    /// Location where loading spinner is displayed
    #[serde(default)]
    pub spinner_location: SpinnerLocation,

    // Window position and state
    #[serde(default)]
    pub window_position_x: i32,
    #[serde(default)]
    pub window_position_y: i32,
    #[serde(default)]
    pub is_fullscreen: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CocoMaskRenderMode {
    /// Polygon-based rendering (vector, scalable)
    Polygon,
    /// Pixel-based rendering (raster, exact)
    Pixel,
}

impl Default for CocoMaskRenderMode {
    fn default() -> Self {
        Self::Polygon  // Keep existing behavior as default
    }
}

/// Location where the loading spinner is displayed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpinnerLocation {
    /// Show spinner in the footer (default)
    Footer,
    /// Show spinner in the menu bar (overlays in fullscreen mode)
    MenuBar,
    /// Don't show spinner
    None,
}

impl Default for SpinnerLocation {
    fn default() -> Self {
        Self::Footer
    }
}

fn default_show_footer() -> bool {
    true
}

fn default_synced_zoom() -> bool {
    true
}

fn default_cache_strategy() -> String {
    "gpu".to_string()
}

fn default_compression_strategy() -> String {
    "none".to_string()
}

fn default_show_copy_buttons() -> bool {
    true
}

fn default_show_metadata() -> bool {
    true
}

// Default functions for advanced settings (using config.rs constants)
fn default_cache_size() -> usize {
    config::DEFAULT_CACHE_SIZE
}

fn default_max_loading_queue_size() -> usize {
    config::DEFAULT_MAX_LOADING_QUEUE_SIZE
}

fn default_max_being_loaded_queue_size() -> usize {
    config::DEFAULT_MAX_BEING_LOADED_QUEUE_SIZE
}

fn default_window_width() -> u32 {
    config::DEFAULT_WINDOW_WIDTH
}

fn default_window_height() -> u32 {
    config::DEFAULT_WINDOW_HEIGHT
}

fn default_atlas_size() -> u32 {
    config::DEFAULT_ATLAS_SIZE
}

fn default_double_click_threshold_ms() -> u16 {
    config::DEFAULT_DOUBLE_CLICK_THRESHOLD_MS
}

fn default_archive_cache_size() -> u64 {
    config::DEFAULT_ARCHIVE_CACHE_SIZE
}

fn default_archive_warning_threshold_mb() -> u64 {
    config::DEFAULT_ARCHIVE_WARNING_THRESHOLD_MB
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            show_fps: false,
            show_footer: true,
            is_horizontal_split: false,
            synced_zoom: true,
            mouse_wheel_zoom: false,
            cache_strategy: "gpu".to_string(),
            compression_strategy: "none".to_string(),
            is_slider_dual: false,
            show_copy_buttons: true,
            show_metadata: true,
            nearest_neighbor_filter: false,
            cache_size: config::DEFAULT_CACHE_SIZE,
            max_loading_queue_size: config::DEFAULT_MAX_LOADING_QUEUE_SIZE,
            max_being_loaded_queue_size: config::DEFAULT_MAX_BEING_LOADED_QUEUE_SIZE,
            window_width: config::DEFAULT_WINDOW_WIDTH,
            window_height: config::DEFAULT_WINDOW_HEIGHT,
            atlas_size: config::DEFAULT_ATLAS_SIZE,
            double_click_threshold_ms: config::DEFAULT_DOUBLE_CLICK_THRESHOLD_MS,
            archive_cache_size: config::DEFAULT_ARCHIVE_CACHE_SIZE,
            archive_warning_threshold_mb: config::DEFAULT_ARCHIVE_WARNING_THRESHOLD_MB,
            coco_disable_simplification: false,
            coco_mask_render_mode: CocoMaskRenderMode::default(),
            use_binary_size: false,  // Default to decimal (GNOME/macOS/Windows style)
            spinner_location: SpinnerLocation::default(),
            window_position_x: 0,
            window_position_y: 0,
            is_fullscreen: false,
        }
    }
}

impl UserSettings {
    /// Get the path to the settings file
    /// On macOS: ~/Library/Application Support/viewskater/settings.yaml
    /// On Linux: ~/.config/viewskater/settings.yaml
    /// On Windows: C:\Users\<user>\AppData\Roaming\viewskater\settings.yaml
    pub fn settings_path() -> PathBuf {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."));

        let app_config_dir = config_dir.join("viewskater");
        app_config_dir.join("settings.yaml")
    }

    /// Load settings from the YAML file
    /// If custom_path is provided, uses that path; otherwise uses the default settings path
    pub fn load(custom_path: Option<&str>) -> Self {
        let path = match custom_path {
            Some(p) => {
                info!("Using custom settings path: {}", p);
                PathBuf::from(p)
            }
            None => Self::settings_path(),
        };

        if !path.exists() {
            info!("Settings file not found at {:?}, using defaults", path);
            return Self::default();
        }

        match fs::read_to_string(&path) {
            Ok(contents) => {
                match serde_yaml::from_str::<UserSettings>(&contents) {
                    Ok(settings) => {
                        info!("Loaded settings from {:?}", path);
                        debug!("Settings: show_fps={}, compression={}, cache={}, mouse_wheel_zoom={}, nearest_neighbor_filter={}",
                            settings.show_fps, settings.compression_strategy, settings.cache_strategy, settings.mouse_wheel_zoom, settings.nearest_neighbor_filter);
                        settings
                    }
                    Err(e) => {
                        error!("Failed to parse settings file at {:?}: {}", path, e);
                        warn!("Using default settings");
                        Self::default()
                    }
                }
            }
            Err(e) => {
                error!("Failed to read settings file at {:?}: {}", path, e);
                warn!("Using default settings");
                Self::default()
            }
        }
    }

    /// Save settings to the YAML file while preserving comments
    #[allow(dead_code)]
    pub fn save(&self) -> Result<(), String> {
        let path = Self::settings_path();

        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create settings directory: {}", e))?;
            }
        }

        // If file exists, try to preserve comments by doing in-place value updates
        if path.exists() {
            match fs::read_to_string(&path) {
                Ok(contents) => {
                    let updated = self.update_yaml_values(&contents);
                    fs::write(&path, updated)
                        .map_err(|e| format!("Failed to write settings file: {}", e))?;
                    info!("Saved settings to {:?} (comments preserved)", path);
                    return Ok(());
                }
                Err(e) => {
                    warn!("Failed to read existing settings file for comment preservation: {}", e);
                    // Fall through to create new file
                }
            }
        }

        // File doesn't exist or couldn't be read, create with comments
        let yaml = self.to_yaml_with_comments();
        fs::write(&path, yaml)
            .map_err(|e| format!("Failed to write settings file: {}", e))?;

        info!("Saved settings to {:?}", path);
        Ok(())
    }

    /// Update YAML values while preserving existing comments and structure
    fn update_yaml_values(&self, yaml_content: &str) -> String {
        let mut result = yaml_content.to_string();

        // Track which keys were found/updated
        let mut missing_keys = Vec::new();

        // Update each field using regex to replace the value while keeping comments
        result = Self::replace_yaml_value_or_track(&result, "show_fps", &self.show_fps.to_string(), &mut missing_keys);
        result = Self::replace_yaml_value_or_track(&result, "show_footer", &self.show_footer.to_string(), &mut missing_keys);
        result = Self::replace_yaml_value_or_track(&result, "is_horizontal_split", &self.is_horizontal_split.to_string(), &mut missing_keys);
        result = Self::replace_yaml_value_or_track(&result, "synced_zoom", &self.synced_zoom.to_string(), &mut missing_keys);
        result = Self::replace_yaml_value_or_track(&result, "mouse_wheel_zoom", &self.mouse_wheel_zoom.to_string(), &mut missing_keys);
        result = Self::replace_yaml_value_or_track(&result, "cache_strategy", &format!("\"{}\"", self.cache_strategy), &mut missing_keys);
        result = Self::replace_yaml_value_or_track(&result, "compression_strategy", &format!("\"{}\"", self.compression_strategy), &mut missing_keys);
        result = Self::replace_yaml_value_or_track(&result, "is_slider_dual", &self.is_slider_dual.to_string(), &mut missing_keys);
        result = Self::replace_yaml_value_or_track(&result, "show_copy_buttons", &self.show_copy_buttons.to_string(), &mut missing_keys);
        result = Self::replace_yaml_value_or_track(&result, "show_metadata", &self.show_metadata.to_string(), &mut missing_keys);
        result = Self::replace_yaml_value_or_track(&result, "nearest_neighbor_filter", &self.nearest_neighbor_filter.to_string(), &mut missing_keys);

        // Update advanced settings
        result = Self::replace_yaml_value_or_track(&result, "cache_size", &self.cache_size.to_string(), &mut missing_keys);
        result = Self::replace_yaml_value_or_track(&result, "max_loading_queue_size", &self.max_loading_queue_size.to_string(), &mut missing_keys);
        result = Self::replace_yaml_value_or_track(&result, "max_being_loaded_queue_size", &self.max_being_loaded_queue_size.to_string(), &mut missing_keys);
        result = Self::replace_yaml_value_or_track(&result, "window_width", &self.window_width.to_string(), &mut missing_keys);
        result = Self::replace_yaml_value_or_track(&result, "window_height", &self.window_height.to_string(), &mut missing_keys);
        result = Self::replace_yaml_value_or_track(&result, "atlas_size", &self.atlas_size.to_string(), &mut missing_keys);
        result = Self::replace_yaml_value_or_track(&result, "double_click_threshold_ms", &self.double_click_threshold_ms.to_string(), &mut missing_keys);
        result = Self::replace_yaml_value_or_track(&result, "archive_cache_size", &self.archive_cache_size.to_string(), &mut missing_keys);
        result = Self::replace_yaml_value_or_track(&result, "archive_warning_threshold_mb", &self.archive_warning_threshold_mb.to_string(), &mut missing_keys);

        // Update COCO settings
        result = Self::replace_yaml_value_or_track(&result, "coco_disable_simplification", &self.coco_disable_simplification.to_string(), &mut missing_keys);
        result = Self::replace_yaml_value_or_track(&result, "coco_mask_render_mode", &format!("\"{}\"", match self.coco_mask_render_mode {
            CocoMaskRenderMode::Polygon => "Polygon",
            CocoMaskRenderMode::Pixel => "Pixel",
        }), &mut missing_keys);

        // Update display settings
        result = Self::replace_yaml_value_or_track(&result, "use_binary_size", &self.use_binary_size.to_string(), &mut missing_keys);
        result = Self::replace_yaml_value_or_track(&result, "spinner_location", &format!("\"{}\"", match self.spinner_location {
            SpinnerLocation::Footer => "Footer",
            SpinnerLocation::MenuBar => "MenuBar",
            SpinnerLocation::None => "None",
        }), &mut missing_keys);

        result = Self::replace_yaml_value_or_track(&result, "window_position_x", &self.window_position_x.to_string(), &mut missing_keys);
        result = Self::replace_yaml_value_or_track(&result, "window_position_y", &self.window_position_y.to_string(), &mut missing_keys);
        result = Self::replace_yaml_value_or_track(&result, "is_fullscreen", &self.is_fullscreen.to_string(), &mut missing_keys);
        // Append missing keys with comments
        if !missing_keys.is_empty() {
            // Check if we need to add the advanced settings header
            let needs_header = missing_keys.iter().any(|k| {
                matches!(k.0.as_str(),
                    "cache_size" | "max_loading_queue_size" | "max_being_loaded_queue_size" |
                    "window_width" | "window_height" | "atlas_size" |
                    "double_click_threshold_ms" | "archive_cache_size" | "archive_warning_threshold_mb")
            });

            if needs_header && !result.contains("# --- Advanced Settings ---") {
                result.push_str("\n# --- Advanced Settings ---\n");
            }

            for (key, value) in missing_keys {
                result.push('\n');
                // Add comment for the key
                let comment = Self::get_comment_for_key(&key);
                if !comment.is_empty() {
                    result.push_str(&comment);
                    result.push('\n');
                }
                result.push_str(&format!("{}: {}\n", key, value));
            }
        }

        result
    }

    /// Get descriptive comment for a settings key
    fn get_comment_for_key(key: &str) -> String {
        match key {
            "cache_size" => "# Cache window size (number of images to keep in cache)".to_string(),
            "max_loading_queue_size" => "# Max size for loading queue".to_string(),
            "max_being_loaded_queue_size" => "# Max size for being loaded queue".to_string(),
            "window_width" => "# Default window width (pixels)".to_string(),
            "window_height" => "# Default window height (pixels)".to_string(),
            "atlas_size" => "# Texture atlas size (affects slider performance, power of 2)".to_string(),
            "double_click_threshold_ms" => "# Double-click detection threshold (milliseconds)".to_string(),
            "archive_cache_size" => "# Max size for compressed file cache (bytes)".to_string(),
            "archive_warning_threshold_mb" => "# Warning threshold for solid archives (megabytes)".to_string(),
            "coco_disable_simplification" => "# COCO: Disable polygon simplification (more accurate but slower)".to_string(),
            "coco_mask_render_mode" => "# COCO: Mask rendering mode (Polygon or Pixel)".to_string(),
            "use_binary_size" => "# Use binary file size units (true = KiB/MiB like ls -lh, false = KB/MB like GNOME)".to_string(),
            "show_metadata" => "# Show image metadata (resolution, file size) in footer".to_string(),
            "spinner_location" => "# Loading spinner location: Footer, MenuBar, or None".to_string(),
            _ => String::new(),
        }
    }

    /// Replace a YAML key's value, or track it as missing if not found
    fn replace_yaml_value_or_track(yaml: &str, key: &str, new_value: &str, missing_keys: &mut Vec<(String, String)>) -> String {
        let pattern = format!(r"(?m)^(\s*{}\s*:\s*).*$", regex::escape(key));

        match regex::Regex::new(&pattern) {
            Ok(re) => {
                if re.is_match(yaml) {
                    // Key exists, replace it
                    let replacement = format!("${{1}}{}", new_value);
                    re.replace_all(yaml, replacement.as_str()).to_string()
                } else {
                    // Key doesn't exist, track it
                    missing_keys.push((key.to_string(), new_value.to_string()));
                    yaml.to_string()
                }
            }
            Err(e) => {
                warn!("Failed to create regex for key '{}': {}", key, e);
                yaml.to_string()
            }
        }
    }


    /// Generate YAML content with comments for new files
    fn to_yaml_with_comments(&self) -> String {
        format!(
            r#"# ViewSkater User Settings
# This file is loaded automatically when the application starts.
# Settings specified here will override the default values.

# Display FPS counter (useful for development/debugging)
show_fps: {}

# Show footer with file information
show_footer: {}

# Use horizontal split for dual-pane mode (false = vertical split)
is_horizontal_split: {}

# Synchronize zoom and pan between panes in dual-pane mode
synced_zoom: {}

# Enable mouse wheel zoom (false = mouse wheel navigates images)
mouse_wheel_zoom: {}

# Cache strategy: "cpu" or "gpu"
# - "gpu": Stores decoded images in GPU memory (faster but uses more VRAM)
# - "cpu": Stores decoded images in system RAM (slower but uses less VRAM)
cache_strategy: "{}"

# Compression strategy: "none" or "bc1"
# - "none": No texture compression (higher quality, more VRAM usage)
# - "bc1": BC1/DXT1 compression (lower quality, less VRAM usage, faster for large images)
compression_strategy: "{}"

# Slider type for navigation
# - true: Dual slider (independent sliders for each pane)
# - false: Single slider (shared across panes)
is_slider_dual: {}

# Show copy filename/filepath buttons in footer
show_copy_buttons: {}

# Show image metadata (resolution, file size) in footer
show_metadata: {}

# Use nearest-neighbor filtering for pixel-perfect scaling (good for pixel art)
# - true: Sharp, blocky pixels when zoomed (nearest-neighbor)
# - false: Smooth, interpolated pixels when zoomed (linear)
nearest_neighbor_filter: {}

# --- Advanced Settings ---

# Cache window size (number of images to keep in cache)
cache_size: {}

# Max size for loading queue
max_loading_queue_size: {}

# Max size for being loaded queue
max_being_loaded_queue_size: {}

# Default window width (pixels)
window_width: {}

# Default window height (pixels)
window_height: {}

# Texture atlas size (affects slider performance, power of 2)
atlas_size: {}

# Double-click detection threshold (milliseconds)
double_click_threshold_ms: {}

# Max size for compressed file cache (bytes)
archive_cache_size: {}

# Warning threshold for solid archives (megabytes)
archive_warning_threshold_mb: {}

# --- COCO Settings ---

# Disable polygon simplification for segmentation masks (more accurate but slower)
coco_disable_simplification: {}

# Mask rendering mode: "Polygon" (vector, scalable) or "Pixel" (raster, exact)
coco_mask_render_mode: "{}"

# --- Display Settings ---

# Use binary file size units (KiB/MiB with 1024 divisor) instead of decimal (KB/MB with 1000)
# - true: Binary units like `ls -lh` (1 KiB = 1024 bytes)
# - false: Decimal units like GNOME/macOS/Windows (1 KB = 1000 bytes)
use_binary_size: {}

# Loading spinner location
# - "Footer": Show spinner in the footer bar (default)
# - "MenuBar": Show spinner in the menu bar (overlays in fullscreen mode)
# - "None": Don't show loading spinner
spinner_location: "{}"
"#,
            self.show_fps,
            self.show_footer,
            self.is_horizontal_split,
            self.synced_zoom,
            self.mouse_wheel_zoom,
            self.cache_strategy,
            self.compression_strategy,
            self.is_slider_dual,
            self.show_copy_buttons,
            self.show_metadata,
            self.nearest_neighbor_filter,
            self.cache_size,
            self.max_loading_queue_size,
            self.max_being_loaded_queue_size,
            self.window_width,
            self.window_height,
            self.atlas_size,
            self.double_click_threshold_ms,
            self.archive_cache_size,
            self.archive_warning_threshold_mb,
            self.coco_disable_simplification,
            match self.coco_mask_render_mode {
                CocoMaskRenderMode::Polygon => "Polygon",
                CocoMaskRenderMode::Pixel => "Pixel",
            },
            self.use_binary_size,
            match self.spinner_location {
                SpinnerLocation::Footer => "Footer",
                SpinnerLocation::MenuBar => "MenuBar",
                SpinnerLocation::None => "None",
            }
        )
    }

    /// Convert cache_strategy string to CacheStrategy enum
    pub fn get_cache_strategy(&self) -> CacheStrategy {
        match self.cache_strategy.to_lowercase().as_str() {
            "cpu" => CacheStrategy::Cpu,
            "gpu" => CacheStrategy::Gpu,
            _ => {
                warn!("Unknown cache strategy '{}', defaulting to GPU", self.cache_strategy);
                CacheStrategy::Gpu
            }
        }
    }

    /// Convert compression_strategy string to CompressionStrategy enum
    pub fn get_compression_strategy(&self) -> CompressionStrategy {
        match self.compression_strategy.to_lowercase().as_str() {
            "none" => CompressionStrategy::None,
            "bc1" => CompressionStrategy::Bc1,
            _ => {
                warn!("Unknown compression strategy '{}', defaulting to None", self.compression_strategy);
                CompressionStrategy::None
            }
        }
    }
}
