use std::error::Error;
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::sync::OnceLock;
use std::time::Instant;
use std::fs::File;
use once_cell::sync::Lazy;

use iced_widget::{container, text};
use iced_winit::core::Length;
use iced_wgpu::Renderer;
use iced_winit::core::Theme as WinitTheme;
use iced_wgpu::wgpu;
use iced_core::image::Handle;
use iced_widget::{center, Container};

use crate::cache::img_cache::PathType;
use crate::config::CONFIG;
use crate::app::Message;
use crate::cache::img_cache::{CachedData, CacheStrategy, ImageCache};
use crate::archive_cache::ArchiveCache;
use crate::file_io::supported_image;
use crate::archive_cache::ArchiveType;
use crate::file_io::ALLOWED_COMPRESSED_FILES;

use crate::menu::PaneLayout;
use crate::widgets::viewer;
use crate::widgets::shader::{image_shader::ImageShader, scene::Scene, cpu_scene::CpuScene};
use crate::file_io::{self, is_file, is_directory, get_file_index, ImageError};
use iced_wgpu::engine::CompressionStrategy;
#[allow(unused_imports)]
use log::{Level, debug, info, warn, error};
use crate::utils::mem;

pub static IMAGE_RENDER_TIMES: Lazy<Mutex<Vec<Instant>>> = Lazy::new(|| {
    Mutex::new(Vec::with_capacity(120))
});
pub static IMAGE_RENDER_FPS: Lazy<Mutex<f32>> = Lazy::new(|| {
    Mutex::new(0.0)
});

pub struct Pane {
    pub directory_path: Option<String>,
    pub dir_loaded: bool,
    pub img_cache: ImageCache,
    pub current_image: CachedData, // <-- Now stores either CPU or GPU image
    pub is_next_image_loaded: bool, // whether the next image in cache is loaded
    pub is_prev_image_loaded: bool, // whether the previous image in cache is loaded
    pub slider_value: u16,
    pub prev_slider_value: u16,
    pub is_selected: bool,
    pub is_selected_cache: bool,
    pub scene: Option<Scene>,
    pub slider_scene: Option<Scene>, // Make sure this is Scene, not CpuScene
    pub slider_image: Option<Handle>,
    pub backend: wgpu::Backend,
    pub device: Option<Arc<wgpu::Device>>,
    pub queue: Option<Arc<wgpu::Queue>>,
    pub pane_id: usize, // New field for pane identification
    pub compression_strategy: CompressionStrategy,
    pub mouse_wheel_zoom: bool,
    pub ctrl_pressed: bool,
    pub has_compressed_file: bool,
    pub archive_cache: Arc<Mutex<ArchiveCache>>,
}

impl Default for Pane {
    fn default() -> Self {
        Self {
            directory_path: None,
            dir_loaded: false,
            img_cache: ImageCache::default(),
            current_image: CachedData::Cpu(vec![]), // Default to empty CPU image
            is_next_image_loaded: true,
            is_prev_image_loaded: true,
            slider_value: 0,
            prev_slider_value: 0,
            is_selected: true,
            is_selected_cache: true,
            scene: None,
            slider_scene: None, // Default to None
            backend: wgpu::Backend::Vulkan,
            device: None,
            queue: None,
            slider_image: None,
            pane_id: 0, // Default to pane 0
            compression_strategy: CompressionStrategy::None,
            mouse_wheel_zoom: false,
            ctrl_pressed: false,
            has_compressed_file: false,
            archive_cache: Arc::new(Mutex::new(ArchiveCache::new())),
        }
    }
}

