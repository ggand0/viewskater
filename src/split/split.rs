//! Use a split to split the available space in two parts to display two different elements.
//!
//! *This API requires the following crate features to be activated: split*
#[cfg(target_os = "linux")]
mod other_os {
    pub use iced;
    pub use iced_widget;
}

#[cfg(not(target_os = "linux"))]
mod macos {
    pub use iced_custom as iced;

    pub use iced_widget_custom as iced_widget;
}

#[cfg(target_os = "linux")]
use other_os::*;

#[cfg(not(target_os = "linux"))]
use macos::*;

use iced_widget::{
    container,
    core::{
        self, event,
        layout::{Limits, Node},
        mouse::{self, Cursor},
        renderer, touch,
        widget::{
            tree::{State, Tag},
            Operation, Tree,
        },
        Clipboard, Color, Element, Event, Layout, Length, Padding, Point, Rectangle, Shell, Size,
        Widget,
    },
    Container, Row,
};

use std::time::{Duration, Instant};
use crate::split::style::StyleSheet;

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
#[allow(dead_code)]
pub struct Split<'a, Message, Renderer>
where
    Renderer: core::Renderer,
    Renderer::Theme: StyleSheet,
{
    /// The first element of the [`Split`].
    first: Element<'a, Message, Renderer>,
    /// The second element of the [`Split`].
    second: Element<'a, Message, Renderer>,

    is_selected: Vec<bool>,

    /// The position of the divider.
    divider_position: Option<u16>,
    divider_init_position: Option<u16>,
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
    // on_drop: Option<Box<dyn Fn(u16) -> Message>>,
    on_drop: Box<dyn Fn(isize, String) -> Message>,
    on_select: Box<dyn Fn(usize, bool) -> Message>,

    /// The style of the [`Split`].
    style: <Renderer::Theme as StyleSheet>::Style,
    default_position: Option<u16>,
    has_been_split: bool,

    // Whether to enable pane selection
    enable_pane_selection: bool,
}

