// Slider Atlas Module
// Ported from iced_wgpu's atlas system for slider-based image caching
//
// This module provides efficient GPU texture atlas management for slider navigation,
// reusing iced_wgpu's proven allocation and caching strategies.

pub mod entry;
pub mod allocation;
pub mod allocator;
pub mod layer;
pub mod atlas;
pub mod pipeline;

// Public exports (will be used in later phases)
#[allow(unused_imports)]
pub use allocation::Allocation;
#[allow(unused_imports)]
pub use allocator::Allocator;
#[allow(unused_imports)]
pub use atlas::Atlas;
#[allow(unused_imports)]
pub use entry::Entry;
#[allow(unused_imports)]
pub use layer::Layer;
#[allow(unused_imports)]
pub use pipeline::AtlasPipeline;

// Re-export commonly used types
#[allow(dead_code)]
pub const ATLAS_SIZE: u32 = atlas::SIZE;