impl Pane {
    pub fn new(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        backend: wgpu::Backend,
        pane_id: usize,
        compression_strategy: CompressionStrategy
    ) -> Self {
        let scene = Scene::new(None);
        // Create a dedicated CPU-based scene for slider
        let slider_scene = Scene::CpuScene(CpuScene::new(vec![], true));

        Self {
            directory_path: None,
            dir_loaded: false,
            img_cache: ImageCache::default(),
            current_image: CachedData::Cpu(vec![]),
            is_next_image_loaded: true,
            is_prev_image_loaded: true,
            slider_value: 0,
            prev_slider_value: 0,
            is_selected: true,
            is_selected_cache: true,
            scene: Some(scene),
            slider_scene: Some(slider_scene),
            backend,
            device: Some(device),
            queue: Some(queue),
            slider_image: None,
            pane_id, // Use the provided pane_id
            compression_strategy,
            mouse_wheel_zoom: false,
            ctrl_pressed: false,
            has_compressed_file: false,
            archive_cache: Arc::new(Mutex::new(ArchiveCache::new())),
        }
    }

    pub fn print_state(&self) {
        debug!("directory_path: {:?}, dir_loaded: {:?}, is_next_image_loaded: {:?}, is_prev_image_loaded: {:?}, slider_value: {:?}, prev_slider_value: {:?}",
            self.directory_path, self.dir_loaded, self.is_next_image_loaded, self.is_prev_image_loaded, self.slider_value, self.prev_slider_value);
        // TODO: print `current_image` too
        //self.img_cache.print_state();
    }

    pub fn reset_state(&mut self) {
        // Clear the scene which holds texture references
        self.scene = None;

        // Clear the slider scene holding texture references
        self.slider_scene = None;

        // Drop the current images
        self.current_image = CachedData::Cpu(vec![]);
        self.slider_image = None;

        // Explicitly reset the image cache
        self.img_cache.clear_cache();
        self.img_cache = ImageCache::default();

        // Reset other state
        self.directory_path = None;
        self.dir_loaded = false;
        self.is_next_image_loaded = true;
        self.is_prev_image_loaded = true;
        self.is_selected = true;
        self.slider_value = 0;
        self.prev_slider_value = 0;
    }

    pub fn resize_panes(panes: &mut Vec<Pane>, new_size: usize) {
        if new_size > panes.len() {
            // Add new panes with proper IDs
            for i in panes.len()..new_size {
                if let Some(first_pane) = panes.first() {
                    if let (Some(device), Some(queue)) = (&first_pane.device, &first_pane.queue) {
                        panes.push(Pane::new(
                            Arc::clone(device),
                            Arc::clone(queue),
                            first_pane.backend,
                            i, // Use the index as the pane_id
                            first_pane.compression_strategy
                        ));
                    } else {
                        // Fallback if no device/queue available
                        let mut new_pane = Pane::default();
                        new_pane.pane_id = i;
                        panes.push(new_pane);
                    }
                } else {
                    // Fallback if no existing panes
                    let mut new_pane = Pane::default();
                    new_pane.pane_id = i;
                    panes.push(new_pane);
                }
            }
        } else if new_size < panes.len() {
            // Truncate panes, preserving the first `new_size` elements
            panes.truncate(new_size);
        }
    }

    pub fn is_pane_cached_next(&self) -> bool {
        debug!("is_selected: {}, dir_loaded: {}, is_next_image_loaded: {}, img_cache.is_next_cache_index_within_bounds(): {}, img_cache.loading_queue.len(): {}, img_cache.being_loaded_queue.len(): {}",
            self.is_selected, self.dir_loaded, self.is_next_image_loaded, self.img_cache.is_next_cache_index_within_bounds(), self.img_cache.loading_queue.len(), self.img_cache.being_loaded_queue.len());

        // May need to consider whether current_index reached the end of the list
        self.is_selected && self.dir_loaded && self.img_cache.is_next_cache_index_within_bounds() &&
            self.img_cache.loading_queue.len() < CONFIG.max_loading_queue_size && self.img_cache.being_loaded_queue.len() < CONFIG.max_being_loaded_queue_size
    }

