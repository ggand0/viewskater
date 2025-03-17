#![windows_subsystem = "windows"]

mod cache;
mod navigation_keyboard;
mod navigation_slider;
mod file_io;
mod menu;
mod widgets;
mod pane;
mod ui;
mod loading_status;
mod loading_handler;
mod config;
mod app;
mod utils;

#[allow(unused_imports)]
use log::{Level, debug, info, warn, error};

use std::task::Wake;
use std::task::Waker;
use std::sync::Arc;
use std::borrow::Cow;
use std::time::Instant;
use std::sync::Mutex;
use std::time::Duration;
use once_cell::sync::Lazy;

use winit::{
    event::WindowEvent,
    event_loop::{ControlFlow, EventLoop},
    keyboard::ModifiersState,
};

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
use iced_winit::winit::event::ElementState;
use iced_winit::Clipboard;
use iced_runtime::Action;
use iced_runtime::task::into_stream;
use iced_winit::winit::event_loop::EventLoopProxy;
use iced_wgpu::graphics::text::font_system;
use iced_winit::futures::futures::task;
use iced_winit::core::window;
use iced_futures::futures::channel::oneshot;

use crate::utils::timing::TimingStats;
use crate::app::{Message, DataViewer};
use crate::widgets::shader::scene::Scene;
use crate::config::CONFIG;
// Import the correct channel types
use std::sync::mpsc::{self as std_mpsc, Receiver as StdReceiver, Sender as StdSender};

static FRAME_TIMES: Lazy<Mutex<Vec<Instant>>> = Lazy::new(|| {
    Mutex::new(Vec::with_capacity(120))
});
static CURRENT_FPS: Lazy<Mutex<f32>> = Lazy::new(|| {
    Mutex::new(0.0)
});
static _STATE_UPDATE_STATS: Lazy<Mutex<TimingStats>> = Lazy::new(|| {
    Mutex::new(TimingStats::new("State Update"))
});
static _WINDOW_EVENT_STATS: Lazy<Mutex<TimingStats>> = Lazy::new(|| {
    Mutex::new(TimingStats::new("Window Event"))
});

static ICON: &[u8] = include_bytes!("../assets/icon_48.png");

// Add these to track the phase relationship
static LAST_RENDER_TIME: Lazy<Mutex<Instant>> = Lazy::new(|| {
    Mutex::new(Instant::now())
});
static LAST_ASYNC_DELIVERY_TIME: Lazy<Mutex<Instant>> = Lazy::new(|| {
    Mutex::new(Instant::now())
});

fn load_icon() -> Option<winit::window::Icon> {
    let image = image::load_from_memory(ICON).ok()?.into_rgba8();
    let (width, height) = image.dimensions();
    winit::window::Icon::from_rgba(image.into_raw(), width, height).ok()
}

fn register_font_manually(font_data: &'static [u8]) {
    use std::sync::RwLockWriteGuard;

    // Get a mutable reference to the font system
    let font_system = font_system();
    let mut font_system_guard: RwLockWriteGuard<_> = font_system
        .write()
        .expect("Failed to acquire font system lock");

    // Load the font into the global font system
    font_system_guard.load_font(Cow::Borrowed(font_data));
}

#[allow(dead_code)]
enum Control {
    ChangeFlow(winit::event_loop::ControlFlow),
    CreateWindow {
        id: window::Id,
        settings: window::Settings,
        title: String,
        monitor: Option<winit::monitor::MonitorHandle>,
        on_open: oneshot::Sender<()>,
    },
    Exit,
}

#[allow(dead_code)]
enum Event<T: 'static> {
    EventLoopAwakened(winit::event::Event<T>),
    WindowCreated {
        id: window::Id,
        window: winit::window::Window,
        exit_on_close_request: bool,
        make_visible: bool,
        on_open: oneshot::Sender<()>,
    },
}

