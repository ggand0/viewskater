use iced::Settings;
use iced::pure::Sandbox;
use iced::pure::widget::{Button, Text, Column, Container};

struct Counter {
    count: i32
}

#[derive(Debug, Clone, Copy)]
enum CounterMessage {
    Increment,
    Decrement
}

impl Sandbox for Counter {
    type Message = CounterMessage;

    fn new() -> Self {
        Counter{ count: 0 }
    }

    fn title(&self) -> String {
        String::from("Counter app")
    }

    fn update(&mut self, message: Self::Message) {
        match message {
            CounterMessage::Increment => self.count += 1,
            CounterMessage::Decrement => self.count -= 1
        }
    }

    fn view(&self) -> iced::pure::Element<'_, Self::Message> {
        // todo!()
        let label = Text::new(format!("Count: {}", self.count));
        let incr = Button::new("Increment").on_press(CounterMessage::Increment);
        let decr = Button::new("Decrement").on_press(CounterMessage::Decrement);
        let col = Column::new().push(incr).push(label).push(decr);
        Container::new(col).center_x().center_y().width(iced::Length::Fill).height(iced::Length::Fill).into()
    }
}

fn main() -> Result<(), iced::Error> {
    Counter::run(Settings::default())
}
