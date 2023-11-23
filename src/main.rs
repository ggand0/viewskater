use iced::event::Event;
use iced::subscription::{self, Subscription};
use iced::keyboard;
use iced::widget::{
    container, row, column, slider, horizontal_space,
};
use iced::widget::Image;
use iced::{Element, Length, Application, Theme, Settings, Command, Color, alignment};

use iced_aw::menu::{CloseCondition, ItemHeight, ItemWidth, PathHighlight};
use iced_aw::menu_bar;

use std::path::Path;
use std::path::PathBuf;

// #[macro_use]
extern crate log;

mod image_cache;
use image_cache::LoadOperation;
mod utils;
use utils::{async_load_image, empty_async_block, is_file, is_directory, get_file_paths, get_file_index, Error};
mod ui;


// Define the application state
// #[derive(Default)]
struct DataViewer {
    // image_path: String,
    // image_paths: Vec<String>,
    // error: Option<io::ErrorKind>,
    dir_loaded: bool,
    
    directory_path: Option<String>,
    current_image_index: usize,
    
    // image_cache: image_cache::ImageCache,
    img_cache: image_cache::ImageCache,
    // current_image: Option<iced::widget::Image<iced::widget::image::Handle>>,
    current_image: iced::widget::image::Handle,
    slider_value: u16,
    prev_slider_value: u16,
    num_files: usize,
    title: String,
}

impl Default for DataViewer {
    fn default() -> Self {
        Self {
            // image_path: String::new(),
            // image_paths: Vec::new(),
            // error: None,
            dir_loaded: false,
            directory_path: None,
            current_image_index: 0,
            img_cache: image_cache::ImageCache::default(),
            current_image: iced::widget::image::Handle::from_memory(vec![]),
            slider_value: 0,
            prev_slider_value: 0,
            num_files: 0,
            title: String::from("Data Viewer"),
        }
    }
}

// Define application messages
#[derive(Debug, Clone)]
pub enum Message {
    OpenFolder,
    OpenFile,
    Close,
    FolderOpened(Result<String, Error>),
    SliderChanged(u16),
    Event(Event),
    // ImageLoaded(Result<(), std::io::ErrorKind>),// std::io::Error doesn't seem to be clonable
    // ImageLoaded(Result<Option<Vec<u8>>, std::io::ErrorKind>),
    ImageLoaded(Result<(Option<Vec<u8>>, Option<LoadOperation>), std::io::ErrorKind>),
    // MenuItemClicked(MenuItem),
    Debug(String),
}

#[derive(Debug, Clone, Copy)]
pub enum MenuItem {
    Open,
    Close,
    Help
}

impl DataViewer {
    fn reset_state(&mut self) {
        self.dir_loaded = false;
        self.directory_path = None;
        self.current_image_index = 0;
        self.img_cache = image_cache::ImageCache::default();
        self.current_image = iced::widget::image::Handle::from_memory(vec![]);
        self.slider_value = 0;
        self.num_files = 0;
        self.title = String::from("Data Viewer");
    }

    fn load_image_by_index(img_cache: &mut image_cache::ImageCache, target_index: usize, operation: LoadOperation) -> Command<<DataViewer as iced::Application>::Message> {
        let path = img_cache.image_paths.get(target_index);
        if let Some(path) = path {
            // println!("target_index: {}, Loading Path: {}", path.clone().to_string_lossy(), target_index );
            let image_loading_task = async_load_image(path.clone(), operation);
            Command::perform(image_loading_task, Message::ImageLoaded)
        } else {
            Command::none()
        }
    }

    fn load_image_by_operation(&mut self) -> Command<Message> {
        if !self.img_cache.loading_queue.is_empty() {
            if let Some(operation) = self.img_cache.loading_queue.pop_front() {
                self.img_cache.enqueue_image_being_loaded(operation.clone());
                match operation {
                    LoadOperation::LoadNext(target_index) => {
                        DataViewer::load_image_by_index(&mut self.img_cache, target_index, operation)
                    }
                    LoadOperation::LoadPrevious(target_index) => {
                        DataViewer::load_image_by_index(&mut self.img_cache, target_index, operation)
                    }
                    LoadOperation::ShiftNext(_target_index) => {
                        let empty_async_block = empty_async_block(operation);
                        Command::perform(empty_async_block, Message::ImageLoaded)
                    }
                    LoadOperation::ShiftPrevious(_target_index) => {
                        let empty_async_block = empty_async_block(operation);
                        Command::perform(empty_async_block, Message::ImageLoaded)
                    }
                }
            } else {
                Command::none()
            }
        } else {
            Command::none()
        }
            
    }

