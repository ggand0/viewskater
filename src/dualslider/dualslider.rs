//! Display an interactive selector of a single value from a range of values.
//!
//! A [`Slider`] has some local [`State`].
#[cfg(target_os = "linux")]
mod other_os {
    pub use iced_widget;
}

#[cfg(not(target_os = "linux"))]
mod macos {
    pub use iced_widget_custom as iced_widget;
}

#[cfg(target_os = "linux")]
use other_os::*;

#[cfg(not(target_os = "linux"))]
use macos::*;

use iced_widget::core::{
    self, event, layout, Element, Layout, Length, Point, Rectangle, Shell, Size, Pixels,
    mouse,
    renderer, touch,
    widget::{
        tree::{self, State, Tree},
        Widget,
    },
    Clipboard, Color, Event,
};

//use crate::dualslider::style::{Appearance, Handle, HandleShape, Rail, StyleSheet};
use crate::dualslider::style::{HandleShape, StyleSheet};

//use num_traits
// use num_traits::NumCast;
use std::ops::RangeInclusive;



/// An horizontal bar and a handle that selects a single value from a range of
/// values.
///
/// A [`Slider`] will try to fill the horizontal space of its container.
///
/// The [`Slider`] range of numeric values is generic and its step size defaults
/// to 1 unit.
///
/// # Example
/// ```no_run
/// # type Slider<'a, T, Message> =
/// #     iced_widget::Slider<'a, Message, T, iced_widget::renderer::Renderer<iced_widget::style::Theme>>;
/// #
/// #[derive(Clone)]
/// pub enum Message {
///     SliderChanged(f32),
/// }
///
/// let value = 50.0;
///
/// Slider::new(0.0..=100.0, value, Message::SliderChanged);
/// ```
///
/// ![Slider drawn by Coffee's renderer](https://github.com/hecrj/coffee/blob/bda9818f823dfcb8a7ad0ff4940b4d4b387b5208/images/ui/slider.png?raw=true)
#[allow(missing_debug_implementations)]
// pub struct DualSlider<'a, T, Message, Renderer = crate::Renderer>
pub struct DualSlider<'a, T, Message, Renderer>
where
    // Renderer: crate::core::Renderer,
    Renderer: core::Renderer,
    Renderer::Theme: StyleSheet,
{
    range: RangeInclusive<T>,
    step: T,
    value: T,
    pane_index: isize, // needs to be isize because of the need to represent "all" panes; -1
    on_change: Box<dyn Fn(isize, T) -> Message + 'a>,
    on_release: Box<dyn Fn(isize, T) -> Message + 'a>,
    width: Length,
    height: f32,
    style: <Renderer::Theme as StyleSheet>::Style,
}

impl<'a, T, Message, Renderer> DualSlider<'a, T, Message, Renderer>
where
    T: Copy + From<u8> + std::cmp::PartialOrd,
    Message: Clone,
    Renderer: core::Renderer,
    Renderer::Theme: StyleSheet,
{
    /// The default height of a [`Slider`].
    pub const DEFAULT_HEIGHT: f32 = 22.0;

    /// Creates a new [`Slider`].
    ///
    /// It expects:
    ///   * an inclusive range of possible values
    ///   * the current value of the [`Slider`]
    ///   * a function that will be called when the [`Slider`] is dragged.
    ///   It receives the new value of the [`Slider`] and must produce a
    ///   `Message`.
    pub fn new<F, G>(range: RangeInclusive<T>, value: T, pane_index: isize, on_change: F, on_release: G) -> Self
    where
        F: 'a + Fn(isize, T) -> Message,
        G: 'a + Fn(isize, T) -> Message,
    {
        let value = if value >= *range.start() {
            value
        } else {
            *range.start()
        };

        let value = if value <= *range.end() {
            value
        } else {
            *range.end()
        };

        DualSlider {
            value,
            range,
            pane_index: pane_index,
            step: T::from(1),
            on_change: Box::new(on_change),
            on_release: Box::new(on_release),
            width: Length::Fill,
            height: Self::DEFAULT_HEIGHT,
            style: Default::default(),
        }
    }

    /// Sets the release message of the [`Slider`].
    /// This is called when the mouse is released from the slider.
    ///
    /// Typically, the user's interaction with the slider is finished when this message is produced.
    /// This is useful if you need to spawn a long-running task from the slider's result, where
    /// the default on_change message could create too many events.
    #[allow(unused_mut)]
    pub fn on_release(mut self, _on_release: Message) -> Self {
        //self.on_release = Some(on_release);
        self
    }

    /// Sets the width of the [`Slider`].
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    /// Sets the height of the [`Slider`].
    pub fn height(mut self, height: impl Into<Pixels>) -> Self {
        self.height = height.into().0;
        self
    }

    /// Sets the style of the [`Slider`].
    pub fn style(
        mut self,
        style: impl Into<<Renderer::Theme as StyleSheet>::Style>,
    ) -> Self {
        self.style = style.into();
        self
    }

    /// Sets the step size of the [`Slider`].
    pub fn step(mut self, step: impl Into<T>) -> Self {
        self.step = step.into();
        self
    }
}