    pub fn is_pane_cached_prev(&self) -> bool {
        debug!("is_selected: {}, dir_loaded: {}, is_prev_image_loaded: {}, img_cache.is_prev_cache_index_within_bounds(): {}, img_cache.loading_queue.len(): {}, img_cache.being_loaded_queue.len(): {}",
            self.is_selected, self.dir_loaded, self.is_prev_image_loaded, self.img_cache.is_prev_cache_index_within_bounds(), self.img_cache.loading_queue.len(), self.img_cache.being_loaded_queue.len());

        self.is_selected && self.dir_loaded && self.img_cache.is_prev_cache_index_within_bounds() &&
            self.img_cache.loading_queue.len() < CONFIG.max_loading_queue_size && self.img_cache.being_loaded_queue.len() < CONFIG.max_being_loaded_queue_size
    }

    pub fn render_next_image(&mut self, pane_layout: &PaneLayout, is_slider_dual: bool) -> bool {
        let img_cache = &mut self.img_cache;
        let mut did_render_happen = false;

        img_cache.print_cache();

        // Safely compute target index as isize
        let target_index_isize = img_cache.cache_count as isize + img_cache.current_offset + 1;
        if target_index_isize >= 0 {
            let next_image_index_to_render = img_cache.cache_count as isize + img_cache.current_offset + 1;
            debug!("BEGINE RENDERING NEXT: next_image_index_to_render: {} current_index: {}, current_offset: {}",
                next_image_index_to_render, img_cache.current_index, img_cache.current_offset);

            // Retrieve the cached image (GPU or CPU)
            if let Ok(cached_image) = img_cache.get_image_by_index(next_image_index_to_render as usize) {
                match cached_image {
                    CachedData::Cpu(image_bytes) => {
                        debug!("Setting CPU image as current_image");
                        self.current_image = CachedData::Cpu(image_bytes.clone());
                        self.scene = Some(Scene::new(Some(&CachedData::Cpu(image_bytes.clone()))));

                        // Ensure texture is created for CPU images
                        if let Some(device) = &self.device {
                            if let Some(queue) = &self.queue {
                                if let Some(scene) = &mut self.scene {
                                    scene.ensure_texture(&device, &queue, self.pane_id);
                                }
                            }
                        }


                    }
                    CachedData::Gpu(texture) => {
                        debug!("Setting GPU texture as current_image");
                        self.current_image = CachedData::Gpu(Arc::clone(&texture));
                        self.scene = Some(Scene::new(Some(&CachedData::Gpu(Arc::clone(texture)))));
                        self.scene.as_mut().unwrap().update_texture(Arc::clone(texture));
                    }
                    CachedData::BC1(texture) => {
                        debug!("Setting BC1 compressed texture as current_image");
                        self.current_image = CachedData::BC1(Arc::clone(&texture));
                        self.scene = Some(Scene::new(Some(&CachedData::BC1(Arc::clone(texture)))));
                        self.scene.as_mut().unwrap().update_texture(Arc::clone(texture));
                    }
                }
            } else {
                debug!("Failed to retrieve next cached image.");
                return false;
            }

            img_cache.current_offset += 1;

            // Since the next image is loaded and rendered, mark the is_next_image_loaded flag
            self.is_next_image_loaded = true;
            did_render_happen = true;

            // Handle current_index
            if img_cache.current_index < img_cache.image_paths.len() - 1 {
                img_cache.current_index += 1;
            }

            if *pane_layout == PaneLayout::DualPane && is_slider_dual {
                self.slider_value = img_cache.current_index as u16;
            }
            debug!("END RENDERING NEXT: current_index: {}, current_offset: {}", img_cache.current_index, img_cache.current_offset);
        }

        did_render_happen
    }

