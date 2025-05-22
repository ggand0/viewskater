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
    event, mouse::{self, Cursor}, touch,
    widget::Row,
    Background, Border, Color, Element, Event, Length, Point,
    Rectangle, Shadow, Size, Vector
};
use iced::border::Radius;
use crate::widgets::split::{horizontal_split, vertical_split, SplitLayoutConfig};

use std::time::{Duration, Instant};
#[allow(unused_imports)]
use log::{Level, debug, info, warn, error};

use iced_core::widget;
use crate::widgets::split::Axis;
use crate::widgets::split::{Catalog, Status, Style, StyleFn};
use crate::CONFIG;

// Add module-level debug flag - set to false to disable all debug logs
const DEBUG_LOGS_ENABLED: bool = false;

// Define a macro for conditional debug printing
macro_rules! debug_log {
    ($($arg:tt)*) => {
        if DEBUG_LOGS_ENABLED {
            debug!($($arg)*);
        }
    };
}

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
            min_size_first: 20,
            min_size_second: 20,
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
        
        let config = SplitLayoutConfig {
            first: &self.first,
            second: &self.second,
            divider_position: self.divider_position,
            spacing: self.spacing,
            padding: self.padding,
            min_size_first: self.min_size_first,
            min_size_second: self.min_size_second,
            debug: false,
        };

        match self.axis {
            Axis::Horizontal => horizontal_split(tree, &config, renderer, limits, &space),
            Axis::Vertical => vertical_split(tree, &config, renderer, limits, &space),
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
            debug_log!("SyncedImageSplit: synced_zoom = {}", split_state.synced_zoom);
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
                debug_log!("Wheel event with delta: {:?}", delta);
                
                // Find which pane the cursor is over
                let cursor_pos = cursor.position().unwrap_or_default();
                let first_bounds = first_layout.bounds();
                let second_bounds = second_layout.bounds();
                
                let (target_pane, target_layout, other_layout) = if first_bounds.contains(cursor_pos) {
                    (0, first_layout, second_layout)
                } else if second_bounds.contains(cursor_pos) {
                    (1, second_layout, first_layout)
                } else {
                    debug_log!("Cursor outside both panes - ignoring");
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
                
                debug_log!("Processing wheel event in pane {}", target_pane);
                
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
                
                debug_log!("Processed wheel events in both panes");
                
                // After processing wheel events in both panes, ensure full state synchronization
                if target_status == event::Status::Captured || other_status == event::Status::Captured {
                    // First, query the state from the target pane
                    let (first_state, second_state) = state.children.split_at_mut(1);
                    let mut query_op = ZoomStateOperation::new_query();
                    
                    if target_pane == 0 {
                        ZoomStateOperation::operate(
                            &mut first_state[0],
                            Rectangle::default(),
                            renderer,
                            &mut query_op
                        );
                    } else {
                        ZoomStateOperation::operate(
                            &mut second_state[0],
                            Rectangle::default(),
                            renderer,
                            &mut query_op
                        );
                    }
                    
                    // Apply the same state to the other pane to ensure complete synchronization
                    let mut apply_op = ZoomStateOperation::new_apply(
                        query_op.scale, query_op.offset
                    );
                    
                    let success = if target_pane == 0 {
                        ZoomStateOperation::operate(
                            &mut second_state[0],
                            Rectangle::default(),
                            renderer,
                            &mut apply_op
                        )
                    } else {
                        ZoomStateOperation::operate(
                            &mut first_state[0],
                            Rectangle::default(),
                            renderer,
                            &mut apply_op
                        )
                    };
                    
                    if !success {
                        debug_log!("SyncedImageSplit: Could not fully sync state after wheel event");
                    } else {
                        debug_log!("SyncedImageSplit: Fully synchronized state after wheel event");
                    }
                }
                
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
                // Check if click is on the divider first
                if divider_layout
                    .bounds()
                    .expand(10.0)
                    .contains(cursor.position().unwrap_or_default())
                {
                    split_state.dragging = true;
                    debug_log!("Starting divider drag operation");

                    // Handle double-click for divider reset
                    if let Some(last_click_time) = split_state.last_click_time {
                        let elapsed = last_click_time.elapsed();
                        if elapsed < Duration::from_millis(CONFIG.double_click_threshold_ms as u64) {
                            // Double-click detected
                            split_state.last_click_time = None;
                            split_state.dragging = false;
                            shell.publish((self.on_double_click)(0));
                            return event::Status::Captured;
                        } else {
                            split_state.last_click_time = Some(Instant::now());
                        }
                    } else {
                        split_state.last_click_time = Some(Instant::now());
                    }
                    
                    return event::Status::Captured;
                }
                
                // Initialize panning state if cursor is over an image pane
                if first_layout.bounds().contains(cursor.position().unwrap_or_default()) {
                    split_state.active_pane_for_pan = Some(0);
                    split_state.pan_start_position = cursor.position().unwrap_or_default();
                    debug_log!("Starting pan operation in first pane");
                    
                    // Handle double-click for reset zoom when synced_zoom is true
                    if split_state.synced_zoom {
                        if let Some(last_click_time) = split_state.last_pane_click_time {
                            let elapsed = last_click_time.elapsed();
                            if elapsed < Duration::from_millis(500) {
                                // Double-click detected on pane - reset zoom for both panes
                                debug_log!("Double-click detected in pane - resetting zoom");
                                split_state.last_pane_click_time = None;
                                
                                // Reset shared zoom state
                                split_state.shared_scale = 1.0;
                                split_state.shared_offset = Vector::default();
                                
                                // Apply reset to both panes
                                let mut reset_op = ZoomStateOperation::new_apply(1.0, Vector::default());
                                
                                ZoomStateOperation::operate(
                                    &mut state.children[0],
                                    Rectangle::default(),
                                    renderer,
                                    &mut reset_op
                                );
                                
                                ZoomStateOperation::operate(
                                    &mut state.children[1],
                                    Rectangle::default(),
                                    renderer,
                                    &mut reset_op
                                );
                                
                                return event::Status::Captured;
                            } else {
                                split_state.last_pane_click_time = Some(Instant::now());
                            }
                        } else {
                            split_state.last_pane_click_time = Some(Instant::now());
                        }
                    }
                } else if second_layout.bounds().contains(cursor.position().unwrap_or_default()) {
                    split_state.active_pane_for_pan = Some(1);
                    split_state.pan_start_position = cursor.position().unwrap_or_default();
                    debug_log!("Starting pan operation in second pane");
                    
                    // Handle double-click for reset zoom when synced_zoom is true
                    if split_state.synced_zoom {
                        if let Some(last_click_time) = split_state.last_pane_click_time {
                            let elapsed = last_click_time.elapsed();
                            if elapsed < Duration::from_millis(500) {
                                // Double-click detected on pane - reset zoom for both panes
                                debug_log!("Double-click detected in pane - resetting zoom");
                                split_state.last_pane_click_time = None;
                                
                                // Reset shared zoom state
                                split_state.shared_scale = 1.0;
                                split_state.shared_offset = Vector::default();
                                
                                // Apply reset to both panes
                                let mut reset_op = ZoomStateOperation::new_apply(1.0, Vector::default());
                                
                                ZoomStateOperation::operate(
                                    &mut state.children[0],
                                    Rectangle::default(),
                                    renderer,
                                    &mut reset_op
                                );
                                
                                ZoomStateOperation::operate(
                                    &mut state.children[1],
                                    Rectangle::default(),
                                    renderer,
                                    &mut reset_op
                                );
                                
                                return event::Status::Captured;
                            } else {
                                split_state.last_pane_click_time = Some(Instant::now());
                            }
                        } else {
                            split_state.last_pane_click_time = Some(Instant::now());
                        }
                    }
                }
                
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
                // Always clear dragging and panning state on button release
                split_state.dragging = false;
                split_state.active_pane_for_pan = None;
                debug_log!("Ending drag/pan operation");
                event::Status::Ignored
            },

            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                // Handle divider dragging first
                if split_state.dragging {
                    let raw_position = match self.axis {
                        Axis::Horizontal => position.y - self.menu_bar_height,
                        Axis::Vertical => position.x,
                    };
                    
                    // Print debugging info
                    let bounds = layout.bounds();
                    let min_left = self.min_size_first as f32;
                    let min_right = self.min_size_second as f32;
                    let max_pos = bounds.x + bounds.width - min_right - self.spacing;
                    let min_pos = bounds.x + min_left;
                    
                    debug_log!("Dragging divider - raw_position: {}, bounds: {:?}", raw_position, bounds);
                    debug_log!("Constraints - min_pos: {}, max_pos: {}, spacing: {}", min_pos, max_pos, self.spacing);
                    debug_log!("min_size_first: {}, min_size_second: {}", self.min_size_first, self.min_size_second);
                    
                    shell.publish((self.on_resize)(raw_position as u16));
                    return event::Status::Captured;
                }
                
                // Then handle panning synchronization
                if self.synced_zoom && split_state.active_pane_for_pan.is_some() {
                    let active_pane = split_state.active_pane_for_pan.unwrap();
                    debug_log!("Pan sync: active_pane={}", active_pane);
                    
                    // Get child layouts
                    let mut children = layout.children();
                    let first_layout = children.next().expect("Missing Split First window");
                    let _divider_layout = children.next().expect("Missing Split Divider");
                    let second_layout = children.next().expect("Missing Split Second window");
                    
                    // Process the event on the active pane first
                    let active_layout = if active_pane == 0 { first_layout } else { second_layout };
                    let event_status = if active_pane == 0 {
                        self.first.as_widget_mut().on_event(
                            &mut state.children[0],
                            event.clone(),
                            active_layout,
                            cursor.clone(),
                            renderer,
                            clipboard,
                            shell,
                            viewport,
                        )
                    } else {
                        self.second.as_widget_mut().on_event(
                            &mut state.children[1],
                            event.clone(),
                            active_layout,
                            cursor.clone(),
                            renderer,
                            clipboard,
                            shell,
                            viewport,
                        )
                    };
                    
                    if event_status == event::Status::Captured {
                        // The active pane has processed the event, now query its state
                        let (first_state, second_state) = state.children.split_at_mut(1);
                        
                        // Create a query operation to get the current state
                        let mut query_op = ZoomStateOperation::new_query();
                        
                        // Query the state from the active pane
                        if active_pane == 0 {
                            ZoomStateOperation::operate(
                                &mut first_state[0],
                                Rectangle::default(),
                                renderer,
                                &mut query_op
                            );
                        } else {
                            ZoomStateOperation::operate(
                                &mut second_state[0],
                                Rectangle::default(),
                                renderer,
                                &mut query_op
                            );
                        }
                        
                        // Update the shared state
                        split_state.shared_scale = query_op.scale;
                        split_state.shared_offset = query_op.offset;
                        
                        debug_log!("Syncing pan state: scale={}, offset=({},{})",
                               query_op.scale, query_op.offset.x, query_op.offset.y);
                        
                        // Apply the same state to the other pane
                        let mut apply_op = ZoomStateOperation::new_apply(
                            query_op.scale, query_op.offset
                        );
                        
                        // Apply to the other pane
                        let success = if active_pane == 0 {
                            ZoomStateOperation::operate(
                                &mut second_state[0],
                                Rectangle::default(),
                                renderer,
                                &mut apply_op
                            )
                        } else {
                            ZoomStateOperation::operate(
                                &mut first_state[0],
                                Rectangle::default(),
                                renderer,
                                &mut apply_op
                            )
                        };
                        
                        if !success {
                            debug_log!("SyncedImageSplit: Could not sync zoom state - target pane doesn't support it");
                        }
                        
                        return event::Status::Captured;
                    }
                }
                
                // If we're not syncing or no active pane, process normally
                let first_status = self.first.as_widget_mut().on_event(
                    &mut state.children[0],
                    event.clone(),
                    first_layout,
                    cursor.clone(),
                    renderer,
                    clipboard,
                    shell,
                    viewport,
                );
                
                let second_status = self.second.as_widget_mut().on_event(
                    &mut state.children[1],
                    event.clone(),
                    second_layout,
                    cursor.clone(),
                    renderer,
                    clipboard,
                    shell,
                    viewport,
                );
                
                first_status.merge(second_status)
            },
            
            // Handle file drop events - retain original functionality
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            Event::Window(iced::window::Event::FileDropped(path, position)) => {
                // Use the position from the event directly instead of cursor position
                let drop_position = Point::new(position.x as f32, position.y as f32);
                
                debug_log!("FileDropped at position: {:?}", drop_position);
                
                // Check first pane (index 0)
                if first_layout.bounds().contains(drop_position) {
                    debug_log!("FileDropped - First pane");
                    shell.publish((self.on_drop)(0, path[0].to_string_lossy().to_string()));
                    event::Status::Captured
                } 
                // Check second pane (index 1)
                else if second_layout.bounds().contains(drop_position) {
                    debug_log!("FileDropped - Second pane");
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
                    debug_log!("FileDropped - First pane");
                    shell.publish((self.on_drop)(0, path.to_string_lossy().to_string()));
                    event::Status::Captured
                } else if second_layout.bounds().contains(cursor_pos) {
                    debug_log!("FileDropped - Second pane");
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
        let split_state = state.state.downcast_mut::<State>();
        
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
                query_only: false,
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
    last_pane_click_time: Option<Instant>,
    panes_seleced: [bool; 2],
    
    // Zoom and pan synchronization state
    synced_zoom: bool,
    shared_scale: f32,
    shared_offset: Vector,
    active_pane_for_pan: Option<usize>,
    pan_start_position: Point,
}

impl State {
    /// Creates a new [`State`] for a [`SyncedImageSplit`].
    pub fn new() -> Self {
        Self {
            dragging: false,
            last_click_time: None,
            last_pane_click_time: None,
            panes_seleced: [false, false],
            
            // Initialize zoom and pan state
            synced_zoom: false,
            shared_scale: 1.0,
            shared_offset: Vector::default(),
            active_pane_for_pan: None,
            pan_start_position: Point::default(),
        }
    }
}


impl<'a, Message, Theme, Renderer> From<SyncedImageSplit<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a + Clone,
    Renderer: 'a + renderer::Renderer,
    Theme: 'a + Catalog,
{
    fn from(split_pane: SyncedImageSplit<'a, Message, Theme, Renderer>) -> Self {
        Element::new(split_pane)
    }
}


/// Custom operation for synchronizing zoom state between image panes
#[derive(Debug)]
pub struct ZoomStateOperation {
    /// The scale factor to apply
    pub scale: f32,
    /// The offset for panning
    pub offset: Vector,
    /// Whether we're querying (true) or setting (false) the state
    pub query_only: bool,
}

impl ZoomStateOperation {
    pub fn new_query() -> Self {
        Self {
            scale: 1.0,
            offset: Vector::default(),
            query_only: true,
        }
    }
    
    pub fn new_apply(scale: f32, offset: Vector) -> Self {
        Self {
            scale,
            offset,
            query_only: false,
        }
    }
}

// Implement the Operation trait
impl<T> widget::Operation<T> for ZoomStateOperation {
    fn container(
        &mut self, 
        _id: Option<&widget::Id>, 
        _bounds: Rectangle, 
        _operate: &mut dyn FnMut(&mut dyn widget::Operation<T>),
    ) {
        // Empty implementation
    }
}

// COMPLETELY SEPARATE STATIC FUNCTION
// This is not part of the Operation trait implementation
impl ZoomStateOperation {
    pub fn operate<T>(
        tree: &mut widget::Tree,
        _bounds: Rectangle,
        _renderer: &T,
        operation: &mut Self,
    ) -> bool {
        // Check if the tree's tag matches ImageShaderState's type before attempting downcast
        if tree.tag == tree::Tag::of::<crate::widgets::shader::image_shader::ImageShaderState>() {
            // Now it's safe to downcast
            let shader_state = tree.state.downcast_mut::<crate::widgets::shader::image_shader::ImageShaderState>();
            
            if !operation.query_only {  // Apply mode
                shader_state.scale = operation.scale;
                shader_state.current_offset = operation.offset;
                debug_log!("ZoomStateOperation: Applied scale={}, offset=({},{})",
                      operation.scale, operation.offset.x, operation.offset.y);
            } else {  // Query mode
                operation.scale = shader_state.scale;
                operation.offset = shader_state.current_offset;
                debug_log!("ZoomStateOperation: Queried scale={}, offset=({},{})",
                      operation.scale, operation.offset.x, operation.offset.y);
            }
            true
        } else {
            // This tree doesn't contain an ImageShaderState
            debug_log!("ZoomStateOperation: Tree doesn't contain ImageShaderState, skipping");
            false
        }
    }
}