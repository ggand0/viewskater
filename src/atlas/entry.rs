//use crate::core::Size;
//use crate::image::atlas;
use iced_core::Size;
use crate::atlas::{atlas, allocation};

#[derive(Debug, Clone)]
pub enum Entry {
    Contiguous(atlas::Allocation),
    Fragmented {
        size: Size<u32>,
        fragments: Vec<Fragment>,
    },
}

impl Entry {
    #[cfg(feature = "image")]
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
    pub allocation: atlas::Allocation,
}
