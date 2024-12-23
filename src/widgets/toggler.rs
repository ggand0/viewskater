//! Togglers let users make binary choices by toggling a switch.
//!
//! # Example
//! ```no_run
//! # mod iced { pub mod widget { pub use iced_widget::*; } pub use iced_widget::Renderer; pub use iced_widget::core::*; }
//! # pub type Element<'a, Message> = iced_widget::core::Element<'a, Message, iced_widget::Theme, iced_widget::Renderer>;
//! #
//! use iced::widget::toggler;
//!
//! struct State {
//!    is_checked: bool,
//! }
//!
//! enum Message {
//!     TogglerToggled(bool),
//! }
//!
//! fn view(state: &State) -> Element<'_, Message> {
//!     toggler(state.is_checked)
//!         .label("Toggle me!")
//!         .on_toggle(Message::TogglerToggled)
//!         .into()
//! }
//!
//! fn update(state: &mut State, message: Message) {
//!     match message {
//!         Message::TogglerToggled(is_checked) => {
//!             state.is_checked = is_checked;
//!         }
//!     }
//! }
//! ```
/*use crate::core::alignment;
use crate::core::event;
use crate::core::layout;
use crate::core::mouse;
use crate::core::renderer;
use crate::core::text;
use crate::core::touch;
use crate::core::widget;
use crate::core::widget::tree::{self, Tree};
use crate::core::{
    Border, Clipboard, Color, Element, Event, Layout, Length, Pixels,
    Rectangle, Shell, Size, Theme, Widget,
};*/
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

use iced::{
    alignment, event, touch,
    advanced::{
        layout, mouse, renderer, text, widget,
        widget::tree::{self, Tree},
        Widget, Clipboard, Shell, Layout
    },
    border::Radius,
    Border, Color, Element, Event, Length, Pixels, Padding,
    Rectangle, Size, Theme,
};
use std::borrow::Cow;
#[allow(unused_imports)]
use log::{Level, debug, info, warn, error};

/// A toggler widget.
///
/// # Example
/// ```no_run
/// # mod iced { pub mod widget { pub use iced_widget::*; } pub use iced_widget::Renderer; pub use iced_widget::core::*; }
/// # pub type Element<'a, Message> = iced_widget::core::Element<'a, Message, iced_widget::Theme, iced_widget::Renderer>;
/// #
/// use iced::widget::toggler;
///
/// struct State {
///    is_checked: bool,
/// }
///
/// enum Message {
///     TogglerToggled(bool),
/// }
///
/// fn view(state: &State) -> Element<'_, Message> {
///     toggler(state.is_checked)
///         .label("Toggle me!")
///         .on_toggle(Message::TogglerToggled)
///         .into()
/// }
///
/// fn update(state: &mut State, message: Message) {
///     match message {
///         Message::TogglerToggled(is_checked) => {
///             state.is_checked = is_checked;
///         }
///     }
/// }
/// ```
#[allow(missing_debug_implementations)]
pub struct Toggler<
    'a,
    Message,
    Theme = crate::Theme,
    //Renderer = crate::Renderer,
    Renderer = iced::Renderer,
> where
    Theme: Catalog,
    Renderer: text::Renderer,
{
    is_toggled: bool,
    on_toggle: Option<Box<dyn Fn(bool) -> Message + 'a>>,
    label: Option<text::Fragment<'a>>,
    width: Length,
    size: f32,
    text_size: Option<Pixels>,
    text_line_height: text::LineHeight,
    text_alignment: alignment::Horizontal,
    text_shaping: text::Shaping,
    text_wrapping: text::Wrapping,
    spacing: f32,
    padding: Padding,
    font: Option<Renderer::Font>,
    class: Theme::Class<'a>,
}

