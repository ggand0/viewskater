/// COCO dataset JSON parser
///
/// This module parses COCO format annotation files.
/// Format specification: https://cocodataset.org/#format-data
use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CocoDataset {
    pub images: Vec<CocoImage>,
    pub annotations: Vec<CocoAnnotation>,
    pub categories: Vec<CocoCategory>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CocoImage {
    pub id: u64,
    pub file_name: String,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CocoAnnotation {
    pub id: u64,
    pub image_id: u64,
    pub category_id: u64,
    pub bbox: Vec<f32>,  // [x, y, width, height] in COCO format
    #[serde(default)]
    pub segmentation: Option<CocoSegmentation>,
    #[serde(default)]
    pub area: f32,
    #[serde(default)]
    pub iscrowd: u8,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum CocoSegmentation {
    Polygon(Vec<Vec<f32>>),      // List of polygons
    RLE(CocoRLE),                 // Run-length encoding
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CocoRLE {
    pub counts: Vec<u32>,
    pub size: Vec<u32>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CocoCategory {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub supercategory: String,
}

impl CocoDataset {
    /// Parse COCO JSON from a file
    pub fn from_file(path: &PathBuf) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read COCO file: {}", e))?;

        Self::from_str(&content)
    }

    /// Parse COCO JSON from a string
    pub fn from_str(content: &str) -> Result<Self, String> {
        serde_json::from_str(content)
            .map_err(|e| format!("Failed to parse COCO JSON: {}", e))
    }

    /// Validate that this looks like a COCO dataset and filter out invalid annotations
    /// Returns number of skipped annotations and warnings
    pub fn validate_and_clean(&mut self) -> (usize, Vec<String>) {
        let mut warnings = Vec::new();

        if self.images.is_empty() {
            warnings.push("COCO dataset has no images".to_string());
        }

        if self.categories.is_empty() {
            warnings.push("COCO dataset has no categories".to_string());
        }

        // Check that annotations reference valid image_ids and category_ids
        let image_ids: std::collections::HashSet<_> =
            self.images.iter().map(|img| img.id).collect();
        let category_ids: std::collections::HashSet<_> =
            self.categories.iter().map(|cat| cat.id).collect();

        let original_count = self.annotations.len();

        // Filter out invalid annotations
        self.annotations.retain(|ann| {
            if !image_ids.contains(&ann.image_id) {
                warnings.push(format!(
                    "Skipping annotation {}: references non-existent image_id {}",
                    ann.id, ann.image_id
                ));
                return false;
            }
            if !category_ids.contains(&ann.category_id) {
                warnings.push(format!(
                    "Skipping annotation {}: references non-existent category_id {}",
                    ann.id, ann.category_id
                ));
                return false;
            }
            if ann.bbox.len() != 4 {
                warnings.push(format!(
                    "Skipping annotation {}: invalid bbox format (expected 4 values, got {})",
                    ann.id, ann.bbox.len()
                ));
                return false;
            }
            true
        });

        let skipped_count = original_count - self.annotations.len();
        (skipped_count, warnings)
    }

    /// Check if JSON content looks like a COCO dataset (quick detection)
    pub fn is_coco_format(content: &str) -> bool {
        // Try to parse as JSON and check for required keys
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(content) {
            if let Some(obj) = value.as_object() {
                return obj.contains_key("images")
                    && obj.contains_key("annotations")
                    && obj.contains_key("categories");
            }
        }
        false
    }

    /// Build a lookup map from filename to annotations
    pub fn build_image_annotation_map(&self) -> HashMap<String, Vec<ImageAnnotation>> {
        let mut map: HashMap<String, Vec<ImageAnnotation>> = HashMap::new();

        // Create category lookup
        let category_map: HashMap<u64, &CocoCategory> =
            self.categories.iter().map(|cat| (cat.id, cat)).collect();

        // Create image lookup
        let image_map: HashMap<u64, &CocoImage> =
            self.images.iter().map(|img| (img.id, img)).collect();

        // Group annotations by image
        for ann in &self.annotations {
            if let Some(image) = image_map.get(&ann.image_id) {
                let category_name = category_map
                    .get(&ann.category_id)
                    .map(|cat| cat.name.clone())
                    .unwrap_or_else(|| format!("Unknown ({})", ann.category_id));

                let image_ann = ImageAnnotation {
                    bbox: BoundingBox {
                        x: ann.bbox[0],
                        y: ann.bbox[1],
                        width: ann.bbox[2],
                        height: ann.bbox[3],
                    },
                    category_id: ann.category_id,
                    category_name,
                    segmentation: ann.segmentation.clone(),
                };

                map.entry(image.file_name.clone())
                    .or_insert_with(Vec::new)
                    .push(image_ann);
            }
        }

        map
    }

    /// Get list of all image filenames in the dataset
    pub fn get_image_filenames(&self) -> Vec<String> {
        self.images.iter().map(|img| img.file_name.clone()).collect()
    }
}

/// Simplified annotation structure for rendering
#[derive(Debug, Clone)]
pub struct ImageAnnotation {
    pub bbox: BoundingBox,
    pub category_id: u64,
    pub category_name: String,
    pub segmentation: Option<CocoSegmentation>,
}

#[derive(Debug, Clone, Copy)]
pub struct BoundingBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl BoundingBox {
    /// Convert COCO bbox (x, y, w, h) to top-left and bottom-right corners
    pub fn to_corners(&self) -> (f32, f32, f32, f32) {
        (self.x, self.y, self.x + self.width, self.y + self.height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coco_detection() {
        let valid_coco = r#"{
            "images": [],
            "annotations": [],
            "categories": []
        }"#;
        assert!(CocoDataset::is_coco_format(valid_coco));

        let invalid = r#"{"foo": "bar"}"#;
        assert!(!CocoDataset::is_coco_format(invalid));
    }

    #[test]
    fn test_coco_parsing() {
        let coco_json = r#"{
            "images": [
                {"id": 1, "file_name": "test.jpg", "width": 640, "height": 480}
            ],
            "annotations": [
                {
                    "id": 1,
                    "image_id": 1,
                    "category_id": 1,
                    "bbox": [10.0, 20.0, 100.0, 200.0],
                    "area": 20000.0,
                    "iscrowd": 0
                }
            ],
            "categories": [
                {"id": 1, "name": "person", "supercategory": "human"}
            ]
        }"#;

        let mut dataset = CocoDataset::from_str(coco_json).unwrap();
        let (skipped_count, _warnings) = dataset.validate_and_clean();
        assert_eq!(skipped_count, 0);
        assert_eq!(dataset.images.len(), 1);
        assert_eq!(dataset.annotations.len(), 1);
        assert_eq!(dataset.categories.len(), 1);
    }
}
