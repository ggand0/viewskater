#[warn(unused_imports)]
#[cfg(target_os = "linux")]
mod other_os {
    //pub use iced;
    pub use iced_custom as iced;
}

#[cfg(not(target_os = "linux"))]
mod macos {
    pub use iced_custom as iced;
}

#[cfg(target_os = "linux")]
use other_os::*;

#[cfg(not(target_os = "linux"))]
use macos::*;

use crate::pane;
use crate::cache::img_cache::{LoadOperation, LoadOperationType, load_images_by_operation, load_all_images_in_queue};
use crate::pane::{Pane, get_master_slider_value};
use crate::widgets::shader::scene::Scene;
use crate::menu::PaneLayout;
use crate::loading_status::LoadingStatus;
use crate::app::Message;
use iced::Task;
use std::io;
use crate::Arc;
use iced_wgpu::wgpu;
use crate::cache::img_cache::{CachedData, CacheStrategy};

#[allow(unused_imports)]
use log::{Level, debug, info, warn, error};


// Function to initialize image_load_state for all panes
fn init_is_next_image_loaded(panes: &mut Vec<&mut Pane>, _pane_layout: &PaneLayout, _is_slider_dual: bool) {
    for pane in panes.iter_mut() {
        pane.is_next_image_loaded = false;
        pane.is_prev_image_loaded = false;
    }
}
fn init_is_prev_image_loaded(panes: &mut Vec<&mut Pane>, _pane_layout: &PaneLayout, _is_slider_dual: bool) {
    for pane in panes.iter_mut() {
        pane.is_prev_image_loaded = false;
        pane.is_next_image_loaded = false;
    }
}


// Function to check if all images are loaded for all panes
fn are_all_next_images_loaded(panes: &mut Vec<&mut Pane>, is_slider_dual: bool, _loading_status: &mut LoadingStatus) -> bool {
    if is_slider_dual {
        panes
        .iter()
        .filter(|pane| pane.is_selected)  // Filter only selected panes
        .all(|pane| !pane.dir_loaded || (pane.dir_loaded && pane.is_next_image_loaded))
    } else {
        panes.iter().all(|pane| !pane.dir_loaded || (pane.dir_loaded && pane.is_next_image_loaded))
    }
}
fn are_all_prev_images_loaded(panes: &mut Vec<&mut Pane>, is_slider_dual: bool, _loading_status: &mut LoadingStatus) -> bool {
    if is_slider_dual {
        panes
        .iter()
        .filter(|pane| pane.is_selected)  // Filter only selected panes
        .all(|pane| !pane.dir_loaded || (pane.dir_loaded && pane.is_prev_image_loaded))
    } else {
        panes.iter().all(|pane| !pane.dir_loaded || (pane.dir_loaded && pane.is_prev_image_loaded))
    }
}

pub fn are_panes_cached_next(panes: &mut Vec<&mut Pane>, _pane_layout: &PaneLayout, _is_slider_dual: bool) -> bool {
    panes
    .iter()
    .filter(|pane| pane.is_selected)  // Filter only selected panes
    .all(|pane| pane.is_pane_cached_next())
}
pub fn are_panes_cached_prev(panes: &mut Vec<&mut Pane>, _pane_layout: &PaneLayout, _is_slider_dual: bool) -> bool {
    panes
    .iter()
    .filter(|pane| pane.is_selected)  // Filter only selected panes
    .all(|pane| pane.is_pane_cached_prev())
}


pub fn set_next_image_all(panes: &mut Vec<&mut Pane>, _pane_layout: &PaneLayout, is_slider_dual: bool) -> bool {
    let mut did_render_happen = false;

    // Set the next image for all panes
    for (_cache_index, pane) in panes.iter_mut().enumerate() {
        let render_happened = pane.set_next_image(_pane_layout, is_slider_dual);
        debug!("set_next_image_all - render_happened: {}", render_happened);

        if render_happened {
            did_render_happen = true;
        }
    }

    did_render_happen
}