impl<'a, Message, Renderer> Split<'a, Message, Renderer>
where
    Message: 'a,
    Renderer: 'a + core::Renderer,
    Renderer::Theme: StyleSheet + container::StyleSheet,
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
        A: Into<Element<'a, Message, Renderer>>,
        B: Into<Element<'a, Message, Renderer>>,
        F: 'static + Fn(u16) -> Message,
        G: 'static + Fn(u16) -> Message,
        H: 'static + Fn(isize, String) -> Message,
        I: 'static + Fn(usize, bool) -> Message,
    {
        Self {
            first: Container::new(first.into())
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
            second: Container::new(second.into())
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
            is_selected: is_selected,
            divider_position,
            divider_init_position: divider_position,
            axis,
            padding: 0.0,
            spacing: 5.0,
            // spacing: 0.0,
            // spacing: 1.0,
            width: Length::Fill,
            height: Length::Fill,
            min_size_first: 5,
            min_size_second: 5,
            on_resize: Box::new(on_resize),
            on_double_click: Box::new(on_double_click),
            on_drop: Box::new(on_drop),
            on_select: Box::new(on_select),
            style: <Renderer::Theme as StyleSheet>::Style::default(),
            default_position: None,
            has_been_split: false,
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
    pub fn width(mut self, width: Length) -> Self {
        self.width = width;
        self
    }

    /// Sets the height of the [`Split`].
    #[must_use]
    pub fn height(mut self, height: Length) -> Self {
        self.height = height;
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
    pub fn style(mut self, style: <Renderer::Theme as StyleSheet>::Style) -> Self {
        self.style = style;
        self
    }

}

impl<'a, Message, Renderer> Widget<Message, Renderer> for Split<'a, Message, Renderer>
where
    Renderer: 'a + core::Renderer,
    Renderer::Theme: StyleSheet,
{
    fn tag(&self) -> Tag {
        Tag::of::<SplitState>()
    }

    fn state(&self) -> State {
        State::new(SplitState::new())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.first), Tree::new(&self.second)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[&self.first, &self.second]);
    }

    fn width(&self) -> Length {
        self.width
    }

    fn height(&self) -> Length {
        self.height
    }

    fn layout(&self, renderer: &Renderer, limits: &Limits) -> Node {
        let space = Row::<Message, Renderer>::new()
            .width(Length::Fill)
            .height(Length::Fill)
            .layout(renderer, limits);

        match self.axis {
            Axis::Horizontal => horizontal_split(self, renderer, limits, &space),
            Axis::Vertical => vertical_split(self, renderer, limits, &space),
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
        // DEBUG
        // debug!("self.divider_position: {:?}", self.divider_position);
        // debug!("Cursor position: {:?}", cursor.position().unwrap_or_default());
        for child_layout in layout.children() {
            let _bounds = child_layout.bounds();
            // debug!("cursor.is_over(bounds): {:?}", cursor.is_over(bounds));
        }

        let split_state: &mut SplitState = state.state.downcast_mut();
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
            .expect("Native: Layout should have a second layout");
        
        match event.clone() {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
            | Event::Touch(touch::Event::FingerPressed { .. }) => {
                // Detect double-click event on the divider
                if divider_layout
                    .bounds()
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


            // #[cfg(target_os = "macos")]
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            Event::Window(iced::window::Event::FileHovered(position)) => {
                // Access the cursor position from the FileHovered event
                // debug!("FileHovered Cursor position: {:?}", cursor.position().unwrap_or_default());
                debug!("FILEHOVER POSITION: {:?}", position);
            }

            #[cfg(any(target_os = "macos", target_os = "windows"))]
            Event::Window(iced::window::Event::FileDropped(paths, position)) => {
                debug!("FILEDROP POSITION: {:?}", position);
                let mut index = 0;
                debug!("layout children length: {}", layout.children().count());
                for child_layout in layout.children() {
                    // debug!("Child layout: {:?}", child_layout);
                    let bounds = child_layout.bounds();
                    debug!("Child bounds: {:?}", bounds);
                    // debug!("FileDropped Cursor position: {:?}", cursor.position().unwrap_or_default());
                    // debug!("Cursor position: {:?}", cursor.position());

                    // TODO: Implement enum LayoutItem { Pane, Divider }
                    /////// BEGIN HACK
                    if (bounds.width - 5.0).abs() < std::f32::EPSILON {
                        // This is a divider
                        continue;
                    }
                    /////// END HACK
            
                    let custom_position = Point::new(position.x as f32, position.y as f32);
                    debug!("custom_position, bounds.contains(custom_position: {:?}, {:?}", custom_position, bounds.contains(custom_position));
                    if bounds.contains(custom_position) {
                        shell.publish((self.on_drop)(index, paths[0].to_string_lossy().to_string()));
                        return event::Status::Captured;
                    }
            
                    index += 1;
                }
            }

            #[cfg(target_os = "linux")]
            Event::Window(iced::window::Event::FileHovered(_path)) => {
                // Access the cursor position from the FileHovered event
                debug!("FileHovered Cursor position: {:?}", cursor.position().unwrap_or_default());
            }
    
            #[cfg(target_os = "linux")]
            Event::Window(iced::window::Event::FileDropped(path)) => {
                let mut index = 0;
                debug!("layout children length: {}", layout.children().count());
                for child_layout in layout.children() {
                    debug!("Child layout: {:?}", child_layout);
                    let bounds = child_layout.bounds();
                    debug!("Child bounds: {:?}", bounds);
                    debug!("FileDropped Cursor position: {:?}", cursor.position().unwrap_or_default());
                    
                    // debug!("Cursor position: {:?}", cursor.position());
    
                    // TODO: Implement enum LayoutItem { Pane, Divider }
                    /////// BEGIN HACK
                    if (bounds.width - 5.0).abs() < std::f32::EPSILON {
                        // This is a divider
                        continue;
                    }
                    /////// END HACK
    
                    // Workaround for the winit bug on Mac OS when dragging files with trackpad
                    /*if cursor.position().unwrap_or_default() == (iced::Point { x: 0.0, y: 0.0 }) {
                        shell.publish(((self.on_drop)(-2, path.to_string_lossy().to_string())));
                        return event::Status::Captured;
                    }*/
            
                    if bounds.contains(cursor.position().unwrap_or_default()) {
                        shell.publish((self.on_drop)(index, path.to_string_lossy().to_string()));
                        return event::Status::Captured;
                    }
            
                    index += 1;
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


        let divider_mouse_interaction = if divider_layout
            .bounds()
            .contains(cursor.position().unwrap_or_default())
        {
            // debug!("Mouse is over the divider, axis: {:?}", self.axis);
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

        let fmi = first_mouse_interaction
            .max(second_mouse_interaction)
            .max(divider_mouse_interaction);
        fmi
    }

    fn draw(
        &self,
        state: &Tree,
        renderer: &mut Renderer,
        theme: &Renderer::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: Cursor,
        viewport: &Rectangle,
    ) {
        let split_state: &SplitState = state.state.downcast_ref();
        // TODO: clipping!
        let mut children = layout.children();

        // Background
        renderer.fill_quad(
            renderer::Quad {
                bounds: layout.bounds(),
                border_radius: (0.0).into(),
                border_width: theme.active(&self.style).border_width,
                border_color: theme.active(&self.style).border_color,
            },
            theme
                .active(&self.style)
                .background
                .unwrap_or_else(|| Color::TRANSPARENT.into()),
        );

        let first_layout = children
            .next()
            .expect("Graphics: Layout should have a first layout");

        // First
        renderer.fill_quad(
            renderer::Quad {
                bounds: first_layout.bounds(),
                border_radius: (0.0).into(),
                border_width: 0.0,
                border_color: Color::TRANSPARENT,
            },
            if first_layout
                .bounds()
                .contains(cursor.position().unwrap_or_default())
            {
                theme.hovered(&self.style).first_background
            } else {
                theme.active(&self.style).first_background
            }
            .unwrap_or_else(|| Color::TRANSPARENT.into()),
        );

        self.first.as_widget().draw(
            &state.children[0],
            renderer,
            theme,
            style,
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

        renderer.fill_quad(
            renderer::Quad {
                bounds: second_layout.bounds(),
                border_radius: (0.0).into(),
                border_width: 0.0,
                border_color: Color::TRANSPARENT,
            },
            if second_layout
                .bounds()
                .contains(cursor.position().unwrap_or_default())
            {
                theme.hovered(&self.style).second_background
            } else {
                theme.active(&self.style).second_background
            }
            .unwrap_or_else(|| Color::TRANSPARENT.into()),
        );

        self.second.as_widget().draw(
            &state.children[1],
            renderer,
            theme,
            style,
            second_layout,
            cursor,
            viewport,
        );

        // Divider
        let divider_style = if split_state.dragging {
            theme.dragged(&self.style)
        } else if divider_layout
            .bounds()
            .contains(cursor.position().unwrap_or_default())
        {
            theme.hovered(&self.style)
        } else {
            theme.active(&self.style)
        };



        let bounds = divider_layout.bounds();
        // Create a modified Rectangle for a thin line (1px width)
        /*let thin_rectangle = Rectangle {
            x: bounds.x,
            y: bounds.y,
            width: bounds.width, // Keep the same width
            height: 1.0,         // Set a height of 1px for a thin line
        };*/
        let is_horizontal = bounds.width >= bounds.height;

        // Create a modified Rectangle for a thin line
        let thin_rectangle = if is_horizontal {
            // For horizontal dividers
            Rectangle {
                x: bounds.x - 5.0,
                y: bounds.y,
                width: bounds.width + 5.0,
                height: 1.0,
            }
        } else {
            // For vertical dividers
            // TODO: when there's another pane above, -5.0/+5.0,
            // when it's not, 0.0/+10.0
            Rectangle {
                x: bounds.x + 2.0,
                y: bounds.y,
                // y: bounds.y,
                width: 1.0,
                height: bounds.height + 10.0,
            }
        };


        // Draw the divider
        renderer.fill_quad(
            renderer::Quad {
                // bounds: divider_layout.bounds(),
                bounds: thin_rectangle,
                border_radius: (0.0).into(),
                border_width: 0.0,//divider_style.divider_border_width,
                border_color: divider_style.divider_border_color,
            },
            Color::from_rgb(0.2, 0.2, 0.2)
        );

        // Draw pane selection status; if selected, draw a border around the pane
        if self.enable_pane_selection {
            if self.is_selected[0] {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: first_layout.bounds(),
                        border_radius: (0.0).into(),
                        border_width: 1.0,
                        border_color: Color::from_rgb(0.0, 1.0, 0.0),
                    },
                    Color::TRANSPARENT,
                );
            }
            if self.is_selected[1] {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: second_layout.bounds(),
                        border_radius: (0.0).into(),
                        border_width: 1.0,
                        border_color: Color::from_rgb(0.0, 1.0, 0.0),
                    },
                    Color::TRANSPARENT,
                );
            }
        }
    }

    fn operate<'b>(
        &'b self,
        state: &'b mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation<Message>,
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
    ) -> Option<core::overlay::Element<'b, Message, Renderer>> {
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
            .overlay(&mut first_state[0], first_layout, renderer)
            .or_else(|| {
                second
                    .as_widget_mut()
                    .overlay(&mut second_state[0], second_layout, renderer)
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
    _split_state: &mut SplitState,
) -> bool {
    debug!("Processing layout");
    if let Some(container_layout) = layout.children().next() {
        if let Some(column_layout) = container_layout.children().next() {
            if let Some(image_layout) = column_layout.children().next() {
                let image_bounds = image_layout.bounds();

                if image_bounds.contains(cursor.position().unwrap_or_default()) {
                    debug!("Cursor is within the Image content bounds");
                    return true;
                }
            }
        }
    }
    false
}

/// Do a horizontal split.
fn horizontal_split<'a, Message, Renderer>(
    split: &Split<'a, Message, Renderer>,
    renderer: &Renderer,
    limits: &Limits,
    space: &Node,
) -> Node
where
    Renderer: 'a + core::Renderer,
    Renderer::Theme: StyleSheet,
{
    if space.bounds().height
        < split.spacing + f32::from(split.min_size_first + split.min_size_second)
    {
        return Node::with_children(
            space.bounds().size(),
            vec![
                split.first.as_widget().layout(
                    renderer,
                    &limits.clone().shrink(Size::new(0.0, space.bounds().height)),
                ),
                Node::new(Size::new(space.bounds().height, split.spacing)),
                split.second.as_widget().layout(
                    renderer,
                    &limits.clone().shrink(Size::new(0.0, space.bounds().width)),
                ),
            ],
        );
    }

    let divider_position = split
        .divider_position
        .unwrap_or_else(|| (space.bounds().height / 2.0) as u16)
        .max((split.spacing / 2.0) as u16);
    let divider_position = (divider_position - (split.spacing / 2.0) as u16).clamp(
        split.min_size_first,
        space.bounds().height as u16 - split.min_size_second - split.spacing as u16,
    );

    let padding = Padding::from(split.padding as u16);
    let first_limits = limits
        .clone()
        .shrink(Size::new(
            0.0,
            space.bounds().height - f32::from(divider_position),
        ))
        .pad(padding);
    let mut first = split.first.as_widget().layout(renderer, &first_limits);
    first.move_to(Point::new(
        space.bounds().x + split.padding,
        space.bounds().y + split.padding,
    ));

    let mut divider = Node::new(Size::new(space.bounds().width, split.spacing));
    divider.move_to(Point::new(space.bounds().x, f32::from(divider_position)));

    let second_limits = limits
        .clone()
        .shrink(Size::new(0.0, f32::from(divider_position) + split.spacing))
        .pad(padding);
    let mut second = split.second.as_widget().layout(renderer, &second_limits);
    second.move_to(Point::new(
        space.bounds().x + split.padding,
        space.bounds().y + f32::from(divider_position) + split.spacing + split.padding,
    ));

    Node::with_children(space.bounds().size(), vec![first, divider, second])
}

/// Do a vertical split.
fn vertical_split<'a, Message, Renderer>(
    split: &Split<'a, Message, Renderer>,
    renderer: &Renderer,
    limits: &Limits,
    space: &Node,
) -> Node
where
    Renderer: 'a + core::Renderer,
    Renderer::Theme: StyleSheet,
{
    if space.bounds().width
        < split.spacing + f32::from(split.min_size_first + split.min_size_second)
    {
        return Node::with_children(
            space.bounds().size(),
            vec![
                split.first.as_widget().layout(
                    renderer,
                    &limits.clone().shrink(Size::new(space.bounds().width, 0.0)),
                ),
                Node::new(Size::new(split.spacing, space.bounds().height)),
                split.second.as_widget().layout(
                    renderer,
                    &limits.clone().shrink(Size::new(space.bounds().width, 0.0)),
                ),
            ],
        );
    }

    let divider_position = split
        .divider_position
        .unwrap_or_else(|| (space.bounds().width / 2.0) as u16)
        .max((split.spacing / 2.0) as u16);
    let divider_position = (divider_position - (split.spacing / 2.0) as u16).clamp(
        split.min_size_first,
        space.bounds().width as u16 - split.min_size_second - split.spacing as u16,
    );

    let padding = Padding::from(split.padding as u16);
    let first_limits = limits
        .clone()
        .shrink(Size::new(
            space.bounds().width - f32::from(divider_position),
            0.0,
        ))
        .pad(padding);
    let mut first = split.first.as_widget().layout(renderer, &first_limits);
    first.move_to(Point::new(
        space.bounds().x + split.padding,
        space.bounds().y + split.padding,
    ));

    let mut divider = Node::new(Size::new(split.spacing, space.bounds().height));
    divider.move_to(Point::new(f32::from(divider_position), space.bounds().y));

    let second_limits = limits
        .clone()
        .shrink(Size::new(f32::from(divider_position) + split.spacing, 0.0))
        .pad(padding);
    let mut second = split.second.as_widget().layout(renderer, &second_limits);
    second.move_to(Point::new(
        space.bounds().x + f32::from(divider_position) + split.spacing + split.padding,
        space.bounds().y + split.padding,
    ));

    Node::with_children(space.bounds().size(), vec![first, divider, second])
}

impl<'a, Message, Renderer> From<Split<'a, Message, Renderer>> for Element<'a, Message, Renderer>
where
    Message: 'a,
    Renderer: 'a + core::Renderer,
    Renderer::Theme: StyleSheet,
{
    fn from(split_pane: Split<'a, Message, Renderer>) -> Self {
        Element::new(split_pane)
    }
}

/// The state of a [`Split`].
#[derive(Clone, Debug, Default)]
pub struct SplitState {
    /// If the divider is dragged by the user.
    dragging: bool,
    last_click_time: Option<Instant>,
    panes_seleced: [bool; 2],
}

impl SplitState {
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
