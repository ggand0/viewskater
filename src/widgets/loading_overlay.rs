use iced_winit::core::{Color, Element};
use iced_widget::{container, stack, center, opaque};
use iced_wgpu::Renderer;
use iced_winit::core::Theme as WinitTheme;

use super::circular::{circular, CircularState};

/// Wraps content with an optional loading spinner overlay.
/// When `show_spinner` is true, displays a semi-transparent backdrop
/// with a centered circular spinner.
pub fn loading_overlay<'a, Message>(
    base: impl Into<Element<'a, Message, WinitTheme, Renderer>>,
    show_spinner: bool,
    spinner_state: &'a CircularState,
) -> Element<'a, Message, WinitTheme, Renderer>
where
    Message: Clone + 'a,
{
    if show_spinner {
        stack![
            base.into(),
            opaque(
                center(circular(spinner_state))
                    .style(|_theme| {
                        container::Style {
                            background: Some(
                                Color {
                                    a: 0.5,
                                    ..Color::BLACK
                                }
                                .into(),
                            ),
                            ..container::Style::default()
                        }
                    })
            )
        ]
        .into()
    } else {
        base.into()
    }
}
