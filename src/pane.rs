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


//use crate::image_cache;
use crate::ui_builder::get_footer;
use crate::app::Message;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use crate::file_io;
use crate::file_io::{is_file, is_directory, get_file_index};

//use iced::widget::{container, column, text};
//use iced::{Element, Length};
use iced_widget::{container, row, column, text};
use iced_winit::core::{Element, Length};
use iced_wgpu::Renderer;
use iced_winit::core::Theme as WinitTheme;
use image::GenericImageView;


use crate::menu::PaneLayout;
use crate::widgets::{dualslider::DualSlider, split::{Axis, Split}, viewer};

use crate::cache::img_cache::ImageCache;
use crate::cache::img_cache::CachedData;
use crate::widgets::shader::scene::Scene;
use crate::atlas::entry::{self, Entry};
use crate::widgets::shader::atlas_scene::AtlasScene;
use crate::config::CONFIG;
use crate::widgets::shader::image_shader::ImageShader;
use iced_wgpu::wgpu;
use iced_core::image::Handle;
use crate::cache::img_cache::CacheStrategy;
use crate::widgets::shader::cpu_scene::CpuScene;
use iced_core::Length::Fill;
use iced_widget::{center, Container};
use iced_widget::shader;

#[allow(unused_imports)]
use log::{Level, debug, info, warn, error};


//#[derive(Clone)]
pub struct Pane {
    pub directory_path: Option<String>,
    pub dir_loaded: bool,
    pub img_cache: ImageCache,
    pub current_image: CachedData, // <-- Now stores either CPU or GPU image
    //pub cpu_preview_image: Option<CachedData>, // for CPU previews
    pub cpu_preview_image: Option<Handle>,
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
            cpu_preview_image: None,
            backend: wgpu::Backend::Vulkan,
            device: None,
            queue: None,
            slider_image: None,
            pane_id: 0, // Default to pane 0
        }
    }
}

impl Pane {
    pub fn new(device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>, backend: wgpu::Backend, pane_id: usize) -> Self {
        let scene = Scene::new(None);
        // Create a dedicated CPU-based scene for slider
        let slider_scene = Scene::CpuScene(CpuScene::new(vec![], true));

        Self {
            directory_path: None,
            dir_loaded: false,
            img_cache: ImageCache::default(),
            current_image: CachedData::Cpu(vec![]),
            cpu_preview_image: None,
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
        }
    }

    pub fn print_state(&self) {
        debug!("directory_path: {:?}, dir_loaded: {:?}, is_next_image_loaded: {:?}, is_prev_image_loaded: {:?}, slider_value: {:?}, prev_slider_value: {:?}",
            self.directory_path, self.dir_loaded, self.is_next_image_loaded, self.is_prev_image_loaded, self.slider_value, self.prev_slider_value);
        // TODO: print `current_image` too
        //self.img_cache.print_state();
    }