    fn initialize_dir_path(&mut self, path: PathBuf) {
        let mut _file_paths: Vec<PathBuf> = Vec::new();
        let initial_index: usize;
        if is_file(&path) {
            println!("Dropped path is a file");
            let directory = path.parent().unwrap_or(Path::new(""));
            let dir = directory.to_string_lossy().to_string();
            self.directory_path = Some(dir);

            // _file_paths = get_file_paths(Path::new(&self.directory_path.clone().unwrap()));
            _file_paths = utils::get_image_paths(Path::new(&self.directory_path.clone().unwrap()));
            let file_index = get_file_index(&_file_paths, &path);
            // let file_index = get_file_index(&self.image_paths.iter().map(PathBuf::from).collect::<Vec<_>>(), &path);
            if let Some(file_index) = file_index {
                println!("File index: {}", file_index);
                initial_index = file_index;
                self.current_image_index = file_index;
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
            self.current_image_index = 0;
            self.slider_value = 0;
        } else {
            println!("Dropped path does not exist or cannot be accessed");
            // Handle the case where the path does not exist or cannot be accessed
            return;
        }

        // Debug print the files
        for path in _file_paths.iter().take(20) {
            println!("{}", path.display());
        }

        // self.image_paths = file_paths.iter().map(|p| p.to_string_lossy().to_string()).collect();
        println!("File paths: {}", _file_paths.len());
        self.num_files = _file_paths.len();
        self.dir_loaded = true;

        // Instantiate a new image cache and load the initial images
        let mut img_cache =  image_cache::ImageCache::new(
            _file_paths,
            2,
            initial_index,
        ).unwrap();
        img_cache.load_initial_images().unwrap();
        self.current_image = iced::widget::image::Handle::from_memory(img_cache.get_current_image().unwrap().to_vec());
        self.img_cache = img_cache;
    }

    fn update_pos(&mut self, pos: usize) {
        // self.current_image_index = pos;
        // self.slider_value = pos as u16;
        // self.title = format!("{}", self.img_cache.image_paths[pos].display());

        let file_paths = self.img_cache.image_paths.clone();

        let mut img_cache =  image_cache::ImageCache::new(
            file_paths,
            2,
            pos,
        ).unwrap();
        img_cache.load_initial_images().unwrap();
        self.current_image = iced::widget::image::Handle::from_memory(img_cache.get_current_image().unwrap().to_vec());
        self.img_cache = img_cache;

        let loaded_image = self.img_cache.get_current_image().unwrap().to_vec();
        self.current_image = iced::widget::image::Handle::from_memory(loaded_image);
    }

    fn move_left(&mut self) -> Command<Message> {
        // v1
        // self.img_cache.move_prev();
        // self.current_image = self.img_cache.get_current_image().unwrap().clone();

        // v2
        let img_cache = &mut self.img_cache;
        if img_cache.current_index <=0 {
            Command::none()
        } else {
            // let next_image_index = img_cache.current_index - 1; // WRONG
            let next_image_index: isize = img_cache.current_index as isize - img_cache.cache_count as isize - 1;
            if img_cache.is_next_image_index_in_queue(next_image_index) {
                if next_image_index < 0 {
                    // No new images to load but shift the cache
                    img_cache.enqueue_image_load(LoadOperation::ShiftPrevious(next_image_index));
                } else {
                    img_cache.enqueue_image_load(LoadOperation::LoadPrevious(next_image_index as usize));
                }
            }
            img_cache.print_queue();
            self.load_image_by_operation()
        }
    }

    fn move_right(&mut self) -> Command<Message> {
        // 1. Naive loading
        // self.image_path = "../data/landscape/".to_string() + &self.image_paths[self.current_image_index].clone();
        // println!("Image path: {}", self.image_path)

        // 2. Load from cache (sync)
        // load the image from cache now
        // STRATEGY: image at current_index: ALREADY LOADED in cache => set to self.current_image
        //      image at current_index + cache_count: NOT LOADED in cache => enqueue load operation

        // since it's a new image, update the cache
        if self.img_cache.image_paths.len() > 0 && self.img_cache.current_index < self.img_cache.image_paths.len() - 1 {
                        
            // let next_image_index = img_cache.current_index + 1; // WRONG
            let next_image_index = self.img_cache.current_index + self.img_cache.cache_count + 1;
            println!("NEXT_IMAGE_INDEX: {}", next_image_index);

            if self.img_cache.is_next_image_index_in_queue(next_image_index as isize) {
                if next_image_index >= self.img_cache.image_paths.len() {
                    // No new images to load, but shift the cache
                    self.img_cache.enqueue_image_load(LoadOperation::ShiftNext(next_image_index));
                } else {
                    self.img_cache.enqueue_image_load(LoadOperation::LoadNext(next_image_index));
                }

            }
            self.img_cache.print_queue();
            self.load_image_by_operation()
            // ImageViewer::load_image_by_operation_with_cache(&mut self.img_cache)
        } else {
            Command::none()
        }
    }
}


impl Application for DataViewer {
    type Message = Message;
    type Theme = Theme;
    type Executor= iced::executor::Default;
    type Flags = ();

