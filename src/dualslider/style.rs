//! Change the apperance of a slider.

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
// use iced_core::{BorderRadius, Color};
use iced_widget::{
    core::{BorderRadius, Color},
    style::Theme, //style::Theme::Light,
};

/// The appearance of a slider.
#[derive(Debug, Clone, Copy)]
pub struct Appearance {
    /// The colors of the rail of the slider.
    pub rail: Rail,
    /// The appearance of the [`Handle`] of the slider.
    pub handle: Handle,
}

/// The appearance of a slider rail
#[derive(Debug, Clone, Copy)]
pub struct Rail {
    /// The colors of the rail of the slider.
    pub colors: (Color, Color),
    /// The width of the stroke of a slider rail.
    pub width: f32,
    /// The border radius of the corners of the rail.
    pub border_radius: BorderRadius,
}

/// The appearance of the handle of a slider.
#[derive(Debug, Clone, Copy)]
pub struct Handle {
    /// The shape of the handle.
    pub shape: HandleShape,
    /// The [`Color`] of the handle.
    pub color: Color,
    /// The border width of the handle.
    pub border_width: f32,
    /// The border [`Color`] of the handle.
    pub border_color: Color,
}

/// The shape of the handle of a slider.
#[derive(Debug, Clone, Copy)]
pub enum HandleShape {
    /// A circular handle.
    Circle {
        /// The radius of the circle.
        radius: f32,
    },
    /// A rectangular shape.
    Rectangle {
        /// The width of the rectangle.
        width: u16,
        /// The border radius of the corners of the rectangle.
        border_radius: BorderRadius,
    },
}

/// A set of rules that dictate the style of a slider.
pub trait StyleSheet {
    /// The supported style of the [`StyleSheet`].
    // type Style: Default;
    type Style: Default;

    /// Produces the style of an active slider.
    fn active(&self, style: &Self::Style) -> Appearance;

    /// Produces the style of an hovered slider.
    fn hovered(&self, style: &Self::Style) -> Appearance;

    /// Produces the style of a slider that is being dragged.
    fn dragging(&self, style: &Self::Style) -> Appearance;
}

/// The default appearance of the [`DualSlider`](crate::dualslider::DualSlider).
#[derive(Default)]
pub enum DualSliderStyles {
    #[default]
    Default,
    Custom(Box<dyn StyleSheet<Style = Theme>>),
}

impl DualSliderStyles {
    /// Creates a custom [`DualSliderStyles`] style variant.
    pub fn custom(style_sheet: impl StyleSheet<Style = Theme> + 'static) -> Self {
        Self::Custom(Box::new(style_sheet))
    }
}

impl StyleSheet for Theme {
    type Style = DualSliderStyles;

    fn active(&self, _style: &Self::Style) -> Appearance {
        // Define default appearance for an active DualSliderStyles
        // You can provide default colors, sizes, etc.
        // Example:
        /*Appearance {
            rail: Rail {
                colors: (Color::BLACK, Color::WHITE),
                width: 2.0,
                border_radius: BorderRadius::default(),
            },
            handle: Handle {
                shape: HandleShape::Rectangle {
                    width: 10,
                    border_radius: BorderRadius::default(),
                },
                color: Color::BLACK,
                border_width: 1.0,
                border_color: Color::BLACK,
            },
        }*/
        Appearance {
            rail: Rail {
                colors: (Color::from_rgb(60.0, 60.0, 60.0), Color::from_rgb(100.0, 100.0, 100.0)),
                width: 2.0,
                border_radius: BorderRadius::default(),
            },
            handle: Handle {
                shape: HandleShape::Rectangle {
                    width: 10,
                    border_radius: BorderRadius::default(),
                },
                color: Color::from_rgb(150.0, 150.0, 150.0),
                border_width: 1.0,
                border_color: Color::from_rgb(150.0, 150.0, 150.0),
            },
        }
    }

    fn hovered(&self, _style: &Self::Style) -> Appearance {
        // Define default appearance for a hovered DualSliderStyles
        // This can have different colors or sizes when the slider is hovered
        // Return a different Appearance instance if needed
        self.active(_style) // Example: Using the same appearance as active for now
    }

    fn dragging(&self, _style: &Self::Style) -> Appearance {
        // Define default appearance for a dragging DualSliderStyles
        // This can have different colors or sizes when the slider is being dragged
        // Return a different Appearance instance if needed
        self.active(_style) // Example: Using the same appearance as active for now
    }
}