    pub fn render_prev_image(&mut self, pane_layout: &PaneLayout, is_slider_dual: bool) -> bool {
        let img_cache = &mut self.img_cache;
        let mut did_render_happen = false;

        // Render the previous one right away
        // Avoid loading around the edges
        if img_cache.cache_count as isize + img_cache.current_offset > 0 &&
            img_cache.is_some_at_index( (img_cache.cache_count as isize + img_cache.current_offset) as usize) {

            let next_image_index_to_render = img_cache.cache_count as isize + (img_cache.current_offset - 1);
            debug!("RENDERING PREV: next_image_index_to_render: {} current_index: {}, current_offset: {}",
                next_image_index_to_render, img_cache.current_index, img_cache.current_offset);

            if img_cache.is_image_index_within_bounds(next_image_index_to_render) {
                // Retrieve the cached image (GPU or CPU)
                if let Ok(cached_image) = img_cache.get_image_by_index(next_image_index_to_render as usize) {
                    match cached_image {
                        CachedData::Cpu(image_bytes) => {
                            debug!("Setting CPU image as current_image");
                            self.current_image = CachedData::Cpu(image_bytes.clone());
                            self.scene = Some(Scene::new(Some(&CachedData::Cpu(image_bytes.clone()))));
                                // Ensure texture is created for CPU images
                            if let Some(device) = &self.device {
                                if let Some(queue) = &self.queue {
                                    if let Some(scene) = &mut self.scene {
                                        scene.ensure_texture(&device, &queue, self.pane_id);
                                    }
                                }
                            }
                        }
                        CachedData::Gpu(texture) => {
                            debug!("Setting GPU texture as current_image");
                            self.current_image = CachedData::Gpu(Arc::clone(&texture)); // Borrow before cloning
                            self.scene = Some(Scene::new(Some(&CachedData::Gpu(Arc::clone(texture)))));
                            self.scene.as_mut().unwrap().update_texture(Arc::clone(texture));
                        }
                        CachedData::BC1(texture) => {
                            debug!("Setting BC1 compressed texture as current_image");
                            self.current_image = CachedData::BC1(Arc::clone(&texture));
                            self.scene = Some(Scene::new(Some(&CachedData::BC1(Arc::clone(texture)))));
                            self.scene.as_mut().unwrap().update_texture(Arc::clone(texture));
                        }
                    }
                } else {
                    debug!("Failed to retrieve next cached image.");
                    return false;
                }


                img_cache.current_offset -= 1;

                assert!(img_cache.current_offset >= -(CONFIG.cache_size as isize)); // e.g. >= -5

                // Since the prev image is loaded and rendered, mark the is_prev_image_loaded flag
                self.is_prev_image_loaded = true;

                if img_cache.current_index > 0 {
                    img_cache.current_index -= 1;
                }
                debug!("RENDERED PREV: current_index: {}, current_offset: {}",
                img_cache.current_index, img_cache.current_offset);

                if *pane_layout == PaneLayout::DualPane && is_slider_dual {
                    self.slider_value = img_cache.current_index as u16;
                }

                did_render_happen = true;
            }
        }

        did_render_happen
    }

