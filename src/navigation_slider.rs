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
use image;
use std::path::PathBuf;
use crate::cache::img_cache::ImageCache;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Instant, Duration};
use std::collections::HashMap;
use once_cell::sync::Lazy;
use std::sync::Mutex;
use image::GenericImageView;

pub static LATEST_SLIDER_POS: AtomicUsize = AtomicUsize::new(0);

// Format duration in a human-readable way
fn format_duration(duration: Duration) -> String {
    if duration.as_millis() < 10 {
        format!("{:.2}μs", duration.as_micros() as f64)
    } else if duration.as_millis() < 1000 {
        format!("{:.2}ms", duration.as_millis() as f64)
    } else {
        format!("{:.2}s", duration.as_secs_f64())
    }
}

// Add these constants at the module level
const THROTTLE_INTERVAL_MS: u64 = 5; // Minimum ms between image loads during sliding
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

/*
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
*/



fn load_slider_image_sync(
    device: &Arc<wgpu::Device>,
    queue: &Arc<wgpu::Queue>,
    panes: &mut Vec<Pane>,
    pane_index: isize,
    pos: usize,
    cache_strategy: CacheStrategy,
) -> Result<(usize, CachedData), usize> {
    // Get the pane
    let pane = if pane_index == -1 {
        // Global slider affects all panes, use the first one as reference
        panes.get_mut(0)
    } else {
        panes.get_mut(pane_index as usize)
    };

    let pane = match pane {
        Some(p) => p,
        None => return Err(pos),
    };

    let img_cache = &mut pane.img_cache;
    let img_path = match img_cache.image_paths.get(pos) {
        Some(path) => path,
        None => return Err(pos),
    };

    // Load the image synchronously based on strategy
    match cache_strategy {
        CacheStrategy::Gpu => {
            // Get or initialize texture
            let mut texture = match &img_cache.slider_texture {
                Some(tex) => Arc::clone(tex),
                None => {
                    // Create a new texture
                    let tex = Arc::new(create_gpu_texture(device, 1, 1));
                    img_cache.slider_texture = Some(Arc::clone(&tex));
                    tex
                }
            };

            // Load image into texture synchronously
            if let Err(err) = load_image_resized_sync(img_path, true, device, queue, &mut texture) {
                debug!("Failed to load image synchronously {}: {}", img_path.display(), err);
                return Err(pos);
            }

            // Update indices in the cache
            update_slider_cache_indices(img_cache, pos);
            
            // Return the loaded texture
            Ok((pos, CachedData::Gpu(texture)))
        },
        CacheStrategy::Cpu => {
            // Load image into memory synchronously
            match image::open(img_path) {
                Ok(img) => {
                    // Resize if needed
                    let img = img.resize(800, 600, image::imageops::FilterType::Triangle);
                    let mut bytes: Vec<u8> = Vec::new();
                    
                    if let Err(err) = img.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageOutputFormat::Png) {
                        debug!("Failed to encode image {}: {}", img_path.display(), err);
                        return Err(pos);
                    }
                    
                    // Update indices in the cache
                    update_slider_cache_indices(img_cache, pos);
                    
                    Ok((pos, CachedData::Cpu(bytes)))
                },
                Err(err) => {
                    debug!("Failed to open image {}: {}", img_path.display(), err);
                    Err(pos)
                }
            }
        },
        _ => {
            debug!("Atlas strategy not supported for synchronous slider loading");
            Err(pos)
        }
    }
}

fn update_slider_cache_indices(img_cache: &mut ImageCache, pos: usize) {
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
}


fn create_async_loading_task(
    image_paths: Vec<PathBuf>,
    pos: usize,
    resize: bool,
) -> impl std::future::Future<Output = Result<(usize, CachedData), usize>> {
    async move {
        let img_path = match image_paths.get(pos) {
            Some(path) => path,
            None => return Err(pos),
        };

        // Load the image asynchronously
        match image::open(img_path) {
            Ok(img) => {
                let img = if resize {
                    // Get original dimensions
                    let (orig_width, orig_height) = img.dimensions();
                    
                    // Define maximum dimensions
                    let max_width = 1920;
                    let max_height = 1080;
                    
                    // Only resize if larger than our target size
                    if orig_width > max_width || orig_height > max_height {
                        // Calculate scaling factors
                        let width_scale = max_width as f32 / orig_width as f32;
                        let height_scale = max_height as f32 / orig_height as f32;
                        
                        // Use the smaller scaling factor to ensure the image fits
                        let scale = width_scale.min(height_scale);
                        
                        // Calculate new dimensions while preserving aspect ratio
                        let new_width = (orig_width as f32 * scale) as u32;
                        let new_height = (orig_height as f32 * scale) as u32;
                        
                        debug!("Resizing image from {}x{} to {}x{}", 
                              orig_width, orig_height, new_width, new_height);
                        
                        img.resize(new_width, new_height, image::imageops::FilterType::Triangle)
                    } else {
                        // Already within size limits, use original
                        debug!("Using original image size: {}x{}", orig_width, orig_height);
                        img
                    }
                } else {
                    // No resize requested
                    img
                };
                
                let mut bytes: Vec<u8> = Vec::new();
                
                if let Err(err) = img.write_to(
                    &mut std::io::Cursor::new(&mut bytes),
                    image::ImageOutputFormat::Png
                ) {
                    debug!("Failed to encode image {}: {}", img_path.display(), err);
                    return Err(pos);
                }
                
                Ok((pos, CachedData::Cpu(bytes)))
            },
            Err(err) => {
                debug!("Failed to open image {}: {}", img_path.display(), err);
                Err(pos)
            }
        }
    }
}

pub fn update_pos(panes: &mut Vec<pane::Pane>, pane_index: isize, pos: usize, use_async: bool) -> Task<Message> {
    // Store the latest position in the atomic variable for reference
    LATEST_SLIDER_POS.store(pos, Ordering::SeqCst);
    
    // Throttling logic during rapid slider movement
    let should_process = {
        let mut last_load = LAST_SLIDER_LOAD.lock().unwrap();
        let now = Instant::now();
        let elapsed = now.duration_since(*last_load);
        
        if elapsed.as_millis() >= THROTTLE_INTERVAL_MS as u128 {
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

    if !use_async {
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
    } else {
        // Modified async loading
        if pane_index == -1 {
            // Only load for the first pane during sliding to reduce load
            let pane = match panes.get(0) {
                Some(p) => p,
                None => return Task::none(),
            };
            
            if pane.dir_loaded && !pane.img_cache.image_paths.is_empty() {
                let img_cache = pane.img_cache.image_paths.clone();
                let resize = true;
                
                Task::perform(
                    create_async_loading_task(img_cache, pos, resize),
                    |result| Message::SliderImageLoaded(result)
                )
            } else {
                Task::none()
            }
        } else {
            // ... existing single pane async loading ...
            // (keep your current implementation for this branch)
            Task::none()
        }
    }
}

// Result<(usize, Arc<wgpu::Texture>), usize> {
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
            
            info!("loaded_image: {:?}", loaded_image.len());
            pane.slider_image = Some(iced::widget::image::Handle::from_bytes(loaded_image));

            Ok(())
        }
        Err(err) => {
            //debug!("update_pos(): Error loading image: {}", err);
            Err(err)
        }
    }
}