pub fn set_prev_image_all(panes: &mut Vec<&mut Pane>, _pane_layout: &PaneLayout, is_slider_dual: bool) -> bool {//, loaindg_status: &mut LoadingStatus
    let mut did_render_happen = false;
    debug!("set_prev_image_all0");

    // First, check if the prev images of all panes are loaded.
    // If not, assume they haven't been loaded yet and wait for the next render cycle.
    // use if img_cache.is_some_at_index
    for (_cache_index, pane) in panes.iter_mut().enumerate() {
        let img_cache = &mut pane.img_cache;
        img_cache.print_cache();
        if !img_cache.is_some_at_index(0) {
            return false;
        }
    }
    debug!("set_prev_image_all1");

    // Set the prev image for all panes
    for (_cache_index, pane) in panes.iter_mut().enumerate() {
        let render_happened = pane.set_prev_image(_pane_layout, is_slider_dual);
        if render_happened {
            did_render_happen = true;
            debug!("set_prev_image_all2");
        }
    }

    did_render_happen
}

pub fn load_next_images_all(
    device: &Arc<wgpu::Device>,
    queue: &Arc<wgpu::Queue>,
    //is_gpu_supported: bool,
    cache_strategy: CacheStrategy,
    panes: &mut Vec<&mut Pane>,
    pane_indices: Vec<usize>,
    loading_status: &mut LoadingStatus,
    _pane_layout: &PaneLayout,
    _is_slider_dual: bool,
) -> Task<Message> {
    // The updated get_target_indices_for_next function now returns Vec<Option<isize>>
    let target_indices = get_target_indices_for_next(panes);
    debug!("load_next_images_all - target_indices: {:?}", target_indices);

    if target_indices.is_empty() {
        return Task::none();
    }

    // Updated calculate_loading_conditions_for_next to work with Vec<Option<isize>>
    debug!("load_next_images_all - target_indices: {:?}", target_indices);
    if let Some((next_image_indices_to_load, is_image_index_within_bounds, any_out_of_bounds)) =
        calculate_loading_conditions_for_next(panes, &target_indices)
    {
        // The LoadOperation::LoadNext variant now takes Vec<Option<isize>>
        let load_next_operation = LoadOperation::LoadNext((pane_indices.clone(), next_image_indices_to_load.clone()));
        debug!("load_next_images_all - next_image_indices_to_load: {:?}", next_image_indices_to_load);


        if should_enqueue_loading(
            is_image_index_within_bounds,
            loading_status,
            &next_image_indices_to_load,
            &load_next_operation,
            panes,
        ) {
            debug!("load_next_images_all - should_enqueue_loading passed  - any_out_of_bounds: {}", any_out_of_bounds);
            if any_out_of_bounds {
                // Now that we use the integration setup, can we disable this?
                loading_status.enqueue_image_load(LoadOperation::ShiftNext((
                    pane_indices,
                    target_indices.clone(),
                )));
                /**/
            } else {
                loading_status.enqueue_image_load(load_next_operation);
            }
            debug!("load_next_images_all - running load_images_by_operation()");
            return load_images_by_operation(
                //Some(Arc::clone(&device)), Some(Arc::clone(&queue)), is_gpu_supported,
                &device, &queue, cache_strategy,
                panes, loading_status);
        }
    }

    Task::none()
}


fn calculate_loading_conditions_for_next(
    panes: &Vec<&mut Pane>,
    target_indices: &Vec<Option<isize>>,
) -> Option<(Vec<Option<isize>>, bool, bool)> {
    let mut next_image_indices_to_load = Vec::new();
    let mut is_image_index_within_bounds = false;
    let mut any_out_of_bounds = false;

    for (i, pane) in panes.iter().enumerate() {
        let img_cache = &pane.img_cache;
        let current_index_before_render = img_cache.current_index - 1;

        if !img_cache.image_paths.is_empty() && current_index_before_render < img_cache.image_paths.len() - 1 {
            match target_indices[i] {
                Some(next_image_index_to_load) => {
                    if img_cache.is_image_index_within_bounds(next_image_index_to_load) {
                        is_image_index_within_bounds = true;
                    }
                    if next_image_index_to_load as usize >= img_cache.num_files || img_cache.current_offset < 0 {
                        any_out_of_bounds = true;
                    }
                    next_image_indices_to_load.push(Some(next_image_index_to_load));
                }
                None => {
                    any_out_of_bounds = true;
                    next_image_indices_to_load.push(None);
                }
            }
        } else {
            next_image_indices_to_load.push(None);
        }
    }
    debug!("calculate_loading_conditions_for_next - next_image_indices_to_load: {:?}", next_image_indices_to_load);
    debug!("calculate_loading_conditions_for_next - is_image_index_within_bounds: {}", is_image_index_within_bounds);

    if next_image_indices_to_load.is_empty() {
        None
    } else {
        Some((next_image_indices_to_load, is_image_index_within_bounds, any_out_of_bounds))
    }
}


