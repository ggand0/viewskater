// Port of iced_wgpu's allocation.rs
// Represents an allocation within the atlas

use iced_core::Size;
use crate::slider_atlas::{allocator, ATLAS_SIZE};

#[derive(Debug, Clone)]
pub enum Allocation {
    Partial {
        layer: usize,
        region: allocator::Region,
    },
    Full {
        layer: usize,
    },
}

impl Allocation {
    pub fn position(&self) -> (u32, u32) {
        match self {
            Allocation::Partial { region, .. } => region.position(),
            Allocation::Full { .. } => (0, 0),
        }
    }

    pub fn size(&self) -> Size<u32> {
        match self {
            Allocation::Partial { region, .. } => region.size(),
            Allocation::Full { .. } => Size::new(ATLAS_SIZE, ATLAS_SIZE),
        }
    }

    pub fn layer(&self) -> usize {
        match self {
            Allocation::Partial { layer, .. } => *layer,
            Allocation::Full { layer } => *layer,
        }
    }
}