pub fn main() -> Result<(), winit::error::EventLoopError> {
    // Set up panic hook to log to a file
    let app_name = "viewskater";
    let shared_log_buffer = file_io::setup_logger(app_name);
    file_io::setup_panic_hook(app_name, shared_log_buffer);

    // Initialize tracing for debugging
    //tracing_subscriber::fmt::init();

    // Initialize winit
    let event_loop = EventLoop::<Action<Message>>::with_user_event()
        .build()
        .unwrap();
    let proxy: EventLoopProxy<Action<Message>> = event_loop.create_proxy();

    // Create channels for event and control communication
    // Use std::sync::mpsc explicitly
    let (event_sender, _event_receiver): (StdSender<Event<Action<Message>>>, StdReceiver<Event<Action<Message>>>) = 
        std_mpsc::channel();
    let (_control_sender, control_receiver): (StdSender<Control>, StdReceiver<Control>) = 
        std_mpsc::channel();

    #[allow(clippy::large_enum_variant)]
    enum Runner {
        Loading {
            proxy: EventLoopProxy<Action<Message>>,
            event_sender: StdSender<Event<Action<Message>>>,
            control_receiver: StdReceiver<Control>,
        },
        Ready {
            window: Arc<winit::window::Window>,
            device: Arc<wgpu::Device>,
            queue: Arc<wgpu::Queue>,
            surface: wgpu::Surface<'static>,
            format: wgpu::TextureFormat,
            engine: Engine,
            renderer: Renderer,
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
            moved: bool,                // Flag to track window movement
            redraw: bool,
            debug: bool,
            debug_tool: Debug,
            _event_sender: StdSender<Event<Action<Message>>>,
            control_receiver: StdReceiver<Control>,
            _context: task::Context<'static>,
            custom_theme: Theme,
        },
    }

    impl Runner {
        fn process_event(
            &mut self,
            event_loop: &winit::event_loop::ActiveEventLoop,
            event: Event<Action<Message>>,
        ) {
            if event_loop.exiting() {
                return;
            }

            match self {
                Runner::Loading { .. } => {
                    // Handle events while loading
                    match event {
                        Event::EventLoopAwakened(winit::event::Event::NewEvents(_)) => {
                            // Continue loading
                        }
                        Event::EventLoopAwakened(winit::event::Event::AboutToWait) => {
                            // Continue loading
                        }
                        _ => {}
                    }
                }
                Runner::Ready {
                    window,
                    device,
                    queue,
                    surface,
                    format,
                    engine,
                    renderer,
                    state,
                    viewport,
                    cursor_position,
                    modifiers,
                    clipboard,
                    runtime,
                    resized,
                    moved,
                    redraw,
                    debug,
                    debug_tool,
                    control_receiver,
                    custom_theme,
                    ..
                } => {
                    // Handle events in ready state
                    match event {
                        Event::EventLoopAwakened(winit::event::Event::WindowEvent {
                            window_id: _window_id,
                            event: window_event,
                        }) => {
                            let _window_event_start = Instant::now();
                            
                            match window_event {
                                WindowEvent::Focused(true) => {
                                    event_loop.set_control_flow(ControlFlow::Poll);
                                    *moved = false;
                                }
                                WindowEvent::Focused(false) => {
                                    event_loop.set_control_flow(ControlFlow::Wait);
                                }
                                WindowEvent::Resized(_size) => {
                                    *resized = true;
                                }
                                WindowEvent::Moved(_) => {
                                    *moved = true;
                                }
                                WindowEvent::CloseRequested => {
                                    event_loop.exit();
                                }
                                WindowEvent::CursorMoved { position, .. } => {
                                    *cursor_position = Some(position);
                                }
                                WindowEvent::MouseInput { state, .. } => {
                                    if state == ElementState::Released {
                                        *moved = false; // Reset flag when mouse is released
                                    }
                                }
                                WindowEvent::ModifiersChanged(new_modifiers) => {
                                    *modifiers = new_modifiers.state();
                                }
                                _ => {}
                            }

                            *redraw = true;

                            // Map window event to iced event
                            if let Some(event) = iced_winit::conversion::window_event(
                                window_event,
                                window.scale_factor(),
                                *modifiers,
                            ) {
                                match &event {
                                    _ => {
                                        state.queue_message(Message::Event(event.clone()));
                                    }
                                }
                                state.queue_event(event);
                                *redraw = true;
                            }

                            // If there are events pending
                            if !state.is_queue_empty() {
                                // We update iced
                                let _update_start = Instant::now();
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
                                    &custom_theme,
                                    &renderer::Style {
                                        text_color: Color::WHITE,
                                    },
                                    clipboard,
                                    debug_tool,
                                );
                                //let update_time = update_start.elapsed();
                                //STATE_UPDATE_STATS.lock().unwrap().add_measurement(update_time);

                                let _ = 'runtime_call: {
                                    let Some(t) = task else {
                                        break 'runtime_call 1;
                                    };
                                    let Some(stream) = into_stream(t) else {
                                        break 'runtime_call 1;
                                    };

                                    runtime.run(stream);
                                    0
                                };
                            }

                            // Handle resizing
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

                            // Render if needed
                            if *redraw {
                                *redraw = false;
                                
                                let frame_start = Instant::now();

                                // Update window title dynamically based on the current image
                                if !*moved {
                                    let new_title = state.program().title();
                                    window.set_title(&new_title);
                                }

                                match surface.get_current_texture() {
                                    Ok(frame) => {
                                        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
                                        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                            label: Some("Render Encoder"),
                                        });

                                        let present_start = Instant::now();
                                        renderer.present(
                                            engine,
                                            device,
                                            queue,
                                            &mut encoder,
                                            None,
                                            frame.texture.format(),
                                            &view,
                                            viewport,
                                            &debug_tool.overlay(),
                                        );
                                        let present_time = present_start.elapsed();
                                        
                                        // Submit the commands to the queue
                                        let submit_start = Instant::now();
                                        engine.submit(queue, encoder);
                                        let submit_time = submit_start.elapsed();
                                        
                                        let present_frame_start = Instant::now();
                                        frame.present();
                                        let present_frame_time = present_frame_start.elapsed();

                                        // Add tracking here to monitor the render cycle
                                        track_render_cycle();

                                        if *debug {
                                            debug!("Renderer present took {:?}", present_time);
                                            debug!("Command submission took {:?}", submit_time);
                                            debug!("Frame presentation took {:?}", present_frame_time);
                                        }

                                        // Update the mouse cursor
                                        window.set_cursor(
                                            iced_winit::conversion::mouse_interaction(
                                                state.mouse_interaction(),
                                            ),
                                        );
                                        
                                        let total_frame_time = frame_start.elapsed();
                                        debug!("Total frame time: {:?}", total_frame_time);
                                    }
                                    Err(error) => match error {
                                        wgpu::SurfaceError::OutOfMemory => {
                                            panic!("Swapchain error: {error}. Rendering cannot continue.");
                                        }
                                        _ => {
                                            // Retry rendering on the next frame
                                            window.request_redraw();
                                        }
                                    },
                                }

                                // Record frame time
                                if let Ok(mut frame_times) = FRAME_TIMES.lock() {
                                    let now = Instant::now();
                                    frame_times.push(now);
                                    
                                    // Calculate FPS every second
                                    if frame_times.len() > 1 {
                                        let oldest = frame_times[0];
                                        let elapsed = now.duration_since(oldest);
                                        
                                        if elapsed.as_secs() >= 1 {
                                            let fps = frame_times.len() as f32 / elapsed.as_secs_f32();
                                            info!("Current FPS: {:.1}", fps);
                                            
                                            // Store the current FPS value
                                            if let Ok(mut current_fps) = CURRENT_FPS.lock() {
                                                *current_fps = fps;
                                            }
                                            
                                            // Keep only recent frames
                                            let cutoff = now - Duration::from_secs(1);
                                            frame_times.retain(|&t| t > cutoff);
                                        }
                                    }
                                }
                            }

                            // Record window event time
                            //let window_event_time = window_event_start.elapsed();
                            //WINDOW_EVENT_STATS.lock().unwrap().add_measurement(window_event_time);

                            // Introduce a short sleep to yield control to the OS and improve responsiveness.
                            // This prevents the event loop from monopolizing the CPU, preventing lags.
                            // A small delay (300µs) seems to be enough to avoid lag while maintaining high performance.
                            std::thread::sleep(std::time::Duration::from_micros(300));
                        }
                        Event::EventLoopAwakened(winit::event::Event::UserEvent(action)) => {
                            match action {
                                Action::Widget(w) => {
                                    state.operate(
                                        renderer,
                                        std::iter::once(w),
                                        Size::new(viewport.physical_size().width as f32, viewport.physical_size().height as f32),
                                        debug_tool,
                                    );
                                }
                                Action::Output(message) => {
                                    state.queue_message(message);
                                }
                                _ => {}
                            }
                            *redraw = true;
                        }
                        Event::EventLoopAwakened(winit::event::Event::AboutToWait) => {
                            // Process any pending control messages
                            loop {
                                match control_receiver.try_recv() {
                                    Ok(control) => match control {
                                        Control::ChangeFlow(flow) => {
                                            use winit::event_loop::ControlFlow;

                                            match (event_loop.control_flow(), flow) {
                                                (
                                                    ControlFlow::WaitUntil(current),
                                                    ControlFlow::WaitUntil(new),
                                                ) if new < current => {}
                                                (
                                                    ControlFlow::WaitUntil(target),
                                                    ControlFlow::Wait,
                                                ) if target > Instant::now() => {}
                                                _ => {
                                                    event_loop.set_control_flow(flow);
                                                }
                                            }
                                        }
                                        Control::Exit => {
                                            event_loop.exit();
                                        }
                                        _ => {}
                                    },
                                    Err(_) => break,
                                }
                            }

                            // Request a redraw if needed
                            if *redraw {
                                window.request_redraw();
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    impl winit::application::ApplicationHandler<Action<Message>> for Runner {
        fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
            match self {
                Self::Loading { proxy, event_sender, control_receiver } => {
                    info!("resumed()...");
                    let custom_theme = Theme::custom_with_fn(
                        "Custom Theme".to_string(),
                        iced_winit::core::theme::Palette {
                            primary: iced_winit::core::Color::from_rgba8(20, 148, 163, 1.0),
                            text: iced_winit::core::Color::from_rgba8(224, 224, 224, 1.0),
                            ..Theme::Dark.palette()
                        },
                        |palette| {
                            // Generate the extended palette from the base palette
                            let mut extended: iced_core::theme::palette::Extended = iced_core::theme::palette::Extended::generate(palette);
                            
                            // Customize specific parts of the extended palette
                            extended.primary.weak.text = iced_winit::core::Color::from_rgba8(224, 224, 224, 1.0);
                            
                            // Return the modified extended palette
                            extended
                        }
                    );
                    
                    let window = Arc::new(
                        event_loop
                        .create_window(
                            winit::window::WindowAttributes::default()
                                .with_inner_size(winit::dpi::PhysicalSize::new(
                                    CONFIG.window_width, 
                                    CONFIG.window_height
                                ))
                                .with_title("ViewSkater")
                                .with_resizable(true)
                        )
                        .expect("Create window"),
                    );

                    if let Some(icon) = load_icon() {
                        window.set_window_icon(Some(icon));
                    }

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
                    let backend = adapter.get_info().backend;

                    // Pass a cloned Arc reference to DataViewer
                    let shader_widget = DataViewer::new(
                        Arc::clone(&device), Arc::clone(&queue), backend);

                    // Initialize iced
                    let mut debug_tool = Debug::new();
                    let engine = Engine::new(
                        &adapter, &device, &queue, format, None);
                    engine.create_image_cache(&device); // Manually create image cache

                    // Manually register fonts
                    register_font_manually(include_bytes!("../assets/fonts/viewskater-fonts.ttf"));
                    register_font_manually(include_bytes!("../assets/fonts/Iosevka-Regular-ascii.ttf"));
                    register_font_manually(include_bytes!("../assets/fonts/Roboto-Regular.ttf"));
                    
                    let mut renderer = Renderer::new(
                        &device,
                        &engine,
                        Font::with_name("Roboto"),
                        Pixels::from(16),
                    );

                    let state = program::State::new(
                        shader_widget,
                        viewport.logical_size(),
                        &mut renderer,
                        &mut debug_tool,
                    );

                    // Set control flow
                    event_loop.set_control_flow(ControlFlow::Poll);

                    let (p, worker) = iced_winit::Proxy::new(proxy.clone());
                    let Ok(executor) = iced_futures::backend::native::tokio::Executor::new() else {
                        panic!("could not create runtime")
                    };
                    executor.spawn(worker);
                    let runtime = iced_futures::Runtime::new(executor, p);

                    // Create a proper static waker
                    let waker = {
                        // Create a waker that does nothing
                        struct NoopWaker;
                        
                        impl Wake for NoopWaker {
                            fn wake(self: Arc<Self>) {}
                            fn wake_by_ref(self: &Arc<Self>) {}
                        }
                        
                        // Create a waker and leak it to make it 'static
                        let waker_arc = Arc::new(NoopWaker);
                        let waker = Waker::from(waker_arc);
                        Box::leak(Box::new(waker))
                    };

                    let context = task::Context::from_waker(waker);

                    // Create a new Ready state with the event_sender and control_receiver
                    // Note: We don't clone the receiver as it's not clonable
                    let event_sender = event_sender.clone();
                    
                    // Move the control_receiver into the Ready state
                    // We need to take ownership of it from the Loading state
                    let control_receiver = std::mem::replace(control_receiver, std_mpsc::channel().1);

                    *self = Self::Ready {
                        window,
                        device,
                        queue,
                        surface,
                        format,
                        engine,
                        renderer,
                        state,
                        cursor_position: None,
                        modifiers: ModifiersState::default(),
                        clipboard,
                        runtime,
                        viewport,
                        resized: false,
                        moved: false,
                        redraw: true,
                        debug: false,
                        debug_tool,
                        _event_sender: event_sender,
                        control_receiver,
                        _context: context,
                        custom_theme
                    };
                }
                Self::Ready { .. } => {
                    // Already initialized
                }
            }
        }

        fn window_event(
            &mut self,
            event_loop: &winit::event_loop::ActiveEventLoop,
            window_id: winit::window::WindowId,
            event: WindowEvent,
        ) {
            self.process_event(
                event_loop,
                Event::EventLoopAwakened(winit::event::Event::WindowEvent {
                    window_id,
                    event,
                }),
            );
        }

        fn user_event(
            &mut self,
            event_loop: &winit::event_loop::ActiveEventLoop,
            action: Action<Message>,
        ) {
            self.process_event(
                event_loop,
                Event::EventLoopAwakened(winit::event::Event::UserEvent(action)),
            );
        }

        fn about_to_wait(
            &mut self,
            event_loop: &winit::event_loop::ActiveEventLoop,
        ) {
            self.process_event(
                event_loop,
                Event::EventLoopAwakened(winit::event::Event::AboutToWait),
            );
        }

        fn new_events(
            &mut self,
            event_loop: &winit::event_loop::ActiveEventLoop,
            cause: winit::event::StartCause,
        ) {
            self.process_event(
                event_loop,
                Event::EventLoopAwakened(winit::event::Event::NewEvents(cause)),
            );
        }
    }

    let mut runner = Runner::Loading {
        proxy,
        event_sender,
        control_receiver,
    };
    
    event_loop.run_app(&mut runner)
}

// Called in render method
fn track_render_cycle() {
    if let Ok(mut time) = LAST_RENDER_TIME.lock() {
        let now = Instant::now();
        let elapsed = now.duration_since(*time);
        *time = now;
        debug!("TIMING: Render frame time: {:?}", elapsed);
    }
}

// Called when an async image completes
fn track_async_delivery() {
    if let Ok(mut time) = LAST_ASYNC_DELIVERY_TIME.lock() {
        let now = Instant::now();
        let elapsed = now.duration_since(*time);
        *time = now;
        debug!("TIMING: Async delivery time: {:?}", elapsed);
    }
    
    // Also check phase alignment
    if let (Ok(render_time), Ok(async_time)) = (LAST_RENDER_TIME.lock(), LAST_ASYNC_DELIVERY_TIME.lock()) {
        let phase_diff = async_time.duration_since(*render_time);
        debug!("TIMING: Phase difference: {:?}", phase_diff);
    }
}
