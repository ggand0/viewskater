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


use log::{debug, error};
use crate::pane;
use crate::image_cache::ImageCache;
use crate::loading_status::LoadingStatus;
use crate::image_cache::LoadOperationType;


pub fn handle_load_operation_all(
    panes: &mut Vec<pane::Pane>,
    loading_status: &mut LoadingStatus,
    pane_indices: &Vec<usize>,
    target_indices: Vec<Option<isize>>,
    image_data: Vec<Option<Vec<u8>>>,
    mut load_fn: Box<dyn FnMut(&mut ImageCache, Option<Vec<u8>>, isize) -> Result<bool, std::io::Error>>,
    operation_type: LoadOperationType,
) {
    loading_status.being_loaded_queue.pop_front();

    // Early return for Shift operations as they don't involve loading
    if matches!(operation_type, LoadOperationType::ShiftNext | LoadOperationType::ShiftPrevious) {
        return;
    }

    // Collect the target panes based on pane_indices
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

    debug!("panes_to_load.len(): {}", panes_to_load.len());

    for (pane_index, pane) in panes_to_load.iter_mut().enumerate() {
        let cache = &mut pane.img_cache;
        let target_index = match &target_indices[pane_index] {
            Some(index) => *index,
            None => {
                // Skip loading if the target index is not valid
                continue;
            }
        };

        // If the operation is blocking, skip the operation
        if cache.is_operation_blocking(operation_type.clone()) {
            return;
        }

        // Determine the target image to load based on operation type
        let target_image_to_load = match operation_type {
            LoadOperationType::LoadNext => Some(cache.get_next_image_to_load() as isize),
            LoadOperationType::LoadPrevious => Some(cache.get_prev_image_to_load() as isize),
            _ => None, // Should never reach this point due to the early return above
        };

        if let Some(target_image_to_load) = target_image_to_load {
            debug!(
                "IMAGES LOADED: target_image_to_load: {}, target_index: {}",
                target_image_to_load, target_index
            );

            // Skip if the cache offset is outside valid bounds
            if (operation_type == LoadOperationType::LoadNext && cache.current_offset > cache.cache_count as isize)
                || (operation_type == LoadOperationType::LoadPrevious && cache.current_offset < -(cache.cache_count as isize))
            {
                return;
            }

            // Perform the actual load if the target image matches
            if target_image_to_load == target_index {
                match load_fn(cache, image_data[pane_index].clone(), target_index) {
                    Ok(reload_current_image) => {
                        if reload_current_image {
                            let loaded_image = cache.get_initial_image().unwrap().to_vec();
                            let handle = iced::widget::image::Handle::from_memory(loaded_image.clone());
                            pane.current_image = handle;
                        }
                    }
                    Err(error) => {
                        error!("Error loading image: {}", error);
                    }
                }
            }

            debug!("IMAGES LOADED: cache_index: {}, current_offset: {}", -1, cache.current_offset);
        }
    }
}



pub fn handle_load_operation(
    panes: &mut Vec<pane::Pane>,
    c_index: isize,
    target_index: isize,
    image_data: Option<Vec<u8>>,
    mut load_fn: Box<dyn FnMut(&mut ImageCache, Option<Vec<u8>>, isize) -> Result<bool, std::io::Error>>,
    operation_type: LoadOperationType,
) {
    let pane = &mut panes[c_index as usize];
    let cache = &mut pane.img_cache;

    // Early return for Shift operations as they don't involve loading
    if matches!(operation_type, LoadOperationType::ShiftNext | LoadOperationType::ShiftPrevious) {
        return;
    }

    // Remove the first item from the loading queue
    cache.being_loaded_queue.pop_front();

    // Skip if the operation is blocking
    if cache.is_operation_blocking(operation_type.clone()) {
        return;
    }

    // Determine the target image to load based on operation type
    let target_image_to_load = match operation_type {
        LoadOperationType::LoadNext => cache.get_next_image_to_load() as isize,
        LoadOperationType::LoadPrevious => cache.get_prev_image_to_load() as isize,
        _ => return,  // This case shouldn't happen due to the early return for Shift operations
    };

    let is_matched = target_image_to_load == target_index;

    debug!(
        "IMAGE LOADED: target_image_to_load: {}, target_index: {}",
        target_image_to_load, target_index
    );
    debug!("load_operation: {:?}", operation_type);

    // Skip loading if the cache offset is outside valid bounds
    if (operation_type == LoadOperationType::LoadNext && cache.current_offset > cache.cache_count as isize)
        || (operation_type == LoadOperationType::LoadPrevious && cache.current_offset < -(cache.cache_count as isize)) 
    {
        return;
    }

    // If the target image matches, or if the target image is not found, load the image data
    if is_matched {
        match load_fn(cache, image_data, target_index) {
            Ok(reload_current_image) => {
                if reload_current_image {
                    let loaded_image = cache.get_initial_image().unwrap().to_vec();
                    let handle = iced::widget::image::Handle::from_memory(loaded_image.clone());
                    pane.current_image = handle;
                }
            }
            Err(error) => {
                error!("Error loading image: {}", error);
            }
        }
    }

    debug!(
        "IMAGE LOADED: cache_index: {}, current_offset: {}",
        c_index, cache.current_offset
    );
}
