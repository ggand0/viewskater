#[warn(unused_imports)]
#[cfg(target_os = "linux")]
mod other_os {
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

use image;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
#[allow(unused_imports)]
use std::time::{Instant, Duration};
use once_cell::sync::Lazy;
use std::sync::Mutex;
use iced::widget::image::Handle;
use iced_wgpu::wgpu;
use iced::Task;
use std::io;
use crate::Arc;

use crate::pane;
use crate::cache::img_cache::{LoadOperation, load_all_images_in_queue};
use crate::widgets::shader::scene::Scene;
use crate::loading_status::LoadingStatus;
use crate::app::Message;
use crate::cache::img_cache::{CachedData, CacheStrategy};
use crate::cache::cache_utils::{load_image_resized_sync, create_gpu_texture};
use crate::file_io;

pub static LATEST_SLIDER_POS: AtomicUsize = AtomicUsize::new(0);

#[allow(dead_code)]
static LAST_SLIDER_LOAD: Lazy<Mutex<Instant>> = Lazy::new(|| Mutex::new(Instant::now()));

const _THROTTLE_INTERVAL_MS: u64 = 100; // Default throttle interval 

fn load_full_res_image(
    device: &Arc<wgpu::Device>,
    queue: &Arc<wgpu::Queue>,
    is_gpu_supported: bool,
    panes: &mut Vec<pane::Pane>,
    pane_index: isize,
    pos: usize,
) -> Task<Message> {
    debug!("load_full_res_image: Reloading full-resolution image at pos {}", pos);

    // Create a list of pane indices to process
    let pane_indices: Vec<usize> = if pane_index == -1 {
        // Process all panes with loaded directories
        panes.iter().enumerate()
            .filter_map(|(idx, pane)| if pane.dir_loaded { Some(idx) } else { None })
            .collect()
    } else {
        // Process only the specified pane
        vec![pane_index as usize]
    };

    // Process each pane in the list
    for idx in pane_indices {
        if let Some(pane) = panes.get_mut(idx) {
            let img_cache = &mut pane.img_cache;
            let img_path = match img_cache.image_paths.get(pos) {
                Some(path) => path.clone(),
                None => {
                    debug!("Image path missing for pos {} in pane {}", pos, idx);
                    continue;
                }
            };

            // Determine the target index inside cache array
            let target_index: usize;
            if pos < img_cache.cache_count {
                target_index = pos;
                img_cache.current_offset = -(img_cache.cache_count as isize - pos as isize);
            } else if pos >= img_cache.image_paths.len() - img_cache.cache_count {
                target_index = img_cache.cache_count + (img_cache.cache_count as isize - 
                              ((img_cache.image_paths.len()-1) as isize - pos as isize)) as usize;
                img_cache.current_offset = img_cache.cache_count as isize - 
                                         ((img_cache.image_paths.len()-1) as isize - pos as isize);
            } else {
                target_index = img_cache.cache_count;
                img_cache.current_offset = 0;
            }

            // Check if this pane has GPU support by checking if device and queue are available
            let has_gpu_support = is_gpu_supported && pane.device.is_some() && pane.queue.is_some();
            
            if has_gpu_support {
                // GPU-based loading
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
                    debug!("Failed to load full-res image {} for pane {}: {}", img_path.display(), idx, err);
                    continue;
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
            } else {
                // CPU-based loading
                // Load the full-resolution image using CPU
                match img_cache.load_image(pos) {
                    Ok(cached_data) => {
                        // Store in cache and update current image
                        img_cache.cached_data[target_index] = Some(cached_data.clone());
                        img_cache.cached_image_indices[target_index] = pos as isize;
                        img_cache.current_index = pos;
                        
                        // Update the currently displayed image
                        pane.current_image = cached_data.clone();
                        
                        // Update scene if using CPU-based cached data
                        if let CachedData::Cpu(_img) = &cached_data {
                            // Create a new scene with the CPU image
                            pane.scene = Some(Scene::new(Some(&cached_data)));
                            
                            // Ensure texture is created for the new scene if device/queue available
                            if let (Some(device), Some(queue)) = (&pane.device, &pane.queue) {
                                if let Some(scene) = &mut pane.scene {
                                    scene.ensure_texture(Arc::clone(device), Arc::clone(queue), pane.pane_id);
                                }
                            }
                        }
                    },
                    Err(err) => {
                        debug!("Failed to load CPU image for pane {}: {}", idx, err);
                        continue;
                    }
                }
            }

            debug!("Full-res image loaded successfully at pos {} for pane {}", pos, idx);
        }
    }

    Task::none()
}

fn get_loading_tasks_slider(
    device: &Arc<wgpu::Device>,
    queue: &Arc<wgpu::Queue>,
    _is_gpu_supported: bool,
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
        let local_tasks = load_all_images_in_queue(device, queue, CacheStrategy::Gpu, panes, loading_status);
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


// Async loading task for Image widget - updated to include pane_idx
pub async fn create_async_image_widget_task(
    img_paths: Vec<PathBuf>, 
    pos: usize,
    pane_idx: usize
) -> Result<(usize, usize, Handle), (usize, usize)> {
    // Check if position is valid
    if pos >= img_paths.len() {
        return Err((pane_idx, pos));
    }
    
    // Load image bytes directly without resizing
    match file_io::read_image_bytes(&img_paths[pos]) {
        Ok(bytes) => {
            // Convert directly to Handle without resizing
            let handle = iced::widget::image::Handle::from_bytes(bytes);
            Ok((pane_idx, pos, handle))
        },
        Err(_) => Err((pane_idx, pos)),
    }
}

pub fn update_pos(panes: &mut Vec<pane::Pane>, pane_index: isize, pos: usize, use_async: bool) -> Task<Message> {
    // Store the latest position in the atomic variable for reference
    LATEST_SLIDER_POS.store(pos, Ordering::SeqCst);
    
    // Platform-specific throttling - use different thresholds for Linux
    #[cfg(target_os = "linux")]
    const PLATFORM_THROTTLE_MS: u64 = 10; // Much lower for Linux/X11
    
    // TODO: Make this an option
    //#[cfg(not(target_os = "linux"))]
    //const PLATFORM_THROTTLE_MS: u64 = THROTTLE_INTERVAL_MS;
    
    // Throttling logic during rapid slider movement - With safety check
    #[cfg(target_os = "linux")]
    let should_process = {
        let mut last_load = LAST_SLIDER_LOAD.lock().unwrap();
        let now = Instant::now();
        debug!("##################################TIMESTAMP DEBUG - now: {:?}, last_load: {:?}", now, *last_load);
        
        // Enhanced safety check for time inconsistencies
        let elapsed = match now.checked_duration_since(*last_load) {
            Some(duration) => duration,
            None => {
                // System clock jumped backward or other timing inconsistency
                debug!("Time inconsistency detected in slider throttling - using default interval");
                
                // Update last_load to current time to avoid repeated issues
                *last_load = now;
                
                // Return a zero duration to ensure we process this event
                Duration::from_millis(PLATFORM_THROTTLE_MS)
            }
        };
        
        if elapsed.as_millis() >= PLATFORM_THROTTLE_MS as u128 {
            *last_load = now;
            true
        } else {
            false
        }
    };

    #[cfg(not(target_os = "linux"))]
    let should_process = true;
    
    // Skip processing if we're throttling
    if !should_process {
        debug!("Throttling slider image load at position {}", pos);
        return Task::none();
    }

    if use_async {
        // Collect tasks for all applicable panes
        let mut tasks = Vec::new();
        
        // Determine which panes to update
        let pane_indices: Vec<usize> = if pane_index == -1 {
            // Master slider - update all panes with loaded directories
            panes.iter().enumerate()
                .filter_map(|(idx, pane)| if pane.dir_loaded { Some(idx) } else { None })
                .collect()
        } else {
            // Individual pane slider - update only that pane
            vec![pane_index as usize]
        };
        
        // Create async image loading task for each pane
        for idx in pane_indices {
            if let Some(pane) = panes.get(idx) {
                if pane.dir_loaded && !pane.img_cache.image_paths.is_empty() {
                    debug!("#####################update_pos - Creating async image loading task for pane {}", idx);
                    let img_paths = pane.img_cache.image_paths.clone();
                    
                    // Create task for this pane
                    let pane_task = Task::perform(
                        create_async_image_widget_task(img_paths.clone(), pos, idx),
                        move |result| Message::SliderImageWidgetLoaded(result)
                    );
                    
                    tasks.push(pane_task);
                }
            }
        }
        
        // Return all tasks batched together
        if !tasks.is_empty() {
            return Task::batch(tasks);
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
                        scene.ensure_texture(Arc::clone(device), Arc::clone(queue), pane.pane_id);
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

            // Use the new method that ensures we get CPU data
            match img_cache.get_initial_image_as_cpu() {
                Ok(bytes) => {
                    pane.slider_image = Some(iced::widget::image::Handle::from_bytes(bytes));
                    Ok(())
                },
                Err(err) => {
                    debug!("Failed to get CPU image data for slider: {}", err);
                    
                    // Fallback: load directly from file
                    if let Some(img_path) = img_cache.image_paths.get(pos) {
                        match std::fs::read(img_path) {
                            Ok(bytes) => {
                                pane.slider_image = Some(iced::widget::image::Handle::from_bytes(bytes));
                                Ok(())
                            },
                            Err(err) => Err(io::Error::new(
                                io::ErrorKind::Other,
                                format!("Failed to read image file for slider: {}", err),
                            ))
                        }
                    } else {
                        Err(io::Error::new(
                            io::ErrorKind::NotFound,
                            "Image path not found for slider",
                        ))
                    }
                }
            }
        }
        Err(err) => {
            debug!("update_pos(): Error loading image: {}", err);
            Err(err)
        }
    }
}
