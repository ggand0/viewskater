#[cfg(target_os = "linux")]
mod other_os {
    pub use iced;
    pub use iced_aw;
    pub use iced_widget;
}

#[cfg(not(target_os = "linux"))]
mod macos {
    pub use iced_custom as iced;
    pub use iced_aw_custom as iced_aw;
    pub use iced_widget_custom as iced_widget;
}

#[cfg(target_os = "linux")]
use other_os::*;

#[cfg(not(target_os = "linux"))]
use macos::*;

// mod image_cache;
//use crate::image_cache::ImageCache;
use crate::image_cache;

use crate::Message;

use std::path::Path;
use std::path::PathBuf;

// mod utils;
use crate::file_io;
//use crate::file_io::{async_load_image, empty_async_block, is_file, is_directory, get_file_paths, get_file_index, Error};
use crate::file_io::{is_file, is_directory, get_file_paths, get_file_index};

use iced::widget::{
    //container, row, column, slider, horizontal_space, text
    container, column, text
};
use iced::widget::Image;
use iced::{Element, Length};
use crate::dualslider::dualslider::DualSlider;

use crate::split::split::{Axis, Split};
use crate::viewer;


// ref: https://github.com/iced-rs/iced/blob/master/examples/todos/src/main.rs
#[derive(Debug, Clone)]
pub enum PaneMessage {
}

#[derive(Clone)]
pub struct Pane {
    /*dir_loaded: vec![false; 2],
    img_caches: vec![image_cache::ImageCache::default(), image_cache::ImageCache::default()],
    current_images: Vec::new(),
    image_load_state: vec![true; 2],
    slider_values: vec![0; 2],
    prev_slider_values: vec![0; 2],*/

    pub directory_path: Option<String>,
    pub dir_loaded: bool,
    pub img_cache: image_cache::ImageCache,
    pub current_image: iced::widget::image::Handle,
    pub image_load_state: bool,
    pub slider_value: u16,
    pub prev_slider_value: u16,

    pub id: usize,
    pub is_selected: bool,
    pub is_selected_cache: bool,
}

impl Default for Pane {
    fn default() -> Self {
        Self {
            directory_path: None,
            dir_loaded: false,
            img_cache: image_cache::ImageCache::default(),
            current_image: iced::widget::image::Handle::from_memory(vec![]),
            image_load_state: true,
            slider_value: 0,
            prev_slider_value: 0,
            id: 0,
            is_selected: true,
            is_selected_cache: true,
        }
    }
}

impl Pane {
    pub fn new() -> Self {
        Self {
            directory_path: None,
            dir_loaded: false,
            img_cache: image_cache::ImageCache::default(),
            current_image: iced::widget::image::Handle::from_memory(vec![]),
            image_load_state: true,
            slider_value: 0,
            prev_slider_value: 0,
            id: 0,
            is_selected: true,
            is_selected_cache: true,
        }
    }

    pub fn reset_state(&mut self) {
        self.directory_path = None;
        self.dir_loaded = false;
        self.img_cache = image_cache::ImageCache::default();
        self.current_image = iced::widget::image::Handle::from_memory(vec![]);
        self.image_load_state = true;
        self.slider_value = 0;
        self.prev_slider_value = 0;
    }

    pub fn initialize_dir_path(&mut self, path: PathBuf) {
        let mut _file_paths: Vec<PathBuf> = Vec::new();
        let initial_index: usize;
        

        if is_file(&path) {
            println!("Dropped path is a file");
            let directory = path.parent().unwrap_or(Path::new(""));
            let dir = directory.to_string_lossy().to_string();
            self.directory_path = Some(dir);

            _file_paths = file_io::get_image_paths(Path::new(&self.directory_path.clone().unwrap()));
            let file_index = get_file_index(&_file_paths, &path);

            if let Some(file_index) = file_index {
                println!("File index: {}", file_index);
                initial_index = file_index;
                // self.current_image_index = file_index;
                
                // self.slider_values[pane_index] = file_index as u16;
                // self.panes[pane_index].slider_value = file_index as u16;
                self.slider_value = file_index as u16;
            } else {
                println!("File index not found");
                return;
            }

        } else if is_directory(&path) {
            println!("Dropped path is a directory");
            self.directory_path = Some(path.to_string_lossy().to_string());
            _file_paths = get_file_paths(Path::new(&self.directory_path.clone().unwrap()));
            initial_index = 0;
            // Display the first 100 paths
            /*for path in _file_paths.iter().take(100) {
                println!("{}", path.display());
            }*/

            // self.current_image_index = 0;
            
            // self.slider_values[pane_index] = 0;
            // self.panes[pane_index].slider_value = 0;
            self.slider_value = 0;
        } else {
            println!("Dropped path does not exist or cannot be accessed");
            // Handle the case where the path does not exist or cannot be accessed
            return;
        }

        // Sort
        alphanumeric_sort::sort_path_slice(&mut _file_paths);

        // Debug print the files
        for path in _file_paths.iter().take(20) {
            println!("{}", path.display());
        }

        println!("File paths: {}", _file_paths.len());
        // self.dir_loaded[pane_index] = true;
        // self.panes[pane_index].dir_loaded = true;
        self.dir_loaded = true;

        // Instantiate a new image cache and load the initial images
        let mut img_cache =  image_cache::ImageCache::new(
            _file_paths,
            2,
            //100,
            initial_index,
        ).unwrap();
        img_cache.load_initial_images().unwrap();
        

        let loaded_image = img_cache.get_current_image().unwrap().to_vec();
        let handle = iced::widget::image::Handle::from_memory(loaded_image.clone());
        self.current_image = handle;

        let file_paths = img_cache.image_paths.clone();
        println!("file_paths.len() {:?}", file_paths.len());
        
        self.img_cache = img_cache;
        println!("img_cache.cache_count {:?}", self.img_cache.cache_count);
        
        
    }

