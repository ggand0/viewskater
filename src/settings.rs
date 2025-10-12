use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use log::{debug, info, warn, error};
use iced_wgpu::engine::CompressionStrategy;
use crate::cache::img_cache::CacheStrategy;

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
        }
    }
}

impl UserSettings {
    /// Get the path to the settings file
    /// On macOS: ~/Library/Application Support/ViewSkater/settings.yaml
    /// On Linux: ~/.config/viewskater/settings.yaml
    /// On Windows: C:\Users\<user>\AppData\Roaming\ViewSkater\settings.yaml
    pub fn settings_path() -> PathBuf {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."));

        let app_config_dir = config_dir.join("ViewSkater");
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
                        debug!("Settings: show_fps={}, compression={}, cache={}, mouse_wheel_zoom={}",
                            settings.show_fps, settings.compression_strategy, settings.cache_strategy, settings.mouse_wheel_zoom);
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

        // Update each field using regex to replace the value while keeping comments
        result = Self::replace_yaml_value(&result, "show_fps", &self.show_fps.to_string());
        result = Self::replace_yaml_value(&result, "show_footer", &self.show_footer.to_string());
        result = Self::replace_yaml_value(&result, "is_horizontal_split", &self.is_horizontal_split.to_string());
        result = Self::replace_yaml_value(&result, "synced_zoom", &self.synced_zoom.to_string());
        result = Self::replace_yaml_value(&result, "mouse_wheel_zoom", &self.mouse_wheel_zoom.to_string());
        result = Self::replace_yaml_value(&result, "cache_strategy", &format!("\"{}\"", self.cache_strategy));
        result = Self::replace_yaml_value(&result, "compression_strategy", &format!("\"{}\"", self.compression_strategy));
        result = Self::replace_yaml_value(&result, "is_slider_dual", &self.is_slider_dual.to_string());

        result
    }

    /// Replace a YAML key's value while preserving the rest of the line
    fn replace_yaml_value(yaml: &str, key: &str, new_value: &str) -> String {
        let pattern = format!(r"(?m)^(\s*{}\s*:\s*).*$", regex::escape(key));
        let replacement = format!("${{1}}{}", new_value);

        // Use regex crate for replacement
        match regex::Regex::new(&pattern) {
            Ok(re) => re.replace_all(yaml, replacement.as_str()).to_string(),
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
"#,
            self.show_fps,
            self.show_footer,
            self.is_horizontal_split,
            self.synced_zoom,
            self.mouse_wheel_zoom,
            self.cache_strategy,
            self.compression_strategy,
            self.is_slider_dual
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