    #[allow(unused_assignments)]
    pub fn initialize_dir_path(
        &mut self,
        device: &Arc<wgpu::Device>,
        queue: &Arc<wgpu::Queue>,
        _is_gpu_supported: bool,
        cache_strategy: CacheStrategy,
        compression_strategy: CompressionStrategy,
        pane_layout: &PaneLayout,
        pane_file_lengths: &[usize],
        _pane_index: usize,
        path: &PathBuf,
        is_slider_dual: bool,
        slider_value: &mut u16,
    ) {
        mem::log_memory("Before pane initialization");

        let mut file_paths: Vec<PathType> = Vec::new();
        let mut initial_index: usize = 0;

        let is_dir_size_bigger: bool;
        let longest_file_length = pane_file_lengths.iter().max().unwrap_or(&0);

        // compressed file
        if path.extension().is_some_and(|ex| ALLOWED_COMPRESSED_FILES.contains(&ex.to_ascii_lowercase().to_str().unwrap_or(""))) {
            let archive;
            match path.extension().unwrap().to_ascii_lowercase().to_str() {
                Some("zip") => {
                    match read_zip_path(path, &mut file_paths) {
                        Ok(_) => {
                            archive = ArchiveType::Zip;
                        },
                        Err(e) => {
                            error!("Failed to read zip file: {e}");
                            return;
                        },
                    }
                },
                Some("rar") => {
                    match read_rar_path(path, &mut file_paths) {
                        Ok(_) => {
                            archive = ArchiveType::Rar;
                        },
                        Err(e) => {
                            error!("Failed to read rar file: {e}");
                            return;
                        },
                    }
                }
                Some("7z") => {
                    match read_7z_path(path, &mut file_paths) {
                        Ok(_) => {
                            archive = ArchiveType::SevenZ;
                        },
                        Err(e) => {
                            error!("Failed to read 7z file: {e}");
                            return;
                        },
                    }
                }
                _ => {
                    error!("File extension not supported");
                    return;
                }
            }
            if file_paths.len() == 0 {
                error!("No supported images found in {path:?}");
                return;
            }
            self.directory_path = Some(path.display().to_string());
            file_paths.sort_by(|a, b| alphanumeric_sort::compare_str(a.file_name(), b.file_name()));
            self.has_compressed_file = true;
            self.archive_cache.lock().unwrap().set_current_archive(path.to_path_buf(), archive);
        } else {
            // Get directory path and image files
            let (dir_path, paths_result) = if is_file(path) {
                debug!("Dropped path is a file");
                let directory = path.parent().unwrap_or(Path::new(""));
                let dir = directory.to_string_lossy().to_string();
                (dir.clone(), file_io::get_image_paths(Path::new(&dir)))
            } else if is_directory(path) {
                debug!("Dropped path is a directory");
                let dir = path.to_string_lossy().to_string();
                (dir, file_io::get_image_paths(path))
            } else {
                error!("Dropped path does not exist or cannot be accessed");
                return;
            };

            // Handle the result from get_image_paths
            file_paths = match paths_result {
                Ok(paths) => paths.iter().map(|item| {
                    PathType::PathBuf(item.to_path_buf())
                }).collect::<Vec<_>>(),
                Err(ImageError::NoImagesFound) => {
                    error!("No supported images found in directory");
                    // TODO: Show a message to the user that no images were found
                    return;
                }
                Err(e) => {
                    error!("Error reading directory: {e}");
                    // TODO: Show error message to user
                    return;
                }
            };
            self.directory_path = Some(dir_path);


            // Determine initial index and update slider
            if is_file(path) {
                let file_index = get_file_index(&file_paths.iter().filter_map(|item| {
                    if let PathType::PathBuf(pb) = item {
                        Some(pb.to_path_buf())
                    } else {
                        None
                    }
                }).collect::<Vec<_>>(), path);
                initial_index = match file_index {
                    Some(idx) => {
                        debug!("File index: {}", idx);
                        idx
                    }
                    None => {
                        debug!("File index not found");
                        return;
                    }
                };
            }
            self.has_compressed_file = false;
        };

        // Calculate if directory size is bigger than other panes
        is_dir_size_bigger = if *pane_layout == PaneLayout::SinglePane {
            true
        } else if *pane_layout == PaneLayout::DualPane && is_slider_dual {
            true
        } else {
            file_paths.len() >= *longest_file_length
        };
        debug!("longest_file_length: {:?}, is_dir_size_bigger: {:?}", longest_file_length, is_dir_size_bigger);

        // Sort
        debug!("File paths: {}", file_paths.len());
        self.dir_loaded = true;

        // Clone device and queue before passing to ImageCache to avoid the move
        let device_clone = Arc::clone(device);
        let queue_clone = Arc::clone(queue);

        // Instantiate a new image cache based on GPU support
        let mut img_cache = ImageCache::new(
            &file_paths,
            CONFIG.cache_size,
            cache_strategy,
            compression_strategy,
            initial_index,
            Some(device_clone),
            Some(queue_clone),
        );

        // Track memory before loading initial images
        mem::log_memory("Pane::initialize_dir_path: Before loading initial images");

        // Load initial images into the cache  
        let mut archive_guard = self.archive_cache.lock().unwrap();
        let archive_cache = if self.has_compressed_file {
            Some(&mut *archive_guard)
        } else {
            None
        };
        
        if let Err(e) = img_cache.load_initial_images(archive_cache) {
            error!("Failed to load initial images: {}", e);
            return;
        }

        mem::log_memory("Pane::initialize_dir_path: After loading initial images");

        for (index, image_option) in img_cache.cached_data.iter().enumerate() {
            match image_option {
                Some(image_bytes) => {
                    let image_info = format!("Image {} - Index {} - Size: {} bytes", index, img_cache.cached_image_indices[index], image_bytes.len());
                    debug!("{}", image_info);
                }
                None => {
                    let no_image_info = format!("No image at index {}", index);
                    debug!("{}", no_image_info);
                }
            }
        }


        if let Ok(initial_image) = img_cache.get_initial_image() {
            match initial_image {
                CachedData::Gpu(texture) => {
                    debug!("Using GPU texture for initial image");
                    self.current_image = CachedData::Gpu(Arc::clone(texture));
                    self.scene = Some(Scene::new(Some(&CachedData::Gpu(Arc::clone(texture)))));
                    self.scene.as_mut().unwrap().update_texture(Arc::clone(texture));
                }
                CachedData::BC1(texture) => {
                    debug!("Using BC1 compressed texture for initial image");
                    self.current_image = CachedData::BC1(Arc::clone(texture));
                    self.scene = Some(Scene::new(Some(&CachedData::BC1(Arc::clone(texture)))));
                    self.scene.as_mut().unwrap().update_texture(Arc::clone(texture));
                }
                CachedData::Cpu(image_bytes) => {
                    debug!("Using CPU image for initial image");
                    self.current_image = CachedData::Cpu(image_bytes.clone());
                    self.scene = Some(Scene::new(Some(&CachedData::Cpu(image_bytes.clone()))));

                    // Ensure texture is created for CPU images
                    if let Some(scene) = &mut self.scene {
                        scene.ensure_texture(device, queue, self.pane_id);
                    }
                }
            }
        } else {
            debug!("Failed to retrieve initial image");
        }

        // Update slider value
        let current_slider_value = initial_index as u16;
        debug!("current_slider_value: {:?}", current_slider_value);
        if is_slider_dual {
            *slider_value = current_slider_value;
            self.slider_value = current_slider_value;
        } else if *pane_layout == PaneLayout::SinglePane || *pane_layout == PaneLayout::DualPane && is_dir_size_bigger {
            *slider_value = current_slider_value;
        }
        debug!("slider_value: {:?}", *slider_value);

        let file_paths = img_cache.image_paths.clone();
        debug!("file_paths.len() {:?}", file_paths.len());

        self.img_cache = img_cache;
        debug!("img_cache.cache_count {:?}", self.img_cache.cache_count);
    }

