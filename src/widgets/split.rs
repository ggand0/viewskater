// Further modified from https://gist.github.com/airstrike/1169980e58ccb20a88e21af23dcf2650
// ---
// Modified from iced_aw to work with iced master branch (~0.13). This
// is provided AS IS—not really tested other than the fact that it compiles
// https://github.com/iced-rs/iced_aw/blob/main/src/widgets/split.rs
// https://github.com/iced-rs/iced_aw/blob/main/src/style/split.rs

// MIT License

// Copyright (c) 2020 Kaiden42

// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:

// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.

// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

//! Use a split to split the available space in two parts to display two different elements.
//!
//! *This API requires the following crate features to be activated: split*


#[cfg(target_os = "linux")]
mod other_os {
    pub use iced_custom as iced;
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
    advanced::{
        layout::{Limits, Node},
        overlay, renderer,
        widget::{tree, Operation, Tree},
        Clipboard, Layout, Shell, Widget,
    },
    theme::palette,
    event, mouse::{self, Cursor}, touch,
    widget::Row,
    Background, Border, Color, Element, Event, Length, Padding, Point,
    Rectangle, Shadow, Size, Theme, Vector
};
use iced::border::Radius;


use std::time::{Duration, Instant};
#[allow(unused_imports)]
use log::{Level, debug, info, warn, error};

/// A split can divide the available space by half to display two different elements.
/// It can split horizontally or vertically.
///
/// # Example
/// ```ignore
/// # use iced_aw::split::{State, Axis, Split};
/// # use iced::widget::Text;
/// #
/// #[derive(Debug, Clone)]
/// enum Message {
///     Resized(u16),
/// }
///
/// let first = Text::new("First");
/// let second = Text::new("Second");
///
/// let split = Split::new(first, second, Some(300), Axis::Vertical, Message::Resized);
/// ```
#[allow(missing_debug_implementations)]
pub struct Split<'a, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer,
    Theme: Catalog,
{
    /// The first element of the [`Split`].
    first: Element<'a, Message, Theme, Renderer>,
    /// The second element of the [`Split`].
    second: Element<'a, Message, Theme, Renderer>,

    is_selected: Vec<bool>,

    /// The position of the divider.
    divider_position: Option<u16>,
    //divider_init_position: Option<u16>,

    /// The axis to split at.
    axis: Axis,
    /// The padding around the elements of the [`Split`].
    padding: f32,
    /// The spacing between the elements of the [`Split`].
    /// This is also the width of the divider.
    spacing: f32,
    /// The width of the [`Split`].
    width: Length,
    /// The height of the [`Split`].
    height: Length,
    /// The minimum size of the first element of the [`Split`].
    min_size_first: u16,
    /// The minimum size of the second element of the [`Split`].
    min_size_second: u16,
    /// The message that is send when the divider of the [`Split`] is moved.
    on_resize: Box<dyn Fn(u16) -> Message>,
    on_double_click: Box<dyn Fn(u16) -> Message>,
    on_drop: Box<dyn Fn(isize, String) -> Message>,
    on_select: Box<dyn Fn(usize, bool) -> Message>,

    class: Theme::Class<'a>,

    // Whether to enable pane selection
    enable_pane_selection: bool,
}