impl<'a, Message, Theme, Renderer> Toggler<'a, Message, Theme, Renderer>
where
    Theme: Catalog,
    Renderer: text::Renderer,
{
    /// The default size of a [`Toggler`].
    pub const DEFAULT_SIZE: f32 = 16.0;

    /// Creates a new [`Toggler`].
    ///
    /// It expects:
    ///   * a boolean describing whether the [`Toggler`] is checked or not
    ///   * An optional label for the [`Toggler`]
    ///   * a function that will be called when the [`Toggler`] is toggled. It
    ///     will receive the new state of the [`Toggler`] and must produce a
    ///     `Message`.
    //pub fn new(is_toggled: bool) -> Self {
    pub fn new<F>(
        label: impl Into<Option<String>>,
        is_toggled: bool,
        f: F,
    ) -> Self
    where
        F: 'a + Fn(bool) -> Message,
    {
        Toggler {
            is_toggled,
            on_toggle: Some(Box::new(f)),
            label: label.into().map(Cow::Owned),
            width: Length::Shrink,
            size: Self::DEFAULT_SIZE,
            //text_size: None,
            text_size: Some(iced::Pixels(14.0)),
            text_line_height: text::LineHeight::default(),
            text_alignment: alignment::Horizontal::Left,
            text_shaping: text::Shaping::default(),
            text_wrapping: text::Wrapping::default(),
            //spacing: Self::DEFAULT_SIZE / 2.0,
            spacing: 0.0,
            padding: DEFAULT_PADDING,
            font: None,
            class: Theme::default(),
        }
    }

    /// Sets the label of the [`Toggler`].
    pub fn label(mut self, label: impl text::IntoFragment<'a>) -> Self {
        self.label = Some(label.into_fragment());
        self
    }

    /// Sets the message that should be produced when a user toggles
    /// the [`Toggler`].
    ///
    /// If this method is not called, the [`Toggler`] will be disabled.
    pub fn on_toggle(
        mut self,
        on_toggle: impl Fn(bool) -> Message + 'a,
    ) -> Self {
        self.on_toggle = Some(Box::new(on_toggle));
        self
    }

    /// Sets the message that should be produced when a user toggles
    /// the [`Toggler`], if `Some`.
    ///
    /// If `None`, the [`Toggler`] will be disabled.
    pub fn on_toggle_maybe(
        mut self,
        on_toggle: Option<impl Fn(bool) -> Message + 'a>,
    ) -> Self {
        self.on_toggle = on_toggle.map(|on_toggle| Box::new(on_toggle) as _);
        self
    }

    /// Sets the size of the [`Toggler`].
    pub fn size(mut self, size: impl Into<Pixels>) -> Self {
        self.size = size.into().0;
        self
    }

    /// Sets the width of the [`Toggler`].
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    /// Sets the text size o the [`Toggler`].
    pub fn text_size(mut self, text_size: impl Into<Pixels>) -> Self {
        self.text_size = Some(text_size.into());
        self
    }

    /// Sets the text [`text::LineHeight`] of the [`Toggler`].
    pub fn text_line_height(
        mut self,
        line_height: impl Into<text::LineHeight>,
    ) -> Self {
        self.text_line_height = line_height.into();
        self
    }

    /// Sets the horizontal alignment of the text of the [`Toggler`]
    pub fn text_alignment(mut self, alignment: alignment::Horizontal) -> Self {
        self.text_alignment = alignment;
        self
    }

    /// Sets the [`text::Shaping`] strategy of the [`Toggler`].
    pub fn text_shaping(mut self, shaping: text::Shaping) -> Self {
        self.text_shaping = shaping;
        self
    }

    /// Sets the [`text::Wrapping`] strategy of the [`Toggler`].
    pub fn text_wrapping(mut self, wrapping: text::Wrapping) -> Self {
        self.text_wrapping = wrapping;
        self
    }

    /// Sets the spacing between the [`Toggler`] and the text.
    pub fn spacing(mut self, spacing: impl Into<Pixels>) -> Self {
        self.spacing = spacing.into().0;
        self
    }

    /// Sets the [`Padding`] of the [`Button`].
    pub fn padding<P: Into<Padding>>(mut self, padding: P) -> Self {
        self.padding = padding.into();
        self
    }

    /// Sets the [`Renderer::Font`] of the text of the [`Toggler`]
    ///
    /// [`Renderer::Font`]: crate::core::text::Renderer
    pub fn font(mut self, font: impl Into<Renderer::Font>) -> Self {
        self.font = Some(font.into());
        self
    }

    /// Sets the style of the [`Toggler`].
    #[must_use]
    pub fn style(mut self, style: impl Fn(&Theme, Status) -> Style + 'a) -> Self
    where
        Theme::Class<'a>: From<StyleFn<'a, Theme>>,
    {
        self.class = (Box::new(style) as StyleFn<'a, Theme>).into();
        self
    }

    /// Sets the style class of the [`Toggler`].
    #[cfg(feature = "advanced")]
    #[must_use]
    pub fn class(mut self, class: impl Into<Theme::Class<'a>>) -> Self {
        self.class = class.into();
        self
    }
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for Toggler<'a, Message, Theme, Renderer>
where
    Theme: Catalog,
    Renderer: text::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<widget::text::State<Renderer::Paragraph>>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(widget::text::State::<Renderer::Paragraph>::default())
    }

    fn size(&self) -> Size<Length> {
        Size {
            width: self.width,
            height: Length::Shrink,
        }
    }

    fn layout(
        &self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        // Use the padded helper to add padding
        layout::padded(
            limits,
            self.width,
            Length::Shrink, // Adjust height as needed
            self.padding,   // Pass the padding value here
            |limits| {
                // Use the existing logic for laying out the children
                layout::next_to_each_other(
                    limits,
                    self.spacing,
                    |_| layout::Node::new(Size::new(2.0 * self.size, self.size)),
                    |limits| {
                        if let Some(label) = self.label.as_deref() {
                            let state = tree
                                .state
                                .downcast_mut::<widget::text::State<Renderer::Paragraph>>();
    
                            widget::text::layout(
                                state,
                                renderer,
                                limits,
                                self.width,
                                Length::Shrink,
                                label,
                                self.text_line_height,
                                self.text_size,
                                self.font,
                                self.text_alignment,
                                alignment::Vertical::Top,
                                self.text_shaping,
                                self.text_wrapping,
                            )
                        } else {
                            layout::Node::new(Size::ZERO)
                        }
                    },
                )
            },
        )
    }
    


    fn on_event(
        &mut self,
        _state: &mut Tree,
        event: Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) -> event::Status {
        let Some(on_toggle) = &self.on_toggle else {
            return event::Status::Ignored;
        };

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
            | Event::Touch(touch::Event::FingerPressed { .. }) => {
                let mouse_over = cursor.is_over(layout.bounds());

                if mouse_over {
                    shell.publish(on_toggle(!self.is_toggled));

                    event::Status::Captured
                } else {
                    event::Status::Ignored
                }
            }
            _ => event::Status::Ignored,
        }
    }

    fn mouse_interaction(
        &self,
        _state: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) {
            if self.on_toggle.is_some() {
                mouse::Interaction::Pointer
            } else {
                mouse::Interaction::NotAllowed
            }
        } else {
            mouse::Interaction::default()
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        /// Makes sure that the border radius of the toggler looks good at every size.
        const BORDER_RADIUS_RATIO: f32 = 32.0 / 13.0;

        /// The space ratio between the background Quad and the Toggler bounds, and
        /// between the background Quad and foreground Quad.
        const SPACE_RATIO: f32 = 0.05;


        let mut children = layout.children();
        
        // The first child layout now represents the padded content
        let padded_content_layout = if let Some(layout) = children.next() {
            layout
        } else {
            warn!("Error: Missing padded content layout");
            return;
        };

        // Retrieve the children of the padded content layout
        let mut padded_children = padded_content_layout.children();


        // let toggler_layout = children.next().unwrap();
        // => This will cause a runtime panic if the next child layout is missing (None)
        // Using a workaround to handle this case
        let toggler_layout = if let Some(layout) = padded_children.next() {
            layout
        } else {
            warn!("Error: Missing toggler layout");
            return;
        };


        let bounds = toggler_layout.bounds();
        let is_mouse_over = cursor.is_over(layout.bounds());

        let status = if self.on_toggle.is_none() {
            Status::Disabled
        } else if is_mouse_over {
            Status::Hovered {
                is_toggled: self.is_toggled,
            }
        } else {
            Status::Active {
                is_toggled: self.is_toggled,
            }
        };

        let style = theme.style(&self.class, status);

        // Fill the entire widget background
        let widget_bounds = layout.bounds(); // Bounds of the entire widget
        let is_hovered = cursor.is_over(widget_bounds);
        renderer.fill_quad(
            renderer::Quad {
                bounds: widget_bounds,
                border: Border {
                    radius: Radius::new(0.0), // No border radius for full background
                    width: 1.0,
                    color: style.widget_background
                },
                ..renderer::Quad::default()
            },
            style.widget_background, // Use the background color from the style
        );

        if self.label.is_some() {
            // Handle the label layout similar to the toggler layout
            // let label_layout = children.next().unwrap();
            if let Some(label_layout) = padded_children.next() {
                let state: &widget::text::State<Renderer::Paragraph> =
                    tree.state.downcast_ref();

                crate::iced::widget::text::draw(
                    renderer,
                    _style,
                    label_layout,
                    state.0.raw(),
                    crate::iced::widget::text::Style::default(),
                    viewport,
                );
            } else {
                warn!("Error: Missing label layout");
                return;
            }
            
        }

        let border_radius = bounds.height / BORDER_RADIUS_RATIO;
        let space = SPACE_RATIO * bounds.height;

        let toggler_background_bounds = Rectangle {
            x: bounds.x + space,
            y: bounds.y + space,
            width: bounds.width - (2.0 * space),
            height: bounds.height - (2.0 * space),
        };

        renderer.fill_quad(
            renderer::Quad {
                bounds: toggler_background_bounds,
                border: Border {
                    radius: border_radius.into(),
                    width: style.background_border_width,
                    color: style.background_border_color,
                },
                ..renderer::Quad::default()
            },
            style.background,
        );

        let toggler_foreground_bounds = Rectangle {
            x: bounds.x
                + if self.is_toggled {
                    bounds.width - 2.0 * space - (bounds.height - (4.0 * space))
                } else {
                    2.0 * space
                },
            y: bounds.y + (2.0 * space),
            width: bounds.height - (4.0 * space),
            height: bounds.height - (4.0 * space),
        };

        renderer.fill_quad(
            renderer::Quad {
                bounds: toggler_foreground_bounds,
                border: Border {
                    radius: border_radius.into(),
                    width: style.foreground_border_width,
                    color: style.foreground_border_color,
                },
                ..renderer::Quad::default()
            },
            style.foreground,
        );
    }
}

