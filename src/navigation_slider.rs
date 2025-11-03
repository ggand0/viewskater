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
use log::{Level, trace, debug, info, warn, error};

use image::codecs::png::PngEncoder;
use image::ImageEncoder;
use image::ExtendedColorType;
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
use crate::pane::IMAGE_RENDER_TIMES;
use crate::pane::IMAGE_RENDER_FPS;
use iced_wgpu::engine::CompressionStrategy;

pub static LATEST_SLIDER_POS: AtomicUsize = AtomicUsize::new(0);

#[allow(dead_code)]
static LAST_SLIDER_LOAD: Lazy<Mutex<Instant>> = Lazy::new(|| Mutex::new(Instant::now()));

const _THROTTLE_INTERVAL_MS: u64 = 100; // Default throttle interval

fn load_full_res_image(
    device: &Arc<wgpu::Device>,
    queue: &Arc<wgpu::Queue>,
    is_gpu_supported: bool,
    compression_strategy: CompressionStrategy,
    panes: &mut [pane::Pane],
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
                    .unwrap_or_else(|| Arc::new(create_gpu_texture(device, 1, 1, compression_strategy)));

                // Load the full-resolution image synchronously
                // Use the archive cache if provided
                let mut archive_guard = pane.archive_cache.lock().unwrap();
                let archive_cache = if pane.has_compressed_file {
                    Some(&mut *archive_guard)
                } else {
                    None
                };
                if let Err(err) = load_image_resized_sync(&img_path, false, device, queue, &mut texture, compression_strategy, archive_cache) {
                    debug!("Failed to load full-res image {} for pane {idx}: {err}", img_path.file_name());
                    continue;
                }

                // Store the full-resolution texture in the cache
                let loaded_image = CachedData::Gpu(texture.clone());
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
                let mut archive_guard = pane.archive_cache.lock().unwrap();
                let archive_cache = if pane.has_compressed_file {
                    Some(&mut *archive_guard)
                } else {
                    None
                };
                match img_cache.load_image(pos, archive_cache) {
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
                                    scene.ensure_texture(device, queue, pane.pane_id);
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

#[allow(clippy::too_many_arguments)]
fn get_loading_tasks_slider(
    device: &Arc<wgpu::Device>,
    queue: &Arc<wgpu::Queue>,
    _is_gpu_supported: bool,
    cache_strategy: CacheStrategy,
    compression_strategy: CompressionStrategy,
    panes: &mut [pane::Pane],
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
                    target_indices_and_cache.push(Some((prev_image_index, center_index - i - 1)));
                }
            }
        }

        // Enqueue the batched LoadPos operation with (image index, cache position) pairs
        let load_operation = LoadOperation::LoadPos((pane_index, target_indices_and_cache));
        loading_status.enqueue_image_load(load_operation);
        debug!("get_loading_tasks_slider - loading_status.loading_queue: {:?}", loading_status.loading_queue);
        loading_status.print_queue();

        // Generate loading tasks
        let local_tasks = load_all_images_in_queue(
            device,
            queue,
            cache_strategy,
            compression_strategy,
            panes,
            loading_status
        );
        tasks.push(local_tasks);
    }

    debug!("get_loading_tasks_slider - loading_status addr: {:p}", loading_status);
    loading_status.print_queue();


    tasks
}