impl<'a, Message, Theme, Renderer> Split<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Renderer: 'a + renderer::Renderer,
    Theme: Catalog,
{
    /// Creates a new [`Split`].
    ///
    /// It expects:
    ///     - The first [`Element`] to display
    ///     - The second [`Element`] to display
    ///     - The position of the divider. If none, the space will be split in half.
    ///     - The [`Axis`] to split at.
    ///     - The message that is send on moving the divider
    pub fn new<A, B, F, G, H, I>(
        enable_pane_selection: bool,
        first: A,
        second: B,
        is_selected: Vec<bool>,
        divider_position: Option<u16>,
        axis: Axis,
        on_resize: F,
        on_double_click: G,
        on_drop: H,
        on_select: I,
    ) -> Self
    where
        A: Into<Element<'a, Message, Theme, Renderer>>,
        B: Into<Element<'a, Message, Theme, Renderer>>,
        F: 'static + Fn(u16) -> Message,
        G: 'static + Fn(u16) -> Message,
        H: 'static + Fn(isize, String) -> Message,
        I: 'static + Fn(usize, bool) -> Message,
    {
        Self {
            first: first.into(),
            // first: Container::new(first.into())
            //     .width(Length::Fill)
            //     .height(Length::Fill)
            //     .into(),
            second: second.into(),
            // second: Container::new(second.into())
            //     .width(Length::Fill)
            //     .height(Length::Fill)
            //     .into(),
            is_selected: is_selected,
            divider_position,
            //divider_init_position: divider_position,
            axis,
            padding: 0.0,
            spacing: 10.0, // was 5.0
            width: Length::Fill,
            height: Length::Fill,
            min_size_first: 5,
            min_size_second: 5,
            on_resize: Box::new(on_resize),
            on_double_click: Box::new(on_double_click),
            on_drop: Box::new(on_drop),
            on_select: Box::new(on_select),
            class: Theme::default(),
            enable_pane_selection: enable_pane_selection,
        }
    }

    /// Sets the padding of the [`Split`] around the inner elements.
    #[must_use]
    pub fn padding(mut self, padding: f32) -> Self {
        self.padding = padding;
        self
    }

    /// Sets the spacing of the [`Split`] between the elements.
    /// This will also be the width of the divider.
    #[must_use]
    pub fn spacing(mut self, spacing: f32) -> Self {
        self.spacing = spacing;
        self
    }

    /// Sets the width of the [`Split`].
    #[must_use]
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    /// Sets the height of the [`Split`].
    #[must_use]
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    /// Sets the minimum size of the first element of the [`Split`].
    #[must_use]
    pub fn min_size_first(mut self, size: u16) -> Self {
        self.min_size_first = size;
        self
    }

    /// Sets the minimum size of the second element of the [`Split`].
    #[must_use]
    pub fn min_size_second(mut self, size: u16) -> Self {
        self.min_size_second = size;
        self
    }

    /// Sets the style of the [`Split`].
    #[must_use]
    pub fn style(mut self, style: impl Fn(&Theme, Status) -> Style + 'a) -> Self 
    where 
        Theme::Class<'a>: From<StyleFn<'a, Theme>>,
    {
        self.class = (Box::new(style) as StyleFn<'a, Theme>).into();
        self
    }

    /// Sets the style class of the [`Split`].
    // #[cfg(feature = "advanced")]
    #[must_use]
    pub fn class(mut self, class: impl Into<Theme::Class<'a>>) -> Self {
        self.class = class.into();
        self
    }
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for Split<'a, Message, Theme, Renderer>
where
    Message: 'a + Clone,
    Renderer: 'a + renderer::Renderer,
    Theme: Catalog,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::new())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.first), Tree::new(&self.second)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[&self.first, &self.second]);
    }

    fn size(&self) -> Size<Length> {
        Size::new(self.width, self.height)
    }

    fn layout(
        &self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &Limits
    ) -> Node {
        let space = Row::<Message, Theme, Renderer>::new()
            .width(Length::Fill)
            .height(Length::Fill)
            .layout(tree, renderer, limits);

        match self.axis {
            Axis::Horizontal => {
                debug!("split.rs - layout() - Horizontal Split");
                horizontal_split(tree, self, renderer, limits, &space)
            },
            Axis::Vertical => {
                debug!("split.rs - layout() - Vertical Split");
                vertical_split(tree, self, renderer, limits, &space)
            },
        }
    }

    fn on_event(
        &mut self,
        state: &mut Tree,
        event: Event,
        layout: Layout<'_>,
        cursor: Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) -> event::Status {

        for child_layout in layout.children() {
            let _bounds = child_layout.bounds();
            // debug!("cursor.is_over(bounds): {:?}", cursor.is_over(bounds));
        }

        let split_state: &mut State = state.state.downcast_mut();

        let mut children = layout.children();
        let first_layout = children
            .next()
            .expect("Native: Layout should have a first layout");
        let first_status = self.first.as_widget_mut().on_event(
            &mut state.children[0],
            event.clone(),
            first_layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );

        let divider_layout = children
            .next()
            .expect("Native: Layout should have a divider layout");

        let second_layout = children
            .next()
            .expect("Graphics: Layout should have a second layout");

        
        match event.clone() {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
            | Event::Touch(touch::Event::FingerPressed { .. }) => {
                // Detect double-click event on the divider
                if divider_layout
                    .bounds()
                    .expand(10.0)
                    .contains(cursor.position().unwrap_or_default())
                {
                    split_state.dragging = true;

                    // Save the current time
                    if let Some(last_click_time) = split_state.last_click_time {
                        let elapsed = last_click_time.elapsed();
                        if elapsed < Duration::from_millis(500) {
                            // Double-click detected
                            split_state.last_click_time = None;
    
                            let double_click_position = match self.axis {
                                Axis::Horizontal => cursor.position().map(|p| p.y),
                                Axis::Vertical => cursor.position().map(|p| p.x),
                            };
    
                            if let Some(position) = double_click_position {
                                self.divider_position = None;
                                split_state.dragging = false;
                                shell.publish((self.on_double_click)(position as u16));
                            }
                        } else {
                            // Reset the timer for a new potential double-click
                            split_state.last_click_time = Some(Instant::now());
                            
                        }
                    } else {
                        split_state.last_click_time = Some(Instant::now());
                    }
                }

                // Detect pane selection
                if self.enable_pane_selection {
                    let is_within_bounds_first = is_cursor_within_bounds::<Message>(first_layout, cursor, 0, split_state);
                    if is_within_bounds_first {
                        split_state.panes_seleced[0] = !split_state.panes_seleced[0];
                        shell.publish((self.on_select)(0, split_state.panes_seleced[0]));
                        
                    }
                    let is_within_bounds_second = is_cursor_within_bounds::<Message>(second_layout, cursor, 1, split_state);
                    if is_within_bounds_second {
                        split_state.panes_seleced[1] = !split_state.panes_seleced[1];
                        shell.publish((self.on_select)(1, split_state.panes_seleced[1]));
                        
                    }
                }
            }


            #[cfg(any(target_os = "macos", target_os = "windows"))]
            Event::Window(iced::window::Event::FileHovered(position)) => {
                // Access the cursor position from the FileHovered event
                debug!("FILEHOVER POSITION: {:?}", position);
            }

            #[cfg(target_os = "linux")]
            Event::Window(iced::window::Event::FileHovered(_path)) => {
                // Access the cursor position from the FileHovered event
                debug!("FileHovered Cursor position: {:?}", cursor.position().unwrap_or_default());
            }

            #[cfg(any(target_os = "macos", target_os = "windows"))]
            Event::Window(iced::window::Event::FileDropped(paths, position)) => {
                debug!("FILEDROP POSITION: {:?}", position);
                
                let mut children = layout.children();
                let first_layout = children.next().expect("Missing first layout");
                let divider_layout = children.next().expect("Missing divider layout");
                let second_layout = children.next().expect("Missing second layout");
                
                // Convert position to Point for checking
                let custom_position = Point::new(position.x as f32, position.y as f32);
                
                // Check which pane contains the position
                if first_layout.bounds().contains(custom_position) {
                    shell.publish((self.on_drop)(0, paths[0].to_string_lossy().to_string()));
                } else if second_layout.bounds().contains(custom_position) {
                    shell.publish((self.on_drop)(1, paths[0].to_string_lossy().to_string()));
                }
            }

            #[cfg(target_os = "linux")]
            Event::Window(iced::window::Event::FileDropped(path, _)) => {
                let mut children = layout.children();
                let first_layout = children.next().expect("Missing first layout");
                let _divider_layout = children.next().expect("Missing divider layout");
                let second_layout = children.next().expect("Missing second layout");
                
                debug!("FileDropped Cursor position: {:?}", cursor.position().unwrap_or_default());
                
                // Check which pane the cursor is over and use the correct index
                let cursor_pos = cursor.position().unwrap_or_default();
                
                // Check first pane (index 0)
                if first_layout.bounds().contains(cursor_pos) {
                    debug!("FileDropped - First pane");
                    shell.publish((self.on_drop)(0, path[0].to_string_lossy().to_string()));
                } 
                // Check second pane (index 1)
                else if second_layout.bounds().contains(cursor_pos) {
                    debug!("FileDropped - Second pane");
                    shell.publish((self.on_drop)(1, path[0].to_string_lossy().to_string()));
                }
            }


            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            | Event::Touch(touch::Event::FingerLifted { .. }) => {
                if split_state.dragging {
                    split_state.dragging = false;
                }
            }

            Event::Mouse(mouse::Event::CursorMoved { position })
            | Event::Touch(touch::Event::FingerMoved { position, .. }) => {
                // debug!("CursorMoved Cursor position: {:?}", position);
                if split_state.dragging {
                    let position = match self.axis {
                        Axis::Horizontal => position.y,
                        Axis::Vertical => position.x,
                    };
                    shell.publish((self.on_resize)(position as u16));

                }
            }

            _ => {}
        }


        let second_status = self.second.as_widget_mut().on_event(
            &mut state.children[1],
            event,
            second_layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );

        first_status.merge(second_status)
    }

    fn mouse_interaction(
        &self,
        state: &Tree,
        layout: Layout<'_>,
        cursor: Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let mut children = layout.children();
        let first_layout = children
            .next()
            .expect("Graphics: Layout should have a first layout");
        let first_mouse_interaction = self.first.as_widget().mouse_interaction(
            &state.children[0],
            first_layout,
            cursor,
            viewport,
            renderer,
        );
        let divider_layout = children
            .next()
            .expect("Graphics: Layout should have a divider layout");
        
        // Increase the hitbox expansion from 5.0 to 10.0 pixels
        let divider_mouse_interaction = if divider_layout
            .bounds().expand(10.0)
            .contains(cursor.position().unwrap_or_default())
        {
            match self.axis {
                Axis::Horizontal => mouse::Interaction::ResizingVertically,
                Axis::Vertical => mouse::Interaction::ResizingHorizontally,
            }
        } else {
            mouse::Interaction::default()
        };
        
        let second_layout = children
            .next()
            .expect("Graphics: Layout should have a second layout");
        let second_mouse_interaction = self.second.as_widget().mouse_interaction(
            &state.children[1],
            second_layout,
            cursor,
            viewport,
            renderer,
        );
        first_mouse_interaction
            .max(second_mouse_interaction)
            .max(divider_mouse_interaction)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        cursor: Cursor,
        viewport: &Rectangle,
    ) {
        // TODO: clipping!
        let mut children = layout.children();

        let bounds = layout.bounds();
        let content_layout = layout.children().next().unwrap();
        let is_mouse_over = cursor.is_over(bounds);

        let status = if is_mouse_over {
            let state = tree.state.downcast_ref::<State>();

            if state.dragging {
                Status::Dragging
            } else {
                Status::Hovered
            }
        } else {
            Status::Active
        };

        let style = theme.style(&self.class, status);

        // Background
        renderer.fill_quad(
            renderer::Quad {
                bounds: content_layout.bounds(),
                border: style.border,
                shadow: Shadow::default(),
            },
            style
                .background
                .unwrap_or_else(|| Color::TRANSPARENT.into()),
        );

        let first_layout = children
            .next()
            .expect("Graphics: Layout should have a first layout");

        let bounds_first = first_layout.bounds();
        let is_mouse_over_first = cursor.is_over(bounds_first);

        let status_first = if is_mouse_over_first {
            let state = tree.state.downcast_ref::<State>();

            if state.dragging {
                Status::Dragging
            } else {
                Status::Hovered
            }
        } else {
            Status::Active
        };

        let style_first = theme.style(&self.class, status_first);

        // First
        renderer.fill_quad(
            renderer::Quad {
                bounds: bounds_first,
                border: style_first.first_border,
                shadow: Shadow::default(),
            },
            style_first
                .first_background
                .unwrap_or_else(|| Color::TRANSPARENT.into()),
        );

        self.first.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            &renderer::Style::default(),
            first_layout,
            cursor,
            viewport,
        );

        let divider_layout = children
            .next()
            .expect("Graphics: Layout should have a divider layout");

        // Second
        let second_layout = children
            .next()
            .expect("Graphics: Layout should have a second layout");

        let bounds_second = second_layout.bounds();
        let is_mouse_over_second = cursor.is_over(bounds_second);

        let status_second = if is_mouse_over_second {
            let state = tree.state.downcast_ref::<State>();

            if state.dragging {
                Status::Dragging
            } else {
                Status::Hovered
            }
        } else {
            Status::Active
        };

        let style_second = theme.style(&self.class, status_second);

        renderer.fill_quad(
            renderer::Quad {
                bounds: bounds_second,
                border: style_second.second_border,
                shadow: Shadow::default(),
            },
            style_second
                .second_background
                .unwrap_or_else(|| Color::TRANSPARENT.into()),
        );

        self.second.as_widget().draw(
            &tree.children[1],
            renderer,
            theme,
            &renderer::Style::default(),
            second_layout,
            cursor,
            viewport,
        );

        let bounds_divider = divider_layout.bounds();
        let is_mouse_over_divider = cursor.is_over(bounds_divider.expand(5.0));

        let status_divider = if is_mouse_over_divider {
            let state = tree.state.downcast_ref::<State>();

            if state.dragging {
                Status::Dragging
            } else {
                Status::Hovered
            }
        } else {
            Status::Active
        };

        let style_divider = theme.style(&self.class, status_divider);

        let bounds = divider_layout.bounds();
        let is_horizontal = bounds.width >= bounds.height;

        // Create a modified Rectangle for a thin line, centered within the divider area
        let thin_rectangle = if is_horizontal {
            // For horizontal dividers
            Rectangle {
                x: bounds.x,
                y: bounds.y + (bounds.height - 1.0) / 2.0, // Center the 1px line
                width: bounds.width + 10.0,
                height: 1.0,
            }
        } else {
            // For vertical dividers
            Rectangle {
                x: bounds.x + (bounds.width - 1.0) / 2.0, // Center the 1px line
                y: bounds.y,
                width: 1.0,
                height: bounds.height + 10.0, // `+ 10.0` is needed to make the divider reach the top edge of footer
            }
        };

        // Draw the divider (thin line)
        renderer.fill_quad(
            renderer::Quad {
                bounds: thin_rectangle,
                border: Border {
                    color: style_divider.border.color,
                    width: 0.0,
                    radius: Radius::new(0.0),
                },
                shadow: Default::default(), // No shadow
            },
            Background::Color(Color::from_rgb(0.8, 0.8, 0.8)), // Using a brighter color for debugging
        );
        

        let style = theme.style(&self.class, Status::Active);
        // Draw pane selection status; if selected, draw a border around the pane
        if self.enable_pane_selection {
            if self.is_selected[0] {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: first_layout.bounds(),
                        border: Border {
                            color: style.primary.base.color,
                            width: 1.0,
                            radius: Radius::new(0.0),
                        },
                        shadow: Default::default(), // Use Default for no shadow
                    },
                    Background::Color(Color::TRANSPARENT),
                );
                
            }
            if self.is_selected[1] {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: second_layout.bounds(),
                        border: Border {
                            color: style.primary.base.color,
                            width: 1.0,
                            radius: Radius::new(0.0),
                        },
                        shadow: Default::default(), // Use Default for no shadow
                    },
                    Background::Color(Color::TRANSPARENT),
                );
                
            }
        }
    }

    fn operate<'b>(
        &'b self,
        state: &'b mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        //operation: &mut dyn Operation<Message>,
        operation: &mut dyn Operation,
    ) {
        let mut children = layout.children();
        let first_layout = children.next().expect("Missing Split First window");
        let _divider_layout = children.next().expect("Missing Split Divider");
        let second_layout = children.next().expect("Missing Split Second window");

        let (first_state, second_state) = state.children.split_at_mut(1);

        self.first
            .as_widget()
            .operate(&mut first_state[0], first_layout, renderer, operation);
        self.second
            .as_widget()
            .operate(&mut second_state[0], second_layout, renderer, operation);
    }

    fn overlay<'b>(
        &'b mut self,
        state: &'b mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let mut children = layout.children();
        let first_layout = children.next()?;
        let _divider_layout = children.next()?;
        let second_layout = children.next()?;

        let first = &mut self.first;
        let second = &mut self.second;

        // Not pretty but works to get two mutable references
        // https://stackoverflow.com/a/30075629
        let (first_state, second_state) = state.children.split_at_mut(1);

        first
            .as_widget_mut()
            .overlay(&mut first_state[0], first_layout, renderer, translation)
            .or_else(|| {
                second.as_widget_mut().overlay(
                    &mut second_state[0],
                    second_layout,
                    renderer,
                    translation,
                )
            })
    }
}



