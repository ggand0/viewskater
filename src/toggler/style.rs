//! Change the appearance of a toggler.
//use iced_core::Color;

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

use iced_widget::{
    core::{Background, Color},
    style::Theme,
};


/// The appearance of a [`Split`](crate::native::split::Split).
#[allow(missing_docs, clippy::missing_docs_in_private_items)]
pub trait StyleSheet {
    type Style: Default;
    /// The normal appearance of a [`Split`](crate::native::split::Split).
    fn active(&self, style: &Self::Style, is_active: bool) -> Appearance;

    /// The appearance when the [`Split`](crate::native::split::Split) is hovered.
    fn hovered(&self, style: &Self::Style, is_active: bool) -> Appearance;
}

/// The appearance of a toggler.
#[derive(Debug, Clone, Copy)]
pub struct Appearance {
    /// The background [`Color`] of the toggler.
    pub background: Color,
    /// The [`Color`] of the background border of the toggler.
    pub background_border: Option<Color>,
    /// The foreground [`Color`] of the toggler.
    pub foreground: Color,
    /// The [`Color`] of the foreground border of the toggler.
    pub foreground_border: Option<Color>,
}


pub enum TogglerStyles {
    Default,
    Custom,
}

impl std::default::Default for TogglerStyles {
    fn default() -> Self {
        Self::Default
    }
}

impl StyleSheet for Theme {
    type Style = TogglerStyles;

    fn active(&self, style: &Self::Style, is_active: bool) -> Appearance {
        if is_active {
            match style {
                TogglerStyles::Default => Appearance {
                    background: Color::from_rgba8(20, 148, 163, 1.0),
                    background_border: None,
                    foreground: Color::WHITE,
                    foreground_border: None,
                },
                TogglerStyles::Custom => unimplemented!(),
            }
        } else {
            match style {
                TogglerStyles::Default => Appearance {
                    background: Color::BLACK,
                    background_border: None,
                    foreground: Color::WHITE,
                    foreground_border: None,
                },
                TogglerStyles::Custom => unimplemented!(),
            }
        }
        
    }

    fn hovered(&self, style: &Self::Style, is_active: bool) -> Appearance {
        if is_active {
            match style {
                TogglerStyles::Default => Appearance {
                    background: Color::from_rgba8(20, 148, 163, 1.0),
                    background_border: None,
                    foreground: Color::WHITE,
                    foreground_border: None,
                },
                TogglerStyles::Custom => unimplemented!(),
            }
        } else {
            match style {
                TogglerStyles::Default => Appearance {
                    background: Color::BLACK,
                    background_border: None,
                    foreground: Color::WHITE,
                    foreground_border: None,
                },
                TogglerStyles::Custom => unimplemented!(),
            }
        }
    }
}