    fn new(_flags: Self::Flags) -> (Self, Command<Self::Message>) {
        (
            Self {
                // image_path: String::new(),
                // image_paths: Vec::new(),
                // error: None,
                dir_loaded: false,
                directory_path: None,
                current_image_index: 0,
                img_cache: image_cache::ImageCache::default(),
                current_image: iced::widget::image::Handle::from_memory(vec![]),
                slider_value: 0,
                prev_slider_value: 0,
                num_files: 0,
                title: String::from("Data Viewer"),
            },
            Command::none()
        )

    }

    fn title(&self) -> String {
        self.title.clone()
    }

    fn update(&mut self, message: Message) -> Command<Self::Message> {
        match message {
            Message::Debug(s) => {
                self.title = s;
                Command::none()
            }
            Message::OpenFolder => {
                Command::perform(utils::pick_folder(), |result| {
                    Message::FolderOpened(result)
                })
            }
            Message::OpenFile => {
                Command::perform(utils::pick_file(), |result| {
                    Message::FolderOpened(result)
                })
            }
            Message::Close => {
                self.reset_state();
                // self.current_image = iced::widget::image::Handle::from_memory(vec![]);
                println!("directory_path: {:?}", self.directory_path);
                println!("self.current_image_index: {}", self.current_image_index);
                println!("self.img_cache.current_index: {}", self.img_cache.current_index);
                println!("self.img_cache.image_paths.len(): {}", self.img_cache.image_paths.len());
                Command::none()
            }

            Message::FolderOpened(result) => {
                match result {
                    Ok(dir) => {
                        println!("Folder opened: {}", dir);
                        self.initialize_dir_path(PathBuf::from(dir));

                        Command::none()
                    }
                    Err(err) => {
                        println!("Folder open failed: {:?}", err);
                        Command::none()
                    }
                }
            }

            Message::ImageLoaded (result) => {
                let img_cache = &mut self.img_cache;
                match result {
                    Ok((image_data, operation)) => {
                        let _ = img_cache.being_loaded_queue.pop_front();

                        // println!("Image loaded [before shift] img_cache.current_index: {:?}, operation: {:?}", img_cache.current_index, operation);
                        println!("    Image Loaded");
                        if let Some(op) = operation {
                            match op {
                                LoadOperation::LoadNext(_target_index) => {
                                    let _ = img_cache.move_next(image_data);
                                }
                                LoadOperation::LoadPrevious(_target_index) => {
                                    let _ = img_cache.move_prev(image_data);
                                }
                                LoadOperation::ShiftNext(_target_index) => {
                                    let _ = img_cache.move_next(None);
                                }
                                LoadOperation::ShiftPrevious(_target_index) => {
                                    let _ = img_cache.move_prev(None);
                                }
                            }
                        }
                        let loaded_image = img_cache.get_current_image().unwrap().to_vec();
                        self.current_image = iced::widget::image::Handle::from_memory(loaded_image);
                        self.current_image_index = img_cache.current_index;
                        self.slider_value = img_cache.current_index as u16;
                        self.title = format!("{}", img_cache.image_paths[img_cache.current_index].display());
                        
                        // println!("loading_queue length: {}", img_cache.loading_queue.len());
                        let command = self.load_image_by_operation();
                        // println!("Current image index: {}", self.current_image_index);
                        command
                            
                    }
                    Err(err) => {
                        println!("Image load failed: {:?}", err);
                        Command::none()
                    }
                }

            }

            Message::SliderChanged(value) => {
                self.prev_slider_value = self.slider_value;
                self.slider_value = value;

                if value == self.prev_slider_value + 1 {
                    // Value changed by +1
                    // Call a function or perform an action for this case
                    self.move_right()

                } else if value == self.prev_slider_value - 1 {
                    // Value changed by -1
                    // Call a different function or perform an action for this case
                    self.move_left()
                } else {
                    // Value changed by more than 1 or it's the initial change
                    // Call another function or handle this case differently
                    self.update_pos(value as usize);
                    Command::none()
                }
            }


            Message::Event(event) => match event {
                Event::Window(iced::window::Event::FileDropped(dropped_path)) => {
                    println!("File dropped: {:?}", dropped_path);

                    self.initialize_dir_path(dropped_path);
                    
                    Command::none()
                }

                Event::Keyboard(keyboard::Event::KeyPressed {
                    key_code: keyboard::KeyCode::Tab,
                    modifiers: _,
                }) => {
                    println!("Tab pressed");
                    Command::none()
                }

                Event::Keyboard(keyboard::Event::KeyPressed {
                    key_code: keyboard::KeyCode::Right,
                    modifiers: _,
                }) => {
                    println!("ArrowRight pressed");
                    self.move_right()
                }


                Event::Keyboard(keyboard::Event::KeyPressed {
                    key_code: keyboard::KeyCode::Left,
                    modifiers: _,
                }) => {
                    println!("ArrowLeft pressed");
                    self.move_left()
                }

                _ => Command::none(),
            },
        }
    }

