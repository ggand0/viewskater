use log::{debug, error};
use std::collections::VecDeque;
use crate::pane;
use crate::image_cache::ImageCache;
use crate::loading_status::LoadingStatus;
use crate::image_cache::LoadOperationType;


pub fn handle_load_operation_all(
    panes: &mut Vec<pane::Pane>,
    loading_status: &mut LoadingStatus,
    pane_indices: &Vec<usize>,
    target_indices: Vec<isize>,
    image_data: Vec<Option<Vec<u8>>>,
    mut load_fn: Box<dyn FnMut(&mut ImageCache, Option<Vec<u8>>, isize) -> Result<bool, std::io::Error>>,
    operation_type: LoadOperationType,
) {
    // Get all image_cache from panes that have dir_loaded and is_selected
    let mut _img_caches: Vec<ImageCache> = Vec::new();
    let _ = loading_status.being_loaded_queue.pop_front();

    // Collect the target panes based on pane_indices
    let mut panes_to_load: Vec<&mut pane::Pane> = Vec::new();
    for (pane_index, pane) in panes.iter_mut().enumerate() {
        if !pane.dir_loaded || !pane.is_selected {
            continue;
        }
        if pane_indices.contains(&pane_index) {
            panes_to_load.push(pane);
        }
    }

    debug!("panes_to_load.len(): {}", panes_to_load.len());

    for (pane_index, pane) in panes_to_load.iter_mut().enumerate() {
        if !pane.dir_loaded || !pane.is_selected {
            continue;
        }

        debug!("handle_load_operation_all0");
        let cache = &mut pane.img_cache;
        let target_index = target_indices[pane_index];

        if cache.is_operation_blocking(operation_type.clone()) {
            // If the operation is blocking, skip the operation
            return;
        }

        debug!("handle_load_operation_all1");
        let target_image_to_load: isize = if operation_type == LoadOperationType::LoadNext {
            cache.get_next_image_to_load() as isize
        } else if operation_type == LoadOperationType::LoadPrevious {
            cache.get_prev_image_to_load() as isize
        } else {
            -99
        };
        let _target_image_to_load_usize = target_image_to_load as usize;
        let is_matched = target_image_to_load == target_index;

        debug!("IMAGES LOADED: target_image_to_load: {}, target_index: {}", target_image_to_load, target_index);
        debug!("load_operation: {:?}", operation_type);

        if target_image_to_load == -99 || is_matched {
            if (operation_type == LoadOperationType::LoadNext || operation_type == LoadOperationType::ShiftNext)
                && cache.current_offset > cache.cache_count as isize
                || (operation_type == LoadOperationType::LoadPrevious || operation_type == LoadOperationType::ShiftPrevious)
                && cache.current_offset < -(cache.cache_count as isize)
            {
                return;
            }

            debug!("handle_load_operation_all2");
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


pub fn handle_load_operation(
    panes: &mut Vec<pane::Pane>,
    c_index: isize,
    target_index: isize,
    image_data: Option<Vec<u8>>,
    mut load_fn: Box<dyn FnMut(&mut ImageCache, Option<Vec<u8>>, isize) -> Result<bool, std::io::Error>>,
    operation_type: LoadOperationType,
) {
    let pane = &mut panes[c_index as usize];

    // TODO: Refactor this part
    let mut img_cache = None;
    img_cache.replace(&mut pane.img_cache);
    let cache_index = c_index;

    if let Some(cache) = img_cache.as_mut() {
        let _ = cache.being_loaded_queue.pop_front();

        if cache.is_operation_blocking(operation_type.clone()) {
            // If the operation is blocking, skip the operation
            return;
        }

        // Check if the next image that is supposed to be loaded matches target_index
        // If not, add image_data to out_of_order_images
        // If it does not match and if the matching image is in out_of_order_images, load it
        // If it matches, load `image_data`
        let target_image_to_load: isize = if operation_type == LoadOperationType::LoadNext {
            cache.get_next_image_to_load() as isize
        } else if operation_type == LoadOperationType::LoadPrevious {
            cache.get_prev_image_to_load() as isize
        } else {
            -99
        };
        let is_matched = target_image_to_load == target_index;

        debug!(
            "IMAGE LOADED: target_image_to_load: {}, target_index: {}",
            target_image_to_load, target_index
        );
        debug!("load_operation: {:?}", operation_type);

        // [1] LOADNEXT
        // 1. If it matches, load `image_data`
        let _last_index = cache.cached_image_indices[cache.cached_image_indices.len() - 1];
        if target_image_to_load == -99 || is_matched {
            // If somehow the LoadNext is called when current_offset is at the right end, skip loading
            // The same goes for LoadPrevious
            if (operation_type == LoadOperationType::LoadNext || operation_type == LoadOperationType::ShiftNext)
                && cache.current_offset > cache.cache_count as isize
                || (operation_type == LoadOperationType::LoadPrevious || operation_type == LoadOperationType::ShiftPrevious)
                && cache.current_offset < -(cache.cache_count as isize)
            {
                return;
            }

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
            cache_index, cache.current_offset
        );
    }
}
