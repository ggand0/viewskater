#[warn(unused_imports)]
#[cfg(target_os = "linux")]
mod other_os {
    pub use iced;
}

#[cfg(not(target_os = "linux"))]
mod macos {
    pub use iced_custom as iced;
}

#[cfg(target_os = "linux")]
use other_os::*;

#[cfg(not(target_os = "linux"))]
use macos::*;

#[allow(unused_imports)]
use log::{debug, error};
use crate::pane;
use crate::loading_status::LoadingStatus;
use crate::image_cache::LoadOperationType;
use crate::image_cache::LoadOperation;


pub fn handle_load_operation_all(
    panes: &mut Vec<pane::Pane>,
    loading_status: &mut LoadingStatus,
    pane_indices: &Vec<usize>,
    target_indices: Vec<Option<isize>>,
    image_data: Vec<Option<Vec<u8>>>,
    op: LoadOperation,  // Use the LoadOperation directly
    operation_type: LoadOperationType,
) {
    loading_status.being_loaded_queue.pop_front();

    // Early return for Shift operations as they don't involve loading
    if matches!(operation_type, LoadOperationType::ShiftNext | LoadOperationType::ShiftPrevious) {
        return;
    }

    let mut panes_to_load: Vec<&mut pane::Pane> = panes.iter_mut()
        .enumerate()
        .filter_map(|(pane_index, pane)| {
            if pane.dir_loaded && pane.is_selected && pane_indices.contains(&pane_index) {
                Some(pane)
            } else {
                None
            }
        })
        .collect();

    for (pane_index, pane) in panes_to_load.iter_mut().enumerate() {
        let cache = &mut pane.img_cache;
        let target_index = match &target_indices[pane_index] {
            Some(index) => *index,
            None => continue,
        };

        if cache.is_operation_blocking(operation_type.clone()) {
            return;
        }

        let target_image_to_load = match operation_type {
            LoadOperationType::LoadNext => Some(cache.get_next_image_to_load() as isize),
            LoadOperationType::LoadPrevious => Some(cache.get_prev_image_to_load() as isize),
            _ => None,
        };

        if let Some(target_image_to_load) = target_image_to_load {
            if target_image_to_load == target_index {
                match op {
                    LoadOperation::LoadNext(..) => {
                        cache.move_next(image_data[pane_index].clone(), target_index).unwrap();
                    }
                    LoadOperation::LoadPrevious(..) => {
                        cache.move_prev(image_data[pane_index].clone(), target_index).unwrap();
                    }
                    LoadOperation::ShiftNext(..) => {
                        cache.move_next_edge(image_data[pane_index].clone(), target_index).unwrap();
                    }
                    LoadOperation::ShiftPrevious(..) => {
                        cache.move_prev_edge(image_data[pane_index].clone(), target_index).unwrap();
                    }
                    LoadOperation::LoadPos((_, ref _target_indices_and_cache)) => {
                        // LoadPos is covered in `handle_load_pos_operation()`
                    }
                }
                // Reload current image if necessary
                let loaded_image = cache.get_initial_image().unwrap().to_vec();
                let handle = iced::widget::image::Handle::from_memory(loaded_image.clone());
                pane.current_image = handle;
            }
        }
    }
}


pub fn handle_load_pos_operation(
    panes: &mut Vec<pane::Pane>,
    loading_status: &mut LoadingStatus,
    pane_index: usize,
    target_indices_and_cache: Vec<Option<(isize, usize)>>,
    image_data: Vec<Option<Vec<u8>>>,
) {
    // Remove the current LoadPos operation from the being_loaded queue
    loading_status.being_loaded_queue.pop_front();

    // Access the pane that needs to update its cache and images
    if let Some(pane) = panes.get_mut(pane_index) {
        let cache = &mut pane.img_cache;

        // Iterate over the target indices and cache positions along with image data
        for (target_opt, image_data_opt) in target_indices_and_cache.iter().zip(image_data.iter()) {
            if let Some((target_index, cache_pos)) = target_opt {
                let target_index_usize = *target_index as usize;

                // Ensure that the target index is within valid bounds
                if target_index_usize < cache.image_paths.len() {
                    // Load the image data into the cache if available
                    if let Some(image) = image_data_opt {
                        // Store the loaded image data in the cache at the specified cache position
                        cache.cached_images[*cache_pos] = Some(image.clone());
                        cache.cached_image_indices[*cache_pos] = *target_index;

                        // If this is the current image, update the pane's current image
                        if cache.current_index == target_index_usize {
                            let loaded_image = cache.get_initial_image().unwrap().to_vec();
                            let handle = iced::widget::image::Handle::from_memory(loaded_image);
                            pane.current_image = handle;
                        }
                    } else {
                        debug!("No image data available for target index: {}", target_index);
                    }
                } else {
                    debug!("Target index {} is out of bounds", target_index);
                }
            }
        }
    }
}