#[allow(clippy::too_many_arguments)]
pub fn load_remaining_images(
    device: &Arc<wgpu::Device>,
    queue: &Arc<wgpu::Queue>,
    is_gpu_supported: bool,
    cache_strategy: CacheStrategy,
    compression_strategy: CompressionStrategy,
    panes: &mut [pane::Pane],
    loading_status: &mut LoadingStatus,
    pane_index: isize,
    pos: usize,
) -> Task<Message> {
    // Clear the global loading queue
    loading_status.reset_image_load_queue();
    loading_status.reset_image_being_loaded_queue();

    let mut tasks = Vec::new();

    // First, load the full-resolution image **synchronously**
    let full_res_task = load_full_res_image(device, queue, is_gpu_supported, compression_strategy, panes, pane_index, pos);
    tasks.push(full_res_task);

    // Then, load the neighboring images asynchronously
    if pane_index == -1 {
        // Dynamic loading: load the central image synchronously, and others asynchronously
        let cache_indices: Vec<usize> = panes
            .iter()
            .enumerate()
            .filter_map(|(cache_index, pane)| if pane.dir_loaded { Some(cache_index) } else { None })
            .collect();

        for cache_index in cache_indices {
            let local_tasks = get_loading_tasks_slider(
                device,
                queue,
                is_gpu_supported,
                cache_strategy,
                compression_strategy,
                panes,
                loading_status,
                cache_index,
                pos
            );
            debug!("load_remaining_images - local_tasks.len(): {}", local_tasks.len());
            tasks.extend(local_tasks);
        }
    } else if let Some(pane) = panes.get_mut(pane_index as usize) {
        if pane.dir_loaded {
            let local_tasks = get_loading_tasks_slider(
                device,
                queue,
                is_gpu_supported,
                cache_strategy,
                compression_strategy,
                panes,
                loading_status,
                pane_index as usize,
                pos);
            tasks.extend(local_tasks);
        } else {
            tasks.push(Task::none());
        }
    }

    debug!("load_remaining_images - loading_status addr: {:p}", loading_status);

    loading_status.print_queue();
    debug!("load_remaining_images - tasks.len(): {}", tasks.len());

    Task::batch(tasks)
}


// Async loading task for Image widget - updated to include pane_idx, archive cache, and RGBA8 bytes
pub async fn create_async_image_widget_task(
    img_path: crate::cache::img_cache::PathSource,
    pos: usize,
    pane_idx: usize,
    archive_cache: Option<Arc<Mutex<crate::archive_cache::ArchiveCache>>>
) -> Result<(usize, usize, Handle, (u32, u32), Vec<u8>), (usize, usize)> {
    // Start overall timer
    let task_start = std::time::Instant::now();


    // Start file reading timer
    let read_start = std::time::Instant::now();

    // Dispatch based on PathSource type
    let bytes_result = match &img_path {
        crate::cache::img_cache::PathSource::Filesystem(path) => {
            // Direct filesystem reading - no archive cache needed
            std::fs::read(path)
        },
        crate::cache::img_cache::PathSource::Archive(_) | crate::cache::img_cache::PathSource::Preloaded(_) => {
            // Archive content requires archive cache
            if let Some(cache_arc) = archive_cache {
                match cache_arc.lock() {
                    Ok(mut cache) => {
                        crate::file_io::read_image_bytes(&img_path, Some(&mut *cache))
                    },
                    Err(_) => {
                        Err(std::io::Error::other("Archive cache lock failed"))
                    },
                }
            } else {
                Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Archive cache required for archive/preloaded content"))
            }
        }
    };

    // Measure file reading time
    let read_time = read_start.elapsed();
    trace!("PERF: File read time for pos {}: {:?}", pos, read_time);

    match bytes_result {
        Ok(bytes) => {
            // Start handle creation timer
            let handle_start = std::time::Instant::now();

            // Extract image dimensions and RGBA8 bytes
            let (dimensions, rgba_bytes) = match image::load_from_memory(&bytes) {
                Ok(img) => {
                    let rgba = img.to_rgba8();
                    let dims = (rgba.width(), rgba.height());
                    let rgba_raw = rgba.into_raw();
                    (dims, rgba_raw)
                }
                Err(_) => {
                    // If we can't decode, return error
                    return Err((pane_idx, pos));
                }
            };

            // Convert directly to Handle without resizing
            let handle = iced::widget::image::Handle::from_bytes(bytes.clone());

            // Measure handle creation time
            let handle_time = handle_start.elapsed();
            trace!("PERF: Handle creation time for pos {}: {:?}", pos, handle_time);

            // Measure total function time
            let total_time = task_start.elapsed();
            trace!("PERF: Total async task time for pos {}: {:?}", pos, total_time);

            Ok((pane_idx, pos, handle, dimensions, rgba_bytes))
        },
        Err(_) => Err((pane_idx, pos)),
    }
}

