//! Circular loading spinner widget that animates automatically.
//!
//! Implements Widget trait directly (not canvas::Program) so it can receive
//! RedrawRequested events in on_event and clear cache for animation.

use iced_winit::core::layout;
use iced_winit::core::mouse;
use iced_winit::core::renderer;
use iced_winit::core::widget::tree::{self, Tree};
use iced_winit::core::{
    Clipboard, Color, Element, Event, Layout, Length, Radians, Rectangle,
    Shell, Size, Vector, Widget,
};
use iced_winit::core::Theme as WinitTheme;
use iced_wgpu::Renderer;
use iced_widget::canvas::{self, Path, Stroke};

use super::easing::{self, Easing};

use std::f32::consts::PI;
use std::time::{Duration, Instant};

const MIN_ANGLE: Radians = Radians(PI / 8.0);
const WRAP_ANGLE: Radians = Radians(2.0 * PI - PI / 4.0);

/// Internal state for the Circular widget
struct State {
    start_time: Instant,
}

impl Default for State {
    fn default() -> Self {
        Self {
            start_time: Instant::now(),
        }
    }
}

/// The circular spinner widget
pub struct Circular<'a> {
    size: f32,
    bar_height: f32,
    easing: &'a Easing,
    cycle_duration: Duration,
    rotation_duration: Duration,
    track_color: Color,
    bar_color: Color,
}

impl<'a> Circular<'a> {
    pub fn new() -> Self {
        Self {
            size: 48.0,
            bar_height: 4.0,
            easing: &easing::EMPHASIZED,
            cycle_duration: Duration::from_secs(1),
            rotation_duration: Duration::from_secs(2),
            track_color: Color::from_rgba(1.0, 1.0, 1.0, 0.3),
            bar_color: Color::WHITE,
        }
    }
}

impl Default for Circular<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, Message> Widget<Message, WinitTheme, Renderer> for Circular<'a>
where
    Message: 'a + Clone,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size {
            width: Length::Fixed(self.size),
            height: Length::Fixed(self.size),
        }
    }

    fn layout(
        &self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::atomic(limits, self.size, self.size)
    }

    fn on_event(
        &mut self,
        _tree: &mut Tree,
        _event: Event,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        _shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) -> iced_winit::core::event::Status {
        // Animation is driven by main loop's request_redraw when is_any_pane_loading() is true
        iced_winit::core::event::Status::Ignored
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        _theme: &WinitTheme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        use iced_winit::core::Renderer as _;
        use iced_graphics::geometry::Renderer as _;

        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        let now = Instant::now();

        // Compute animation from wall clock time
        let elapsed = now.duration_since(state.start_time);
        let cycle_duration = self.cycle_duration.as_secs_f32();
        let rotation_duration = self.rotation_duration.as_secs_f32();

        // Total elapsed in cycles
        let total_cycles = elapsed.as_secs_f32() / cycle_duration;
        let cycle_index = total_cycles.floor() as u32;
        let progress_in_cycle = total_cycles.fract();

        // Alternate between expanding (even) and contracting (odd)
        let is_expanding = cycle_index % 2 == 0;
        let progress = progress_in_cycle;

        // Compute rotation
        let rotation = (elapsed.as_secs_f32() / rotation_duration).fract();

        // Create frame and draw
        let mut frame = canvas::Frame::new(renderer, bounds.size());

        let track_radius = frame.width() / 2.0 - self.bar_height;
        let track_path = Path::circle(frame.center(), track_radius);

        frame.stroke(
            &track_path,
            Stroke::default()
                .with_color(self.track_color)
                .with_width(self.bar_height),
        );

        let mut builder = canvas::path::Builder::new();
        let start = Radians(rotation * 2.0 * PI);

        if is_expanding {
            builder.arc(canvas::path::Arc {
                center: frame.center(),
                radius: track_radius,
                start_angle: start,
                end_angle: start + MIN_ANGLE + WRAP_ANGLE * self.easing.y_at_x(progress),
            });
        } else {
            // Contracting: the start moves forward while end stays
            let wrap_progress = WRAP_ANGLE * self.easing.y_at_x(progress);
            builder.arc(canvas::path::Arc {
                center: frame.center(),
                radius: track_radius,
                start_angle: start + wrap_progress,
                end_angle: start + MIN_ANGLE + WRAP_ANGLE,
            });
        }

        let bar_path = builder.build();

        frame.stroke(
            &bar_path,
            Stroke::default()
                .with_color(self.bar_color)
                .with_width(self.bar_height),
        );

        let geometry = frame.into_geometry();

        renderer.with_translation(Vector::new(bounds.x, bounds.y), |renderer| {
            renderer.draw_geometry(geometry);
        });
    }
}

impl<'a, Message> From<Circular<'a>> for Element<'a, Message, WinitTheme, Renderer>
where
    Message: Clone + 'a,
{
    fn from(circular: Circular<'a>) -> Self {
        Self::new(circular)
    }
}

/// Create a circular spinner element
pub fn circular<'a, Message: Clone + 'a>() -> Element<'a, Message, WinitTheme, Renderer> {
    Circular::new().into()
}