    fn view(&self) -> Element<Message> {
        let mb =  { menu_bar!(ui::menu_1())
                    .item_width(ItemWidth::Uniform(180))
                    .item_height(ItemHeight::Uniform(27)) }
                    .spacing(4.0)
                    .bounds_expand(30)
                    .main_offset(13)
                    .cross_offset(16)
                    .path_highlight(Some(PathHighlight::MenuActive))
                    .close_condition(CloseCondition {
                        leave: true,
                        click_outside: false,
                        click_inside: false,
                    });
        let r = row!(mb, horizontal_space(Length::Fill))
            .padding([2, 8])
            .align_items(alignment::Alignment::Center);
        let top_bar_style: fn(&iced::Theme) -> container::Appearance =
            |_theme| container::Appearance {
                background: Some(Color::TRANSPARENT.into()),
                ..Default::default()
            };
        let top_bar = container(r).width(Length::Fill).style(top_bar_style);

        let h_slider: iced::widget::Slider<u16, Message>;
        if self.dir_loaded {
            h_slider =
                slider(0..= (self.num_files-1) as u16, self.slider_value, Message::SliderChanged)
                    .width(Length::Fill);
        } else {
            h_slider =
                slider(0..= 0 as u16, 0, Message::SliderChanged)
                    .width(Length::Fill);
        }


        let image: Element<Message> = Image::new(self.current_image.clone())
            .width(Length::Fill)
            .height(Length::Fill)
            .into();

        let container = if self.dir_loaded {
            container(
                column![
                    top_bar,
                    image,
                    h_slider,
                ]
                .spacing(25),
            )
            .center_y()
        } else {
            container(
                column![
                    top_bar,
                ]
                .spacing(25),
            )

        };
        
        container
        .height(Length::Fill)
        .width(Length::Fill)
        .center_x()
        // .title(format!("{}", current_image_path.display()))
        //.title(current_image_path.to_string_lossy().to_string())
        .into()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        // subscription::events().map(Message::Event)

        Subscription::batch(vec![
            subscription::events().map(Message::Event),
        ])
    }

    fn theme(&self) -> Self::Theme {
        Theme::Dark
    }
}

fn main() -> iced::Result {
    env_logger::init();
    DataViewer::run(Settings::default())
}