// Helper function to process a layout and check for cursor position
// This function assumes that the first child of the container is the Image widget
// TODO: Fix hardcoding
fn is_cursor_within_bounds<Message>(
    layout: Layout<'_>,
    cursor: Cursor,
    _pane_index: usize,
    _split_state: &mut State,
) -> bool {

    if let Some(container_layout) = layout.children().next() {
        if let Some(image_layout) = container_layout.children().next() {
            let image_bounds = image_layout.bounds();
            if image_bounds.contains(cursor.position().unwrap_or_default()) {
                return true;
            }
        }
        
    }
    false
}

/// The state of a [`Split`].
#[derive(Clone, Debug, Default)]
pub struct State {
    /// If the divider is dragged by the user.
    dragging: bool,
    last_click_time: Option<Instant>,
    panes_seleced: [bool; 2],
}

impl State {
    /// Creates a new [`State`] for a [`Split`].
    ///
    /// It expects:
    ///     - The optional position of the divider. If none, the available space will be split in half.
    ///     - The [`Axis`] to split at.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            dragging: false,
            last_click_time: None,
            //panes_seleced: [false, false],
            panes_seleced: [true, true],
        }
    }
}

/// Do a horizontal split.
fn horizontal_split<'a, Message, Theme, Renderer>(
    tree: &mut Tree,
    split: &Split<'a, Message, Theme, Renderer>,
    renderer: &Renderer,
    limits: &Limits,
    space: &Node,
) -> Node
where
    Renderer: 'a + renderer::Renderer,
    Theme: Catalog,
{
    let total_height = space.bounds().height;

    if total_height < split.spacing + f32::from(split.min_size_first + split.min_size_second) {
        return Node::with_children(space.bounds().size(), vec![
            split.first.as_widget().layout(
                &mut tree.children[0],
                renderer,
                &limits.clone().shrink(Size::new(0.0, total_height)),
            ),
            Node::new(Size::new(space.bounds().width, split.spacing)),
            split.second.as_widget().layout(
                &mut tree.children[1],
                renderer,
                &limits.clone().shrink(Size::new(0.0, total_height)),
            ),
        ]);
    }

    // Calculate available content height (total minus spacing)
    let available_content_height = total_height - split.spacing;
    
    // Calculate equal height for both panes
    let equal_pane_height = available_content_height / 2.0;
    
    // Default divider position is set to create equal panes
    let divider_position = split.divider_position.unwrap_or_else(|| equal_pane_height as u16);
    
    // divider_position is always positive: measure from start (top)
    let effective_position = divider_position.max(((split.spacing / 2.0) as i16).try_into().unwrap()) as f32;

    // Clamp the effective position to respect minimum sizes
    let clamped_position = effective_position.clamp(
        split.min_size_first as f32,
        total_height - split.min_size_second as f32 - split.spacing,
    );

    let padding = Padding::from(split.padding as u16);

    // Layout first element
    let first_limits = limits
        .clone()
        .shrink(Size::new(0.0, total_height - clamped_position))
        .shrink(padding);
    let mut first = split
        .first
        .as_widget()
        .layout(&mut tree.children[0], renderer, &first_limits);
    first.move_to_mut(Point::new(
        space.bounds().x + split.padding,
        space.bounds().y + split.padding,
    ));

    // Keep the divider code mostly unchanged, but make the node as tall as split.spacing
    // The actual divider line will be drawn centered within this space
    let mut divider = Node::new(Size::new(space.bounds().width, split.spacing));
    divider.move_to_mut(Point::new(space.bounds().x, clamped_position));

    // Layout second element
    let second_limits = limits
        .clone()
        .shrink(Size::new(0.0, clamped_position + split.spacing))
        .shrink(padding);
    let mut second = split
        .second
        .as_widget()
        .layout(&mut tree.children[1], renderer, &second_limits);
    second.move_to_mut(Point::new(
        space.bounds().x + split.padding,
        space.bounds().y + clamped_position + split.spacing + split.padding,
    ));

    // Add debug logs to verify positions and heights
    debug!("HORIZONTAL Split: equal_pane_height={}, first_y={}, divider_y={}, second_y={}, first_height={}, second_height={}", 
           equal_pane_height,
           space.bounds().y + split.padding,
           clamped_position,
           space.bounds().y + clamped_position + split.spacing + split.padding,
           clamped_position - (space.bounds().y + split.padding),
           total_height - (clamped_position + split.spacing + split.padding*2.0));

    // Maintain the original 3-node structure expected by other methods
    Node::with_children(space.bounds().size(), vec![first, divider, second])
}


