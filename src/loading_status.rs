use crate::cache::img_cache::{LoadOperation, LoadOperationType};
use std::collections::VecDeque;
use crate::pane::Pane;
#[allow(unused_imports)]
use log::{Level, debug, info, warn, error};

#[derive(Debug, Clone, PartialEq)]
pub struct LoadingStatus {
    pub loading_queue: VecDeque<LoadOperation>,
    pub being_loaded_queue: VecDeque<LoadOperation>,    // Queue of image indices being loaded
    pub out_of_order_images: Vec<(usize, Vec<u8>)>,
    pub is_next_image_loaded: bool,                     // whether the next image in cache is loaded
    pub is_prev_image_loaded: bool,                     // whether the previous image in cache is loaded
}

impl Default for LoadingStatus {
    fn default() -> Self {
        Self::new()
    }
}

// Edited in the duplicated vscode worksapce window
impl LoadingStatus {
    pub fn new() -> Self {
        Self {
            loading_queue: VecDeque::new(),
            being_loaded_queue: VecDeque::new(),
            out_of_order_images: Vec::new(),
            is_next_image_loaded: false, // global flag, whether the next images in all the panes' cache are loaded
            is_prev_image_loaded: false,
        }
    }

    pub fn print_queue(&self) {
        debug!("loading_queue: {:?}", self.loading_queue);
        debug!("being_loaded_queue: {:?}", self.being_loaded_queue);
    }

    pub fn enqueue_image_load(&mut self, operation: LoadOperation) {
        self.loading_queue.push_back(operation);
    }

    pub fn reset_image_load_queue(&mut self) {
        self.loading_queue.clear();
    }

    pub fn enqueue_image_being_loaded(&mut self, operation: LoadOperation) {
        self.being_loaded_queue.push_back(operation);
    }

    pub fn reset_image_being_loaded_queue(&mut self) {
        self.being_loaded_queue.clear();
    }

    pub fn reset_load_next_queue_items(&mut self) {
        // Discard all queue items that are LoadNext or ShiftNext
        self.loading_queue.retain(|op| match op {
            LoadOperation::LoadNext(..) => false,
            LoadOperation::ShiftNext(..) => false,
            _ => true,
        });
    }
    pub fn reset_load_previous_queue_items(&mut self) {
        // Discard all queue items that are LoadPrevious or ShiftPrevious
        self.loading_queue.retain(|op| match op {
            LoadOperation::LoadPrevious(..) => false,
            LoadOperation::ShiftPrevious(..) => false,
            _ => true,
        });
    }


    #[allow(dead_code)]
    pub fn is_next_image_index_in_queue(&self, _cache_index: usize, next_image_index: isize) -> bool {
        // Check both the loading queue and being-loaded queue
        self.loading_queue.iter().all(|op| match op {
            LoadOperation::LoadNext((_c_index, img_indices)) 
            | LoadOperation::LoadPrevious((_c_index, img_indices))
            | LoadOperation::ShiftNext((_c_index, img_indices))
            | LoadOperation::ShiftPrevious((_c_index, img_indices)) => {
                !img_indices.contains(&Some(next_image_index))
            }
            LoadOperation::LoadPos((_c_index, target_indices_and_cache)) => {
                !target_indices_and_cache
                    .iter()
                    .any(|&item| item.map(|(index, _)| index == next_image_index).unwrap_or(false))
            }
        }) && self.being_loaded_queue.iter().all(|op| match op {
            LoadOperation::LoadNext((_c_index, img_indices)) 
            | LoadOperation::LoadPrevious((_c_index, img_indices))
            | LoadOperation::ShiftNext((_c_index, img_indices))
            | LoadOperation::ShiftPrevious((_c_index, img_indices)) => {
                !img_indices.contains(&Some(next_image_index))
            }
            LoadOperation::LoadPos((_c_index, target_indices_and_cache)) => {
                !target_indices_and_cache
                    .iter()
                    .any(|&item| item.map(|(index, _)| index == next_image_index).unwrap_or(false))
            }
        })
    }

    pub fn are_next_image_indices_in_queue(&self, next_image_indices: Vec<Option<isize>>) -> bool {
        self.loading_queue.iter().all(|op| match op {
            LoadOperation::LoadNext((_c_index, img_indices)) 
            | LoadOperation::LoadPrevious((_c_index, img_indices))
            | LoadOperation::ShiftNext((_c_index, img_indices))
            | LoadOperation::ShiftPrevious((_c_index, img_indices)) => {
                img_indices != &next_image_indices
            }
            LoadOperation::LoadPos((_c_index, target_indices_and_cache)) => {
                let extracted_indices: Vec<Option<isize>> = target_indices_and_cache
                    .iter()
                    .map(|item| item.map(|(index, _)| index))
                    .collect();
                extracted_indices != next_image_indices
            }
        }) && self.being_loaded_queue.iter().all(|op| match op {
            LoadOperation::LoadNext((_c_index, img_indices)) 
            | LoadOperation::LoadPrevious((_c_index, img_indices))
            | LoadOperation::ShiftNext((_c_index, img_indices))
            | LoadOperation::ShiftPrevious((_c_index, img_indices)) => {
                img_indices != &next_image_indices
            }
            LoadOperation::LoadPos((_c_index, target_indices_and_cache)) => {
                let extracted_indices: Vec<Option<isize>> = target_indices_and_cache
                    .iter()
                    .map(|item| item.map(|(index, _)| index))
                    .collect();
                extracted_indices != next_image_indices
            }
        })
    }

    /// If there are certain loading operations in the queue and the new loading op would cause bugs, return true
    /// e.g. When current_offset==5 and LoadPrevious op is at the head of the queue(queue.front()),
    /// the new op is LoadNext: this would make current_offset==6 and cache would be out of bounds
    pub fn is_blocking_loading_ops_in_queue(&self, panes: &mut Vec<&mut Pane>, loading_operation: LoadOperation) -> bool {
        for pane in panes {
            let img_cache = &pane.img_cache;
            if img_cache.is_blocking_loading_ops_in_queue(loading_operation.clone(), self) {
                return true;
            }
        }
        false
    }

    pub fn is_operation_in_queues(&self, operation: LoadOperationType) -> bool {
        self.loading_queue.iter().any(|op| op.operation_type() == operation) ||
        self.being_loaded_queue.iter().any(|op| op.operation_type() == operation)
    }
}