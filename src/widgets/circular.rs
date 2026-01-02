//! Show a circular progress indicator using the canvas widget.
use iced_winit::core::{Color, Element, Length, Radians, mouse};
use iced_winit::core::Theme as WinitTheme;
use iced_wgpu::Renderer;
use iced_widget::canvas::{self, Canvas, Frame, Geometry, Path, Stroke};

use super::easing::{self, Easing};

use std::f32::consts::PI;
use std::time::Duration;

const MIN_ANGLE: Radians = Radians(PI / 8.0);
const WRAP_ANGLE: Radians = Radians(2.0 * PI - PI / 4.0);
const BASE_ROTATION_SPEED: u32 = u32::MAX / 80;

/// State for the circular spinner animation
#[derive(Debug, Clone, Copy)]
pub struct CircularState {
    animation: Animation,
}

impl Default for CircularState {
    fn default() -> Self {
        Self {
            animation: Animation::default(),
        }
    }
}

impl CircularState {
    /// Update the animation state based on elapsed time
    pub fn update(&mut self, now: std::time::Instant, cycle_duration: Duration, rotation_duration: Duration) {
        self.animation = self.animation.timed_transition(cycle_duration, rotation_duration, now);
    }
}

#[derive(Clone, Copy, Debug)]
enum Animation {
    Expanding {
        start: std::time::Instant,
        progress: f32,
        rotation: u32,
        last: std::time::Instant,
    },
    Contracting {
        start: std::time::Instant,
        progress: f32,
        rotation: u32,
        last: std::time::Instant,
    },
}

impl Default for Animation {
    fn default() -> Self {
        let now = std::time::Instant::now();
        Self::Expanding {
            start: now,
            progress: 0.0,
            rotation: 0,
            last: now,
        }
    }
}

impl Animation {
    fn next(&self, additional_rotation: u32, now: std::time::Instant) -> Self {
        match self {
            Self::Expanding { rotation, .. } => Self::Contracting {
                start: now,
                progress: 0.0,
                rotation: rotation.wrapping_add(additional_rotation),
                last: now,
            },
            Self::Contracting { rotation, .. } => Self::Expanding {
                start: now,
                progress: 0.0,
                rotation: rotation.wrapping_add(
                    BASE_ROTATION_SPEED.wrapping_add(
                        (f64::from(WRAP_ANGLE / (2.0 * Radians::PI)) * f64::MAX)
                            as u32,
                    ),
                ),
                last: now,
            },
        }
    }

    fn start(&self) -> std::time::Instant {
        match self {
            Self::Expanding { start, .. } | Self::Contracting { start, .. } => *start,
        }
    }

    fn last(&self) -> std::time::Instant {
        match self {
            Self::Expanding { last, .. } | Self::Contracting { last, .. } => *last,
        }
    }

    fn timed_transition(
        &self,
        cycle_duration: Duration,
        rotation_duration: Duration,
        now: std::time::Instant,
    ) -> Self {
        let elapsed = now.duration_since(self.start());
        let additional_rotation = ((now - self.last()).as_secs_f32()
            / rotation_duration.as_secs_f32()
            * (u32::MAX) as f32) as u32;

        if elapsed > cycle_duration {
            self.next(additional_rotation, now)
        } else {
            self.with_elapsed(cycle_duration, additional_rotation, elapsed, now)
        }
    }

    fn with_elapsed(
        &self,
        cycle_duration: Duration,
        additional_rotation: u32,
        elapsed: Duration,
        now: std::time::Instant,
    ) -> Self {
        let progress = elapsed.as_secs_f32() / cycle_duration.as_secs_f32();
        match self {
            Self::Expanding { start, rotation, .. } => Self::Expanding {
                start: *start,
                progress,
                rotation: rotation.wrapping_add(additional_rotation),
                last: now,
            },
            Self::Contracting { start, rotation, .. } => Self::Contracting {
                start: *start,
                progress,
                rotation: rotation.wrapping_add(additional_rotation),
                last: now,
            },
        }
    }

    fn rotation(&self) -> f32 {
        match self {
            Self::Expanding { rotation, .. } | Self::Contracting { rotation, .. } => {
                *rotation as f32 / u32::MAX as f32
            }
        }
    }

    fn progress(&self) -> f32 {
        match self {
            Self::Expanding { progress, .. } | Self::Contracting { progress, .. } => *progress,
        }
    }

    fn is_expanding(&self) -> bool {
        matches!(self, Self::Expanding { .. })
    }
}

/// Program for the circular spinner canvas
pub struct Circular<'a> {
    state: &'a CircularState,
    easing: &'a Easing,
    track_color: Color,
    bar_color: Color,
    bar_height: f32,
}

impl<'a> Circular<'a> {
    pub fn new(state: &'a CircularState) -> Self {
        Self {
            state,
            easing: &easing::EMPHASIZED,
            track_color: Color::from_rgba(1.0, 1.0, 1.0, 0.3),
            bar_color: Color::WHITE,
            bar_height: 4.0,
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
        let mut frame = Frame::new(renderer, bounds.size());

        let track_radius = frame.width() / 2.0 - self.bar_height;
        let track_path = Path::circle(frame.center(), track_radius);

        frame.stroke(
            &track_path,
            Stroke::default()
                .with_color(self.track_color)
                .with_width(self.bar_height),
        );

        let mut builder = canvas::path::Builder::new();
        let start = Radians(self.state.animation.rotation() * 2.0 * PI);
        let progress = self.state.animation.progress();

        if self.state.animation.is_expanding() {
            builder.arc(canvas::path::Arc {
                center: frame.center(),
                radius: track_radius,
                start_angle: start,
                end_angle: start + MIN_ANGLE + WRAP_ANGLE * self.easing.y_at_x(progress),
            });
        } else {
            builder.arc(canvas::path::Arc {
                center: frame.center(),
                radius: track_radius,
                start_angle: start + WRAP_ANGLE * self.easing.y_at_x(progress),
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

        vec![frame.into_geometry()]
    }
}

/// Create a circular spinner element
pub fn circular<'a, Message: 'a>(state: &'a CircularState) -> Element<'a, Message, WinitTheme, Renderer> {
    Canvas::new(Circular::new(state))
        .width(Length::Fixed(48.0))
        .height(Length::Fixed(48.0))
        .into()
}