    pub fn update(&mut self, message: PaneMessage) {
        match message {
        }
    }

    // pub fn view(&self) -> iced::Element<PaneMessage> {    
    // }

    pub fn build_ui(&self) -> iced::widget::Container<Message> {
        let img: iced::widget::Container<Message>  = if self.dir_loaded {
            container(column![
                Image::new(self.current_image.clone())
                .width(Length::Fill)
                .height(Length::Fill),
                //slider(0..= (self.img_caches[0].num_files-1) as u16, self.slider_values[0], Message::SliderChanged)
                /*slider(
                    0..= (self.img_caches[0].num_files - 1) as u16,
                    self.slider_values[0],
                    |value| {
                        let pane_index = 0; // Replace this with the desired pane index
                        Message::SliderChanged((pane_index, value))
                    }
                )*/
                DualSlider::new(
                    0..= (self.img_cache.num_files - 1) as u16,
                    // self.slider_values[0],
                    self.slider_value,
                    -1,
                    Message::SliderChanged
                )
                .width(Length::Fill)
                ]
            )
        } else {
            container(column![
            text(String::from(""))
            .width(Length::Fill)
            .height(Length::Fill)
            
            ])
        };
        img
    }

    pub fn build_ui_dual_pane_slider1(&self) -> iced::widget::Container<Message> {
        let img: iced::widget::Container<Message>  = if self.dir_loaded {
            container(column![
                //Image::new(self.current_image.clone())
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

    pub fn build_ui_dual_pane_slider2(&self) -> iced::widget::Container<Message> {
        let img: iced::widget::Container<Message>  = if self.dir_loaded {
            container(column![
                container(
                Image::new(self.current_image.clone())
                .width(Length::Fill)
                .height(Length::Fill)),
                

                DualSlider::new(
                    0..= (self.img_cache.num_files - 1) as u16,
                    self.slider_value,
                    0, // this needs to pane_index instead of 0
                    Message::SliderChanged
                )
                .width(Length::Fill)
                ]
            )
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

pub fn build_ui_dual_pane_slider1(panes: &[Pane], ver_divider_position: Option<u16>) -> Element<Message> {
    let first_img: iced::widget::Container<Message>  = panes[0].build_ui_dual_pane_slider1();
    let second_img: iced::widget::Container<Message> = panes[1].build_ui_dual_pane_slider1();
    Split::new(
        false,
        first_img,
        second_img,
        ver_divider_position,
        Axis::Vertical,
        Message::OnVerResize,
        Message::ResetSplit,
        Message::FileDropped,
        Message::PaneSelected
    )
    .into()
}

pub fn build_ui_dual_pane_slider2(panes: &[Pane], ver_divider_position: Option<u16>) -> Element<Message> {
    let first_img: iced::widget::Container<Message> = if panes[0].dir_loaded {
        container(column![
            // NOTE: Wrapping the image in a container messes up the layout
            //Image::new(panes[0].current_image.clone())
            viewer::Viewer::new(panes[0].current_image.clone())
            .width(Length::Fill)
            .height(Length::Fill),
            DualSlider::new(
                0..= (panes[0].img_cache.num_files - 1) as u16,
                panes[0].slider_value,
                0,
                Message::SliderChanged
            )
            .width(Length::Fill)
            ]
        )
    } else {
        container(column![
            text(String::from(""))
                .width(Length::Fill)
                .height(Length::Fill),
        ])
    };

    let second_img: iced::widget::Container<Message> = if panes[1].dir_loaded {
        container(column![
            // NOTE: Wrapping the image in a container messes up the layout
            // Image::new(panes[1].current_image.clone())
            viewer::Viewer::new(panes[1].current_image.clone())
            .width(Length::Fill)
            .height(Length::Fill),
            DualSlider::new(
                0..= (panes[1].img_cache.num_files - 1) as u16,
                panes[1].slider_value,
                1,
                Message::SliderChanged
            )
            .width(Length::Fill)
            ]
        )
    } else {
        container(column![
            text(String::from(""))
                .width(Length::Fill)
                .height(Length::Fill),
        ])
    };

    Split::new(
        true,
        first_img,
        second_img,
        ver_divider_position,
        Axis::Vertical,
        Message::OnVerResize,
        Message::ResetSplit,
        Message::FileDropped,
        Message::PaneSelected
    )
    .into()
}