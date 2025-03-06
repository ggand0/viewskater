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

use iced_graphics::image::image_rs::DynamicImage;
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
use image;
use std::path::PathBuf;
use crate::cache::img_cache::ImageCache;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Instant, Duration};
use std::collections::HashMap;
use once_cell::sync::Lazy;
use std::sync::Mutex;
use image::GenericImageView;
use crate::file_io;
use iced::widget::image::Handle;

pub static LATEST_SLIDER_POS: AtomicUsize = AtomicUsize::new(0);
static LAST_SLIDER_LOAD: Lazy<Mutex<Instant>> = Lazy::new(|| Mutex::new(Instant::now()));

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


// Async loading task for Image widget
pub async fn create_async_image_widget_task(
    img_paths: Vec<PathBuf>, 
    pos: usize
) -> Result<(usize, Handle), usize> {
    // Check if position is valid
    if pos >= img_paths.len() {
        return Err(pos);
    }
    
    // Load image bytes directly without resizing
    match file_io::read_image_bytes(&img_paths[pos]) {
        Ok(bytes) => {
            // Convert directly to Handle without resizing
            let handle = iced::widget::image::Handle::from_bytes(bytes);
            Ok((pos, handle))
        },
        Err(_) => Err(pos),
    }
}

pub fn update_pos(panes: &mut Vec<pane::Pane>, pane_index: isize, pos: usize, use_async: bool) -> Task<Message> {
    // Store the latest position in the atomic variable for reference
    LATEST_SLIDER_POS.store(pos, Ordering::SeqCst);
    
    // Platform-specific throttling - use different thresholds for Linux
    #[cfg(target_os = "linux")]
    const PLATFORM_THROTTLE_MS: u64 = 10; // Much lower for Linux/X11
    
    #[cfg(not(target_os = "linux"))]
    const PLATFORM_THROTTLE_MS: u64 = THROTTLE_INTERVAL_MS;
    
    // Throttling logic during rapid slider movement
    let should_process = {
        let mut last_load = LAST_SLIDER_LOAD.lock().unwrap();
        let now = Instant::now();
        let elapsed = now.duration_since(*last_load);
        
        if elapsed.as_millis() >= PLATFORM_THROTTLE_MS as u128 {
            *last_load = now;
            true
        } else {
            false
        }
    };
    
    // Skip processing if we're throttling
    if !should_process {
        debug!("Throttling slider image load at position {}", pos);
        return Task::none();
    }

    // Always use async on Linux for better responsiveness
    #[cfg(target_os = "linux")]
    let use_async = true;

    if use_async {
        // Simplified approach: always use pane_index = -1 for both master slider and individual panes
        // Get the appropriate pane based on pane_index
        let pane = if pane_index == -1 {
            // Master slider - use first pane
            match panes.get(0) {
                Some(p) => p,
                None => return Task::none(),
            }
        } else {
            // Individual pane slider
            match panes.get(pane_index as usize) {
                Some(p) => p,
                None => return Task::none(),
            }
        };
        
        if pane.dir_loaded && !pane.img_cache.image_paths.is_empty() {
            let img_paths = pane.img_cache.image_paths.clone();
            
            // Use the async image loading task with SliderImageWidgetLoaded for all cases
            return Task::perform(
                create_async_image_widget_task(img_paths, pos),
                |result| Message::SliderImageWidgetLoaded(result)
            );
        }
        
        Task::none()
    } else {
        if pane_index == -1 {
            // Perform dynamic loading:
            // Load the image at pos (center) synchronously,
            // and then load the rest of the images within the cache window asynchronously
            let mut tasks = Vec::new();
            for (cache_index, pane) in panes.iter_mut().enumerate() {
                if pane.dir_loaded {
                    //match load_current_slider_image(pane, pos) {
                    match load_current_slider_image_widget(pane, pos) {
                        Ok(()) => {
                            debug!("update_pos - Image loaded successfully for pane {}", cache_index);
                        }
                        Err(err) => {
                            debug!("update_pos - Error loading image for pane {}: {}", cache_index, err);
                        }
                    }
                } else {
                    tasks.push(Task::none());
                }
            }
            Task::batch(tasks)

        } else {
            let pane_index = pane_index as usize;
            let pane = &mut panes[pane_index];
            if pane.dir_loaded {
                //match load_current_slider_image(pane, pos) {
                match load_current_slider_image_widget(pane, pos) {
                    Ok(()) => {
                        debug!("update_pos - Image loaded successfully for pane {}", pane_index);
                    }
                    Err(err) => {
                        debug!("update_pos - Error loading image for pane {}: {}", pane_index, err);
                    }
                }
            }
            Task::none()
        }
    }
}