fn get_target_indices_for_next(panes: &mut Vec<&mut Pane>) -> Vec<Option<isize>> {
    panes.iter_mut().map(|pane| {
        if !pane.is_selected || !pane.dir_loaded {
            // Use None to indicate that the pane is not selected or loaded
            None
        } else {
            let cache = &mut pane.img_cache;
            debug!("get_target_indices_for_next - current_index: {}, current_offset: {}, cache_count: {}", cache.current_index, cache.current_offset, cache.cache_count);
            Some(cache.current_index as isize - cache.current_offset + cache.cache_count as isize + 1)
        }
    }).collect()
}


pub fn load_prev_images_all(
    device: &Arc<wgpu::Device>,
    queue: &Arc<wgpu::Queue>,
    //is_gpu_supported: bool,
    cache_strategy: CacheStrategy,
    panes: &mut Vec<&mut Pane>,
    pane_indices: Vec<usize>,
    loading_status: &mut LoadingStatus,
    _pane_layout: &PaneLayout,
    _is_slider_dual: bool,
) -> Task<Message> {
    let target_indices = get_target_indices_for_previous(panes);

    // NOTE: target_indices.is_empty() would return true on [None]
    if target_indices.len() == 0 {
        return Task::none();
    }

    if let Some((prev_image_indices_to_load, is_image_index_within_bounds, any_none_index)) =
        calculate_loading_conditions_for_previous(panes, &target_indices)
    {
        let load_prev_operation = LoadOperation::LoadPrevious((pane_indices.clone(), prev_image_indices_to_load.clone()));

        if should_enqueue_loading(
            is_image_index_within_bounds,
            loading_status,
            &prev_image_indices_to_load,
            &load_prev_operation,
            panes,
        ) {
            if any_none_index {
                // Now that we use the integration setup, can we disable this??
                // Use ShiftPrevious if any index is out of bounds (`None`)
                loading_status.enqueue_image_load(LoadOperation::ShiftPrevious((pane_indices, target_indices)));
            } else {
                loading_status.enqueue_image_load(load_prev_operation);
            }
            return load_images_by_operation(
                //Some(Arc::clone(&device)), Some(Arc::clone(&queue)), is_gpu_supported,
                &device, &queue, cache_strategy,
                panes, loading_status);
        }
    }

    Task::none()
}


fn calculate_loading_conditions_for_previous(
    panes: &Vec<&mut Pane>,
    target_indices: &Vec<Option<isize>>,
) -> Option<(Vec<Option<isize>>, bool, bool)> {
    let mut prev_image_indices_to_load = Vec::new();
    let mut is_image_index_within_bounds = false;
    let mut any_none_index = false;

    for (i, pane) in panes.iter().enumerate() {
        let img_cache = &pane.img_cache;
        let current_index_before_render = img_cache.current_index + 1;

        if !img_cache.image_paths.is_empty() && current_index_before_render > 0 {
            match target_indices[i] {
                Some(prev_image_index_to_load) => {
                    if img_cache.is_image_index_within_bounds(prev_image_index_to_load) {
                        is_image_index_within_bounds = true;
                    }
                    prev_image_indices_to_load.push(Some(prev_image_index_to_load));
                }
                None => {
                    // If the index is out of bounds, mark as such
                    any_none_index = true;
                    is_image_index_within_bounds = true; // true because we need to enqueue Shift operations
                    prev_image_indices_to_load.push(None);
                }
            }
        } else {
            // If the pane has no images or current index is invalid, mark as `None`
            prev_image_indices_to_load.push(None);
        }
    }

    // NOTE: prev_image_indices_to_load.is_empty() would return true for [None]
    if prev_image_indices_to_load.len() == 0 {
        None
    } else {
        Some((prev_image_indices_to_load, is_image_index_within_bounds, any_none_index))
    }
}


