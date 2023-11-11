// use iced::alignment::{Horizontal, Vertical};
use iced::event::{self, Event};
use iced::subscription::{self, Subscription};
// use iced::theme;
// use iced::executor;
use iced::keyboard;
use iced::widget::{
    button, checkbox, column, container, horizontal_space, image, radio, row,
    scrollable, slider, text, text_input, toggler, vertical_space,
};
use iced::widget::{Image, Container};
use iced::{Color, Element, Font, Length, Pixels, Renderer, Application, Theme, Settings, Command};

use std::fs;
use std::path::Path;
use std::path::PathBuf;
// use log::Level;

use tokio::fs::File;
use tokio::io::AsyncReadExt;
use std::io::Read;

// #[macro_use]
extern crate log;

mod image_cache;
use image_cache::LoadOperation;
use std::collections::VecDeque;
use std::sync::Arc;

// Define the application state
// #[derive(Default)]
struct ImageViewer {
    dir_loaded: bool,
    // load_button_state: button::State,
    image_path: String,
    current_image_index: usize,

    image_paths: Vec<String>,
    image_cache: image_cache::ImageCache,
    // current_image: Option<iced::widget::Image<iced::widget::image::Handle>>,
    current_image: iced::widget::image::Handle,

    slider_value: u16,
}

impl Default for ImageViewer {
    fn default() -> Self {
        Self {
            dir_loaded: false,
            // load_button_state: button::State::new(),
            image_path: String::new(),
            current_image_index: 0,
            image_paths: Vec::new(),
            image_cache: image_cache::ImageCache::new("../data/landscape", 10).unwrap(),
            current_image: iced::widget::image::Handle::from_memory(vec![]),
            slider_value: 0,
        }
    }
}

// Define application messages
#[derive(Debug, Clone)]
enum Message {
    LoadImage,
    SliderChanged(u16),
    Event(Event),
    // ImageLoaded(Result<(), std::io::ErrorKind>),// std::io::Error doesn't seem to be clonable
    // ImageLoaded(Result<Option<Vec<u8>>, std::io::ErrorKind>),
    ImageLoaded(Result<(Option<Vec<u8>>, Option<LoadOperation>), std::io::ErrorKind>)
}


async fn async_load_image(path: impl AsRef<Path>, operation: LoadOperation) -> Result<(Option<Vec<u8>>, Option<LoadOperation>), std::io::ErrorKind> {
    let file_path = path.as_ref();

    match tokio::fs::File::open(file_path).await {
        Ok(mut file) => {
            let mut buffer = Vec::new();
            if file.read_to_end(&mut buffer).await.is_ok() {
                // Ok(Some(buffer))
                Ok((Some(buffer), Some(operation) ))
            } else {
                Err(std::io::ErrorKind::InvalidData)
            }
        }
        Err(e) => Err(e.kind()),
    }
}

impl ImageViewer {
    // Define your custom function
    fn my_custom_function(&mut self) {
        // Implement your custom logic here
        println!("My custom function is called!");
    }

    fn load_image_by_index(&mut self, target_index: usize, operation: LoadOperation) -> Command<<ImageViewer as iced::Application>::Message> {
        let path = self.image_cache.image_paths.get(target_index);
        if let Some(path) = path {
            println!("Path: {}", path.clone().to_string_lossy());
            let image_loading_task = async_load_image(path.clone(), operation);
            Command::perform(image_loading_task, Message::ImageLoaded)
            
        } else {
            // Handle the case when there are no more images to load
            // You can return an empty Command or some other variant as needed
            Command::none()
        }
    }

    // , loading_queue: VecDeque<LoadOperation>
    fn load_image_by_operation(&mut self) -> Command<<ImageViewer as iced::Application>::Message> {
        println!("load_image_by_operation");
        println!("loading_queue length: {}", self.image_cache.loading_queue.len());
        if !self.image_cache.loading_queue.is_empty() {
        // if !loading_queue.is_empty() {
            println!("load_image_by_operation: not empty");
            if let Some(operation) = self.image_cache.loading_queue.pop_front() {
                println!("load_image_by_operation: operation: {:?}", operation);
                let loaded_image = self.image_cache.get_current_image().unwrap().to_vec();
                self.current_image = iced::widget::image::Handle::from_memory(loaded_image);

                self.current_image_index = self.image_cache.current_index;
                self.slider_value = self.current_image_index as u16;
                println!("Current image index at base struct: {}", self.current_image_index);
                println!("image_cache.current_index: {}", self.image_cache.current_index);
                match operation {
                    LoadOperation::LoadNext(target_index) => {
                        self.image_cache.move_next(None);
                        self.load_image_by_index(target_index, operation);
                    }
                    LoadOperation::LoadPrevious(target_index) => {
                        self.image_cache.move_prev(None);
                        self.load_image_by_index(target_index, operation);
                    }
                }

                

                Command::none()
            } else {
                // Handle the case when there are no more images to load
                // You can return an empty Command or some other variant as needed
                Command::none()
            }
        } else {
            Command::none()
        }   
    }
}