#[allow(dead_code)]
/// Loads the image at pos synchronously into the cache using CpuScene
fn load_current_slider_image(pane: &mut pane::Pane, pos: usize) -> Result<(), io::Error> {
    // Load the image at pos synchronously 
    let img_cache = &mut pane.img_cache;
    
    // Update indices in the cache
    let target_index: usize;
    if pos < img_cache.cache_count {
        target_index = pos;
        img_cache.current_offset = -(img_cache.cache_count as isize - pos as isize);
    } else if pos >= img_cache.image_paths.len() - img_cache.cache_count {
        target_index = img_cache.cache_count + (img_cache.cache_count as isize - ((img_cache.image_paths.len()-1) as isize - pos as isize)) as usize;
        img_cache.current_offset = img_cache.cache_count as isize - ((img_cache.image_paths.len()-1) as isize - pos as isize);
    } else {
        target_index = img_cache.cache_count;
        img_cache.current_offset = 0;
    }
    
    img_cache.cached_image_indices[target_index] = pos as isize;
    img_cache.current_index = pos;
    
    // Get direct access to the image file for CPU loading
    let img_path = match img_cache.image_paths.get(pos) {
        Some(path) => path,
        None => return Err(io::Error::new(io::ErrorKind::NotFound, "Image path not found")),
    };
    
    // Always load from file directly for best slider performance
    match image::open(img_path) {
        Ok(img) => {
            // Resize the image to smaller dimensions for slider
            /*let resized = img.resize(
                800, // Width for slider
                600, // Height for slider
                image::imageops::FilterType::Triangle
            );*/
            
            // Create the CPU bytes
            let mut bytes: Vec<u8> = Vec::new();
            if let Err(err) = img.write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageOutputFormat::Png
            ) {
                debug!("Failed to encode slider image: {}", err);
                return Err(io::Error::new(io::ErrorKind::Other, "Failed to encode image"));
            }
            
            // Update the current image to CPU data
            pane.current_image = CachedData::Cpu(bytes.clone());
            pane.slider_scene = Some(Scene::new(Some(&CachedData::Cpu(bytes.clone()))));
        
            // Ensure texture is created for CPU images
            if let Some(device) = &pane.device {
                if let Some(queue) = &pane.queue {
                    if let Some(scene) = &mut pane.slider_scene {
                        scene.ensure_texture(Arc::clone(device), Arc::clone(queue));
                    }
                }
            }
            
            Ok(())
        },
        Err(err) => {
            debug!("Failed to open image for slider: {}", err);
            Err(io::Error::new(io::ErrorKind::Other, format!("Failed to open image: {}", err)))
        }
    }
}


/// Loads the image at pos synchronously into the cache using Iced's image widget
fn load_current_slider_image_widget(pane: &mut pane::Pane, pos: usize ) -> Result<(), io::Error> {
    // Load the image at pos synchronously into the center position of cache
    // Assumes that the image at pos is already in the cache
    let img_cache = &mut pane.img_cache;
    match img_cache.load_image(pos as usize) {
        Ok(image) => {
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
            img_cache.cached_data[target_index] = Some(image);
            img_cache.cached_image_indices[target_index] = pos as isize;
            img_cache.current_index = pos;

            let loaded_image = img_cache.get_initial_image().unwrap().as_vec().unwrap();
            pane.slider_image = Some(iced::widget::image::Handle::from_bytes(loaded_image));
            
            /*let img_path = match img_cache.image_paths.get(pos) {
                Some(path) => path,
                None => return Err(io::Error::new(io::ErrorKind::NotFound, "Image path not found")),
            };

            //let bytes = resize_and_load_image(img_path, true).unwrap();
            //pane.slider_image = Some(iced::widget::image::Handle::from_bytes(bytes));

            let (img, orig_width, orig_height) = resize_and_load_image(img_path, true).unwrap();
            info!("slider image size: {}x{}", orig_width, orig_height);
            let rgba = img.to_rgba8();
            let raw_bytes = rgba.into_raw();
            pane.slider_image = Some(iced::widget::image::Handle::from_rgba(orig_width, orig_height, raw_bytes));*/

            Ok(())
        }
        Err(err) => {
            //debug!("update_pos(): Error loading image: {}", err);
            Err(err)
        }
    }
}
