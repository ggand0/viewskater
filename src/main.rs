
use iced_wgpu::graphics::Viewport;
use iced_wgpu::{wgpu, Engine, Renderer};
use iced_winit::{conversion, Proxy};
use iced_winit::core::mouse;
use iced_winit::core::renderer;
use iced_winit::core::{Color, Font, Pixels, Size, Theme};
use iced_winit::futures;
use iced_winit::runtime::program;
use iced_winit::runtime::Debug;
use iced_winit::winit;
use iced_wgpu::wgpu::util::DeviceExt;
use iced_winit::winit::event::{ElementState};
use iced_winit::winit::keyboard::{KeyCode, PhysicalKey};

#[allow(unused_imports)]
use log::{Level, debug, info, warn, error};

mod cache;
use crate::cache::img_cache::LoadOperation;
mod navigation;
use crate::navigation::{move_right_all, move_left_all, update_pos, load_remaining_images};
mod file_io;
mod menu;
use menu::PaneLayout;
mod widgets;
mod pane;
use crate::pane::Pane;
mod ui_builder;
mod loading_status;
mod loading;
mod config;
use crate::widgets::shader::scene::Scene;
mod app;
use crate::app::{Message, DataViewer};

use iced_winit::Clipboard;
use iced_runtime::Action;
use iced_runtime::task::into_stream;
use iced_winit::winit::event_loop::{ActiveEventLoop, EventLoopProxy};

use winit::{
    event::WindowEvent,
    event_loop::{ControlFlow, EventLoop},
    keyboard::ModifiersState,
};

use std::sync::Arc;

