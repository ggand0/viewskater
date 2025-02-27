#[cfg(target_os = "linux")]
mod other_os {
    pub use iced;
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
use crate::Message;
use std::path::Path;
use std::path::PathBuf;

use crate::file_io;
use crate::file_io::{is_file, is_directory, get_file_index};

use iced::widget::{
    container, column, text
};
use iced::{Element, Length};

use crate::menu::PaneLayout;
use crate::widgets::{dualslider::DualSlider, split::{Axis, Split}, viewer};
use crate::image_cache::ImageCache;
use crate::config::CONFIG;
use crate::file_io::ImageError;

#[allow(unused_imports)]
use log::{Level, debug, info, warn, error};


#[derive(Clone)]
pub struct Pane {
    pub directory_path: Option<String>,
    pub dir_loaded: bool,
    pub img_cache: ImageCache,
    pub current_image: iced::widget::image::Handle,
    pub is_next_image_loaded: bool, // whether the next image in cache is loaded
    pub is_prev_image_loaded: bool, // whether the previous image in cache is loaded
    pub slider_value: u16,
    pub prev_slider_value: u16,
    pub is_selected: bool,
    pub is_selected_cache: bool,
}

impl Default for Pane {
    fn default() -> Self {
        Self {
            directory_path: None,
            dir_loaded: false,
            img_cache: ImageCache::default(),
            current_image: iced::widget::image::Handle::from_bytes(vec![]),
            is_next_image_loaded: true,
            is_prev_image_loaded: true,
            slider_value: 0,
            prev_slider_value: 0,
            is_selected: true,
            is_selected_cache: true,
        }
    }
}

impl Pane {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            directory_path: None,
            dir_loaded: false,
            img_cache: ImageCache::default(),
            current_image: iced::widget::image::Handle::from_bytes(vec![]),
            is_next_image_loaded: true,
            is_prev_image_loaded: true,
            slider_value: 0,
            prev_slider_value: 0,
            is_selected: true,
            is_selected_cache: true,
        }
    }

    pub fn print_state(&self) {
        debug!("directory_path: {:?}, dir_loaded: {:?}, current_image: {:?}, is_next_image_loaded: {:?}, is_prev_image_loaded: {:?}, slider_value: {:?}, prev_slider_value: {:?}",
            self.directory_path, self.dir_loaded, self.current_image, self.is_next_image_loaded, self.is_prev_image_loaded, self.slider_value, self.prev_slider_value);
        self.img_cache.print_state();
    }

    pub fn reset_state(&mut self) {
        self.directory_path = None;
        self.dir_loaded = false;
        self.img_cache = ImageCache::default();
        //self.current_image = iced::widget::image::Handle::from_bytes(vec![]);
        self.current_image = iced::widget::image::Handle::from_bytes(vec![]);
        self.is_next_image_loaded = true;
        self.slider_value = 0;
        self.prev_slider_value = 0;
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

            let loaded_image = img_cache.get_image_by_index(next_image_index_to_render as usize).unwrap().to_vec();
            //let handle = iced::widget::image::Handle::from_bytes(loaded_image.clone());
            let handle = iced::widget::image::Handle::from_bytes(loaded_image.clone());

            self.current_image = handle;
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
                let loaded_image = img_cache.get_image_by_index(next_image_index_to_render as usize).unwrap().to_vec();
                //let handle = iced::widget::image::Handle::from_bytes(loaded_image.clone());
                let handle = iced::widget::image::Handle::from_bytes(loaded_image.clone());
                self.current_image = handle;
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


    pub fn initialize_dir_path(&mut self, pane_layout: &PaneLayout,
        pane_file_lengths: &[usize], _pane_index: usize, path: PathBuf,
        is_slider_dual: bool, slider_value: &mut u16) {
        let mut _file_paths: Vec<PathBuf>;
        let initial_index: usize;

        // Get directory path and image files
        let (dir_path, paths_result) = if is_file(&path) {
            debug!("Dropped path is a file");
            let directory = path.parent().unwrap_or(Path::new(""));
            let dir = directory.to_string_lossy().to_string();
            (dir.clone(), file_io::get_image_paths(Path::new(&dir)))
        } else if is_directory(&path) {
            debug!("Dropped path is a directory");
            let dir = path.to_string_lossy().to_string();
            (dir, file_io::get_image_paths(&path))
        } else {
            error!("Dropped path does not exist or cannot be accessed");
            return;
        };

        // Handle the result from get_image_paths
        _file_paths = match paths_result {
            Ok(paths) => paths,
            Err(ImageError::NoImagesFound) => {
                error!("No supported images found in directory");
                // TODO: Show a message to the user that no images were found
                return;
            }
            Err(e) => {
                error!("Error reading directory: {}", e);
                // TODO: Show error message to user
                return;
            }
        };

        self.directory_path = Some(dir_path);
        let mut is_dir_size_bigger = false;

        // Determine initial index and update slider
        if is_file(&path) {
            let file_index = get_file_index(&_file_paths, &path);
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
        } else {
            initial_index = 0;
        }

        // Calculate if directory size is bigger than other panes
        let longest_file_length = pane_file_lengths.iter().max().unwrap_or(&0);
        is_dir_size_bigger = if *pane_layout == PaneLayout::SinglePane {
            true
        } else if *pane_layout == PaneLayout::DualPane && is_slider_dual {
            true
        } else {
            _file_paths.len() >= *longest_file_length
        };
        debug!("longest_file_length: {:?}, is_dir_size_bigger: {:?}", longest_file_length, is_dir_size_bigger);

        // Update slider value
        let current_slider_value = initial_index as u16;
        debug!("current_slider_value: {:?}", current_slider_value);
        if is_slider_dual {
            *slider_value = current_slider_value;
            self.slider_value = current_slider_value;
        } else if is_dir_size_bigger {
            *slider_value = current_slider_value;
        }
        debug!("slider_value: {:?}", *slider_value);

        // Sort and load images
        debug!("File paths: {}", _file_paths.len());
        self.dir_loaded = true;

        // Initialize image cache
        let mut img_cache = match ImageCache::new(
            _file_paths,
            CONFIG.cache_size,
            initial_index,
        ) {
            Ok(cache) => cache,
            Err(e) => {
                error!("Failed to create image cache: {}", e);
                return;
            }
        };

        if let Err(e) = img_cache.load_initial_images() {
            error!("Failed to load initial images: {}", e);
            return;
        }
        img_cache.print_cache();

        // Load the initial image
        match img_cache.get_initial_image() {
            Ok(image_data) => {
                let handle = iced::widget::image::Handle::from_bytes(image_data.to_vec());
                self.current_image = handle;
            }
            Err(e) => {
                error!("Failed to get initial image: {}", e);
                return;
            }
        }

        let file_paths = img_cache.image_paths.clone();
        debug!("file_paths.len() {:?}", file_paths.len());
        
        self.img_cache = img_cache;
        debug!("img_cache.cache_count {:?}", self.img_cache.cache_count);
    }

    pub fn build_ui_dual_pane_slider1(&self) -> iced::widget::Container<Message> {
        let img: iced::widget::Container<Message>  = if self.dir_loaded {
            container(column![
                viewer::Viewer::new(self.current_image.clone())
                .width(Length::Fill)
                .height(Length::Fill),
            ])   
        } else {
            container(column![
                text(String::from(""))
                .width(Length::Fill)
                .height(Length::Fill)
            ])
        };
        img
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

pub fn build_ui_dual_pane_slider1(panes: &[Pane], ver_divider_position: Option<u16>) -> Element<Message> {
    let first_img: iced::widget::Container<Message>  = panes[0].build_ui_dual_pane_slider1();
    let second_img: iced::widget::Container<Message> = panes[1].build_ui_dual_pane_slider1();

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

pub fn build_ui_dual_pane_slider2(panes: &[Pane], ver_divider_position: Option<u16>, show_footer: bool) -> Element<Message> {
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

    let first_img: iced::widget::Container<Message> = if panes[0].dir_loaded {
        container(
            if show_footer { column![
                // NOTE: Wrapping the image in a container messes up the layout
                viewer::Viewer::new(panes[0].current_image.clone())
                .width(Length::Fill)
                .height(Length::Fill),
                DualSlider::new(
                    0..= (panes[0].img_cache.num_files - 1) as u16,
                    panes[0].slider_value,
                    0,
                    Message::SliderChanged,
                    Message::SliderReleased
                )
                .width(Length::Fill),
                get_footer(footer_texts[0].clone(), 0)
            ]} else { column![
                viewer::Viewer::new(panes[0].current_image.clone())
                .width(Length::Fill)
                .height(Length::Fill),
                DualSlider::new(
                    0..= (panes[0].img_cache.num_files - 1) as u16,
                    panes[0].slider_value,
                    0,
                    Message::SliderChanged,
                    Message::SliderReleased
                )
                .width(Length::Fill),
            ]}
        )
    } else {
        container(column![
            text(String::from(""))
                .width(Length::Fill)
                .height(Length::Fill),
        ])
    };

    let second_img: iced::widget::Container<Message> = if panes[1].dir_loaded {
        container(
            if show_footer { column![
                // NOTE: Wrapping the image in a container messes up the layout
                viewer::Viewer::new(panes[1].current_image.clone())
                .width(Length::Fill)
                .height(Length::Fill),
                DualSlider::new(
                    0..= (panes[1].img_cache.num_files - 1) as u16,
                    panes[1].slider_value,
                    1,
                    Message::SliderChanged,
                    Message::SliderReleased
                )
                .width(Length::Fill),
                get_footer(footer_texts[1].clone(), 1)
            ]} else { column![
                viewer::Viewer::new(panes[1].current_image.clone())
                .width(Length::Fill)
                .height(Length::Fill),
                DualSlider::new(
                    0..= (panes[1].img_cache.num_files - 1) as u16,
                    panes[1].slider_value,
                    1,
                    Message::SliderChanged,
                    Message::SliderReleased
                )
                .width(Length::Fill),
            ]}

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