    pub fn build_ui_container(&self, is_slider_moving: bool, is_horizontal_split: bool) -> Container<'_, Message, WinitTheme, Renderer> {
        if self.dir_loaded {
            if is_slider_moving && self.slider_image.is_some() {
                // Use regular Image widget during slider movement (much faster)
                let image_handle = self.slider_image.clone().unwrap();

                container(
                    center(
                        viewer::Viewer::new(image_handle)
                            .content_fit(iced_winit::core::ContentFit::Contain)
                    )
                )
                .width(Length::Fill)
                .height(Length::Fill)
            } else if let Some(scene) = &self.scene {
                let shader_widget = ImageShader::new(Some(scene))
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .content_fit(iced_winit::core::ContentFit::Contain)
                        .horizontal_split(is_horizontal_split)
                        .with_interaction_state(self.mouse_wheel_zoom, self.ctrl_pressed);

                container(center(shader_widget))
                    .width(Length::Fill)
                    .height(Length::Fill)
            } else {
                container(text("No image loaded"))
                    .width(Length::Fill)
                    .height(Length::Fill)
            }
        } else {
            container(text(""))
                .width(Length::Fill)
                .height(Length::Fill)
        }
    }
}

#[allow(dead_code)]
pub fn get_pane_with_largest_dir_size(panes: &Vec<&mut Pane>) -> isize {
    let mut max_dir_size = 0;
    let mut max_dir_size_index = -1;
    for (i, pane) in panes.iter().enumerate() {
        if pane.dir_loaded {
            if pane.img_cache.num_files > max_dir_size {
                max_dir_size = pane.img_cache.num_files;
                max_dir_size_index = i as isize;
            }
        }
    }
    max_dir_size_index
}