fn should_enqueue_loading(
    is_image_index_within_bounds: bool,
    loading_status: &LoadingStatus,
    image_indices_to_load: &Vec<Option<isize>>,
    load_operation: &LoadOperation,
    panes: &mut Vec<&mut Pane>,
) -> bool {
    //debug!("should_enqueue_loading - is_image_index_within_bounds: {}", is_image_index_within_bounds);
    //debug!("should_enqueue_loading - are_next_image_indices_in_queue: {}", loading_status.are_next_image_indices_in_queue(image_indices_to_load.clone()));
    //debug!("should_enqueue_loading - is_blocking_loading_ops_in_queue: {}", loading_status.is_blocking_loading_ops_in_queue(panes, load_operation.clone()));
    is_image_index_within_bounds &&
        loading_status.are_next_image_indices_in_queue(image_indices_to_load.clone()) &&
        !loading_status.is_blocking_loading_ops_in_queue(panes, load_operation.clone())
}


fn get_target_indices_for_previous(panes: &mut Vec<&mut Pane>) -> Vec<Option<isize>> {
    panes.iter_mut().map(|pane| {
        if !pane.is_selected || !pane.dir_loaded {
            // Use None for panes that are not selected or not loaded
            None
        } else {
            let cache = &mut pane.img_cache;
            let target_index = (cache.current_index as isize + (-(cache.cache_count as isize) - cache.current_offset) as isize) - 1;
            if target_index < 0 {
                // Use None for out-of-bounds values
                None
            } else {
                // Valid target index
                Some(target_index)
            }
        }
    }).collect()
}



