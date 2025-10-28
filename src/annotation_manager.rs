/// Annotation manager for COCO datasets
///
/// Manages loading, caching, and accessing COCO annotations.
/// Associates annotation files with image directories.

use std::collections::HashMap;
use std::path::PathBuf;
use log::info;

use crate::coco_parser::{CocoDataset, ImageAnnotation};

/// Manages COCO annotations for the current session
pub struct AnnotationManager {
    /// Currently loaded dataset (if any)
    current_dataset: Option<LoadedDataset>,

    /// Path to the currently loaded COCO JSON file
    current_json_path: Option<PathBuf>,
}

/// A loaded COCO dataset with its associated directory
struct LoadedDataset {
    /// The parsed COCO dataset
    dataset: CocoDataset,

    /// Image directory associated with this dataset
    image_directory: PathBuf,

    /// Cached lookup map: filename -> annotations
    annotation_map: HashMap<String, Vec<ImageAnnotation>>,
}

impl AnnotationManager {
    /// Create a new annotation manager
    pub fn new() -> Self {
        Self {
            current_dataset: None,
            current_json_path: None,
        }
    }

    /// Load a COCO dataset from a JSON file
    ///
    /// Returns Ok(true) if image directory was found automatically,
    /// Ok(false) if caller needs to prompt for image directory,
    /// Err if parsing failed
    pub fn load_coco_file(&mut self, json_path: PathBuf) -> Result<bool, String> {
        info!("Loading COCO file: {}", json_path.display());

        // Parse the COCO JSON
        let dataset = CocoDataset::from_file(&json_path)?;

        // Validate the dataset
        dataset.validate()?;

        info!("COCO dataset parsed: {} images, {} annotations, {} categories",
              dataset.images.len(), dataset.annotations.len(), dataset.categories.len());

        // Try to find the image directory automatically
        let json_dir = json_path.parent()
            .ok_or_else(|| "Could not determine JSON file directory".to_string())?
            .to_path_buf();

        let image_dir = self.find_image_directory(&dataset, &json_dir)?;

        if let Some(dir) = image_dir {
            // Found the directory automatically
            self.set_image_directory(dataset, json_path, dir)?;
            Ok(true)
        } else {
            // Need to prompt user for directory
            self.current_json_path = Some(json_path);
            Ok(false)
        }
    }

    /// Set the image directory for a loaded dataset
    pub fn set_image_directory(
        &mut self,
        dataset: CocoDataset,
        json_path: PathBuf,
        image_directory: PathBuf,
    ) -> Result<(), String> {
        // Verify that at least some images exist in this directory
        let found = self.verify_images_in_directory(&dataset, &image_directory)?;

        if found == 0 {
            return Err(format!(
                "No images from the COCO dataset found in directory: {}",
                image_directory.display()
            ));
        }

        info!("Found {} images in directory: {}", found, image_directory.display());

        // Build the annotation lookup map
        let annotation_map = dataset.build_image_annotation_map();

        self.current_dataset = Some(LoadedDataset {
            dataset,
            image_directory,
            annotation_map,
        });
        self.current_json_path = Some(json_path);

        Ok(())
    }

    /// Try to find the image directory automatically
    ///
    /// Checks:
    /// 1. Same directory as JSON file
    /// 2. "images" subdirectory
    /// 3. Common COCO directory names
    fn find_image_directory(
        &self,
        dataset: &CocoDataset,
        json_dir: &PathBuf,
    ) -> Result<Option<PathBuf>, String> {
        let candidates = vec![
            json_dir.clone(),
            json_dir.join("images"),
            json_dir.join("img"),
            json_dir.join("data"),
            json_dir.join("train"),
            json_dir.join("val"),
            json_dir.join("test"),
        ];

        // Get first few image filenames to check
        let test_filenames: Vec<_> = dataset.get_image_filenames()
            .into_iter()
            .take(5)
            .collect();

        if test_filenames.is_empty() {
            return Ok(None);
        }

        // Check each candidate directory
        for candidate in candidates {
            if !candidate.exists() || !candidate.is_dir() {
                continue;
            }

            // Check if at least 2 test images exist in this directory
            let mut found = 0;
            for filename in &test_filenames {
                let image_path = candidate.join(filename);
                if image_path.exists() {
                    found += 1;
                    if found >= 2 {
                        info!("Auto-detected image directory: {}", candidate.display());
                        return Ok(Some(candidate));
                    }
                }
            }
        }

        Ok(None)
    }

    /// Verify how many images from the dataset exist in the given directory
    fn verify_images_in_directory(
        &self,
        dataset: &CocoDataset,
        directory: &PathBuf,
    ) -> Result<usize, String> {
        let mut found = 0;
        let filenames = dataset.get_image_filenames();

        // Check first 20 images or all if fewer
        let check_count = filenames.len().min(20);

        for filename in filenames.iter().take(check_count) {
            let image_path = directory.join(filename);
            if image_path.exists() {
                found += 1;
            }
        }

        Ok(found)
    }

    /// Get annotations for a given image filename
    pub fn get_annotations(&self, filename: &str) -> Option<&Vec<ImageAnnotation>> {
        self.current_dataset.as_ref()
            .and_then(|ds| ds.annotation_map.get(filename))
    }

    /// Check if annotations are currently loaded
    pub fn has_annotations(&self) -> bool {
        self.current_dataset.is_some()
    }

    /// Get the current image directory (if loaded)
    pub fn get_image_directory(&self) -> Option<&PathBuf> {
        self.current_dataset.as_ref()
            .map(|ds| &ds.image_directory)
    }

    /// Get the current JSON path (if loaded)
    pub fn get_json_path(&self) -> Option<&PathBuf> {
        self.current_json_path.as_ref()
    }

    /// Get dataset statistics
    pub fn get_stats(&self) -> Option<DatasetStats> {
        self.current_dataset.as_ref().map(|ds| DatasetStats {
            num_images: ds.dataset.images.len(),
            num_annotations: ds.dataset.annotations.len(),
            num_categories: ds.dataset.categories.len(),
        })
    }

    /// Clear the currently loaded dataset
    pub fn clear(&mut self) {
        self.current_dataset = None;
        self.current_json_path = None;
        info!("Cleared COCO annotations");
    }
}

impl Default for AnnotationManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about the loaded dataset
#[derive(Debug, Clone)]
pub struct DatasetStats {
    pub num_images: usize,
    pub num_annotations: usize,
    pub num_categories: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_annotation_manager_creation() {
        let manager = AnnotationManager::new();
        assert!(!manager.has_annotations());
        assert!(manager.get_annotations("test.jpg").is_none());
    }
}