    pub fn reset_state(&mut self) {
        self.directory_path = None;
        self.dir_loaded = false;
        self.img_cache = ImageCache::default();
        self.current_image = CachedData::Cpu(vec![]);
        self.is_next_image_loaded = true;
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
                            i // Use the index as the pane_id
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

    pub fn set_next_image(&mut self, pane_layout: &PaneLayout, is_slider_dual: bool) -> bool {
        let img_cache = &mut self.img_cache;
        let mut did_render_happen = false;

        img_cache.print_cache();

        if img_cache.is_some_at_index(img_cache.cache_count as usize + img_cache.current_offset as usize + 1
        ) {
            let next_image_index_to_render = img_cache.cache_count as isize + img_cache.current_offset + 1; 
            debug!("BEGINE RENDERING NEXT: next_image_index_to_render: {} current_index: {}, current_offset: {}",
                next_image_index_to_render, img_cache.current_index, img_cache.current_offset);

            /*let loaded_image = img_cache
                .get_image_by_index(next_image_index_to_render as usize)
                .unwrap()
                .as_vec()
                .expect("Failed to convert CachedData to Vec<u8>");
            let handle = iced::widget::image::Handle::from_bytes(loaded_image.clone());
            self.current_image = handle;*/

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
                                    scene.ensure_texture(Arc::clone(device), Arc::clone(queue), self.pane_id);
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
                    CachedData::Atlas { atlas, entry } => {  // Use struct pattern with named fields
                        debug!("Setting Atlas as current_image");
                        self.current_image = CachedData::Atlas {  // Create with named fields
                            atlas: Arc::clone(atlas),
                            entry: entry.clone(),
                        };
                        
                        // If you need to update scene with the atlas, add that here
                        // For example:
                        // self.scene = Some(Scene::new_with_atlas(...));
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

    pub fn set_prev_image(&mut self, pane_layout: &PaneLayout, is_slider_dual: bool) -> bool {
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

                /*let loaded_image = img_cache
                    .get_image_by_index(next_image_index_to_render as usize)
                    .unwrap()
                    .as_vec()
                    .expect("Failed to convert CachedData to Vec<u8>");
                let handle = iced::widget::image::Handle::from_bytes(loaded_image.clone());
                self.current_image = handle;*/
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
                                        scene.ensure_texture(Arc::clone(device), Arc::clone(queue), self.pane_id);
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
                        CachedData::Atlas { atlas, entry } => {  // Use struct pattern with named fields
                            debug!("Setting Atlas as current_image");
                            self.current_image = CachedData::Atlas {  // Create with named fields
                                atlas: Arc::clone(atlas),
                                entry: entry.clone(),
                            };
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
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        is_gpu_supported: bool,
        pane_layout: &PaneLayout,
        pane_file_lengths: &[usize],
        pane_index: usize,
        path: PathBuf,
        is_slider_dual: bool,
        slider_value: &mut u16,
    ) {
        let mut _file_paths: Vec<PathBuf> = Vec::new();
        let initial_index: usize;
        let mut is_dir_size_bigger: bool = false;

        if is_file(&path) {
            debug!("Dropped path is a file");
            let directory = path.parent().unwrap_or(Path::new(""));
            let dir = directory.to_string_lossy().to_string();
            self.directory_path = Some(dir);

            _file_paths = file_io::get_image_paths(Path::new(&self.directory_path.clone().unwrap()));
            let file_index = get_file_index(&_file_paths, &path);

            let longest_file_length = pane_file_lengths.iter().max().unwrap_or(&0);
            is_dir_size_bigger = if *pane_layout == PaneLayout::SinglePane {
                true
            } else if *pane_layout == PaneLayout::DualPane && is_slider_dual {
                true
            } else {
                _file_paths.len() >= *longest_file_length
            };
            debug!("longest_file_length: {:?}, is_dir_size_bigger: {:?}", longest_file_length, is_dir_size_bigger);

            if let Some(file_index) = file_index {
                debug!("File index: {}", file_index);
                initial_index = file_index;
                let current_slider_value = file_index as u16;
                debug!("current_slider_value: {:?}", current_slider_value);
                if is_slider_dual {
                    *slider_value = current_slider_value;
                    self.slider_value = current_slider_value;
                } else {
                    if is_dir_size_bigger {
                        *slider_value = current_slider_value;
                    }
                }
                debug!("slider_value: {:?}", *slider_value);
            } else {
                debug!("File index not found");
                return;
            }

        } else if is_directory(&path) {
            debug!("Dropped path is a directory");
            self.directory_path = Some(path.to_string_lossy().to_string());
            _file_paths = file_io::get_image_paths(Path::new(&self.directory_path.clone().unwrap()));
            initial_index = 0;

            let longest_file_length = pane_file_lengths.iter().max().unwrap_or(&0);
            is_dir_size_bigger = if *pane_layout == PaneLayout::SinglePane {
                true
            } else if *pane_layout == PaneLayout::DualPane && is_slider_dual {
                true
            } else {
                _file_paths.len() >= *longest_file_length
            };
            debug!("longest_file_length: {:?}, is_dir_size_bigger: {:?}", longest_file_length, is_dir_size_bigger);
            let current_slider_value = 0;
            debug!("current_slider_value: {:?}", current_slider_value);
            if is_slider_dual {
                *slider_value = current_slider_value;
                self.slider_value = current_slider_value;
            } else {
                if is_dir_size_bigger {
                    *slider_value = current_slider_value;
                }
            }
            debug!("slider_value: {:?}", *slider_value);
        } else {
            debug!("Dropped path does not exist or cannot be accessed");
            // Handle the case where the path does not exist or cannot be accessed
            return;
        }

        // Sort
        debug!("File paths: {}", _file_paths.len());
        self.dir_loaded = true;

        // Instantiate a new image cache and load the initial images
        /*let mut img_cache =  ImageCache::new(
            _file_paths,
            CONFIG.cache_size,
            initial_index,
            is_gpu_supported,
            device.unwrap(),
        ).unwrap();
        img_cache.load_initial_images().unwrap();
        img_cache.print_cache();

        let loaded_image = img_cache.get_initial_image().unwrap().to_vec();
        let handle = iced::widget::image::Handle::from_bytes(loaded_image.clone());
        self.current_image = handle;*/

        // Clone device and queue before passing to ImageCache to avoid the move
        let device_clone = Arc::clone(&device);
        let queue_clone = Arc::clone(&queue);

        // Instantiate a new image cache based on GPU support
        let mut img_cache = ImageCache::new(
            _file_paths,
            CONFIG.cache_size,
            //CacheStrategy::Atlas,
            CacheStrategy::Cpu,
            //CacheStrategy::Gpu,
            initial_index,
            Some(device_clone),
            Some(queue_clone),
            self.backend,
        )
        .unwrap();

        // Load initial images into the cache
        img_cache.load_initial_images().unwrap();
        ////img_cache.print_cache();
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

                    /*if let Some(scene) = &mut self.scene {
                        scene.ensure_texture(Arc::clone(&device), Arc::clone(&queue));
                    }*/
                }
                CachedData::Cpu(image_bytes) => {
                    debug!("Using CPU image for initial image");
                    self.current_image = CachedData::Cpu(image_bytes.clone());
                    self.scene = Some(Scene::new(Some(&CachedData::Cpu(image_bytes.clone()))));
                    
                    // Ensure texture is created for CPU images
                    if let Some(scene) = &mut self.scene {
                        scene.ensure_texture(Arc::clone(&device), Arc::clone(&queue), self.pane_id);
                    }
                }
                CachedData::Atlas { atlas, entry } => {
                    debug!("Using Atlas entry for initial image");
                    self.current_image = CachedData::Atlas {
                        atlas: Arc::clone(atlas),
                        entry: entry.clone(),
                    };
                    
                    // Get size information from the entry
                    let size = match &entry {
                        entry::Entry::Contiguous(allocation) => allocation.size(),
                        entry::Entry::Fragmented { size, .. } => *size,
                    };
                    
                    // Create the atlas scene with the Arc<RwLock<Atlas>>
                    // No need to access the atlas guard here as AtlasScene now works with RwLock
                    let mut atlas_scene = AtlasScene::new(Arc::clone(atlas));
                    
                    // Update the atlas scene with the entry
                    atlas_scene.update_image(entry.clone(), size.width, size.height);
                    self.scene = Some(Scene::AtlasScene(atlas_scene));
                }
            }
        } else {
            debug!("Failed to retrieve initial image");
        }
        
        
        


        let longest_file_length = pane_file_lengths.iter().max().unwrap_or(&0);
        debug!("longest_file_length: {:?}, is_dir_size_bigger: {:?}", longest_file_length, is_dir_size_bigger);
        let current_slider_value = initial_index as u16;
        debug!("current_slider_value: {:?}", current_slider_value);
        if is_slider_dual {
            //*slider_value = current_slider_value;
        } else {
            if is_dir_size_bigger {
                *slider_value = current_slider_value;
            }
        }
        debug!("slider_value: {:?}", *slider_value);

        let file_paths = img_cache.image_paths.clone();
        debug!("file_paths.len() {:?}", file_paths.len());
        
        self.img_cache = img_cache;
        debug!("img_cache.cache_count {:?}", self.img_cache.cache_count);

        
    }

    fn build_ui_container(&self, is_slider_moving: bool) -> Container<'_, Message, WinitTheme, Renderer> {
        if self.dir_loaded {
            if is_slider_moving && self.slider_image.is_some() {
                // Use regular Image widget during slider movement (much faster)
                let image_handle = self.slider_image.clone().unwrap();
                
                container(
                    center(
                        iced_widget::image(image_handle)
                            .content_fit(iced_winit::core::ContentFit::Contain)
                    )
                )
                .width(Length::Fill)
                .height(Length::Fill)
            } else if let Some(scene) = &self.scene {
                // Use shader/scene for normal viewing (better quality)
                //let shader_widget = shader(scene)
                //    .width(Fill)
                //    .height(Fill);
                let shader_widget = ImageShader::new(Some(scene))
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .content_fit(iced_winit::core::ContentFit::Contain);
                
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
pub fn get_pane_with_largest_dir_size(panes: &mut Vec<&mut Pane>) -> isize {
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

pub fn build_ui_dual_pane_slider1(
    panes: &[Pane],
    ver_divider_position: Option<u16>,
    is_slider_moving: bool
) -> Element<Message, WinitTheme, Renderer> {
    let first_img = panes[0].build_ui_container(is_slider_moving);
    let second_img = panes[1].build_ui_container(is_slider_moving);
    
    let is_selected: Vec<bool> = panes.iter().map(|pane| pane.is_selected).collect();
    Split::new(
        false,
        first_img,
        second_img,
        is_selected,
        ver_divider_position,
        Axis::Vertical,
        Message::OnVerResize,
        Message::ResetSplit,
        Message::FileDropped,
        Message::PaneSelected
    )
    .into()
}

pub fn build_ui_dual_pane_slider2(
    panes: &[Pane],
    ver_divider_position: Option<u16>,
    show_footer: bool,
    is_slider_moving: bool
) -> Element<Message, WinitTheme, Renderer> {
    let footer_texts = vec![
        format!(
            "{}/{}",
            panes[0].img_cache.current_index + 1,
            panes[0].img_cache.num_files
        ),
        format!(
            "{}/{}",
            panes[1].img_cache.current_index + 1,
            panes[1].img_cache.num_files
        )
    ];

    let first_img = if panes[0].dir_loaded {
        container(
            if show_footer { 
                column![
                    panes[0].build_ui_container(is_slider_moving),
                    DualSlider::new(
                        0..=(panes[0].img_cache.num_files - 1) as u16,
                        panes[0].slider_value,
                        0,
                        Message::SliderChanged,
                        Message::SliderReleased
                    )
                    .width(Length::Fill),
                    get_footer(footer_texts[0].clone(), 0)
                ]
            } else { 
                column![
                    panes[0].build_ui_container(is_slider_moving),
                    DualSlider::new(
                        0..=(panes[0].img_cache.num_files - 1) as u16,
                        panes[0].slider_value,
                        0,
                        Message::SliderChanged,
                        Message::SliderReleased
                    )
                    .width(Length::Fill),
                ]
            }
        )
    } else {
        container(column![
            text(String::from(""))
                .width(Length::Fill)
                .height(Length::Fill),
        ])
    };

    let second_img = if panes[1].dir_loaded {
        container(
            if show_footer { 
                column![
                    panes[1].build_ui_container(is_slider_moving),
                    DualSlider::new(
                        0..=(panes[1].img_cache.num_files - 1) as u16,
                        panes[1].slider_value,
                        1,
                        Message::SliderChanged,
                        Message::SliderReleased
                    )
                    .width(Length::Fill),
                    get_footer(footer_texts[1].clone(), 1)
                ]
            } else { 
                column![
                    panes[1].build_ui_container(is_slider_moving),
                    DualSlider::new(
                        0..=(panes[1].img_cache.num_files - 1) as u16,
                        panes[1].slider_value,
                        1,
                        Message::SliderChanged,
                        Message::SliderReleased
                    )
                    .width(Length::Fill),
                ]
            }
        )
    } else {
        container(column![
            text(String::from(""))
                .width(Length::Fill)
                .height(Length::Fill),
        ])
    };

    let is_selected: Vec<bool> = panes.iter().map(|pane| pane.is_selected).collect();
    Split::new(
        true,
        first_img,
        second_img,
        is_selected,
        ver_divider_position,
        Axis::Vertical,
        Message::OnVerResize,
        Message::ResetSplit,
        Message::FileDropped,
        Message::PaneSelected
    )
    .into()
}