pub fn move_right_all(
    device: &Arc<wgpu::Device>,
    queue: &Arc<wgpu::Queue>,
    cache_strategy: CacheStrategy,
    panes: &mut Vec<pane::Pane>, 
    loading_status: &mut LoadingStatus,
    slider_value: &mut u16,
    pane_layout: &PaneLayout,
    is_slider_dual: bool,
    last_opened_pane: usize
) -> Task<Message> {
    debug!("##########MOVE_RIGHT_ALL()##########");

    // Prevent movement while LoadPos is still in the queue
    loading_status.print_queue();
    if loading_status.is_operation_in_queues(LoadOperationType::LoadPos) {
        debug!("move_right_all() - LoadPos operation in queue, skipping move_right_all()");
        return Task::none();
    }

    for pane in panes.iter_mut() {
        pane.print_state();
    }
    debug!("move_right_all() - loading_status.is_next_image_loaded: {:?}", loading_status.is_next_image_loaded);


    // 1. Filter active panes
    // Collect mutable references to the panes that haven't reached the edge
    let mut panes_to_load: Vec<&mut pane::Pane> = vec![];
    let mut indices_to_load: Vec<usize> = vec![];
    for (index, pane) in panes.iter_mut().enumerate() {
        if pane.is_selected && pane.dir_loaded && pane.img_cache.current_index < pane.img_cache.image_paths.len() - 1 {
            panes_to_load.push(pane);
            indices_to_load.push(index);
        }
    }
    if panes_to_load.len() == 0 {
        return Task::none();
    }


    // 2. Rendering preparation
    // If all panes have been rendered, start rendering the next image; reset is_next_image_loaded
    if are_all_next_images_loaded(&mut panes_to_load, is_slider_dual, loading_status) {
        debug!("move_right_all() - all next images loaded");
        init_is_next_image_loaded(&mut panes_to_load, pane_layout, is_slider_dual);
        loading_status.is_next_image_loaded = false;
    }

    let mut tasks = Vec::new();
    // Load next images for all panes concurrently
    // Use the representative pane to determine the loading conditions
    // file_io::load_image_async() loads the next images for all panes at the same time,
    // so we can assume that the rest of the panes have the same loading conditions as the representative pane.
    debug!("move_right_all() - PROCESSING");
    if !are_panes_cached_next(&mut panes_to_load, pane_layout, is_slider_dual) {
        debug!("move_right_all() - not all panes cached next, skipping...");
        loading_status.print_queue();

        // Since user tries to move the next image but image is not cached, enqueue loading the next image
        // Only do this when the loading queues don't have "Next" operations
        if !loading_status.is_operation_in_queues(LoadOperationType::LoadNext) ||
            !loading_status.is_operation_in_queues(LoadOperationType::ShiftNext)
        {
            tasks.push(load_next_images_all(
                //Some(Arc::clone(&self.device)), Some(Arc::clone(&self.queue)), self.is_gpu_supported,
                &device, &queue, cache_strategy,
                &mut panes_to_load, indices_to_load.clone(), loading_status, pane_layout, is_slider_dual));
        }

        // If panes already reached the edge, mark their is_next_image_loaded as true
        // We already picked the pane with the largest dir size, so we don't have to worry about the rest
        for (_cache_index, pane) in panes_to_load.iter_mut().enumerate() {
            if pane.img_cache.current_index == pane.img_cache.image_paths.len() - 1 {
                pane.is_next_image_loaded = true;
                loading_status.is_next_image_loaded = true;
            }
        }
    }

    debug!("move_right_all() - are_all_next_images_loaded(): {}", are_all_next_images_loaded(&mut panes_to_load, is_slider_dual, loading_status));
    debug!("move_right_all() - panes[0].is_next_image_loaded: {}", panes_to_load[0].is_next_image_loaded);
    if !are_all_next_images_loaded(&mut panes_to_load, is_slider_dual, loading_status) {
        //debug!("move_right_all() - setting next image...");
        let did_render_happen: bool = set_next_image_all(&mut panes_to_load, pane_layout, is_slider_dual);

        if did_render_happen {// NOTE: I may need to edit the condition here
            loading_status.is_next_image_loaded = true;
            for pane in panes_to_load.iter_mut() {
                pane.is_next_image_loaded = true;
            }

            //debug!("move_right_all() - loading next images...");
            tasks.push(load_next_images_all(
                //Some(Arc::clone(&self.device)), Some(Arc::clone(&self.queue)), self.is_gpu_supported,
                &device, &queue, cache_strategy,
                &mut panes_to_load, indices_to_load.clone(), loading_status, pane_layout, is_slider_dual));
        }
    }

    
    let did_new_render_happen = are_all_next_images_loaded(&mut panes_to_load, is_slider_dual, loading_status);

    // Update master slider when !is_slider_dual
    if did_new_render_happen && !is_slider_dual || *pane_layout == PaneLayout::SinglePane {
        // Use the current_index of the pane with largest dir size
        *slider_value = (get_master_slider_value(&mut panes_to_load, pane_layout, is_slider_dual, last_opened_pane)) as u16;
    }

    // print tasks
    //debug!("move_right_all() - tasks count: {}", tasks.len());

    Task::batch(tasks)
}


