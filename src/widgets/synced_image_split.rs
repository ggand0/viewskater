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

use iced_core::widget;
use crate::widgets::split::Axis;

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
/// let split = SyncedImageSplit::new(first, second, Some(300), Axis::Vertical, Message::Resized);
/// ```
#[allow(missing_debug_implementations)]
pub struct SyncedImageSplit<'a, Message, Theme, Renderer>
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

    // Add a new field for the menu bar height
    menu_bar_height: f32,

    // Add a flag to control synced zooming
    synced_zoom: bool,
    
    // Add zoom control parameters
    min_scale: f32,
    max_scale: f32, 
    scale_step: f32,
}

impl<'a, Message, Theme, Renderer> SyncedImageSplit<'a, Message, Theme, Renderer>
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
        // Add menu_bar_height parameter, with a default of 0
        menu_bar_height: f32,
        synced_zoom: bool, // Add synced_zoom parameter
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
            spacing: 5.0, // was 5.0
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
            menu_bar_height,
            
            // Initialize zoom settings
            synced_zoom,
            min_scale: 0.25,
            max_scale: 10.0,
            scale_step: 0.10,
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

    // Add methods to customize zoom parameters
    #[must_use]
    pub fn min_scale(mut self, min_scale: f32) -> Self {
        self.min_scale = min_scale;
        self
    }

    #[must_use]
    pub fn max_scale(mut self, max_scale: f32) -> Self {
        self.max_scale = max_scale;
        self
    }

    #[must_use]
    pub fn scale_step(mut self, scale_step: f32) -> Self {
        self.scale_step = scale_step;
        self
    }
    
    #[must_use]
    pub fn synced_zoom(mut self, synced_zoom: bool) -> Self {
        self.synced_zoom = synced_zoom;
        self
    }
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for SyncedImageSplit<'a, Message, Theme, Renderer>
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
                horizontal_split(tree, self, renderer, limits, &space)
            },
            Axis::Vertical => {
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
        // Get split state
        let split_state = state.state.downcast_mut::<State>();
        
        // Ensure synced_zoom state is updated
        split_state.synced_zoom = self.synced_zoom;
        
        let is_wheel_event = matches!(event, Event::Mouse(mouse::Event::WheelScrolled { .. }));
        if is_wheel_event {
            debug!("SyncedImageSplit: synced_zoom = {}", split_state.synced_zoom);
        }
        
        let mut children = layout.children();
        let first_layout = children
            .next()
            .expect("Native: Layout should have a first layout");
        
        let divider_layout = children
            .next()
            .expect("Native: Layout should have a divider layout");
        
        let second_layout = children
            .next()
            .expect("Native: Layout should have a second layout");
        
        // Special direct handling for wheel events when synced_zoom is enabled
        if split_state.synced_zoom && is_wheel_event {
            if let Event::Mouse(mouse::Event::WheelScrolled { delta }) = event {
                debug!("Wheel event with delta: {:?}", delta);
                
                // Find which pane the cursor is over
                let cursor_pos = cursor.position().unwrap_or_default();
                let first_bounds = first_layout.bounds();
                let second_bounds = second_layout.bounds();
                
                let (target_pane, target_layout, other_layout) = if first_bounds.contains(cursor_pos) {
                    (0, first_layout, second_layout)
                } else if second_bounds.contains(cursor_pos) {
                    (1, second_layout, first_layout)
                } else {
                    debug!("Cursor outside both panes - ignoring");
                    // Outside either pane, process normally
                    let first_status = self.first.as_widget_mut().on_event(
                        &mut state.children[0], event.clone(), first_layout, cursor, 
                        renderer, clipboard, shell, viewport,
                    );
                    
                    let second_status = self.second.as_widget_mut().on_event(
                        &mut state.children[1], event.clone(), second_layout, cursor, 
                        renderer, clipboard, shell, viewport,
                    );
                    
                    return first_status.merge(second_status);
                };
                
                // Calculate relative cursor position within target pane (0.0-1.0)
                // This gives us the anchor point for zoom
                let target_bounds = target_layout.bounds();
                let normalized_cursor = Point::new(
                    (cursor_pos.x - target_bounds.x) / target_bounds.width,
                    (cursor_pos.y - target_bounds.y) / target_bounds.height
                );
                
                // Calculate corresponding point in other pane
                let other_bounds = other_layout.bounds();
                let other_cursor_pos = Point::new(
                    other_bounds.x + normalized_cursor.x * other_bounds.width,
                    other_bounds.y + normalized_cursor.y * other_bounds.height
                );
                
                debug!("Processing wheel event in pane {}", target_pane);
                
                // Handle target pane with actual cursor
                let target_status = if target_pane == 0 {
                    self.first.as_widget_mut().on_event(
                        &mut state.children[0],
                        event.clone(),
                        target_layout,
                        cursor,
                        renderer,
                        clipboard,
                        shell,
                        viewport,
                    )
                } else {
                    self.second.as_widget_mut().on_event(
                        &mut state.children[1],
                        event.clone(),
                        target_layout,
                        cursor,
                        renderer,
                        clipboard,
                        shell,
                        viewport,
                    )
                };
                
                // Handle other pane with simulated cursor at corresponding position
                let other_status = if target_pane == 0 {
                    self.second.as_widget_mut().on_event(
                        &mut state.children[1],
                        event.clone(),
                        other_layout,
                        Cursor::Available(other_cursor_pos),
                        renderer,
                        clipboard,
                        shell,
                        viewport,
                    )
                } else {
                    self.first.as_widget_mut().on_event(
                        &mut state.children[0],
                        event.clone(),
                        other_layout,
                        Cursor::Available(other_cursor_pos),
                        renderer,
                        clipboard,
                        shell,
                        viewport,
                    )
                };
                
                debug!("Processed wheel events in both panes");
                
                // Verify state after wheel processing
                let mut verify_first_op = ZoomStateOperation::new_query();
                let mut verify_second_op = ZoomStateOperation::new_query();
                
                self.first.as_widget().operate(
                    &mut state.children[0],
                    first_layout,
                    renderer,
                    &mut verify_first_op
                );
                
                self.second.as_widget().operate(
                    &mut state.children[1],
                    second_layout,
                    renderer,
                    &mut verify_second_op
                );
                
                debug!("VERIFICATION: First pane scale={}, offset=({}, {})", 
                       verify_first_op.scale, verify_first_op.offset.x, verify_first_op.offset.y);
                debug!("VERIFICATION: Second pane scale={}, offset=({}, {})", 
                       verify_second_op.scale, verify_second_op.offset.x, verify_second_op.offset.y);
                
                // Return the status from the target pane that was directly interacted with
                return target_status.merge(other_status);
            }
        }
        
        // For non-wheel events, process normally
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
        
        let second_status = self.second.as_widget_mut().on_event(
            &mut state.children[1],
            event.clone(),
            second_layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        
        // Handle other split widget specific events
        let event_status = match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
            | Event::Touch(touch::Event::FingerPressed { .. }) => {
                // Detect double-click event on the divider
                if divider_layout
                    .bounds()
                    .expand(10.0)
                    .contains(cursor.position().unwrap_or_default())
                {
                    split_state.dragging = true;

                    // Handle double-click detection
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
                                return event::Status::Captured;
                            }
                        } else {
                            // Reset the timer for a new potential double-click
                            split_state.last_click_time = Some(Instant::now());
                        }
                    } else {
                        split_state.last_click_time = Some(Instant::now());
                    }
                    
                    event::Status::Captured
                } else if self.enable_pane_selection {
                    // Only handle pane selection if enabled
                    let is_within_bounds_first = is_cursor_within_bounds::<Message>(first_layout, cursor, 0, split_state);
                    if is_within_bounds_first {
                        split_state.panes_seleced[0] = !split_state.panes_seleced[0];
                        shell.publish((self.on_select)(0, split_state.panes_seleced[0]));
                        event::Status::Captured
                    } else {
                        let is_within_bounds_second = is_cursor_within_bounds::<Message>(second_layout, cursor, 1, split_state);
                        if is_within_bounds_second {
                            split_state.panes_seleced[1] = !split_state.panes_seleced[1];
                            shell.publish((self.on_select)(1, split_state.panes_seleced[1]));
                            event::Status::Captured
                        } else {
                            event::Status::Ignored
                        }
                    }
                } else {
                    event::Status::Ignored
                }
            },
            
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            | Event::Touch(touch::Event::FingerLifted { .. }) => {
                // Always clear dragging state on button release
                split_state.dragging = false;
                event::Status::Ignored
            },
            
            Event::Mouse(mouse::Event::CursorMoved { position })
            | Event::Touch(touch::Event::FingerMoved { position, .. }) => {
                if split_state.dragging {
                    let position = match self.axis {
                        Axis::Horizontal => position.y - self.menu_bar_height,
                        Axis::Vertical => position.x,
                    };
                    shell.publish((self.on_resize)(position as u16));
                    event::Status::Captured
                } else {
                    event::Status::Ignored
                }
            },
            
            // Handle file drop events - retain original functionality
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            Event::Window(iced::window::Event::FileDropped(path, _)) => {
                let cursor_pos = cursor.position().unwrap_or_default();
                
                // Check first pane (index 0)
                if first_layout.bounds().contains(cursor_pos) {
                    debug!("FileDropped - First pane");
                    shell.publish((self.on_drop)(0, path[0].to_string_lossy().to_string()));
                    event::Status::Captured
                } 
                // Check second pane (index 1)
                else if second_layout.bounds().contains(cursor_pos) {
                    debug!("FileDropped - Second pane");
                    shell.publish((self.on_drop)(1, path[0].to_string_lossy().to_string()));
                    event::Status::Captured
                } else {
                    event::Status::Ignored
                }
            },
            
            // File drop for Linux
            #[cfg(target_os = "linux")]
            Event::Window(iced::window::Event::FileDropped(paths, _)) => {
                if paths.is_empty() {
                    return event::Status::Ignored;
                }
                
                let cursor_pos = cursor.position().unwrap_or_default();
                let path = &paths[0];
                
                if first_layout.bounds().contains(cursor_pos) {
                    debug!("FileDropped - First pane");
                    shell.publish((self.on_drop)(0, path.to_string_lossy().to_string()));
                    event::Status::Captured
                } else if second_layout.bounds().contains(cursor_pos) {
                    debug!("FileDropped - Second pane");
                    shell.publish((self.on_drop)(1, path.to_string_lossy().to_string()));
                    event::Status::Captured
                } else {
                    event::Status::Ignored
                }
            },
            
            _ => event::Status::Ignored,
        };
        
        first_status.merge(second_status).merge(event_status)
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
            Background::Color(Color::from_rgb(0.2, 0.2, 0.2)),
        );
        

        // Draw pane selection status only if enabled
        if self.enable_pane_selection {
            let style = theme.style(&self.class, Status::Active);
            
            if self.is_selected[0] {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: first_layout.bounds(),
                        border: Border {
                            color: style.primary.base.color,
                            width: 1.0,
                            radius: Radius::new(0.0),
                        },
                        shadow: Default::default(),
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
                        shadow: Default::default(),
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
        operation: &mut dyn Operation,
    ) {
        let split_state = state.state.downcast_ref::<State>();
        
        // Check if synced zoom is enabled before injecting state
        if self.synced_zoom {
            // Get child layouts
            let mut children = layout.children();
            let first_layout = children.next().expect("Missing Split First window");
            let _divider_layout = children.next().expect("Missing Split Divider");
            let second_layout = children.next().expect("Missing Split Second window");
            
            // Split the tree for mutable access to both children
            let (first_state, second_state) = state.children.split_at_mut(1);
            
            // Create a zoom operation with the current shared zoom state
            let mut zoom_op = ZoomStateOperation {
                scale: split_state.shared_scale,
                offset: split_state.shared_offset,
                is_setting: false,
            };
            
            // Propagate to first child
            self.first.as_widget().operate(
                &mut first_state[0],
                first_layout,
                renderer,
                &mut zoom_op,
            );
            
            // Propagate to second child
            self.second.as_widget().operate(
                &mut second_state[0],
                second_layout,
                renderer,
                &mut zoom_op,
            );
        }
        
        // Continue with the original operation
        let mut children = layout.children();
        let first_layout = children.next().expect("Missing Split First window");
        let _divider_layout = children.next().expect("Missing Split Divider");
        let second_layout = children.next().expect("Missing Split Second window");
        
        let (first_state, second_state) = state.children.split_at_mut(1);
        
        // Forward the original operation to children
        self.first.as_widget().operate(
            &mut first_state[0], 
            first_layout, 
            renderer, 
            operation
        );
        
        self.second.as_widget().operate(
            &mut second_state[0], 
            second_layout, 
            renderer, 
            operation
        );
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

/// The state of a [`SyncedImageSplit`].
#[derive(Clone, Debug)]
pub struct State {
    /// If the divider is dragged by the user.
    dragging: bool,
    last_click_time: Option<Instant>,
    panes_seleced: [bool; 2],
    
    // Add fields for synced zooming
    synced_zoom: bool,
    shared_scale: f32,
    shared_offset: Vector,
}

impl State {
    /// Creates a new [`State`] for a [`SyncedImageSplit`].
    pub fn new() -> Self {
        Self {
            dragging: false,
            last_click_time: None,
            panes_seleced: [true, true],
            
            // Initialize zoom state
            synced_zoom: true,
            shared_scale: 1.0,
            shared_offset: Vector::default(),
            
        }
    }
}

/// Do a horizontal split.
fn horizontal_split<'a, Message, Theme, Renderer>(
    tree: &mut Tree,
    split: &SyncedImageSplit<'a, Message, Theme, Renderer>,
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

    // Debug logs to verify positions and heights
    //debug!("HORIZONTAL Split: equal_pane_height={}, first_y={}, divider_y={}, second_y={}, first_height={}, second_height={}", 
    //       equal_pane_height,
    //       space.bounds().y + split.padding,
    //       clamped_position,
    //       space.bounds().y + clamped_position + split.spacing + split.padding,
    //       clamped_position - (space.bounds().y + split.padding),
    //       total_height - (clamped_position + split.spacing + split.padding*2.0));

    // Maintain the original 3-node structure expected by other methods
    Node::with_children(space.bounds().size(), vec![first, divider, second])
}


/// Do a vertical split.
fn vertical_split<'a, Message, Theme, Renderer>(
    tree: &mut Tree,
    split: &SyncedImageSplit<'a, Message, Theme, Renderer>,
    renderer: &Renderer,
    limits: &Limits,
    space: &Node,
) -> Node
where
    Renderer: 'a + renderer::Renderer,
    Theme: Catalog,
{
    let _bounds = space.bounds();
    //debug!("VERTICAL Split calculation - bounds: {:?}", bounds);
    
    if space.bounds().width
        < split.spacing + f32::from(split.min_size_first + split.min_size_second)
    {
        //debug!("VERTICAL Split - insufficient width for proper split, using fallback layout");
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
    
    //debug!("VERTICAL Split calculation: available_width={}, divider_position={}, spacing={}", 
    //       available_width, divider_position, split.spacing);
    
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

    //debug!("VERTICAL Spacing: {}, First right edge: {}, Divider left: {}, Divider right: {}, Second left: {}, Gap size: {}", 
    //    split.spacing,
    //    first_end_x,
    //    divider_left_x,
    //    divider_left_x + divider_width,
    //    second_start_x,
    //    gap
    //);

    let result = Node::with_children(space.bounds().size(), vec![first, divider, second]);
    
    // Debug output to verify the bounds are correct
    let children = result.children();
    if children.len() >= 3 {
        //debug!("VERTICAL First pane bounds: {:?}", children[0].bounds());
        //debug!("VERTICAL Divider bounds: {:?}", children[1].bounds());
        //debug!("VERTICAL Second pane bounds: {:?}", children[2].bounds());
        
        // Add total width check to verify we don't exceed available space
        //let total_used_width = children[0].bounds().width + divider_width + (2.0 * gap) + children[2].bounds().width;
        //debug!("VERTICAL Total width used: {} (available: {})", total_used_width, bounds.width);
    }
    
    result
}

impl<'a, Message, Theme, Renderer> From<SyncedImageSplit<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
    Renderer: renderer::Renderer + 'a,
    Theme: Catalog + 'a,
{
    fn from(split_pane: SyncedImageSplit<'a, Message, Theme, Renderer>) -> Self {
        Element::new(split_pane)
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

/// Custom operation for synchronizing zoom state between image panes
#[derive(Debug, Clone, Copy)]
pub struct ZoomStateOperation {
    /// The scale factor to apply
    pub scale: f32,
    /// The offset for panning
    pub offset: Vector,
    /// Whether we're querying (false) or setting (true) the state
    pub is_setting: bool,
}

impl widget::Operation for ZoomStateOperation {
    fn container(
        &mut self,
        _id: Option<&widget::Id>,
        _bounds: Rectangle,
        operate_on_children: &mut dyn FnMut(&mut dyn widget::Operation),
    ) {
        // Just forward the operation to children
        operate_on_children(self);
    }

    // In the ImageShader custom method, we'll check for this type
    fn custom(&mut self, state: &mut dyn std::any::Any, _id: Option<&widget::Id>) {
        // Try to downcast to ImageShaderState - add more detailed logging
        debug!("ZoomStateOperation: Attempting to downcast state type: {}", std::any::type_name_of_val(state));
        
        // Try multiple potential state types - we need the correct one
        if let Some(shader_state) = state.downcast_mut::<crate::widgets::shader::image_shader::ImageShaderState>() {
            if self.is_setting {
                // Set zoom values
                debug!("ZoomStateOperation: SETTING scale={} -> {}, offset=({},{}) -> ({},{})",
                       shader_state.scale, self.scale,
                       shader_state.current_offset.x, shader_state.current_offset.y,
                       self.offset.x, self.offset.y);
                
                shader_state.scale = self.scale;
                shader_state.current_offset = self.offset;
            } else {
                // Query zoom values (update our own scale/offset from the state)
                debug!("ZoomStateOperation: QUERYING scale={}, offset=({},{})",
                       shader_state.scale, 
                       shader_state.current_offset.x, shader_state.current_offset.y);
                
                self.scale = shader_state.scale;
                self.offset = shader_state.current_offset;
            }
        } else {
            // Try alternative namespaces - the actual path might be different
            debug!("ZoomStateOperation: Failed to downcast to ImageShaderState");
        }
    }
}

impl ZoomStateOperation {
    pub fn new_query() -> Self {
        Self {
            scale: 1.0,
            offset: Vector::default(),
            is_setting: false,
        }
    }
}