impl<'a, Message, Theme, Renderer> From<Toggler<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: Catalog + 'a,
    Renderer: text::Renderer + 'a,
{
    fn from(
        toggler: Toggler<'a, Message, Theme, Renderer>,
    ) -> Element<'a, Message, Theme, Renderer> {
        Element::new(toggler)
    }
}

/// The default [`Padding`] of a [`Toggler`].
pub(crate) const DEFAULT_PADDING: Padding = Padding {
    top: 5.0,
    bottom: 5.0,
    right: 10.0,
    left: 10.0,
};

/// The possible status of a [`Toggler`]. Same as Button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The [`Toggler`] can be interacted with.
    Active {
        /// Indicates whether the [`Toggler`] is toggled.
        is_toggled: bool,
    },
    /// The [`Toggler`] is being hovered.
    Hovered {
        /// Indicates whether the [`Toggler`] is toggled.
        is_toggled: bool,
    },
    /// The [`Toggler`] is disabled.
    Disabled,
}

/// The appearance of a toggler.
#[derive(Debug, Clone, Copy)]
pub struct Style {
    /// The background [`Color`] of the toggler.
    pub background: Color,
    /// The width of the background border of the toggler.
    pub background_border_width: f32,
    /// The [`Color`] of the background border of the toggler.
    pub background_border_color: Color,
    /// The foreground [`Color`] of the toggler.
    pub foreground: Color,
    /// The width of the foreground border of the toggler.
    pub foreground_border_width: f32,
    /// The [`Color`] of the foreground border of the toggler.
    pub foreground_border_color: Color,
    /// The background [`Color`] of the entire widget.
    pub widget_background: Color,
}