/// Do a vertical split.
fn vertical_split<'a, Message, Theme, Renderer>(
    tree: &mut Tree,
    split: &Split<'a, Message, Theme, Renderer>,
    renderer: &Renderer,
    limits: &Limits,
    space: &Node,
) -> Node
where
    Renderer: 'a + renderer::Renderer,
    Theme: Catalog,
{
    let bounds = space.bounds();
    debug!("VERTICAL Split calculation - bounds: {:?}", bounds);
    
    if space.bounds().width
        < split.spacing + f32::from(split.min_size_first + split.min_size_second)
    {
        debug!("VERTICAL Split - insufficient width for proper split, using fallback layout");
        return Node::with_children(
            space.bounds().size(),
            vec![
                split.first.as_widget().layout(
                    &mut tree.children[0],
                    renderer,
                    &limits.clone().shrink(Size::new(space.bounds().width, 0.0)),
                ),
                Node::new(Size::new(split.spacing, space.bounds().height)),
                split.second.as_widget().layout(
                    &mut tree.children[1],
                    renderer,
                    &limits.clone().shrink(Size::new(space.bounds().width, 0.0)),
                ),
            ],
        );
    }

    // Calculate the actual size available for content (excluding padding)
    let available_width = space.bounds().width - (2.0 * split.padding);
    
    // Define spacing around the divider (on each side)
    let gap = split.spacing; // Gap between content and divider
    let divider_width = 1.0; // Width of the actual divider line
    let total_spacing = 2.0 * gap + divider_width; // Total space needed for divider + gaps
    
    // Calculate the divider position (position where the divider's center will be)
    let divider_position = split
        .divider_position
        .unwrap_or_else(|| (available_width / 2.0) as u16)
        .max((total_spacing / 2.0) as u16);
    
    // Ensure divider position remains within bounds
    let divider_position = divider_position.clamp(
        split.min_size_first,
        (available_width - f32::from(split.min_size_second) - total_spacing) as u16,
    );
    
    debug!("VERTICAL Split calculation: available_width={}, divider_position={}, spacing={}", 
           available_width, divider_position, split.spacing);
    
    // Calculate positions of elements
    let divider_center_x = space.bounds().x + split.padding + f32::from(divider_position);
    
    // The first element should end before the left gap
    let first_end_x = divider_center_x - gap - divider_width/2.0;
    
    // The divider should be in the center of the gap
    let divider_left_x = divider_center_x - divider_width/2.0;
    
    // The second element should start after the right gap
    let second_start_x = divider_center_x + gap + divider_width/2.0;

    // Layout the first element with appropriate width
    let first_width = first_end_x - (space.bounds().x + split.padding);
    let first_limits = limits
        .clone()
        .width(first_width)
        .shrink(Padding::from(split.padding as u16));
    
    let mut first = split
        .first
        .as_widget()
        .layout(&mut tree.children[0], renderer, &first_limits);
    first.move_to_mut(Point::new(
        space.bounds().x + split.padding,
        space.bounds().y + split.padding,
    ));

    // Create the divider node (thin line in center)
    let mut divider = Node::new(Size::new(divider_width, space.bounds().height));
    divider.move_to_mut(Point::new(divider_left_x, space.bounds().y));

    // Layout the second element
    let second_width = space.bounds().width - second_start_x;
    let second_limits = limits
        .clone()
        .width(second_width)
        .shrink(Padding::from(split.padding as u16));
    
    let mut second =
        split
            .second
            .as_widget()
            .layout(&mut tree.children[1], renderer, &second_limits);
    second.move_to_mut(Point::new(
        second_start_x,
        space.bounds().y + split.padding,
    ));

    debug!("VERTICAL Spacing: {}, First right edge: {}, Divider left: {}, Divider right: {}, Second left: {}, Gap size: {}", 
        split.spacing,
        first_end_x,
        divider_left_x,
        divider_left_x + divider_width,
        second_start_x,
        gap
    );

    let result = Node::with_children(space.bounds().size(), vec![first, divider, second]);
    
    // Debug output to verify the bounds are correct
    let children = result.children();
    if children.len() >= 3 {
        debug!("VERTICAL First pane bounds: {:?}", children[0].bounds());
        debug!("VERTICAL Divider bounds: {:?}", children[1].bounds());
        debug!("VERTICAL Second pane bounds: {:?}", children[2].bounds());
        
        // Add total width check to verify we don't exceed available space
        let total_used_width = children[0].bounds().width + divider_width + (2.0 * gap) + children[2].bounds().width;
        debug!("VERTICAL Total width used: {} (available: {})", total_used_width, bounds.width);
    }
    
    result
}

