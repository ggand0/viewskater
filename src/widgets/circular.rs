//! Show a circular progress indicator using the canvas widget.
use iced_winit::core::{Color, Element, Length, Radians, mouse};
use iced_winit::core::Theme as WinitTheme;
use iced_wgpu::Renderer;
use iced_widget::canvas::{self, Canvas, Cache, Geometry, Path, Stroke};

use super::easing::{self, Easing};

use std::f32::consts::PI;
use std::time::{Duration, Instant};

const MIN_ANGLE: Radians = Radians(PI / 8.0);
const WRAP_ANGLE: Radians = Radians(2.0 * PI - PI / 4.0);

/// State for the circular spinner animation
/// Stores the start time and a cache that gets cleared on each update
#[derive(Debug)]
pub struct CircularState {
    start_time: Instant,
    cache: Cache<Renderer>,
}

impl Default for CircularState {
    fn default() -> Self {
        Self {
            start_time: Instant::now(),
            cache: Cache::new(),
        }
    }
}

impl Clone for CircularState {
    fn clone(&self) -> Self {
        Self {
            start_time: self.start_time,
            cache: Cache::new(), // Create new cache on clone
        }
    }
}

impl CircularState {
    /// Clear the canvas cache to force a redraw on next render
    pub fn update(&mut self, _now: Instant, _cycle_duration: Duration, _rotation_duration: Duration) {
        self.cache.clear();
    }

    /// Get animation values based on wall clock time
    fn get_animation(&self, cycle_duration: Duration, rotation_duration: Duration) -> (f32, f32, bool) {
        let now = Instant::now();
        let total_elapsed = now.duration_since(self.start_time).as_secs_f32();

        // Calculate rotation (continuous)
        let rotation = (total_elapsed / rotation_duration.as_secs_f32()).fract();

        // Calculate cycle progress (expanding then contracting)
        let cycle_secs = cycle_duration.as_secs_f32();
        let full_cycle = cycle_secs * 2.0; // expand + contract
        let cycle_pos = total_elapsed % full_cycle;

        let (progress, is_expanding) = if cycle_pos < cycle_secs {
            // Expanding phase
            (cycle_pos / cycle_secs, true)
        } else {
            // Contracting phase
            ((cycle_pos - cycle_secs) / cycle_secs, false)
        };

        (rotation, progress, is_expanding)
    }
}

/// Program for the circular spinner canvas
pub struct Circular<'a> {
    state: &'a CircularState,
    easing: &'a Easing,
    track_color: Color,
    bar_color: Color,
    bar_height: f32,
    cycle_duration: Duration,
    rotation_duration: Duration,
}

impl<'a> Circular<'a> {
    pub fn new(state: &'a CircularState) -> Self {
        Self {
            state,
            easing: &easing::EMPHASIZED,
            track_color: Color::from_rgba(1.0, 1.0, 1.0, 0.3),
            bar_color: Color::WHITE,
            bar_height: 4.0,
            cycle_duration: Duration::from_secs(1), // 1s expand, 1s contract
            rotation_duration: Duration::from_secs(4),
        }
    }
}

impl<Message> canvas::Program<Message, WinitTheme, Renderer> for Circular<'_> {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &WinitTheme,
        bounds: iced_winit::core::Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry<Renderer>> {
        // Use the cache - it will redraw when cleared by update()
        let geometry = self.state.cache.draw(renderer, bounds.size(), |frame| {
            let track_radius = frame.width() / 2.0 - self.bar_height;
            let track_path = Path::circle(frame.center(), track_radius);

            frame.stroke(
                &track_path,
                Stroke::default()
                    .with_color(self.track_color)
                    .with_width(self.bar_height),
            );

            // Get animation values from wall clock time
            let (rotation, progress, is_expanding) = self.state.get_animation(
                self.cycle_duration,
                self.rotation_duration,
            );

            let mut builder = canvas::path::Builder::new();
            let start = Radians(rotation * 2.0 * PI);
            let eased_progress = self.easing.y_at_x(progress);

            if is_expanding {
                builder.arc(canvas::path::Arc {
                    center: frame.center(),
                    radius: track_radius,
                    start_angle: start,
                    end_angle: start + MIN_ANGLE + WRAP_ANGLE * eased_progress,
                });
            } else {
                builder.arc(canvas::path::Arc {
                    center: frame.center(),
                    radius: track_radius,
                    start_angle: start + WRAP_ANGLE * eased_progress,
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
        });

        vec![geometry]
    }
}

/// Create a circular spinner element
pub fn circular<'a, Message: 'a>(state: &'a CircularState) -> Element<'a, Message, WinitTheme, Renderer> {
    Canvas::new(Circular::new(state))
        .width(Length::Fixed(48.0))
        .height(Length::Fixed(48.0))
        .into()
}
