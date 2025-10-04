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

    /// Save settings to the YAML file
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

        let yaml = serde_yaml::to_string(self)
            .map_err(|e| format!("Failed to serialize settings: {}", e))?;

        fs::write(&path, yaml)
            .map_err(|e| format!("Failed to write settings file: {}", e))?;

        info!("Saved settings to {:?}", path);
        Ok(())
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