/// The theme catalog of a [`Toggler`].
pub trait Catalog: Sized {
    /// The item class of the [`Catalog`].
    type Class<'a>;

    /// The default class produced by the [`Catalog`].
    fn default<'a>() -> Self::Class<'a>;

    /// The [`Style`] of a class with the given status.
    fn style(&self, class: &Self::Class<'_>, status: Status) -> Style;
}

/// A styling function for a [`Toggler`].
///
/// This is just a boxed closure: `Fn(&Theme, Status) -> Style`.
pub type StyleFn<'a, Theme> = Box<dyn Fn(&Theme, Status) -> Style + 'a>;

impl Catalog for Theme {
    type Class<'a> = StyleFn<'a, Self>;

    fn default<'a>() -> Self::Class<'a> {
        Box::new(default)
    }

    fn style(&self, class: &Self::Class<'_>, status: Status) -> Style {
        class(self, status)
    }
}

/// The default style of a [`Toggler`].
pub fn default(theme: &Theme, status: Status) -> Style {
    let palette = theme.extended_palette();

    let widget_background = match status {
        Status::Active { is_toggled } => {
            palette.background.base.color
        }
        Status::Hovered { is_toggled } => {
            palette.background.weak.color
        }
        Status::Disabled => palette.background.weak.color, // Muted color for disabled state
    };

    let background = match status {
        Status::Active { is_toggled } | Status::Hovered { is_toggled } => {
            if is_toggled {
                palette.primary.strong.color
            } else {
                palette.background.strong.color
            }
        }
        Status::Disabled => palette.background.weak.color,
    };

    let foreground = match status {
        Status::Active { is_toggled } | Status::Hovered { is_toggled } => {
            if is_toggled {
                palette.primary.strong.text
            } else {
                palette.background.base.color
            }
        }
        Status::Disabled => palette.background.base.color,
    };

    Style {
        widget_background, 
        background,
        foreground,
        foreground_border_width: 0.0,
        foreground_border_color: Color::TRANSPARENT,
        background_border_width: 0.0,
        background_border_color: Color::TRANSPARENT,
    }
}