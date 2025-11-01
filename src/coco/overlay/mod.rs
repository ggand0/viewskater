/// Overlay rendering for COCO annotations
///
/// This module provides GPU-accelerated rendering of bounding boxes
/// and segmentation masks using WGPU shaders.
pub mod bbox_overlay;
pub mod bbox_shader;
pub mod polygon_shader;

// Re-export the main overlay rendering function
pub use bbox_overlay::render_bbox_overlay;