pub fn main() -> Result<(), winit::error::EventLoopError> {
    // Adapted event loop logic from benediktweihs’ fork of Iced:
    // https://github.com/benediktweihs/iced (checked on 2025-02-17)

    // Initialize tracing for debugging
    tracing_subscriber::fmt::init();

    // Initialize winit
    let event_loop = EventLoop::<Action<Message>>::with_user_event()
        .build()
        .unwrap();
    let proxy: EventLoopProxy<Action<Message>> = event_loop.create_proxy();

    #[allow(clippy::large_enum_variant)]
    enum Runner {
        Loading(EventLoopProxy<Action<Message>>),
        Ready {
            window: Arc<winit::window::Window>,
            device: Arc<wgpu::Device>,
            queue: Arc<wgpu::Queue>,
            surface: wgpu::Surface<'static>,
            format: wgpu::TextureFormat,
            engine: Engine,
            renderer: Renderer,
            //scene: Scene,
            state: program::State<DataViewer>,
            cursor_position: Option<winit::dpi::PhysicalPosition<f64>>,
            clipboard: Clipboard,
            runtime: iced_futures::Runtime<
                iced_futures::backend::native::tokio::Executor,
                    Proxy<Message>,
                    Action<Message>,
                >,
            viewport: Viewport,
            modifiers: ModifiersState,
            resized: bool,
            debug: Debug,
        },
    }

    impl winit::application::ApplicationHandler<Action<Message>> for Runner {
        fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
            if let Self::Loading(proxy) = self {
                println!("resumed()...");
                let window = Arc::new(
                    event_loop
                        .create_window(
                            winit::window::WindowAttributes::default(),
                        )
                        .expect("Create window"),
                );

                let physical_size = window.inner_size();
                let viewport = Viewport::with_physical_size(
                    Size::new(physical_size.width, physical_size.height),
                    window.scale_factor(),
                );
                let clipboard = Clipboard::connect(window.clone());
                let backend = wgpu::util::backend_bits_from_env().unwrap_or_default();

                let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
                    backends: backend,
                    ..Default::default()
                });
                let surface = instance
                    .create_surface(window.clone())
                    .expect("Create window surface");

                let (format, adapter, device, queue) =
                    futures::futures::executor::block_on(async {
                        let adapter =
                            wgpu::util::initialize_adapter_from_env_or_default(
                                &instance,
                                Some(&surface),
                            )
                            .await
                            .expect("Create adapter");

                            let adapter_features = adapter.features();

                            let capabilities = surface.get_capabilities(&adapter);
                            
                            let (device, queue) = adapter
                                .request_device(
                                    &wgpu::DeviceDescriptor {
                                        label: None,
                                        required_features: adapter_features & wgpu::Features::default(),
                                        required_limits: wgpu::Limits::default(),
                                    },
                                    None,
                                )
                                .await
                                .expect("Request device");
                            
                            (
                                capabilities
                                    .formats
                                    .iter()
                                    .copied()
                                    .find(wgpu::TextureFormat::is_srgb)
                                    .or_else(|| {
                                        capabilities.formats.first().copied()
                                    })
                                    .expect("Get preferred format"),
                                adapter,
                                device,
                                queue,
                            )
                    });

                surface.configure(
                    &device,
                    &wgpu::SurfaceConfiguration {
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                        format,
                        width: physical_size.width,
                        height: physical_size.height,
                        present_mode: wgpu::PresentMode::AutoVsync,
                        alpha_mode: wgpu::CompositeAlphaMode::Auto,
                        view_formats: vec![],
                        desired_maximum_frame_latency: 2,
                    },
                );


                // Create shared Arc instances of device and queue
                let device = Arc::new(device);
                let queue = Arc::new(queue);

                // Pass a cloned Arc reference to DataViewer
                let shader_widget = DataViewer::new(Arc::clone(&device), Arc::clone(&queue));


                // Initialize iced
                let mut debug = Debug::new();
                let engine = Engine::new(
                    &adapter, &device, &queue, format, None);
                let mut renderer = Renderer::new(
                    &device, &engine, Font::default(), Pixels::from(16));

                let state = program::State::new(
                    shader_widget,
                    viewport.logical_size(),
                    &mut renderer,
                    &mut debug,
                );

                // You should change this if you want to render continuously
                //event_loop.set_control_flow(ControlFlow::Wait);
                event_loop.set_control_flow(ControlFlow::Poll); // Forces continuous updates

                let (p, worker) = iced_winit::Proxy::new(proxy.clone());
                let Ok(executor) = iced_futures::backend::native::tokio::Executor::new() else {
                    panic!("could not create runtime")
                };
                executor.spawn(worker);
                let mut runtime = iced_futures::Runtime::new(executor, p);


                *self = Self::Ready {
                    window,
                    device: Arc::clone(&device),
                    queue: Arc::clone(&queue),
                    surface,
                    format,
                    engine,
                    renderer,
                    //scene,
                    state,
                    cursor_position: None,
                    modifiers: ModifiersState::default(),
                    clipboard,
                    runtime,
                    viewport,
                    resized: false,
                    debug,
                };
            }
        }

        fn window_event(
            &mut self,
            event_loop: &winit::event_loop::ActiveEventLoop,
            _window_id: winit::window::WindowId,
            event: WindowEvent,
        ) {
            let Self::Ready {
                window,
                device,
                queue,
                surface,
                format,
                engine,
                renderer,
                //scene,
                state,
                viewport,
                cursor_position,
                modifiers,
                clipboard,
                runtime,
                resized,
                debug,
            } = self
            else {
                return;
            };

            match event {
                WindowEvent::Focused(true) => {
                    // Handle window focus gain
                    event_loop.set_control_flow(ControlFlow::Poll);
                }
                WindowEvent::Focused(false) => {
                    event_loop.set_control_flow(ControlFlow::Wait);
                }
                WindowEvent::RedrawRequested => {
                    // Update window title dynamically based on the current image
                    let new_title = state.program().title();
                    window.set_title(&new_title);

                    if *resized {
                        let size = window.inner_size();

                        *viewport = Viewport::with_physical_size(
                            Size::new(size.width, size.height),
                            window.scale_factor(),
                        );

                        surface.configure(
                            device,
                            &wgpu::SurfaceConfiguration {
                                format: *format,
                                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                                width: size.width,
                                height: size.height,
                                present_mode: wgpu::PresentMode::AutoVsync,
                                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                                view_formats: vec![],
                                desired_maximum_frame_latency: 2,
                            },
                        );

                        *resized = false;
                    }

                    match surface.get_current_texture() {
                        Ok(frame) => {
                            // v0: render with wgpu
                            /*let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
                            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                label: Some("Render Encoder"),
                            });

                            // Call scene.draw with the encoder and the texture view
                            scene.draw(&mut encoder, &view);

                            // Submit the commands to the queue
                            queue.submit(Some(encoder.finish()));
                            frame.present();*/

                            // v1: render with iced's shader widget
                            let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
                            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                label: Some("Render Encoder"),
                            });

                            // Render the iced program
                            renderer.present(
                                engine,
                                device,
                                queue,
                                &mut encoder,
                                //None,
                                Some(iced_core::Color { r: 0.1, g: 0.1, b: 0.1, a: 0.5 }), // Force black background for debugging
                                frame.texture.format(),
                                &view,
                                viewport,
                                &debug.overlay(),
                            );


                            // Submit the commands to the queue
                            engine.submit(queue, encoder);
                            frame.present();

                            // Update the mouse cursor
                            window.set_cursor(
                                iced_winit::conversion::mouse_interaction(
                                    state.mouse_interaction(),
                                ),
                            );
                        }
                        Err(error) => match error {
                            wgpu::SurfaceError::OutOfMemory => {
                                panic!(
                                    "Swapchain error: {error}. \
                                        Rendering cannot continue."
                                );
                            }
                            _ => {
                                // Retry rendering on the next frame
                                window.request_redraw();
                            }
                        },
                    }
                }
                WindowEvent::Resized(size) => {
                    *resized = true;
                }
                WindowEvent::CloseRequested => {
                    event_loop.exit();
                }
                WindowEvent::CursorMoved { position, .. } => {
                    *cursor_position = Some(position);
                }
                /*
                */
                WindowEvent::KeyboardInput { ref event, .. } => {
                    /*if let Some(key_code) = event.physical_key.as_ref() {
                        let modifiers = event.modifiers;
                        state.queue_message
                        (Message::KeyPressed(*key_code, modifiers));
                    }*/
                }
                WindowEvent::ModifiersChanged(new_modifiers) => {
                    //debug!("ModifiersChanged event received: {:?}", new_modifiers);
                    *modifiers = new_modifiers.state(); // ✅ Now updating `modifiers`
                }
                _ => {}
            }

            // Map window event to iced event
            if let Some(event) = iced_winit::conversion::window_event(
                event,
                window.scale_factor(),
                *modifiers,
            ) {
                match &event {
                    iced_core::event::Event::Mouse(_) | // Filters out mouse events
                    iced_core::event::Event::Touch(_) => {} // Filters out touch events too
                    _ => {
                        ////debug!("Converted to Iced event: {:?}, modifiers: {:?}", event, modifiers);
                        // Manually trigger your app’s message handling
                        state.queue_message(Message::Event(
                            event.clone()));
                    }
                }
                state.queue_event(event);
            }

            // If there are events pending
            if !state.is_queue_empty() {
                // We update iced
                let (_, task) = state.update(
                    viewport.logical_size(),
                    cursor_position
                        .map(|p| {
                            conversion::cursor_position(
                                p,
                                viewport.scale_factor(),
                            )
                        })
                        .map(mouse::Cursor::Available)
                        .unwrap_or(mouse::Cursor::Unavailable),
                    renderer,
                    &Theme::Dark,
                    &renderer::Style {
                        text_color: Color::WHITE,
                    },
                    clipboard,
                    debug,
                );
                debug!(
                    "state.update() returned task: {}",
                    if task.is_some() { "Some(Task<Message>)" } else { "None" }
                );
                


                let _ = 'runtime_call: {
                    debug!("Executing Task::perform for"); // This will at least log that a task is picked up.
                    let Some(t) = task else {
                        debug!("No task to execute");
                        break 'runtime_call 1;
                    };
                    let Some(stream) = into_stream(t) else {
                        debug!("Task could not be converted into a stream");
                        break 'runtime_call 1;
                    };

                    runtime.run(stream);
                    debug!("Task completed execution.");
                    0
                };

                // and request a redraw
                window.request_redraw();
            }
            
        }

        fn user_event(&mut self, event_loop: &ActiveEventLoop, event: Action<Message>) {
            /*let Self::Ready {
                ref mut renderer,
                state,
                viewport,
                ref mut debug, ..
            } = self
            else {
                return;
            };*/
            let Self::Ready {
                window,
                device,
                queue,
                surface,
                format,
                engine,
                renderer,
                //scene,
                state,
                viewport,
                cursor_position,
                modifiers,
                clipboard,
                runtime,
                resized,
                debug,
            } = self
            else {
                return;
            };

            debug!("user_event() received: {:?}", event);

            match event {
                Action::Widget(w) => {
                    //debug!("Processing widget event");
                    state.operate(
                        renderer,
                        std::iter::once(w),
                        Size::new(viewport.physical_size().width as f32, viewport.physical_size().height as f32),
                        debug,
                    );
                }
                Action::Output(message) => {
                    debug!("Forwarding message to update(): {:?}", message);
                    state.queue_message(message); // Ensures the message gets triggered in the next `update()`
                }
                _ => {}
            }
        }
        
    }

    let mut runner = Runner::Loading(proxy);
    event_loop.run_app(&mut runner)
}
