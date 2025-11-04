// Port of iced_wgpu's entry.rs
// Represents an entry/allocation in the atlas that can be contiguous or fragmented

use iced_core::Size;
use crate::slider_atlas::Allocation;

#[derive(Debug, Clone)]
pub enum Entry {
    Contiguous(Allocation),
    Fragmented {
        size: Size<u32>,
        fragments: Vec<Fragment>,
    },
}

impl Entry {
    pub fn size(&self) -> Size<u32> {
        match self {
            Entry::Contiguous(allocation) => allocation.size(),
            Entry::Fragmented { size, .. } => *size,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Fragment {
    pub position: (u32, u32),
    pub allocation: Allocation,
}


