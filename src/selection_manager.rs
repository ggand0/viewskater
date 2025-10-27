use std::collections::{HashMap, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use serde::{Deserialize, Serialize};

#[allow(unused_imports)]
use log::{debug, error, info, warn};

/// Represents the marking state of an image
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageMark {
    Unmarked,
    Selected,   // Will be included in "copy selected" export
    Excluded,   // Will be excluded in "copy non-excluded" export
}

impl Default for ImageMark {
    fn default() -> Self {
        ImageMark::Unmarked
    }
}

/// Stores selection state for a directory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectionState {
    pub directory_path: String,
    pub marks: HashMap<String, ImageMark>,
    #[serde(skip, default = "SystemTime::now")]
    pub last_modified: SystemTime,
    #[serde(skip, default)]
    pub dirty: bool,  // Flag to track if we need to save
}

impl SelectionState {
    pub fn new(directory_path: String) -> Self {
        Self {
            directory_path,
            marks: HashMap::new(),
            last_modified: SystemTime::now(),
            dirty: false,
        }
    }

    /// Mark an image with a specific state
    pub fn mark_image(&mut self, filename: &str, mark: ImageMark) {
        self.marks.insert(filename.to_string(), mark);
        self.last_modified = SystemTime::now();
        self.dirty = true;
        debug!("Marked {} as {:?}", filename, mark);
    }

    /// Get the mark for an image (returns Unmarked if not found)
    pub fn get_mark(&self, filename: &str) -> ImageMark {
        self.marks.get(filename).copied().unwrap_or(ImageMark::Unmarked)
    }

    /// Toggle selection state for an image
    pub fn toggle_selected(&mut self, filename: &str) {
        let current = self.get_mark(filename);
        let new_mark = if current == ImageMark::Selected {
            ImageMark::Unmarked
        } else {
            ImageMark::Selected
        };
        self.mark_image(filename, new_mark);
    }

    /// Toggle exclusion state for an image
    pub fn toggle_excluded(&mut self, filename: &str) {
        let current = self.get_mark(filename);
        let new_mark = if current == ImageMark::Excluded {
            ImageMark::Unmarked
        } else {
            ImageMark::Excluded
        };
        self.mark_image(filename, new_mark);
    }

    /// Clear the mark for an image
    pub fn clear_mark(&mut self, filename: &str) {
        if self.marks.remove(filename).is_some() {
            self.last_modified = SystemTime::now();
            self.dirty = true;
            debug!("Cleared mark for {}", filename);
        }
    }

    /// Get count of selected images
    #[allow(dead_code)]
    pub fn selected_count(&self) -> usize {
        self.marks.values().filter(|&&m| m == ImageMark::Selected).count()
    }

    /// Get count of excluded images
    #[allow(dead_code)]
    pub fn excluded_count(&self) -> usize {
        self.marks.values().filter(|&&m| m == ImageMark::Excluded).count()
    }

    /// Get count of marked images (selected or excluded)
    #[allow(dead_code)]
    pub fn marked_count(&self) -> usize {
        self.marks.len()
    }
}

/// Manages selection states across multiple directories
pub struct SelectionManager {
    current_state: Option<SelectionState>,
    data_dir: PathBuf,
}

impl SelectionManager {
    pub fn new() -> Self {
        let data_dir = Self::get_selections_dir();

        // Create the directory if it doesn't exist
        if let Err(e) = std::fs::create_dir_all(&data_dir) {
            error!("Failed to create selections directory: {}", e);
        } else {
            info!("Selection data directory: {}", data_dir.display());
        }

        Self {
            current_state: None,
            data_dir,
        }
    }

    /// Get the platform-specific directory for storing selection data
    fn get_selections_dir() -> PathBuf {
        let data_dir = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."));
        data_dir.join("viewskater").join("selections")
    }

    /// Generate a hash-based filename for a directory path
    fn get_selection_file_path(&self, dir_path: &str) -> PathBuf {
        let mut hasher = DefaultHasher::new();
        dir_path.hash(&mut hasher);
        let hash = hasher.finish();
        self.data_dir.join(format!("{:x}.json", hash))
    }