pub fn move_left_all(
    device: &Arc<wgpu::Device>,
    queue: &Arc<wgpu::Queue>,
    //is_gpu_supported: bool,
    cache_strategy: CacheStrategy,
    panes: &mut Vec<pane::Pane>,
    loading_status: &mut LoadingStatus,
    slider_value: &mut u16,
    pane_layout: &PaneLayout,
    is_slider_dual: bool,
    last_opened_pane: usize
) -> Task<Message> {
    debug!("##########MOVE_LEFT_ALL()##########");

    // Prevent movement while LoadPos is still in the queue
    if loading_status.is_operation_in_queues(LoadOperationType::LoadPos) {
        debug!("move_left_all() - LoadPos operation in queue, skipping move_right_all()");
        return Task::none();
    }

    // Collect mutable references to the panes that haven't reached the edge
    let mut panes_to_load: Vec<&mut pane::Pane> = vec![];
    let mut indices_to_load: Vec<usize> = vec![];
    
    for (index, pane) in panes.iter_mut().enumerate() {
        if pane.is_selected && pane.dir_loaded && pane.img_cache.current_index > 0 {
            panes_to_load.push(pane);
            indices_to_load.push(index);
        }
    }
    if panes_to_load.len() == 0 {
        return Task::none();
    }

    
    // If all panes have been rendered, start rendering the next(prev) image; reset is_next_image_loaded
    if are_all_prev_images_loaded(&mut panes_to_load, is_slider_dual, loading_status) {
        debug!("move_left_all() - all prev images loaded");
        for pane in panes_to_load.iter_mut() {
            pane.print_state();
        }
        init_is_prev_image_loaded(&mut panes_to_load, pane_layout, is_slider_dual);
        loading_status.is_prev_image_loaded = false;
    }

    let mut tasks = Vec::new();
    debug!("move_left_all() - PROCESSING");
    if !are_panes_cached_prev(&mut panes_to_load, pane_layout, is_slider_dual) {
        debug!("move_left_all() - not all panes cached prev, skipping...");
        loading_status.print_queue();
        debug!("move_left_all() - loading_status.is_operation_in_queues(LoadOperationType::LoadPrevious): {}", loading_status.is_operation_in_queues(LoadOperationType::LoadPrevious));
        debug!("move_left_all() - loading_status.is_operation_in_queues(LoadOperationType::ShiftPrevious): {}", loading_status.is_operation_in_queues(LoadOperationType::ShiftPrevious));
        // Since user tries to move the next image but image is not cached, enqueue loading the next image
        // Only do this when the loading queues don't have "Prev" operations
        if !loading_status.is_operation_in_queues(LoadOperationType::LoadPrevious) ||
            !loading_status.is_operation_in_queues(LoadOperationType::ShiftPrevious)
        {
            tasks.push(load_prev_images_all(
                //Some(Arc::clone(&self.device)), Some(Arc::clone(&self.queue)), self.is_gpu_supported,
                &device, &queue, cache_strategy,
                &mut panes_to_load, indices_to_load.clone(), loading_status, pane_layout, is_slider_dual));
        }
        // If panes already reached the edge, mark their is_next_image_loaded as true
        // We already picked the pane with the largest dir size, so we don't have to worry about the rest
        for (_cache_index, pane) in panes_to_load.iter_mut().enumerate() {
            if pane.img_cache.current_index == 0 {
                pane.is_prev_image_loaded = true;
                loading_status.is_prev_image_loaded = true;
            }
        }
    }


    debug!("move_left_all() - are_all_prev_images_loaded(): {}", are_all_prev_images_loaded(&mut panes_to_load, is_slider_dual, loading_status));
    debug!("move_left_all() - loading_status.is_prev_image_loaded: {}", loading_status.is_prev_image_loaded);
    
    if !are_all_prev_images_loaded(&mut panes_to_load, is_slider_dual, loading_status) {
        debug!("move_left_all() - setting prev image...");
        let did_render_happen: bool = set_prev_image_all(&mut panes_to_load, pane_layout, is_slider_dual);
        for pane in panes_to_load.iter_mut() {
            pane.print_state();
        }

        if did_render_happen {
            loading_status.is_prev_image_loaded = true;

            debug!("move_left_all() - loading prev images...");
            tasks.push(load_prev_images_all(
                //Some(Arc::clone(&self.device)), Some(Arc::clone(&self.queue)), self.is_gpu_supported,
                &device, &queue, cache_strategy,
                &mut panes_to_load, indices_to_load.clone(), loading_status, pane_layout, is_slider_dual));
        }
    }

    let did_new_render_happen = are_all_prev_images_loaded(&mut panes_to_load, is_slider_dual, loading_status);
    // Update master slider when !is_slider_dual
    if did_new_render_happen && !is_slider_dual || *pane_layout == PaneLayout::SinglePane {
        *slider_value = (get_master_slider_value(&mut panes_to_load, pane_layout, is_slider_dual, last_opened_pane) ) as u16;
    }

    Task::batch(tasks)
}
