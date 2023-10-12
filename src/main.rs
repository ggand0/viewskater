use iced::alignment::{Horizontal, Vertical};
use iced::theme;
use iced::widget::{
    button, checkbox, column, container, horizontal_space, image, radio, row,
    scrollable, slider, text, text_input, toggler, vertical_space,
};
use iced::widget::{Button, Image, Text, Row, Column, Container, Slider};
use iced::{Color, Element, Font, Length, Pixels, Renderer, Sandbox, Settings};

// Define the application state
#[derive(Default)]
struct ImageViewer {
    image_path: String,
    load_button_state: button::State,
}

// Define application messages
#[derive(Debug, Clone)]
enum Message {
    LoadImage,
}


impl Sandbox for ImageViewer {
    type Message = Message;

    fn new() -> Self {
        ImageViewer::default()
    }

    fn title(&self) -> String {
        String::from("Image Viewer")
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::LoadImage => {
                // Simulate loading an image (replace with actual image loading logic)
                self.image_path = "sample.jpg".to_string();
            }
        }
    }

    fn view(&self) -> Element<Message> {
        let image: Element<Message> = if self.image_path.is_empty() {
            Text::new("No image loaded")
                .size(30)
                .width(Length::Fill)
                .height(Length::Fill)
                .horizontal_alignment(Horizontal::Center)
                .vertical_alignment(Vertical::Center)
                .into()
        } else {
            Image::new(&self.image_path)
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        };

        // let load_button: Element<Message> = Button::new(&mut self.load_button_state, Text::new("Load Image"))
        let load_button: Element<Message> = button("Load Image")
            .on_press(Message::LoadImage)
            .into();

        // Create a simple UI layout with a button and the image
        let content: Element<Message> = Column::new()
            .push(load_button)
            .push(Row::new().push(image).spacing(10))
            .spacing(20)
            .padding(20)
            .into();

        // Wrap the content in a container
        Container::new(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x()
            .center_y()
            .into()
    }
}

fn main() -> iced::Result {
    ImageViewer::run(Settings::default())
}