    /// Load selection state for a directory
    pub fn load_for_directory(&mut self, dir_path: &str) -> Result<(), std::io::Error> {
        let file_path = self.get_selection_file_path(dir_path);

        if !file_path.exists() {
            debug!("No existing selection file for directory: {}", dir_path);
            self.current_state = Some(SelectionState::new(dir_path.to_string()));
            return Ok(());
        }

        match std::fs::read_to_string(&file_path) {
            Ok(json_str) => {
                match serde_json::from_str::<SelectionState>(&json_str) {
                    Ok(mut state) => {
                        state.dirty = false;
                        state.last_modified = SystemTime::now();
                        info!("Loaded {} marks for directory: {}", state.marks.len(), dir_path);
                        self.current_state = Some(state);
                        Ok(())
                    }
                    Err(e) => {
                        error!("Failed to parse selection file: {}", e);
                        self.current_state = Some(SelectionState::new(dir_path.to_string()));
                        Err(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
                    }
                }
            }
            Err(e) => {
                error!("Failed to read selection file: {}", e);
                self.current_state = Some(SelectionState::new(dir_path.to_string()));
                Err(e)
            }
        }
    }

    /// Save the current selection state to disk
    pub fn save(&mut self) -> Result<(), std::io::Error> {
        if let Some(ref state) = self.current_state {
            if !state.dirty {
                debug!("Selection state not dirty, skipping save");
                return Ok(());
            }

            let file_path = self.get_selection_file_path(&state.directory_path);

            match serde_json::to_string_pretty(state) {
                Ok(json_str) => {
                    match std::fs::write(&file_path, json_str) {
                        Ok(_) => {
                            info!("Saved selection state to: {}", file_path.display());
                            // Mark as clean after successful save
                            if let Some(ref mut s) = self.current_state {
                                s.dirty = false;
                            }
                            Ok(())
                        }
                        Err(e) => {
                            error!("Failed to write selection file: {}", e);
                            Err(e)
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to serialize selection state: {}", e);
                    Err(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
                }
            }
        } else {
            debug!("No current selection state to save");
            Ok(())
        }
    }

    /// Export the current selection state to a specific JSON file
    pub fn export_to_file(&self, export_path: &Path) -> Result<(), std::io::Error> {
        if let Some(ref state) = self.current_state {
            let json_str = serde_json::to_string_pretty(state)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

            std::fs::write(export_path, json_str)?;
            info!("Exported selection state to: {}", export_path.display());
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "No selection state to export"
            ))
        }
    }

    /// Get a mutable reference to the current state
    #[allow(dead_code)]
    pub fn current_state_mut(&mut self) -> Option<&mut SelectionState> {
        self.current_state.as_mut()
    }

    /// Get a reference to the current state
    #[allow(dead_code)]
    pub fn current_state(&self) -> Option<&SelectionState> {
        self.current_state.as_ref()
    }

    /// Mark an image in the current directory
    #[allow(dead_code)]
    pub fn mark_image(&mut self, filename: &str, mark: ImageMark) {
        if let Some(ref mut state) = self.current_state {
            state.mark_image(filename, mark);
        }
    }

    /// Toggle selected state for an image
    pub fn toggle_selected(&mut self, filename: &str) {
        if let Some(ref mut state) = self.current_state {
            state.toggle_selected(filename);
        }
    }

    /// Toggle excluded state for an image
    pub fn toggle_excluded(&mut self, filename: &str) {
        if let Some(ref mut state) = self.current_state {
            state.toggle_excluded(filename);
        }
    }

    /// Clear mark for an image
    pub fn clear_mark(&mut self, filename: &str) {
        if let Some(ref mut state) = self.current_state {
            state.clear_mark(filename);
        }
    }

    /// Get the mark for an image
    pub fn get_mark(&self, filename: &str) -> ImageMark {
        self.current_state
            .as_ref()
            .map(|s| s.get_mark(filename))
            .unwrap_or(ImageMark::Unmarked)
    }
}

impl Default for SelectionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selection_state() {
        let mut state = SelectionState::new("/test/path".to_string());

        // Test marking
        state.mark_image("test.jpg", ImageMark::Selected);
        assert_eq!(state.get_mark("test.jpg"), ImageMark::Selected);
        assert_eq!(state.selected_count(), 1);

        // Test toggling
        state.toggle_excluded("test2.jpg");
        assert_eq!(state.get_mark("test2.jpg"), ImageMark::Excluded);
        assert_eq!(state.excluded_count(), 1);

        // Test clearing
        state.clear_mark("test.jpg");
        assert_eq!(state.get_mark("test.jpg"), ImageMark::Unmarked);
    }
}