impl<'a, Message, Theme, Renderer> From<Split<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
    Renderer: renderer::Renderer + 'a,
    Theme: Catalog + 'a,
{
    fn from(split_pane: Split<'a, Message, Theme, Renderer>) -> Self {
        Element::new(split_pane)
    }
}

/// The axis to split at.
#[derive(Clone, Copy, Debug)]
pub enum Axis {
    /// Split horizontally.
    Horizontal,
    /// Split vertically.
    Vertical,
}

impl Default for Axis {
    fn default() -> Self {
        Self::Vertical
    }
}

/// The possible statuses of a [`Split`].
pub enum Status {
    /// The [`Split`] can be dragged.
    Active,
    /// The [`Split`] can be dragged and it is being hovered.
    Hovered,
    /// The [`Split`] is being dragged.
    Dragging,
    /// The [`Split`] cannot be dragged.
    Disabled,
}

/// The style of a [`Split`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Style {
    /// The optional background of the [`Split`].
    pub background: Option<Background>,
    /// The optional background of the first element of the [`Split`].
    pub first_background: Option<Background>,
    /// The optional background of the second element of the [`Split`].
    pub second_background: Option<Background>,
    /// The [`Border`] of the [`Split`].
    pub border: Border,
    /// The [`Border`] of the [`Split`].
    pub first_border: Border,
    /// The [`Border`] of the [`Split`].
    pub second_border: Border,
    /// The background of the divider of the [`Split`].
    pub divider_background: Background,
    /// The [`Border`] of the divider of the [`Split`].
    pub divider_border: Border,
    /// The primary color of the [`Split`].
    pub primary: palette::Primary,
}