impl Application for ImageViewer {
    type Message = Message;
    type Theme = Theme;
    type Executor= iced::executor::Default;
    type Flags = ();

    fn new(flags: Self::Flags) -> (Self, Command<Self::Message>) {

        let mut image_cache =  image_cache::ImageCache::new("../data/landscape", 10).unwrap();
        image_cache.load_initial_images().unwrap();
        
        (
            Self {
                dir_loaded: false,
                // load_button_state: button::State::new(),
                image_path: String::new(),
                current_image_index: 0,
                image_paths: Vec::new(),
                image_cache: image_cache,
                current_image: iced::widget::image::Handle::from_memory(vec![]),
                slider_value: 0,
                
            },
            Command::none()
            // Command::perform(async_load_image(
            //     Path::new("../data/landscape/00000000.jpg")), Message::ImageLoaded)
            
        )

    }

    fn title(&self) -> String {
        String::from("Image Viewer")
    }

    fn update(&mut self, message: Message) -> Command<Self::Message> {
        match message {
            Message::LoadImage => {
                // Simulate loading an image (replace with actual image loading logic)
                //self.image_path = "sample.jpg".to_string();

                // Trying to load images in a directory
                let data_dir = "../data/landscape";
                let mut file_paths: Vec<String> = Vec::new();
                let paths = fs::read_dir(data_dir).unwrap();
                for entry in paths {
                    if let Ok(entry) = entry {
                        if let Some(file_name) = entry.file_name().to_str() {
                            // Convert the file name to a String and add it to the vector
                            file_paths.push(file_name.to_string());
                        }
                    }
                }
                self.image_paths = file_paths.clone();
                println!("File paths: {}", file_paths.len());
                let file_name = file_paths.get(0).cloned().unwrap_or_default();

                self.image_path = Path::new(data_dir).join(file_name).to_string_lossy().to_string();
                println!("Image path: {}", self.image_path);
                self.dir_loaded = true;

                Command::none()
            }
            Message::ImageLoaded (result) => {
                match result {
                    Ok((image_data, operation)) => {
                        if let Some(op) = operation {
                            match op {
                                LoadOperation::LoadNext(target_index) => {
                                    self.image_cache.move_next(image_data);
                                }
                                LoadOperation::LoadPrevious(target_index) => {
                                    self.image_cache.move_prev(image_data);
                                }
                            }
                        }
                        let loaded_image = self.image_cache.get_current_image().unwrap().to_vec();
                        self.current_image = iced::widget::image::Handle::from_memory(loaded_image);
                        self.current_image_index = self.image_cache.current_index;
                        println!("loading_queue length: {}", self.image_cache.loading_queue.len());
                        let command = self.load_image_by_operation();
                        println!("Current image index: {}", self.current_image_index);
                        command
                            
                    }
                    Err(err) => {
                        // println!("Image load failed");
                        println!("Image load failed: {:?}", err);
                        Command::none()
                    }
                }
            }

            Message::SliderChanged(value) => {
                self.slider_value = value;
                // Create an async command to perform the async task
                /*let command = Command::perform(async {
                    // Call your async function here
                    let new_index = value as usize;
                    self.image_cache.on_slider_value_changed(new_index).await;

                    self.slider_value = new_index as u16;
                    self.current_image_index = new_index;
                    println!("Current image index: {}", self.current_image_index);
                    self.current_image = self.image_cache.get_current_image().unwrap().to_vec();

                    // Return a result or a unit value as needed
                    Ok(())
                }, |result| {
                    // Handle the result of the async task
                    match result {
                        Ok(_) => Message::AsyncTaskCompleted,
                        Err(_) => Message::AsyncTaskFailed,
                    }
                });

                // Dispatch the command
                Some(command);*/

                /*println!("Slider value: {}", value);
                let new_index = value as usize;
                

                self.slider_value = new_index as u16;
                self.current_image_index = new_index;
                
                
                Command::perform(slider_update(new_index))*/

                Command::none()
            }


            Message::Event(event) => match event {
                Event::Keyboard(keyboard::Event::KeyPressed {
                    key_code: keyboard::KeyCode::Tab,
                    modifiers,
                }) => {
                    println!("Tab pressed");
                    Command::none()

                }

                Event::Keyboard(keyboard::Event::KeyPressed {
                    key_code: keyboard::KeyCode::Right,
                    modifiers,
                }) => {
                    println!("ArrowRight pressed");

                    // 1. Naive loading
                    // self.image_path = "../data/landscape/".to_string() + &self.image_paths[self.current_image_index].clone();
                    // println!("Image path: {}", self.image_path)

                    // 2. Load from cache (sync)
                    // load the image from cache now
                    let loaded_image = self.image_cache.get_current_image().unwrap().to_vec();
                    self.current_image = iced::widget::image::Handle::from_memory(loaded_image);


                    // since it's a new image, update the cache
                    if self.image_cache.current_index < self.image_cache.image_paths.len() - 1 {
                        
                        let next_image_index = self.image_cache.current_index + 1;
                        if self.image_cache.loading_queue.iter().all(|op| match op {
                            LoadOperation::LoadNext(index) => index != &next_image_index,
                            LoadOperation::LoadPrevious(index) => index != &next_image_index,
                        }) {
                            // The next image index is not in the queue, so you can enqueue the load operation.
                            println!("Enqueue next image load operation");
                            self.image_cache.enqueue_image_load(LoadOperation::LoadNext(next_image_index));
                            self.slider_value = next_image_index as u16;
                        }

                        if !self.image_cache.loading_queue.is_empty() {
                            println!("Right: load_image_by_operation: not empty");
                            if let Some(operation) = self.image_cache.loading_queue.pop_front() {   
                                // println!("load_image_by_operation: operation: {:?}", operation);
                                // self.current_image = self.image_cache.get_current_image().unwrap().to_vec();
                                // self.current_image_index = self.image_cache.current_index;
                                // println!("Current image index at base struct: {}", self.current_image_index);
                                match operation {
                                    LoadOperation::LoadNext(target_index) => {
                                        println!("Load next image target_index: {}", target_index);
                                        self.load_image_by_index(target_index, operation)
                                    }
                                    LoadOperation::LoadPrevious(target_index) => {
                                        self.load_image_by_index(target_index, operation)
                                    }
                                }
                
                                // Command::none()
                            } else {
                                // Handle the case when there are no more images to load
                                // You can return an empty Command or some other variant as needed
                                Command::none()
                            }
                        } else {
                            Command::none()
                        }
                    } else {
                        Command::none()
                    }
                }

                Event::Keyboard(keyboard::Event::KeyPressed {
                    key_code: keyboard::KeyCode::Left,
                    modifiers,
                }) => {
                    println!("ArrowLeft pressed");
                    // v1
                    // self.image_cache.move_prev();
                    // self.current_image = self.image_cache.get_current_image().unwrap().clone();

                    // v2
                    println!("debug0");
                    if self.image_cache.current_index <=0 {
                        Command::none()
                    } else {
                        let next_image_index = self.image_cache.current_index - 1;
                        println!("Left: loading_queue length: {}", self.image_cache.loading_queue.len());

                        if self.image_cache.loading_queue.iter().all(|op| match op {
                            LoadOperation::LoadNext(index) => index != &next_image_index,
                            LoadOperation::LoadPrevious(index) => index != &next_image_index,
                        }) {
                            // The next image index is not in the queue, so you can enqueue the load operation.
                            println!("Enqueue next image load operation");
                            self.image_cache.enqueue_image_load(LoadOperation::LoadPrevious(next_image_index));
                            self.slider_value = next_image_index as u16;
                        }

                        if !self.image_cache.loading_queue.is_empty() {
                            println!("Left: load_image_by_operation: not empty");
                            if let Some(operation) = self.image_cache.loading_queue.pop_front() {   
                                match operation {
                                    LoadOperation::LoadNext(target_index) => {
                                        println!("Load next image target_index: {}", target_index);
                                        self.load_image_by_index(target_index, operation)
                                    }
                                    LoadOperation::LoadPrevious(target_index) => {
                                        self.load_image_by_index(target_index, operation)
                                    }
                                }
                
                                // Command::none()
                            } else {
                                // Handle the case when there are no more images to load
                                // You can return an empty Command or some other variant as needed
                                Command::none()
                                
                            }
                        } else {
                            Command::none()
                        }
                    }

                }

                _ => Command::none(),
            },
        }
    }

