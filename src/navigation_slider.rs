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

#[allow(unused_imports)]
use log::{Level, debug, info, warn, error};

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
use crate::cache::cache_utils::{load_image_resized, load_image_resized_sync, create_gpu_texture};



fn load_full_res_image(
    device: &Arc<wgpu::Device>,
    queue: &Arc<wgpu::Queue>,
    is_gpu_supported: bool,
    panes: &mut Vec<pane::Pane>,
    pane_index: isize,
    pos: usize,
) -> Task<Message> {
    debug!("load_full_res_image: Reloading full-resolution image at pos {}", pos);

    let pane = if pane_index == -1 {
        panes.get_mut(0) // Apply to all panes if global slider
    } else {
        panes.get_mut(pane_index as usize)
    };

    if let Some(pane) = pane {
        let img_cache = &mut pane.img_cache;
        let img_path = match img_cache.image_paths.get(pos) {
            Some(path) => path.clone(),
            None => {
                debug!("Image path missing for pos {}", pos);
                return Task::none();
            }
        };

        // Get or create a texture
        let mut texture = img_cache.cached_data.get(pos)
            .and_then(|opt| opt.as_ref())
            .and_then(|cached| match cached {
                CachedData::Gpu(tex) => Some(tex.clone()),
                _ => None,
            })
            .unwrap_or_else(|| Arc::new(create_gpu_texture(device, 1, 1))); // Placeholder

        // Load the full-resolution image synchronously
        if let Err(err) = load_image_resized_sync(&img_path, false, device, queue, &mut texture) {
            debug!("Failed to load full-res image {}: {}", img_path.display(), err);
            return Task::none();
        }

        // Get the index inside cache array
        let target_index: usize;
        if pos < img_cache.cache_count {
            target_index = pos;
            img_cache.current_offset = -(img_cache.cache_count as isize - pos as isize);
        } else if pos >= img_cache.image_paths.len() - img_cache.cache_count {
            //target_index = img_cache.image_paths.len() - pos;
            target_index = img_cache.cache_count + (img_cache.cache_count as isize - ((img_cache.image_paths.len()-1) as isize - pos as isize)) as usize;
            img_cache.current_offset = img_cache.cache_count as isize - ((img_cache.image_paths.len()-1) as isize - pos as isize);
        } else {
            target_index = img_cache.cache_count;
            img_cache.current_offset = 0;
        }

        // Store the full-resolution texture in the cache
        let loaded_image = CachedData::Gpu(texture.clone().into());
        img_cache.cached_data[target_index] = Some(loaded_image.clone());
        img_cache.cached_image_indices[target_index] = pos as isize;
        img_cache.current_index = pos;

        // Update the currently displayed image
        pane.current_image = loaded_image;
        pane.scene = Some(Scene::new(Some(&CachedData::Gpu(Arc::clone(&texture))))); 
        pane.scene.as_mut().unwrap().update_texture(Arc::clone(&texture));

        debug!("Full-res image loaded successfully at pos {}", pos);
        return Task::none();
    }

    Task::none()
}



fn get_loading_tasks_slider(
    device: &Arc<wgpu::Device>,
    queue: &Arc<wgpu::Queue>,
    is_gpu_supported: bool,
    panes: &mut Vec<pane::Pane>,
    loading_status: &mut LoadingStatus,
    pane_index: usize,
    pos: usize,
) -> Vec<Task<Message>> {
    let mut tasks = Vec::new();

    if let Some(pane) = panes.get_mut(pane_index) {
        let img_cache = &pane.img_cache;
        let cache_count = img_cache.cache_count;
        let last_index = cache_count * 2 + 1;

        // Collect pairs of (image index, cache position)
        let mut target_indices_and_cache = Vec::new();

        // Example: Handling first cache window case
        if pos < cache_count {
            for i in 0..last_index {
                let image_index = i as isize;
                let cache_pos = i;
                target_indices_and_cache.push(Some((image_index, cache_pos)));
            }
        }
        // Example: Handling the last cache window case
        else if pos >= img_cache.image_paths.len() - cache_count {
            for i in 0..last_index {
                let image_index = (img_cache.image_paths.len() - last_index + i) as isize;
                let cache_pos = i;
                target_indices_and_cache.push(Some((image_index, cache_pos)));
            }
        }
        // Example: Default handling for neighboring images
        else {
            let center_index = cache_count;
            for i in 0..cache_count {
                let next_image_index = pos + i + 1;
                let prev_image_index = (pos as isize - i as isize - 1).max(0);

                // Enqueue neighboring images with cache positions
                if next_image_index < img_cache.image_paths.len() {
                    target_indices_and_cache.push(Some((next_image_index as isize, center_index + i + 1)));
                }
                if prev_image_index >= 0 {
                    target_indices_and_cache.push(Some((prev_image_index as isize, center_index - i - 1)));
                }
            }
        }

        // Enqueue the batched LoadPos operation with (image index, cache position) pairs
        let load_operation = LoadOperation::LoadPos((pane_index, target_indices_and_cache));
        loading_status.enqueue_image_load(load_operation);
        debug!("get_loading_tasks_slider - loading_status.loading_queue: {:?}", loading_status.loading_queue);
        loading_status.print_queue();

        // Generate loading tasks
        //let local_tasks = load_all_images_in_queue(panes, loading_status);
        let local_tasks = load_all_images_in_queue(device, queue, CacheStrategy::Gpu, panes, loading_status);

        // Convert `panes` into a vector of mutable references
        let mut pane_refs: Vec<&mut Pane> = panes.iter_mut().collect();

        // NOTE: temporary workaround to make it compile
        // Call the function with `pane_refs`
        /*let local_tasks = load_images_by_operation(
            device, queue, is_gpu_supported,
            &mut pane_refs, loading_status);*/
        //debug!("get_loading_tasks_slider - local_tasks.len(): {}", local_tasks.len());


        tasks.push(local_tasks);
    }

    debug!("get_loading_tasks_slider - loading_status addr: {:p}", loading_status);
    loading_status.print_queue();


    tasks
}