impl Style {
    /// Updates the [`Style`] with the given [`Background`].
    pub fn with_background(self, background: impl Into<Background>) -> Self {
        Self {
            background: Some(background.into()),
            ..self
        }
    }
}

impl Default for Style {
    fn default() -> Self {
        Self {
            background: None,
            first_background: None,
            second_background: None,
            border: Border::default(),
            first_border: Border::default(),
            second_border: Border::default(),
            divider_background: Background::Color(Color::TRANSPARENT),
            divider_border: Border::default(),
            primary: palette::Primary::generate(
                Color::TRANSPARENT,
                Color::TRANSPARENT,
                Color::TRANSPARENT,
            ),
        }
    }
}


/// The theme catalog of a [`Split`].
pub trait Catalog {
    /// The item class of the [`Split`].
    type Class<'a>;

    /// The default class produced by the [`Split`].
    fn default<'a>() -> Self::Class<'a>;

    /// The [`Style`] of a class with the given status.
    fn style(&self, class: &Self::Class<'_>, status: Status) -> Style;
}

/// A styling function for a [`Split`].
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

pub fn default(theme: &Theme, status: Status) -> Style {
    let palette = theme.extended_palette();
    let base = base(*palette);

    match status {
        Status::Active => base,
        Status::Hovered => base,
        Status::Dragging => base,
        Status::Disabled => disabled(base),
    }
}

fn base(palette: palette::Extended) -> Style {
    Style {
        background: Some(Background::Color(palette.background.base.color)),
        border: Border::rounded(Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: Radius::new(0.0),
        }, 2.0),
        primary: palette.primary,
        ..Style::default()
    }
}

fn disabled(style: Style) -> Style {
    Style {
        background: style
            .background
            .map(|background| background.scale_alpha(0.5)),
        ..style
    }
}