impl<'a, T, Message, Renderer> Widget<Message, Renderer>
    for DualSlider<'a, T, Message, Renderer>
where
    T: Copy + Into<f64> + num_traits::FromPrimitive,
    Message: Clone,
    Renderer: core::Renderer,
    Renderer::Theme: StyleSheet,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(SliderState::new())
    }

    fn width(&self) -> Length {
        self.width
    }

    fn height(&self) -> Length {
        Length::Shrink
    }

    fn layout(
            &self,
            _renderer: &Renderer,
            limits: &layout::Limits,
        ) -> layout::Node {
        let limits = limits.width(self.width).height(self.height);
        let size = limits.resolve(Size::ZERO);

        layout::Node::new(size)
    }

    fn on_event(
        &mut self,
        tree: &mut Tree,
        event: Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) -> event::Status {
        let state = tree.state.downcast_mut::<SliderState>();
        let range = &self.range;


        let is_dragging = state.is_dragging;

        let mut change = |cursor_position: Point| {
            let bounds = layout.bounds();
            let new_value = if cursor_position.x <= bounds.x {
                *range.start()
            } else if cursor_position.x >= bounds.x + bounds.width {
                *range.end()
            } else {
                let step = self.step.into();
                let start = (*range.start()).into();
                let end = (*range.end()).into();

                let percent = f64::from(cursor_position.x - bounds.x)
                    / f64::from(bounds.width);

                let steps = (percent * (end - start) / step).round();
                let value = steps * step + start;

                if let Some(value) = T::from_f64(value) {
                    value
                } else {
                    return;
                }
            };

            if ((self.value).into() - new_value.into()).abs() > f64::EPSILON {
                shell.publish((self.on_change)( self.pane_index, new_value ));
    
                self.value = new_value;
            }
        };

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
            | Event::Touch(touch::Event::FingerPressed { .. }) => {
                if let Some(cursor_position) = cursor.position_over(layout.bounds())
                {
                    change(cursor_position);
                    state.is_dragging = true;

                    return event::Status::Captured;
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            | Event::Touch(touch::Event::FingerLifted { .. })
            | Event::Touch(touch::Event::FingerLost { .. }) => {
                if is_dragging {
                    shell.publish((self.on_release)( self.pane_index, self.value ));
                    state.is_dragging = false;

                    return event::Status::Captured;
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. })
            | Event::Touch(touch::Event::FingerMoved { .. }) => {
                if is_dragging {
                    let _ = cursor.position().map(change);

                    return event::Status::Captured;
                }
            }
            _ => {}
        }

        event::Status::Ignored
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Renderer::Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        draw(
            renderer,
            layout,
            cursor,
            tree.state.downcast_ref::<SliderState>(),
            self.value,
            &self.range,
            theme,
            &self.style,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        mouse_interaction(layout, cursor, tree.state.downcast_ref::<SliderState>())
    }
}

impl<'a, T, Message, Renderer> From<DualSlider<'a, T, Message, Renderer>>
    for Element<'a, Message, Renderer>
where
    T: 'a + Copy + Into<f64> + num_traits::FromPrimitive,
    Message: 'a + Clone,
    Renderer: 'a + core::Renderer,
    Renderer::Theme: StyleSheet,
{
    fn from(
        slider: DualSlider<'a, T, Message, Renderer>,
    ) -> Element<'a, Message, Renderer> {
        Element::new(slider)
    }
}


