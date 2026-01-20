//! Render helpers for animation and spinner support.
//!
//! This module provides helper functions for rendering frames, particularly
//! for animated widgets like the loading spinner that need to render directly
//! in response to UserEvents (bypassing the normal WindowEvent flow).
//!
//! Background: iced separates Events (go to widgets via on_event) from Messages
//! (go to app via update). SpinnerTick fires as a Message, so widgets never see
//! it. We render directly here so the Circular widget's draw() gets called.

use std::rc::Rc;
use std::sync::{Arc, Mutex};

use iced_wgpu::graphics::Viewport;
use iced_wgpu::{wgpu, Engine, Renderer};
use iced_winit::runtime::Debug;

/// Render a frame immediately for spinner animation.
///
/// This is used in the UserEvent handler when `is_any_pane_loading()` is true.
/// It bypasses the normal WindowEvent::RedrawRequested flow because SpinnerTick
/// fires as a Message, not an Event, and widgets only receive Events in on_event().
///
/// Returns true if the frame was rendered successfully.
pub fn render_spinner_frame(
    surface: &wgpu::Surface<'static>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    engine: &Arc<Mutex<Engine>>,
    renderer: &Rc<Mutex<Renderer>>,
    viewport: &Viewport,
    debug_tool: &Debug,
) -> bool {
    match surface.get_current_texture() {
        Ok(frame) => {
            let view = frame
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Spinner Render Encoder"),
                });

            {
                let mut engine_guard = engine.lock().unwrap();
                let mut renderer_guard = renderer.lock().unwrap();
                renderer_guard.present(
                    &mut engine_guard,
                    device,
                    queue,
                    &mut encoder,
                    None,
                    frame.texture.format(),
                    &view,
                    viewport,
                    &debug_tool.overlay(),
                );
                engine_guard.submit(queue, encoder);
            }
            frame.present();
            true
        }
        Err(_) => false,
    }
}