    fn view(&self) -> Element<Message> {
        // let image: Element<Message> = Image::new(iced::widget::image::Handle::from_memory(self.current_image.clone()))
        /*let image: Element<Message> = Image::new(iced::widget::image::Handle::from_memory(self.current_image))
            .width(Length::Fill)
            .height(Length::Fill)
            .into();*/

        // let load_button: Element<Message> = Button::new(&mut self.load_button_state, Text::new("Load Image"))
        let load_button: Element<Message> = button("Load Image")
            .on_press(Message::LoadImage)
            .into();

        let h_slider =
            slider(0..= (self.image_cache.image_paths.len()-1) as u16, self.slider_value, Message::SliderChanged)
                .width(Length::Fill);


        let image: Element<Message> = Image::new(self.current_image.clone())
            .width(Length::Fill)
            .height(Length::Fill)
            .into();

        container(
            column![
                // load_button,
                image,
                h_slider,
            ]
            .spacing(25),
        )
        .height(Length::Fill)
        .width(Length::Fill)
        .center_x()
        .center_y()
        .into()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        subscription::events().map(Message::Event)
    }
}

fn main() -> iced::Result {
    env_logger::init();
    // console_log::init().expect("Initialize logger");
    // console_log::init_with_level(Level::Debug);

    ImageViewer::run(Settings::default())
}