/// Draws a [`Slider`].
pub fn draw<T, R>(
    renderer: &mut R,
    layout: Layout<'_>,
    cursor: mouse::Cursor,
    state: &SliderState,
    value: T,
    range: &RangeInclusive<T>,
    style_sheet: &dyn StyleSheet<Style = <R::Theme as StyleSheet>::Style>,
    style: &<R::Theme as StyleSheet>::Style,
) where
    T: Into<f64> + Copy,
    R: core::Renderer,
    R::Theme: StyleSheet,
{
    let bounds = layout.bounds();
    let is_mouse_over = cursor.is_over(bounds);

    let style = if state.is_dragging {
        style_sheet.dragging(style)
    } else if is_mouse_over {
        style_sheet.hovered(style)
    } else {
        style_sheet.active(style)
    };

    let (handle_width, handle_height, handle_border_radius) =
        match style.handle.shape {
            HandleShape::Circle { radius } => {
                (radius * 2.0, radius * 2.0, radius.into())
            }
            HandleShape::Rectangle {
                width,
                border_radius,
            } => (f32::from(width), bounds.height, border_radius),
        };

    let value = value.into() as f32;
    let (range_start, range_end) = {
        let (start, end) = range.clone().into_inner();

        (start.into() as f32, end.into() as f32)
    };

    let offset = if range_start >= range_end {
        0.0
    } else {
        (bounds.width - handle_width) * (value - range_start)
            / (range_end - range_start)
    };

    let rail_y = bounds.y + bounds.height / 2.0;

    renderer.fill_quad(
        renderer::Quad {
            bounds: Rectangle {
                x: bounds.x,
                y: rail_y - style.rail.width / 2.0,
                width: offset + handle_width / 2.0,
                height: style.rail.width,
            },
            border_radius: style.rail.border_radius,
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
        },
        style.rail.colors.0,
    );

    renderer.fill_quad(
        renderer::Quad {
            bounds: Rectangle {
                x: bounds.x + offset + handle_width / 2.0,
                y: rail_y - style.rail.width / 2.0,
                width: bounds.width - offset - handle_width / 2.0,
                height: style.rail.width,
            },
            border_radius: style.rail.border_radius,
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
        },
        style.rail.colors.1,
    );

    renderer.fill_quad(
        renderer::Quad {
            bounds: Rectangle {
                x: bounds.x + offset,
                y: rail_y - handle_height / 2.0,
                width: handle_width,
                height: handle_height,
            },
            border_radius: handle_border_radius,
            border_width: style.handle.border_width,
            border_color: style.handle.border_color,
        },
        style.handle.color,
    );
}

/// Computes the current [`mouse::Interaction`] of a [`Slider`].
pub fn mouse_interaction(
    layout: Layout<'_>,
    cursor: mouse::Cursor,
    state: &SliderState,
) -> mouse::Interaction {
    let bounds = layout.bounds();
    let is_mouse_over = cursor.is_over(bounds);

    if state.is_dragging {
        mouse::Interaction::Grabbing
    } else if is_mouse_over {
        mouse::Interaction::Grab
    } else {
        mouse::Interaction::default()
    }
}

/// The local state of a [`Slider`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SliderState {
    is_dragging: bool,
}

impl SliderState {
    /// Creates a new [`SliderState`].
    pub fn new() -> SliderState {
        SliderState::default()
    }
}
