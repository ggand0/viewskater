//! Zoom and pan on an image.

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
    core::{
        event::{self, Event},
        image,
        layout,
        mouse,
        renderer,
        widget::tree::{self, Tree},
        Clipboard, Element, Layout, Length, Pixels, Point, Rectangle, Shell, Size, Vector,
        Widget,
    },
};
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;
use std::time::{Duration, Instant};


/// A frame that displays an image with the ability to zoom in/out and pan.
#[allow(missing_debug_implementations)]
pub struct Viewer<Handle> {
    padding: f32,
    width: Length,
    height: Length,
    min_scale: f32,
    max_scale: f32,
    scale_step: f32,
    handle: Handle,
    handle_hash: u64,
}

impl<Handle: Hash> Viewer<Handle> {
    /// Creates a new [`Viewer`] with the given [`State`].
    pub fn new(handle: Handle) -> Self {
        let mut viewer = Viewer {
            padding: 0.0,
            width: Length::Shrink,
            height: Length::Shrink,
            min_scale: 0.25,
            max_scale: 10.0,
            scale_step: 0.10,
            handle,
            handle_hash: 0,
        };
        viewer.compute_and_store_handle_hash();
        viewer
    }

    // New method to compute and store hash
    fn compute_and_store_handle_hash(&mut self) {
        let mut hasher = DefaultHasher::new();
        self.handle.hash(&mut hasher);
        //self.handle.data().hash(&mut hasher);
        self.handle_hash = hasher.finish();
    }

    /// Sets the padding of the [`Viewer`].
    pub fn padding(mut self, padding: impl Into<Pixels>) -> Self {
        self.padding = padding.into().0;
        self
    }

    /// Sets the width of the [`Viewer`].
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    /// Sets the height of the [`Viewer`].
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    /// Sets the max scale applied to the image of the [`Viewer`].
    ///
    /// Default is `10.0`
    pub fn max_scale(mut self, max_scale: f32) -> Self {
        self.max_scale = max_scale;
        self
    }

    /// Sets the min scale applied to the image of the [`Viewer`].
    ///
    /// Default is `0.25`
    pub fn min_scale(mut self, min_scale: f32) -> Self {
        self.min_scale = min_scale;
        self
    }

    /// Sets the percentage the image of the [`Viewer`] will be scaled by
    /// when zoomed in / out.
    ///
    /// Default is `0.10`
    pub fn scale_step(mut self, scale_step: f32) -> Self {
        self.scale_step = scale_step;
        self
    }
}