pub fn get_master_slider_value(panes: &[&mut Pane],
    _pane_layout: &PaneLayout, _is_slider_dual: bool, _last_opened_pane: usize) -> usize {
    let mut max_dir_size = 0;
    let mut max_dir_size_index = 0;
    for (i, pane) in panes.iter().enumerate() {
        if pane.dir_loaded {
            if pane.img_cache.num_files > max_dir_size {
                max_dir_size = pane.img_cache.num_files;
                max_dir_size_index = i;
            }
        }
    }

    let pane = &panes[max_dir_size_index];
    pane.img_cache.current_index as usize
}

fn read_zip_path(path: &PathBuf, file_paths: &mut Vec<PathType>) -> Result<(), Box<dyn Error>> {
    let mut archive = zip::ZipArchive::new(std::io::BufReader::new(
        File::open(path)?))?;
    for i in 0..archive.len() {
        let file = archive.by_index(i)?;
        if file.is_file() && supported_image(file.name()) {
            file_paths.push(PathType::FileByte(file.name().to_string(), OnceLock::new()));
        }
    }
    Ok(())
}

fn read_rar_path(path: &PathBuf, file_paths: &mut Vec<PathType>) -> Result<(), Box<dyn Error>> {
    let mut archive = unrar::Archive::new(path)
            .open_for_processing()?;
    while let Some(header) = archive.read_header()? {
        let filename = header.entry().filename.to_string_lossy().to_string();
        archive = if header.entry().is_file() {
            if supported_image(&filename) {
                file_paths.push(PathType::FileByte(filename, OnceLock::new()));
                header.skip()?
            } else {
                debug!("Unsupported file {filename} in {}", path.to_string_lossy().to_string());
                header.skip()?
            }
        } else {
            debug!("Skipping directory {filename}");
            header.skip()?
        };
    }

    Ok(())
}

fn read_7z_path(path: &PathBuf, file_paths: &mut Vec<PathType>) -> Result<(), Box<dyn Error>> {
    use std::thread;
    let password = sevenz_rust2::Password::empty();
    let mut file = File::open(path)?;
    let archive = sevenz_rust2::Archive::read(&mut file, &password)?;
    let is_solid = archive.is_solid;
    // solid file is too slow for lazy loading
    if is_solid {
        let block_count = archive.blocks.len();
        debug!("{path:?} block_count: {block_count}");
        let cpu_threads = if thread::available_parallelism().is_ok() {
            thread::available_parallelism()?.get() as u32
        } else { 4 };

        debug!("Using {cpu_threads} threads to read {path:?}");
        let sevenz_list = Mutex::new(Vec::new());
        for block_index in 0..block_count {
            thread::scope(|s| {
                s.spawn(||{
                    let mut source = File::open(path).unwrap();
                    // 2. For decoders that supports it, we can set the thread_count on the block decoder
                    //    so that it uses multiple threads to decode the block. Currently only LZMA2 is
                    //    supporting this. We try to use all threads report from std::thread.
                    let block_decoder = sevenz_rust2::BlockDecoder::new(cpu_threads, block_index, &archive, &password, &mut source);
                    block_decoder.for_each_entries(&mut |entry, reader| {
                        if !entry.is_directory && supported_image(entry.name()) {
                            let ol = OnceLock::new();
                            let mut buffer = Vec::new();
                            reader.read_to_end(&mut buffer)?;
                            let _ = ol.set(buffer);
                            sevenz_list.lock().unwrap().push(PathType::FileByte(entry.name().to_string(), ol));
                        }
                        Ok(true)
                    })
                    .expect("Failed block reading 7z file");
                });
            });
        }
        file_paths.append(&mut sevenz_list.into_inner()?);
    } else {
        sevenz_rust2::ArchiveReader::open(path, password)?.for_each_entries(|entry, _|{
            if !entry.is_directory && supported_image(entry.name()) {
                file_paths.push(PathType::FileByte(entry.name().to_string(), OnceLock::new()));
            }
            Ok(true)
        })?;
    }

    Ok(())
}