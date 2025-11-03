// Port of iced_wgpu's layer.rs
// Represents a layer in the atlas texture array

use crate::slider_atlas::Allocator;

#[derive(Debug)]
pub enum Layer {
    Empty,
    Busy(Allocator),
    Full,
}

impl Layer {
    pub fn is_empty(&self) -> bool {
        matches!(self, Layer::Empty)
    }

    pub fn allocations(&self) -> usize {
        match self {
            Layer::Empty => 0,
            Layer::Busy(allocator) => allocator.allocations(),
            Layer::Full => 1,
        }
    }
}