pub fn update_pos(
    panes: &mut [pane::Pane],
    pane_index: isize,
    pos: usize,
    use_async: bool,
    throttle: bool
) -> Task<Message> {
    // Store the latest position in the atomic variable for reference
    LATEST_SLIDER_POS.store(pos, Ordering::SeqCst);

    // Determine if we should process this update based on throttling settings
    let should_process = if throttle {
        // Platform-specific throttling - use different thresholds for Linux
        #[cfg(target_os = "linux")]
        const PLATFORM_THROTTLE_MS: u64 = 10;

        #[cfg(not(target_os = "linux"))]
        const PLATFORM_THROTTLE_MS: u64 = _THROTTLE_INTERVAL_MS;

        // Throttling logic during rapid slider movement - With safety check
        let mut last_load = LAST_SLIDER_LOAD.lock().unwrap();
        let now = Instant::now();

        // Enhanced safety check for time inconsistencies
        let elapsed = match now.checked_duration_since(*last_load) {
            Some(duration) => duration,
            None => {
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
    } else {
        // No throttling, always process
        true
    };

    // Skip processing if we're throttling
    if !should_process {
        //debug!("Throttling slider image load at position {}", pos);
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
                if pane.dir_loaded && !pane.img_cache.image_paths.is_empty() && pos < pane.img_cache.image_paths.len() {
                    debug!("#####################update_pos - Creating async image loading task for pane {}", idx);

                    // Get only the single path we need from each pane
                    let img_path = pane.img_cache.image_paths[pos].clone();

                    // Check if the pane has compressed files and get the archive cache
                    let archive_cache = if pane.has_compressed_file {
                        Some(Arc::clone(&pane.archive_cache))
                    } else {
                        None
                    };

                    // Create task for this pane
                    let pane_task = Task::perform(
                        create_async_image_widget_task(img_path, pos, idx, archive_cache),
                        Message::SliderImageWidgetLoaded
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
    } else if pane_index == -1 {
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
    // Use the safe load_original_image function to prevent crashes with oversized images
    let mut archive_guard = pane.archive_cache.lock().unwrap();
    // For PathBuf, we'll use ArchiveCache if the pane has compressed files
    let archive_cache = if pane.has_compressed_file {
        Some(&mut *archive_guard)
    } else {
        None
    };
    match crate::cache::cache_utils::load_original_image(img_path, archive_cache) {
        Ok(img) => {
            // Resize the image to smaller dimensions for slider
            /*let resized = img.resize(
                800, // Width for slider
                600, // Height for slider
                image::imageops::FilterType::Triangle
            );*/

            // Create the CPU bytes
            let mut bytes: Vec<u8> = Vec::new();
            if let Err(err) = {
                let encoder = PngEncoder::new(std::io::Cursor::new(&mut bytes));
                encoder.write_image(
                    img.as_bytes(),
                    img.width(),
                    img.height(),
                    ExtendedColorType::Rgba8
                )
            } {
                debug!("Failed to encode slider image: {}", err);
                return Err(io::Error::other("Failed to encode image"));
            }

            // Update the current image to CPU data
            pane.current_image = CachedData::Cpu(bytes.clone());
            pane.slider_scene = Some(Scene::new(Some(&CachedData::Cpu(bytes.clone()))));

            // Ensure texture is created for CPU images
            if let Some(device) = &pane.device {
                if let Some(queue) = &pane.queue {
                    if let Some(scene) = &mut pane.slider_scene {
                        scene.ensure_texture(device, queue, pane.pane_id);
                    }
                }
            }

            Ok(())
        },
        Err(err) => {
            debug!("Failed to open image for slider: {}", err);
            Err(io::Error::other(format!("Failed to open image: {}", err)))
        }
    }
}


/// Loads the image at pos synchronously into the cache using Iced's image widget
fn load_current_slider_image_widget(pane: &mut pane::Pane, pos: usize ) -> Result<(), io::Error> {
    // Load the image at pos synchronously into the center position of cache
    // Assumes that the image at pos is already in the cache
    let img_cache = &mut pane.img_cache;
    let mut archive_guard = pane.archive_cache.lock().unwrap();
    let archive_cache = if pane.has_compressed_file {
        Some(&mut *archive_guard)
    } else {
        None
    };
    match img_cache.load_image(pos, archive_cache) {
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
            let mut archive_guard = pane.archive_cache.lock().unwrap();
            let archive_cache = if pane.has_compressed_file {
                Some(&mut *archive_guard)
            } else {
                None
            };
            match img_cache.get_initial_image_as_cpu(archive_cache) {
                Ok(bytes) => {
                    // Decode image to get RGBA8 bytes and dimensions
                    match image::load_from_memory(&bytes) {
                        Ok(img) => {
                            let rgba = img.to_rgba8();
                            let dimensions = (rgba.width(), rgba.height());
                            
                            // Store RGBA8 bytes for atlas-based rendering
                            pane.slider_image_rgba = Some(rgba.into_raw());
                            pane.slider_image_dimensions = Some(dimensions);
                            
                            // Keep the old Handle for backward compatibility (can be removed later)
                            pane.slider_image = Some(iced::widget::image::Handle::from_bytes(bytes));
                        }
                        Err(err) => {
                            debug!("Failed to decode image for slider: {}", err);
                            // Fallback: just use Handle
                            pane.slider_image = Some(iced::widget::image::Handle::from_bytes(bytes));
                        }
                    }

                    // Record image rendering time for FPS calculation
                    if let Ok(mut render_times) = IMAGE_RENDER_TIMES.lock() {
                        let now = Instant::now();
                        render_times.push(now);

                        // Calculate image rendering FPS
                        if render_times.len() > 1 {
                            let oldest = render_times[0];
                            let elapsed = now.duration_since(oldest);

                            if elapsed.as_secs_f32() > 0.0 {
                                let fps = render_times.len() as f32 / elapsed.as_secs_f32();

                                // Store the current image rendering FPS
                                if let Ok(mut image_fps) = IMAGE_RENDER_FPS.lock() {
                                    *image_fps = fps;
                                }

                                // Keep only recent frames (last 3 seconds)
                                let cutoff = now - std::time::Duration::from_secs(3);
                                render_times.retain(|&t| t > cutoff);
                            }
                        }
                    }

                    Ok(())
                },
                Err(err) => {
                    debug!("Failed to get CPU image data for slider: {}", err);

                    // Fallback: load directly from file
                    if let Some(img_path) = img_cache.image_paths.get(pos) {
                        // Dispatch based on PathSource type
                        let bytes_result = match img_path {
                            crate::cache::img_cache::PathSource::Filesystem(path) => {
                                // Direct filesystem reading - no archive cache needed
                                std::fs::read(path)
                            },
                            crate::cache::img_cache::PathSource::Archive(_) | crate::cache::img_cache::PathSource::Preloaded(_) => {
                                // Archive content requires archive cache
                                let mut archive_cache = pane.archive_cache.lock().unwrap();
                                crate::file_io::read_image_bytes(img_path, Some(&mut *archive_cache))
                            }
                        };

                        match bytes_result {
                            Ok(bytes) => {
                                // Decode image to get RGBA8 bytes and dimensions
                                match image::load_from_memory(&bytes) {
                                    Ok(img) => {
                                        let rgba = img.to_rgba8();
                                        let dimensions = (rgba.width(), rgba.height());
                                        
                                        // Store RGBA8 bytes for atlas-based rendering
                                        pane.slider_image_rgba = Some(rgba.into_raw());
                                        pane.slider_image_dimensions = Some(dimensions);
                                        
                                        // Keep the old Handle for backward compatibility
                                        pane.slider_image = Some(iced::widget::image::Handle::from_bytes(bytes));
                                    }
                                    Err(err) => {
                                        debug!("Failed to decode image for slider (fallback): {}", err);
                                        // Fallback: just use Handle
                                        pane.slider_image = Some(iced::widget::image::Handle::from_bytes(bytes));
                                    }
                                }

                                // Record image rendering time for FPS calculation (for fallback path)
                                if let Ok(mut render_times) = IMAGE_RENDER_TIMES.lock() {
                                    let now = Instant::now();
                                    render_times.push(now);

                                    // Calculate image rendering FPS
                                    if render_times.len() > 1 {
                                        let oldest = render_times[0];
                                        let elapsed = now.duration_since(oldest);

                                        if elapsed.as_secs_f32() > 0.0 {
                                            let fps = render_times.len() as f32 / elapsed.as_secs_f32();

                                            // Store the current image rendering FPS
                                            if let Ok(mut image_fps) = IMAGE_RENDER_FPS.lock() {
                                                *image_fps = fps;
                                            }

                                            // Keep only recent frames (last 3 seconds)
                                            let cutoff = now - std::time::Duration::from_secs(3);
                                            render_times.retain(|&t| t > cutoff);
                                        }
                                    }
                                }

                                Ok(())
                            },
                            Err(err) => Err(io::Error::other(
                                format!("Failed to read image file for slider: {}", err)
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