pub fn load_remaining_images(
    device: &Arc<wgpu::Device>,
    queue: &Arc<wgpu::Queue>,
    is_gpu_supported: bool,
    panes: &mut Vec<pane::Pane>,
    loading_status: &mut LoadingStatus,
    pane_index: isize,
    pos: usize,
) -> Task<Message> {
    // Clear the global loading queue
    loading_status.reset_image_load_queue();
    loading_status.reset_image_being_loaded_queue();


    let mut tasks = Vec::new();

    // 🔹 First, load the full-resolution image **synchronously**
    let full_res_task = load_full_res_image(device, queue, is_gpu_supported, panes, pane_index, pos);
    tasks.push(full_res_task); // Ensure it's executed first

    if pane_index == -1 {
        // Dynamic loading: load the central image synchronously, and others asynchronously
        let cache_indices: Vec<usize> = panes
            .iter()
            .enumerate()
            .filter_map(|(cache_index, pane)| if pane.dir_loaded { Some(cache_index) } else { None })
            .collect();

        for cache_index in cache_indices {
            let local_tasks = get_loading_tasks_slider(
                device, queue, is_gpu_supported,
                panes, loading_status, cache_index, pos);
            debug!("load_remaining_images - local_tasks.len(): {}", local_tasks.len());
            tasks.extend(local_tasks);
        }
    } else {
        if let Some(pane) = panes.get_mut(pane_index as usize) {
            if pane.dir_loaded {
                let local_tasks = get_loading_tasks_slider(
                    device, queue, is_gpu_supported,
                    panes, loading_status, pane_index as usize, pos);
                tasks.extend(local_tasks);
            } else {
                tasks.push(Task::none());
            }
        }
    }

    debug!("load_remaining_images - loading_status addr: {:p}", loading_status);

    loading_status.print_queue();
    debug!("load_remaining_images - tasks.len(): {}", tasks.len());

    Task::batch(tasks)
}


async fn load_current_slider_image(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pane: &Pane,
    pos: usize,
) -> Result<(usize, Arc<wgpu::Texture>), usize> {
    debug!("load_current_slider_image: Loading image at pos {}", pos);
    let img_cache = &pane.img_cache;

    // 🔹 Prevent redundant loads
    if img_cache.current_index == pos {
        return Ok((pos, img_cache.slider_texture.as_ref().unwrap().clone()));
    }

    //let img_path = img_cache.image_paths.get(pos).ok_or(pos)?;
    let img_path = match img_cache.image_paths.get(pos) {
        Some(path) => {
            debug!("load_current_slider_image: Loading image {}", path.display());
            path
        }
        None => {
            debug!("load_current_slider_image: Image path missing for position {}", pos);
            return Err(pos);
        }
    };

    // 🔹 Use the existing `slider_texture`
    let mut texture = match img_cache.slider_texture.clone() {
        Some(tex) => {
            debug!("load_current_slider_image: Using existing texture for {}", img_path.display());
            tex
        }
        None => {
            debug!("load_current_slider_image: slider_texture is None at pos {}", pos);
            return Err(pos);
        }
    };
    

    // 🔹 Load image asynchronously into the existing texture
    if let Err(err) = load_image_resized(img_path, true, device, queue, &mut texture).await {
        debug!("Failed to load image {}: {}", img_path.display(), err);
        return Err(pos);
    }

    Ok((pos, Arc::clone(&texture)))
}

pub fn update_pos(
    device: &Arc<wgpu::Device>,
    queue: &Arc<wgpu::Queue>,
    panes: &mut Vec<Pane>, // Immutable
    pane_index: isize,
    pos: usize,
) -> Task<Message> {
    let is_slider_move = true;

    // Prevent excessive enqueuing
    const MAX_QUEUE_SIZE: usize = 3;
    if panes[0].img_cache.loading_queue_slider.len() > MAX_QUEUE_SIZE {
        panes[0].img_cache.loading_queue_slider.pop_front();
    }
    panes[0].img_cache.loading_queue_slider.push_back(pos);

    let device_clone = Arc::clone(device);
    let queue_clone = Arc::clone(queue);

    // Extract only the required data from ImageCache to avoid `Send` errors
    let img_cache = panes.get(0).map(|pane| {
        (
            pane.img_cache.image_paths.clone(), // Clone only image paths
            pane.img_cache.slider_texture.clone(), // Clone Arc<wgpu::Texture>
        )
    });

    debug!("Task::perform started for slider pos {}", pos);

    let images_loading_task = async move {
        //debug!("update_pos: async block");
        //debug!("img_cache: {:?}", img_cache);
        match img_cache {
            Some((image_paths, texture)) => {
                //debug!("update_pos: (image_paths, texture) block");
                if let Some(texture) = texture {
                    //debug!("update_pos: texture block");
                    let img_path = image_paths.get(pos).ok_or(pos)?;
                    let mut texture_clone = texture.clone();

                    if let Err(err) = load_image_resized(img_path, true, &device_clone, &queue_clone, &mut texture_clone).await {
                        debug!("Failed to load image {}: {}", img_path.display(), err);
                        return Err(pos);
                    }
                    Ok((pos, texture))
                } else {
                    Err(pos) // If no texture, return error
                }
            }
            None => Err(pos), // If no pane exists, return error
        }
    };

    Task::perform(images_loading_task, Message::SliderImageLoaded) // Now fully Send-safe
}
