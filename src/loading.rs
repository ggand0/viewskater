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
                    LoadOperation::LoadPos((_, ref target_indices_and_cache)) => {
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
        for ((target_opt, image_data_opt)) in target_indices_and_cache.iter().zip(image_data.iter()) {
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
        LoadOperationType::LoadNext => Some(cache.get_next_image_to_load() as isize),
        LoadOperationType::LoadPrevious => Some(cache.get_prev_image_to_load() as isize),
        LoadOperationType::LoadPos => None,  // `LoadPos` needs to load the specified `target_index`
        _ => return,
    };

    let is_matched = target_image_to_load.map_or(false, |load| load == target_index);


    debug!(
        "IMAGE LOADED: target_image_to_load: {:?}, target_index: {}",
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
    if is_matched || matches!(operation_type, LoadOperationType::LoadPos) {
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
