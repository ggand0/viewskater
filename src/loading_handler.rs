#[allow(unused_imports)]
use log::{debug, error, warn, info};
use crate::Arc;
use crate::pane;
use crate::loading_status::LoadingStatus;
use crate::cache::img_cache::{LoadOperation, LoadOperationType};
use crate::cache::img_cache::CachedData;
use crate::widgets::shader::scene::Scene;

pub fn handle_load_operation_all(
    panes: &mut Vec<pane::Pane>,
    loading_status: &mut LoadingStatus,
    pane_indices: &Vec<usize>,
    target_indices: Vec<Option<isize>>,
    image_data: Vec<Option<CachedData>>,
    op: LoadOperation,
    operation_type: LoadOperationType,
) {
    info!("Handling load operation");
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
        info!("Loading pane {}", pane_index);
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
                // Convert `Option<Vec<u8>>` to `Option<CachedData>`
                let mut converted_data = match image_data[pane_index].clone() {
                    Some(CachedData::Cpu(data)) => {
                        debug!("Cpu data");
                        Some(CachedData::Cpu(data))
                    }
                    Some(CachedData::Gpu(texture)) => {
                        debug!("Gpu texture");
                        Some(CachedData::Gpu(Arc::clone(&texture)))
                    }
                    Some(CachedData::BC1(texture)) => {
                        debug!("BC1 texture");
                        Some(CachedData::BC1(Arc::clone(&texture)))
                    }
                    None => None,
                };
                
                    
                match op {
                    LoadOperation::LoadNext(..) => {
                        cache.move_next(converted_data.take(), target_index).unwrap();
                    }
                    LoadOperation::LoadPrevious(..) => {
                        cache.move_prev(converted_data.take(), target_index).unwrap();
                    }
                    LoadOperation::ShiftNext(..) => {
                        cache.move_next_edge(converted_data.take(), target_index).unwrap();
                    }
                    LoadOperation::ShiftPrevious(..) => {
                        cache.move_prev_edge(converted_data.take(), target_index).unwrap();
                    }
                    LoadOperation::LoadPos((_, ref _target_indices_and_cache)) => {
                        // LoadPos is covered in `handle_load_pos_operation()`
                    }
                }

                // Reload current image if necessary
                if let Ok(cached_image) = cache.get_initial_image() {
                    match cached_image {
                        CachedData::Cpu(data) => {
                            info!("Setting CPU image as current_image");
                            pane.current_image = CachedData::Cpu(data.clone());
                            pane.scene = Some(Scene::new(Some(&CachedData::Cpu(data.clone()))));
                            
                            // Ensure texture is created immediately to avoid black screens
                            if let Some(device) = &pane.device {
                                if let Some(queue) = &pane.queue {
                                    if let Some(scene) = &mut pane.scene {
                                        debug!("Ensuring texture is created for loaded image");
                                        scene.ensure_texture(Arc::clone(device), Arc::clone(queue), pane.pane_id);
                                    }
                                }
                            } else {
                                warn!("Cannot create texture: device or queue not available");
                            }
                        }
                        CachedData::Gpu(texture) => {
                            debug!("Setting GPU texture as current_image");
                            pane.current_image = CachedData::Gpu(Arc::clone(&texture));
                            pane.scene = Some(Scene::new(Some(&CachedData::Gpu(Arc::clone(texture)))));
                        }
                        CachedData::BC1(texture) => {
                            info!("Setting BC1 compressed texture as current_image");
                            pane.current_image = CachedData::BC1(Arc::clone(texture));
                            pane.scene = Some(Scene::new(Some(&CachedData::BC1(Arc::clone(texture)))));
                            
                            // Ensure texture is created immediately to avoid black screens
                            if let Some(scene) = &mut pane.scene {
                                if let (Some(device), Some(queue)) = (&pane.device, &pane.queue) {
                                    scene.ensure_texture(Arc::clone(device), Arc::clone(queue), pane.pane_id);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}


pub fn handle_load_pos_operation(
    panes: &mut Vec<pane::Pane>,
    loading_status: &mut LoadingStatus,
    pane_index: usize,
    target_indices_and_cache: Vec<Option<(isize, usize)>>,
    image_data: Vec<Option<CachedData>>,
) {
    debug!("Handling LoadPos operation");
    // Remove the current LoadPos operation from the being_loaded queue
    loading_status.being_loaded_queue.pop_front();

    // Log (target_index, cache_pos) pairs
    let mut processed_indices: Vec<(isize, usize)> = Vec::new();

    // Access the pane that needs to update its cache and images
    if let Some(pane) = panes.get_mut(pane_index) {
        let cache = &mut pane.img_cache;

        // Iterate over the target indices and cache positions along with image data
        for (target_opt, image_data_opt) in target_indices_and_cache.iter().zip(image_data.iter()) {
            debug!("Target index and cache position: {:?}", target_opt);

            if let Some((target_index, cache_pos)) = target_opt {
                processed_indices.extend(target_opt.clone());
                let target_index_usize = *target_index as usize;

                // Ensure that the target index is within valid bounds
                if target_index_usize < cache.image_paths.len() {
                    // Load the image data into the cache if available
                    if let Some(image) = image_data_opt {
                        // Store the loaded image data in the cache at the specified cache position
                        match image {
                            CachedData::Cpu(data) => {
                                cache.set_cached_data(*cache_pos, CachedData::Cpu(data.clone()));
                            }
                            CachedData::Gpu(texture) => {
                                cache.set_cached_data(*cache_pos, CachedData::Gpu(Arc::clone(texture)));
                            }
                            CachedData::BC1(texture) => {
                                info!("Creating BC1 compressed texture for scene");
                                cache.set_cached_data(*cache_pos, CachedData::BC1(Arc::clone(texture)));
                            }
                        }
                        
                        if cache.current_index == target_index_usize {
                            // Reload current image if necessary
                            if let Ok(cached_image) = cache.get_initial_image() {
                                match cached_image {
                                    CachedData::Cpu(data) => {
                                        debug!("Setting CPU image as current_image");
                                        pane.current_image = CachedData::Cpu(data.clone());
                                        pane.scene = Some(Scene::new(Some(&CachedData::Cpu(data.clone()))));
                                    }
                                    CachedData::Gpu(texture) => {
                                        debug!("Setting GPU texture as current_image");
                                        pane.current_image = CachedData::Gpu(Arc::clone(&texture));
                                        pane.scene = Some(Scene::new(Some(&CachedData::Gpu(Arc::clone(texture)))));
                                    }
                                    CachedData::BC1(texture) => {
                                        info!("Setting BC1 compressed texture as current_image");
                                        pane.current_image = CachedData::BC1(Arc::clone(texture));
                                        pane.scene = Some(Scene::new(Some(&CachedData::BC1(Arc::clone(texture)))));
                                        
                                        // Ensure texture is created immediately to avoid black screens
                                        if let Some(scene) = &mut pane.scene {
                                            if let (Some(device), Some(queue)) = (&pane.device, &pane.queue) {
                                                scene.ensure_texture(Arc::clone(device), Arc::clone(queue), pane.pane_id);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        debug!("No image data available for target index: {}", target_index);
                    }
                } else {
                    debug!("Target index {} is out of bounds", target_index);
                }
            }
        }

        debug!("Processed indices: {:?}", processed_indices);
    }
}
