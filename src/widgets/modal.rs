//#[cfg(target_os = "linux")]
//mod other_os {
//    //pub use iced;
//    pub use iced_custom as iced;
//}
//
//#[cfg(not(target_os = "linux"))]
//mod macos {
//    pub use iced_custom as iced;
//}
//
//#[cfg(target_os = "linux")]
//use other_os::*;
//
//#[cfg(not(target_os = "linux"))]
//use macos::*;

use iced_winit::core::{
    Color, Element
};
use iced_widget::{container, stack, mouse_area, center, opaque};
use iced_wgpu::Renderer;
use iced_winit::core::Theme as WinitTheme;

pub fn modal<'a, Message>(
    base: impl Into<Element<'a, Message, WinitTheme, Renderer>>,
    content: impl Into<Element<'a, Message, WinitTheme, Renderer>>,
    on_blur: Message,
) -> Element<'a, Message, WinitTheme, Renderer>
where
    Message: Clone + 'a,
{
    stack![
        base.into(),
        opaque(
            mouse_area(center(opaque(content)).style(|_theme| {
                container::Style {
                    background: Some(
                        Color {
                            a: 0.8,
                            ..Color::BLACK
                        }
                        .into(),
                    ),
                    ..container::Style::default()
                }
            }))
            .on_press(on_blur)
        )
    ]
    .into()
}