impl<Message, Renderer, Handle> Widget<Message, Renderer> for Viewer<Handle>
where
    Renderer: image::Renderer<Handle = Handle>,
    Handle: Clone + Hash,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::new())
    }

    fn width(&self) -> Length {
        self.width
    }

    fn height(&self) -> Length {
        self.height
    }

    fn layout(
        &self,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let Size { width, height } = renderer.dimensions(&self.handle);

        let mut size = limits
            .width(self.width)
            .height(self.height)
            .resolve(Size::new(width as f32, height as f32));

        let expansion_size = if height > width {
            self.width
        } else {
            self.height
        };

        // Only calculate viewport sizes if the images are constrained to a limited space.
        // If they are Fill|Portion let them expand within their alotted space.
        match expansion_size {
            Length::Shrink | Length::Fixed(_) => {
                let aspect_ratio = width as f32 / height as f32;
                let viewport_aspect_ratio = size.width / size.height;
                if viewport_aspect_ratio > aspect_ratio {
                    size.width = width as f32 * size.height / height as f32;
                } else {
                    size.height = height as f32 * size.width / width as f32;
                }
            }
            Length::Fill | Length::FillPortion(_) => {}
        }

        layout::Node::new(size)
    }

    fn on_event(
        &mut self,
        tree: &mut Tree,
        event: Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        _shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) -> event::Status {
        let bounds = layout.bounds();


        // Detect if the handle has changed and reset zoom state
        // TODO: Avoid calling this block every time
        let state = tree.state.downcast_mut::<State>();
        if state.handle_hash_changed(&self.handle_hash) {
            // Handle has changed, perform necessary actions
            // to reset state
            state.reset();
    
            // Update the previous handle hash
            state.update_previous_handle_hash(self.handle_hash);
        }


        match event {
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let Some(_cursor_position) = cursor.position() else {
                    return event::Status::Ignored;
                };

                if let Some(cursor_position) = cursor.position_over(bounds) {
                    match delta {
                        mouse::ScrollDelta::Lines { y, .. }
                        | mouse::ScrollDelta::Pixels { y, .. } => {
                            let state = tree.state.downcast_mut::<State>();
                            let previous_scale = state.scale;

                            if y < 0.0 && previous_scale > self.min_scale
                                || y > 0.0 && previous_scale < self.max_scale
                            {
                                state.scale = (if y > 0.0 {
                                    state.scale * (1.0 + self.scale_step)
                                } else {
                                    state.scale / (1.0 + self.scale_step)
                                })
                                .clamp(self.min_scale, self.max_scale);

                                let image_size = image_size(
                                    renderer,
                                    &self.handle,
                                    state,
                                    bounds.size(),
                                );

                                let factor = state.scale / previous_scale - 1.0;

                                let cursor_to_center =
                                    cursor_position - bounds.center();

                                let adjustment = cursor_to_center * factor
                                    + state.current_offset * factor;

                                state.current_offset = Vector::new(
                                    if image_size.width > bounds.width {
                                        state.current_offset.x + adjustment.x
                                    } else {
                                        0.0
                                    },
                                    if image_size.height > bounds.height {
                                        state.current_offset.y + adjustment.y
                                    } else {
                                        0.0
                                    },
                                );
                            }
                        }
                    }
                

                    event::Status::Captured
                } else {
                    event::Status::Ignored
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(_cursor_position) = cursor.position_over(bounds) {
                    let Some(cursor_position) = cursor.position() else {
                        return event::Status::Ignored;
                    };

                    //let state = tree.state.downcast_mut::<State>();
                    state.cursor_grabbed_at = Some(cursor_position);
                    state.starting_offset = state.current_offset;

                    // double click to reset zoom
                    // Save the current time
                    if let Some(last_click_time) = state.last_click_time {
                        let elapsed = last_click_time.elapsed();
                        if elapsed < Duration::from_millis(500) {
                            // Double-click detected
                            state.last_click_time = None;
    
                            let double_click_position = cursor.position();
                            if let Some(_position) = double_click_position {
                                // Reset the state
                                /*state.scale = 1.0;
                                state.starting_offset = Vector::default();
                                state.current_offset = Vector::default();
                                state.cursor_grabbed_at = None;*/
                                state.reset();
                            }
                        } else {
                            // Reset the timer for a new potential double-click
                            state.last_click_time = Some(Instant::now());
                        }
                    } else {
                        state.last_click_time = Some(Instant::now());
                    }

                    event::Status::Captured
                } else {
                    event::Status::Ignored
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if let Some(_cursor_position) = cursor.position_over(bounds) {
                    let state = tree.state.downcast_mut::<State>();

                    if state.cursor_grabbed_at.is_some() {
                        state.cursor_grabbed_at = None;

                        event::Status::Captured
                    } else {
                        event::Status::Ignored
                    }
                } else {
                    event::Status::Ignored
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if bounds.contains(position) {
                    let state = tree.state.downcast_mut::<State>();

                    if let Some(origin) = state.cursor_grabbed_at {
                        let image_size = image_size(
                            renderer,
                            &self.handle,
                            state,
                            bounds.size(),
                        );

                        let hidden_width = (image_size.width - bounds.width / 2.0)
                            .max(0.0)
                            .round();

                        let hidden_height = (image_size.height
                            - bounds.height / 2.0)
                            .max(0.0)
                            .round();

                        let delta = position - origin;

                        let x = if bounds.width < image_size.width {
                            (state.starting_offset.x - delta.x)
                                .clamp(-hidden_width, hidden_width)
                        } else {
                            0.0
                        };

                        let y = if bounds.height < image_size.height {
                            (state.starting_offset.y - delta.y)
                                .clamp(-hidden_height, hidden_height)
                        } else {
                            0.0
                        };

                        state.current_offset = Vector::new(x, y);

                        event::Status::Captured
                    } else {
                        event::Status::Ignored
                    }
                } else {
                    event::Status::Ignored
                }
            }
            _ => event::Status::Ignored,
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        let is_mouse_over = cursor.is_over(bounds);

        if state.is_cursor_grabbed() {
            mouse::Interaction::Grabbing
        } else if is_mouse_over {
            mouse::Interaction::Grab
        } else {
            mouse::Interaction::Idle
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Renderer::Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();

        let image_size =
            image_size(renderer, &self.handle, state, bounds.size());

        // Adjust bounds size and position for padding
        let padding = 1.0; // Adjust the padding value as needed
        let padded_bounds = Rectangle {
            x: bounds.x + padding,
            y: bounds.y + padding,
            width: image_size.width - 2.0 * padding,
            height: image_size.height - 2.0 * padding,
        };
        let _padded_image_size = image_size - Size::new(2.0 * padding, 2.0 * padding);
        //println!("image_size: {:?}, padded_image_size: {:?}", image_size, padded_image_size);

        let translation = {
            /*let image_top_left = Vector::new(
                bounds.width / 2.0 - image_size.width / 2.0,
                bounds.height / 2.0 - image_size.height / 2.0,
            );
            image_top_left - state.offset(bounds, image_size)
            */
            let image_top_left = Vector::new(
                bounds.width / 2.0 - image_size.width / 2.0,
                bounds.height / 2.0 - image_size.height / 2.0,
                //bounds.width / 2.0 - padded_bounds.width / 2.0,
                //bounds.height / 2.0 - padded_bounds.height / 2.0,
            ) + Vector::new(padding, padding);
            image_top_left - state.offset(bounds, image_size)
        };

        renderer.with_layer(bounds, |renderer| {
            renderer.with_translation(translation, |renderer| {
                image::Renderer::draw(
                    renderer,
                    self.handle.clone(),
                    /*Rectangle {
                        x: bounds.x,
                        y: bounds.y,
                        ..Rectangle::with_size(image_size)
                    },*/
                    Rectangle {
                        x: bounds.x,
                        y: bounds.y,
                        width: padded_bounds.width,
                        height: padded_bounds.height,
                    },
                )
            });
        });
    }
}

/// The local state of a [`Viewer`].
#[derive(Debug, Clone, Copy)]
pub struct State {
    scale: f32,
    starting_offset: Vector,
    current_offset: Vector,
    cursor_grabbed_at: Option<Point>,
    previous_handle_hash: u64,
    last_click_time: Option<Instant>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            scale: 1.0,
            starting_offset: Vector::default(),
            current_offset: Vector::default(),
            cursor_grabbed_at: None,
            previous_handle_hash: 0,
            last_click_time: None,
        }
    }
}

impl State {
    /// Creates a new [`State`].
    pub fn new() -> Self {
        State::default()
    }

    fn reset(&mut self) {
        // Reset the state
        self.scale = 1.0;
        self.starting_offset = Vector::default();
        self.current_offset = Vector::default();
        self.cursor_grabbed_at = None;
    }

    fn update_previous_handle_hash(&mut self, handle_hash: u64) {
        self.previous_handle_hash = handle_hash;
    }

    fn handle_hash_changed(&self, handle_hash: &u64) -> bool {
        &self.previous_handle_hash != handle_hash
    }

    /// Returns the current offset of the [`State`], given the bounds
    /// of the [`Viewer`] and its image.
    fn offset(&self, bounds: Rectangle, image_size: Size) -> Vector {
        let hidden_width =
            (image_size.width - bounds.width / 2.0).max(0.0).round();

        let hidden_height =
            (image_size.height - bounds.height / 2.0).max(0.0).round();

        Vector::new(
            self.current_offset.x.clamp(-hidden_width, hidden_width),
            self.current_offset.y.clamp(-hidden_height, hidden_height),
        )
    }
    
    /// Returns if the cursor is currently grabbed by the [`Viewer`].
    pub fn is_cursor_grabbed(&self) -> bool {
        self.cursor_grabbed_at.is_some()
    }
}

impl<'a, Message, Renderer, Handle> From<Viewer<Handle>>
    for Element<'a, Message, Renderer>
where
    Renderer: 'a + image::Renderer<Handle = Handle>,
    Message: 'a,
    Handle: Clone + Hash + 'a,
{
    fn from(viewer: Viewer<Handle>) -> Element<'a, Message, Renderer> {
        Element::new(viewer)
    }
}

/// Returns the bounds of the underlying image, given the bounds of
/// the [`Viewer`]. Scaling will be applied and original aspect ratio
/// will be respected.
pub fn image_size<Renderer>(
    renderer: &Renderer,
    handle: &<Renderer as image::Renderer>::Handle,
    state: &State,
    bounds: Size,
) -> Size
where
    Renderer: image::Renderer,
{
    let Size { width, height } = renderer.dimensions(handle);

    let (width, height) = {
        let dimensions = (width as f32, height as f32);

        let width_ratio = bounds.width / dimensions.0;
        let height_ratio = bounds.height / dimensions.1;

        let ratio = width_ratio.min(height_ratio);
        let scale = state.scale;

        if ratio < 1.0 {
            (dimensions.0 * ratio * scale, dimensions.1 * ratio * scale)
        } else {
            (dimensions.0 * scale, dimensions.1 * scale)
        }
    };

    Size::new(width, height)
}