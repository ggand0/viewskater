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
mod build_info;

#[allow(unused_imports)]
use log::{Level, trace, debug, info, warn, error};

use std::task::Wake;
use std::task::Waker;
use std::sync::Arc;
use std::borrow::Cow;
use std::time::Instant;
use std::sync::Mutex;
use std::time::Duration;
use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::collections::VecDeque;

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
use iced_wgpu::engine::CompressionStrategy;

use crate::utils::timing::TimingStats;
use crate::app::{Message, DataViewer};
use crate::widgets::shader::scene::Scene;
use crate::config::CONFIG;
use std::sync::mpsc::{self as std_mpsc, Receiver as StdReceiver, Sender as StdSender};
use iced_wgpu::{get_image_rendering_diagnostics, log_image_rendering_stats};
use iced_wgpu::engine::ImageConfig;
use std::sync::mpsc::{self, Receiver};

static ICON: &[u8] = include_bytes!("../assets/icon_48.png");

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
static CURRENT_MEMORY_USAGE: Lazy<Mutex<u64>> = Lazy::new(|| {
    Mutex::new(0)
});
static LAST_MEMORY_UPDATE: Lazy<Mutex<Instant>> = Lazy::new(|| {
    Mutex::new(Instant::now())
});
static LAST_STATS_UPDATE: Lazy<Mutex<Instant>> = Lazy::new(|| {
    Mutex::new(Instant::now())
});
static LAST_RENDER_TIME: Lazy<Mutex<Instant>> = Lazy::new(|| {
    Mutex::new(Instant::now())
});
static LAST_ASYNC_DELIVERY_TIME: Lazy<Mutex<Instant>> = Lazy::new(|| {
    Mutex::new(Instant::now())
});

static LAST_QUEUE_LENGTH: AtomicUsize = AtomicUsize::new(0);
const QUEUE_LOG_THRESHOLD: usize = 20;
const QUEUE_RESET_THRESHOLD: usize = 50;

// Store the actual shared log buffer from the file_io module
static SHARED_LOG_BUFFER: Lazy<Arc<Mutex<Option<Arc<Mutex<VecDeque<String>>>>>>> = Lazy::new(|| {
    Arc::new(Mutex::new(None))
});

// Store the stdout buffer for global access
static SHARED_STDOUT_BUFFER: Lazy<Arc<Mutex<Option<Arc<Mutex<VecDeque<String>>>>>>> = Lazy::new(|| {
    Arc::new(Mutex::new(None))
});

pub fn get_shared_log_buffer() -> Option<Arc<Mutex<VecDeque<String>>>> {
    SHARED_LOG_BUFFER.lock().unwrap().clone()
}

pub fn set_shared_log_buffer(buffer: Arc<Mutex<VecDeque<String>>>) {
    *SHARED_LOG_BUFFER.lock().unwrap() = Some(buffer);
}

pub fn get_shared_stdout_buffer() -> Option<Arc<Mutex<VecDeque<String>>>> {
    SHARED_STDOUT_BUFFER.lock().unwrap().clone()
}

pub fn set_shared_stdout_buffer(buffer: Arc<Mutex<VecDeque<String>>>) {
    *SHARED_STDOUT_BUFFER.lock().unwrap() = Some(buffer);
}

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

fn monitor_message_queue(state: &mut program::State<DataViewer>) {
    // Check queue length
    let queue_len = state.queued_messages_len();
    LAST_QUEUE_LENGTH.store(queue_len, Ordering::SeqCst);

    trace!("Message queue size: {}", queue_len);
    
    // Log if the queue is getting large
    if queue_len > QUEUE_LOG_THRESHOLD {
        debug!("Message queue size: {}", queue_len);
    }
    
    // Reset queue if it exceeds our threshold
    if queue_len > QUEUE_RESET_THRESHOLD {
        warn!("MESSAGE QUEUE OVERLOAD: {} messages pending - clearing queue", queue_len);
        state.clear_queued_messages();
    }
}

// Define a message type for renderer configuration requests
enum RendererRequest {
    UpdateCompressionStrategy(CompressionStrategy),
    ClearPrimitiveStorage,
    // Add other renderer configuration requests here if needed
}

fn update_memory_usage() {
    // Just delegate to the utils::mem implementation
    utils::mem::update_memory_usage();
}

pub fn main() -> Result<(), winit::error::EventLoopError> {
    // Set up stdout capture FIRST, before any println! statements
    let shared_stdout_buffer = file_io::setup_stdout_capture();
    set_shared_stdout_buffer(Arc::clone(&shared_stdout_buffer));
    
    println!("ViewSkater starting...");
    
    // Set up panic hook to log to a file
    let app_name = "viewskater";
    let shared_log_buffer = file_io::setup_logger(app_name);
    
    // Store the log buffer reference for global access
    set_shared_log_buffer(Arc::clone(&shared_log_buffer));
    
    file_io::setup_panic_hook(app_name, shared_log_buffer);

    // Initialize winit FIRST
    let event_loop = EventLoop::<Action<Message>>::with_user_event()
        .build()
        .unwrap();

    // Set up the file channel AFTER winit initialization
    let (file_sender, file_receiver) = mpsc::channel();

    // Register file handler BEFORE creating the runner
    // This is required on macOS so the app can receive file paths
    // when launched by opening a file (e.g. double-clicking in Finder)
    // or using "Open With". Must be set up early in app lifecycle.
    #[cfg(target_os = "macos")]
    {
        macos_file_handler::set_file_channel(file_sender);
        macos_file_handler::register_file_handler();
        
        // Try to restore full disk access from previous session
        debug!("🔍 Attempting to restore full disk access on startup");
        let restore_result = macos_file_handler::restore_full_disk_access();
        debug!("🔍 Restore full disk access result: {}", restore_result);
        
        println!("macOS file handler registered");
    }

    // Handle command line arguments for Linux (and Windows)
    // This supports double-click and "Open With" functionality via .desktop files on Linux
    #[cfg(not(target_os = "macos"))]
    {
        let args: Vec<String> = std::env::args().collect();
        if args.len() > 1 {
            let file_path = &args[1];
            println!("File path from command line: {}", file_path);
            
            // Validate that the path exists and is a file or directory
            if std::path::Path::new(file_path).exists() {
                if let Err(e) = file_sender.send(file_path.clone()) {
                    println!("Failed to send file path through channel: {}", e);
                } else {
                    println!("Successfully queued file path for loading: {}", file_path);
                }
            } else {
                println!("Warning: Specified file path does not exist: {}", file_path);
            }
        }
    }

    // Rest of the initialization...
    let proxy: EventLoopProxy<Action<Message>> = event_loop.create_proxy();

    // Create channels for event and control communication
    let (event_sender, _event_receiver) = std_mpsc::channel();
    let (_control_sender, control_receiver) = std_mpsc::channel();

    #[allow(clippy::large_enum_variant)]
    enum Runner {
        Loading {
            proxy: EventLoopProxy<Action<Message>>,
            event_sender: StdSender<Event<Action<Message>>>,
            control_receiver: StdReceiver<Control>,
            file_receiver: Receiver<String>,
        },
        Ready {
            window: Arc<winit::window::Window>,
            device: Arc<wgpu::Device>,
            queue: Arc<wgpu::Queue>,
            surface: wgpu::Surface<'static>,
            format: wgpu::TextureFormat,
            engine: Arc<Mutex<Engine>>,
            renderer: Arc<Mutex<Renderer>>,
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
            renderer_request_receiver: Receiver<RendererRequest>,
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
                    renderer_request_receiver,
                    ..
                } => {
                    // Handle events in ready state
                    match event {
                        Event::EventLoopAwakened(winit::event::Event::WindowEvent {
                            window_id: _window_id,
                            event: window_event,
                        }) => {
                            let _window_event_start = Instant::now();
                            
                            // Monitor the message queue and clear it if it's getting large
                            monitor_message_queue(state);
                            
                            match window_event {
                                WindowEvent::Focused(true) => {
                                    event_loop.set_control_flow(ControlFlow::Poll);
                                    *moved = false;
                                }
                                WindowEvent::Focused(false) => {
                                    event_loop.set_control_flow(ControlFlow::Wait);
                                }
                                WindowEvent::Resized(size) => {
                                    if size.width > 0 && size.height > 0 {
                                        *resized = true;
                                    } else {
                                        // Skip resizing and avoid configuring the surface
                                        *resized = false;
                                    }
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
                                //let update_start = Instant::now();
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
                                    &mut *renderer.lock().unwrap(),
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

                            // Process any pending renderer requests to update iced_wgpu's config
                            // NOTE1: we need to use a while loop to process all pending requests
                            // because try_recv() only returns one request at a time
                            // NOTE2: we need to do this sender/receiver pattern to avoid deadlocks
                            while let Ok(request) = renderer_request_receiver.try_recv() {
                                match request {
                                    RendererRequest::UpdateCompressionStrategy(strategy) => {
                                        debug!("Main thread handling compression strategy update to {:?}", strategy);
                                        
                                        let config = ImageConfig {
                                            atlas_size: CONFIG.atlas_size,
                                            compression_strategy: strategy,
                                        };
                                        
                                        // We already have locks for renderer and engine in the rendering code
                                        let mut engine_guard = engine.lock().unwrap();
                                        let mut renderer_guard = renderer.lock().unwrap();
                                        
                                        // Update the config safely from the main render thread
                                        renderer_guard.update_image_config(&device, &mut *engine_guard, config);
                                        
                                        debug!("Compression strategy updated successfully in main thread");
                                    }
                                    RendererRequest::ClearPrimitiveStorage => {
                                        debug!("Main thread handling primitive storage clear request");
                                        
                                        // Get engine lock
                                        let mut engine_guard = engine.lock().unwrap();
                                        
                                        // Access the primitive storage directly
                                        engine_guard.clear_primitive_storage();
                                        debug!("Primitive storage cleared successfully");
                                    },
                                }
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
                                        {
                                            let mut engine_guard = engine.lock().unwrap();
                                            let mut renderer_guard = renderer.lock().unwrap();
                                            
                                            renderer_guard.present(
                                                &mut *engine_guard,
                                                &device,
                                                &queue,
                                                &mut encoder,
                                                None,
                                                frame.texture.format(),
                                                &view,
                                                viewport,
                                                &debug_tool.overlay(),
                                            );
                                            
                                            // Submit commands while still holding the lock
                                            engine_guard.submit(&queue, encoder);
                                        }
                                        let present_time = present_start.elapsed();
                                        
                                        // Submit the commands to the queue
                                        let submit_start = Instant::now();
                                        let submit_time = submit_start.elapsed();
                                        
                                        let present_frame_start = Instant::now();
                                        frame.present();
                                        let present_frame_time = present_frame_start.elapsed();

                                        // Add tracking here to monitor the render cycle
                                        if *debug {
                                            track_render_cycle();
                                        }

                                        // Always log these if they're abnormally long
                                        if present_time.as_millis() > 50 {
                                            warn!("BOTTLENECK: Renderer present took {:?}", present_time);
                                        }
                                        
                                        if submit_time.as_millis() > 50 {
                                            warn!("BOTTLENECK: Command submission took {:?}", submit_time);
                                        }
                                        
                                        if present_frame_time.as_millis() > 50 {
                                            warn!("BOTTLENECK: Frame presentation took {:?}", present_frame_time);
                                        }

                                        // Original debug logging
                                        if *debug {
                                            trace!("Renderer present took {:?}", present_time);
                                            trace!("Command submission took {:?}", submit_time);
                                            trace!("Frame presentation took {:?}", present_frame_time);
                                        }

                                        // Update the mouse cursor
                                        window.set_cursor(
                                            iced_winit::conversion::mouse_interaction(
                                                state.mouse_interaction(),
                                            ),
                                        );
                                        
                                        if *debug {
                                            let total_frame_time = frame_start.elapsed();
                                            trace!("Total frame time: {:?}", total_frame_time);
                                        }
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

                                // Record frame time and update memory usage
                                if let Ok(mut frame_times) = FRAME_TIMES.lock() {
                                    let now = Instant::now();
                                    frame_times.push(now);
                                    
                                    // Only update stats once per second
                                    let should_update_stats = {
                                        if let Ok(last_update) = LAST_STATS_UPDATE.lock() {
                                            last_update.elapsed().as_secs() >= 1
                                        } else {
                                            false
                                        }
                                    };
                                    
                                    if should_update_stats {
                                        // Update the timestamp
                                        if let Ok(mut last_update) = LAST_STATS_UPDATE.lock() {
                                            *last_update = now;
                                        }
                                        
                                        // Clean up old frames
                                        let cutoff = now - Duration::from_secs(1);
                                        frame_times.retain(|&t| t > cutoff);
                                        
                                        // Calculate FPS
                                        let fps = frame_times.len() as f32;
                                        trace!("Current FPS: {:.1}", fps);
                                        
                                        // Store the current FPS value
                                        if let Ok(mut current_fps) = CURRENT_FPS.lock() {
                                            *current_fps = fps;
                                        }
                                        
                                        // Update memory usage (which has its own throttling as a backup)
                                        update_memory_usage();
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
                                        &mut *renderer.lock().unwrap(),
                                        std::iter::once(w),
                                        Size::new(viewport.physical_size().width as f32, viewport.physical_size().height as f32),
                                        debug_tool,
                                    );
                                }
                                Action::Clipboard(action) => {
                                    match action {
                                        iced_runtime::clipboard::Action::Write { target, contents } => {
                                            debug!("Main thread received clipboard write request: {:?}, {:?}", target, contents);
                                            
                                            // Write to the clipboard using the Clipboard instance
                                            clipboard.write(target, contents);
                                            debug!("Successfully wrote to clipboard");
                                        }
                                        iced_runtime::clipboard::Action::Read { target, channel } => {
                                            debug!("Main thread received clipboard read request: {:?}", target);
                                            
                                            // Read from clipboard and send result back through the channel
                                            let content = clipboard.read(target);
                                            
                                            if let Err(err) = channel.send(content) {
                                                error!("Failed to send clipboard content through channel: {:?}", err);
                                            }
                                        }
                                    }
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
                Self::Loading { proxy, event_sender, control_receiver, file_receiver } => {
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

                                let capabilities = surface.get_capabilities(&adapter);
                                
                                let (device, queue) = adapter
                                    .request_device(
                                        &wgpu::DeviceDescriptor {
                                            label: Some("Main Device"),
                                            required_features: wgpu::Features::empty() | wgpu::Features::TEXTURE_COMPRESSION_BC,
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

                    // Initialize iced
                    let mut debug_tool = Debug::new();

                    let config = ImageConfig {
                        atlas_size: CONFIG.atlas_size,
                        compression_strategy: CompressionStrategy::Bc1,
                    };
                    let engine = Arc::new(Mutex::new(Engine::new(
                        &adapter, &device, &queue, format, None, Some(config))));
                    {
                        let engine_guard = engine.lock().unwrap();
                        engine_guard.create_image_cache(&device);
                    }

                    // Manually register fonts
                    register_font_manually(include_bytes!("../assets/fonts/viewskater-fonts.ttf"));
                    register_font_manually(include_bytes!("../assets/fonts/Iosevka-Regular-ascii.ttf"));
                    register_font_manually(include_bytes!("../assets/fonts/Roboto-Regular.ttf"));
                    
                    // Create renderer with Arc<Mutex>
                    let renderer = Arc::new(Mutex::new(Renderer::new(
                        &device,
                        &engine.lock().unwrap(),
                        Font::with_name("Roboto"),
                        Pixels::from(16),
                    )));

                    // Create the renderer request channel
                    let (renderer_request_sender, renderer_request_receiver) = mpsc::channel();

                    // Pass a cloned Arc reference to DataViewer
                    let shader_widget = DataViewer::new(
                        Arc::clone(&device), 
                        Arc::clone(&queue), 
                        backend,
                        renderer_request_sender,
                        std::mem::replace(file_receiver, mpsc::channel().1),
                    );

                    // Update state creation to lock renderer
                    let mut renderer_guard = renderer.lock().unwrap();
                    let state = program::State::new(
                        shader_widget,
                        viewport.logical_size(),
                        &mut *renderer_guard,
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
                        renderer: renderer.clone(),
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
                        custom_theme,
                        renderer_request_receiver,
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
        file_receiver,
    };
    
    event_loop.run_app(&mut runner)
}

// Called in render method
fn track_render_cycle() {
    if let Ok(mut time) = LAST_RENDER_TIME.lock() {
        let now = Instant::now();
        let elapsed = now.duration_since(*time);
        *time = now;
        
        // Use the new diagnostics APIs
        let (fps, upload_secs, render_secs, min_render, max_render, frame_count) = 
            get_image_rendering_diagnostics();
        
        // Check for bottlenecks
        if elapsed.as_millis() > 50 {
            warn!("LONG FRAME DETECTED: Render time: {:?}", elapsed);
            warn!("Image stats: FPS={:.1}, Upload={:.2}ms, Render={:.2}ms, Min={:.2}ms, Max={:.2}ms",
                 fps, upload_secs * 1000.0, render_secs * 1000.0, 
                 min_render * 1000.0, max_render * 1000.0);
            
            // Log detailed stats to console
            log_image_rendering_stats();
            
            // Check if upload or render is the bottleneck
            if upload_secs > 0.050 {  // 50ms threshold
                warn!("BOTTLENECK: GPU texture upload is slow: {:.2}ms avg", upload_secs * 1000.0);
            }
            
            if render_secs > 0.050 {  // 50ms threshold
                warn!("BOTTLENECK: GPU render time is slow: {:.2}ms avg", render_secs * 1000.0);
            }
        }
        
        // Display diagnostics in UI during development
        if frame_count % 60 == 0 {
            // Periodic stats logging
            trace!("Image FPS: {:.1}, Upload: {:.2}ms, Render: {:.2}ms", 
                  fps, upload_secs * 1000.0, render_secs * 1000.0);
        }
        
        //trace!("TIMING: Render frame time: {:?}", elapsed);
    }
}

// Called when an async image completes
fn track_async_delivery() {
    if let Ok(mut time) = LAST_ASYNC_DELIVERY_TIME.lock() {
        let now = Instant::now();
        let elapsed = now.duration_since(*time);
        *time = now;
        trace!("TIMING: Interval time between async deliveries: {:?}", elapsed);
    }

    // Check image rendering FPS from custom iced_wgpu
    let image_fps = iced_wgpu::get_image_fps();
    trace!("TIMING: Image FPS: {}", image_fps);
    
    // Also check phase alignment
    if let (Ok(render_time), Ok(async_time)) = (LAST_RENDER_TIME.lock(), LAST_ASYNC_DELIVERY_TIME.lock()) {
        let phase_diff = async_time.duration_since(*render_time);
        trace!("TIMING: Phase difference: {:?}", phase_diff);
    }
}

/// macOS integration for opening image files via Finder.
///
/// This module handles cases where the user launches ViewSkater by double-clicking
/// an image file or using "Open With" in Finder. macOS sends the file path through
/// the `application:openFiles:` message, which is delivered to the app's delegate.
///
/// This code:
/// - Subclasses the existing `NSApplicationDelegate` to override `application:openFiles:`
/// - Forwards received file paths to Rust using an MPSC channel
/// - Disables automatic argument parsing by setting `NSTreatUnknownArgumentsAsOpen = NO`
///
/// The channel is set up in `main.rs` and connected to the rest of the app so that
/// the selected image can be loaded on startup.

#[cfg(target_os = "macos")]
mod macos_file_handler {
    use std::sync::mpsc::Sender;
    use std::sync::Mutex;
    use std::collections::HashMap;
    use objc2::rc::autoreleasepool;
    use objc2::{msg_send, sel};
    use objc2::declare::ClassBuilder;
    use objc2::runtime::{AnyObject, Sel, AnyClass};
    use objc2_app_kit::{NSApplication, NSOpenPanel, NSModalResponse, NSModalResponseOK};
    use objc2_foundation::{MainThreadMarker, NSArray, NSString, NSDictionary, NSUserDefaults, NSURL};
    use objc2::rc::Retained;
    use once_cell::sync::Lazy;
    
    #[allow(unused_imports)]
    use log::{debug, info, warn, error};

    static mut FILE_CHANNEL: Option<Sender<String>> = None;
    
    // Security bookmark constants (from Apple's NSURLBookmarkCreationOptions)
    const NSURL_BOOKMARK_CREATION_WITH_SECURITY_SCOPE: usize = 0x800; // 1UL << 11
    const NSURL_BOOKMARK_RESOLUTION_WITH_SECURITY_SCOPE: usize = 0x100; // 1UL << 8

    // Store security-scoped URLs globally to maintain access throughout app lifetime
    static SECURITY_SCOPED_URLS: Lazy<Mutex<HashMap<String, Retained<NSURL>>>> = 
        Lazy::new(|| Mutex::new(HashMap::new()));

    // Store user preferences for directory access decisions
    static DIRECTORY_PERMISSIONS: Lazy<Mutex<HashMap<String, bool>>> = 
        Lazy::new(|| Mutex::new(HashMap::new()));

    // Store the full disk access bookmark persistently
    static FULL_DISK_ACCESS_GRANTED: Lazy<Mutex<bool>> = 
        Lazy::new(|| Mutex::new(false));

    pub fn set_file_channel(sender: Sender<String>) {
        unsafe {
            FILE_CHANNEL = Some(sender);
        }
    }

    // Function to store a security-scoped URL
    fn store_security_scoped_url(path: &str, url: Retained<NSURL>) {
        if let Ok(mut urls) = SECURITY_SCOPED_URLS.lock() {
            urls.insert(path.to_string(), url);
            println!("Stored security-scoped URL for path: {}", path);
        }
    }

    // Public function to check if we have access to a path
    pub fn has_security_scoped_access(path: &str) -> bool {
        if let Ok(urls) = SECURITY_SCOPED_URLS.lock() {
            urls.contains_key(path)
        } else {
            false
        }
    }

    // Public function to get all accessible paths (for debugging)
    pub fn get_accessible_paths() -> Vec<String> {
        if let Ok(urls) = SECURITY_SCOPED_URLS.lock() {
            urls.keys().cloned().collect()
        } else {
            Vec::new()
        }
    }

    // Public function to check if we have access (simplified)
    pub fn ensure_security_scoped_access(path: &str) -> bool {
        has_security_scoped_access(path)
    }

    // Function to perform directory operations while ensuring security-scoped access is maintained
    pub fn read_directory_with_security_scoped_access(path: &str) -> Option<Result<std::fs::ReadDir, std::io::Error>> {
        if let Ok(urls) = SECURITY_SCOPED_URLS.lock() {
            // Try exact path first
            if let Some(url) = urls.get(path) {
                println!("Using security-scoped access for exact path: {}", path);
                debug!("Starting security-scoped resource access for exact path");
                
                // CRITICAL FIX: Test if the security-scoped URL is working
                if test_security_scoped_url_access(url, path) {
                    let result = std::fs::read_dir(path);
                    return Some(result);
                } else {
                    debug!("Failed to start security-scoped resource access for exact path");
                    // If we can't access it, the bookmark might be stale - remove just this one
                    println!("❌ Security-scoped access failed for stored URL, removing stale entry");
                    drop(urls); // Release the lock before calling remove function
                    remove_failed_security_scoped_url(path);
                    return None;
                }
            }
            
            // Try parent directory access if we have it
            if let Ok(path_obj) = std::path::Path::new(path).canonicalize() {
                if let Some(parent) = path_obj.parent() {
                    let parent_str = parent.to_string_lossy();
                    if let Some(parent_url) = urls.get(parent_str.as_ref()) {
                        println!("Using security-scoped access for parent directory: {}", parent_str);
                        debug!("Starting security-scoped resource access for parent directory");
                        
                        if test_security_scoped_url_access(parent_url, &parent_str) {
                            let result = std::fs::read_dir(path);
                            return Some(result);
                        } else {
                            debug!("Failed to start security-scoped resource access for parent");
                            return None;
                        }
                    }
                }
            }
            
            // Try variations with/without trailing slashes
            let path_variations = if path.ends_with('/') {
                vec![path.trim_end_matches('/').to_string()]
            } else {
                vec![format!("{}/", path)]
            };
            
            for variation in path_variations {
                if let Some(url) = urls.get(&variation) {
                    println!("Using security-scoped access for path variation: {}", variation);
                    debug!("Starting security-scoped resource access for path variation");
                    
                    if test_security_scoped_url_access(url, &variation) {
                        let result = std::fs::read_dir(path);
                        return Some(result);
                    } else {
                        debug!("Failed to start security-scoped resource access for variation");
                    }
                }
            }
            
            // CRITICAL FIX: Check if we have individual file access but no directory access
            // This is the key case when user double-clicks an image file - we get file access but not directory access
            let has_individual_file_access = urls.keys().any(|key| {
                let key_path = std::path::Path::new(key);
                // Check if any stored URL is a file within the directory we're trying to access
                if key_path.is_file() {
                    if let Some(file_parent) = key_path.parent() {
                        return file_parent.to_string_lossy() == path;
                    }
                }
                false
            });
            
            if has_individual_file_access {
                println!("DETECTED: Individual file access within directory {} but no directory access - this is the 'Open With' case", path);
                println!("Available security-scoped URLs: {:?}", urls.keys().collect::<Vec<_>>());
                
                // Check if we already have full disk access
                if has_full_disk_access() {
                    println!("Full disk access already granted - this should work");
                    debug!("Attempting directory read with existing full disk access");
                    return Some(std::fs::read_dir(path));
                } else {
                    println!("Individual file access detected but no full disk access - need to request directory access");
                    // Trigger NSOpenPanel request
                    drop(urls); // Release lock before calling request function
                    if request_directory_access_with_nsopenpanel(path) {
                        // Retry after getting access
                        return Some(std::fs::read_dir(path));
                    } else {
                        return None;
                    }
                }
            }
            
            // If we have no security-scoped access at all, request it
            if urls.is_empty() {
                println!("No security-scoped URLs stored - requesting directory access");
                drop(urls); // Release lock before calling request function
                if request_directory_access_with_nsopenpanel(path) {
                    // Retry after getting access
                    return Some(std::fs::read_dir(path));
                } else {
                    return None;
                }
            }
        }
        
        println!("No security-scoped access available for path: {}", path);
        None
    }

    // Function to check if user has already decided about directory access
    fn has_directory_permission_decision(dir_path: &str) -> Option<bool> {
        if let Ok(perms) = DIRECTORY_PERMISSIONS.lock() {
            perms.get(dir_path).copied()
        } else {
            None
        }
    }

    // Function to store user's directory access decision
    fn store_directory_permission_decision(dir_path: &str, granted: bool) {
        if let Ok(mut perms) = DIRECTORY_PERMISSIONS.lock() {
            perms.insert(dir_path.to_string(), granted);
        }
    }

    // Function to check if full disk access has been granted
    pub fn has_full_disk_access() -> bool {
        let access_granted = if let Ok(access) = FULL_DISK_ACCESS_GRANTED.lock() {
            *access
        } else {
            false
        };
        debug!("has_full_disk_access() returning: {}", access_granted);
        access_granted
    }

    // NEW: NSOpenPanel-based directory access request
    fn request_directory_access_with_nsopenpanel(requested_path: &str) -> bool {
        debug!("🔍 Requesting directory access via NSOpenPanel for: {}", requested_path);
        
        autoreleasepool(|pool| {
            unsafe {
                let mtm = MainThreadMarker::new().expect("Must be on main thread");
                
                // Create NSOpenPanel
                let panel = NSOpenPanel::openPanel(mtm);
                
                // Configure panel for directory selection
                panel.setCanChooseDirectories(true);
                panel.setCanChooseFiles(false);
                panel.setAllowsMultipleSelection(false);
                panel.setCanCreateDirectories(false);
                
                // Set the directory to the requested path's parent if possible
                if let Some(parent_dir) = std::path::Path::new(requested_path).parent() {
                    let parent_str = parent_dir.to_string_lossy();
                    let parent_nsstring = NSString::from_str(&parent_str);
                    let parent_url = NSURL::fileURLWithPath(&parent_nsstring);
                    panel.setDirectoryURL(Some(&parent_url));
                }
                
                // Set title and prompt
                let title = NSString::from_str("Grant Directory Access");
                panel.setTitle(Some(&title));
                
                let message = NSString::from_str(&format!(
                    "ViewSkater needs access to browse images in this directory:\n\n{}\n\nPlease select the directory to grant access.",
                    requested_path
                ));
                panel.setMessage(Some(&message));
                
                // Show the panel
                debug!("Showing NSOpenPanel...");
                let response: NSModalResponse = panel.runModal();
                
                if response == NSModalResponseOK {
                    debug!("User selected directory in NSOpenPanel");
                    
                    // Get the selected URLs (security-scoped)
                    let urls = panel.URLs();
                    
                    if urls.len() > 0 {
                        let selected_url = &urls[0];
                        
                        // Get the path from the selected URL
                        if let Some(selected_path_nsstring) = selected_url.path() {
                            let selected_path = selected_path_nsstring.as_str(pool);
                            debug!("Selected directory: {}", selected_path);
                            
                            // CRITICAL: Create security-scoped bookmark from the selected URL
                            use objc2_foundation::{NSURLBookmarkCreationOptions, NSURLBookmarkResolutionOptions};
                            let bookmark_options = NSURLBookmarkCreationOptions(NSURL_BOOKMARK_CREATION_WITH_SECURITY_SCOPE);
                            
                            match selected_url.bookmarkDataWithOptions_includingResourceValuesForKeys_relativeToURL_error(
                                bookmark_options,
                                None,
                                None,
                            ) {
                                Ok(bookmark_data) => {
                                    debug!("Created security-scoped bookmark successfully");
                                    
                                    // Resolve the bookmark to get a security-scoped URL
                                    let resolution_options = NSURLBookmarkResolutionOptions(NSURL_BOOKMARK_RESOLUTION_WITH_SECURITY_SCOPE);
                                    let mut is_stale = objc2::runtime::Bool::new(false);
                                    
                                    match NSURL::URLByResolvingBookmarkData_options_relativeToURL_bookmarkDataIsStale_error(
                                        &bookmark_data,
                                        resolution_options,
                                        None,
                                        &mut is_stale,
                                    ) {
                                        Ok(resolved_url) => {
                                            debug!("Resolved security-scoped bookmark successfully");
                                            
                                            // Start accessing the security-scoped resource
                                            let access_granted = resolved_url.startAccessingSecurityScopedResource();
                                            debug!("Security-scoped resource access result: {}", access_granted);
                                            
                                            if access_granted {
                                                // Store the resolved URL for future use
                                                store_security_scoped_url(selected_path, resolved_url.clone());
                                                
                                                // Also store bookmark data in UserDefaults for persistence
                                                let defaults = NSUserDefaults::standardUserDefaults();
                                                let key = NSString::from_str("ViewSkaterSecurityBookmark");
                                                defaults.setObject_forKey(Some(&*bookmark_data), &key);
                                                
                                                debug!("✅ Successfully granted and stored directory access for: {}", selected_path);
                                                println!("✅ Directory access granted for: {}", selected_path);
                                                
                                                return true;
                                            } else {
                                                debug!("❌ Failed to start accessing security-scoped resource");
                                                println!("❌ Failed to activate directory access");
                                            }
                                        }
                                        Err(e) => {
                                            debug!("❌ Failed to resolve security-scoped bookmark: {:?}", e);
                                            println!("❌ Failed to resolve security bookmark");
                                        }
                                    }
                                }
                                Err(e) => {
                                    debug!("❌ Failed to create security-scoped bookmark: {:?}", e);
                                    println!("❌ Failed to create security bookmark");
                                }
                            }
                        }
                    }
                } else {
                    debug!("User cancelled NSOpenPanel");
                    println!("Directory access cancelled by user");
                }
            }
            
            false
        })
    }

    // Better UX approach: Guide user to grant Full Disk Access in System Preferences
    pub fn request_parent_directory_permission_dialog(file_path: &str) -> bool {
        debug!("🔍 request_parent_directory_permission_dialog() called for: {}", file_path);
        
        let file_path_obj = std::path::Path::new(file_path);
        if let Some(parent_dir) = file_path_obj.parent() {
            let parent_dir_str = parent_dir.to_string_lossy();
            
            // Use NSOpenPanel instead of AppleScript
            return request_directory_access_with_nsopenpanel(&parent_dir_str);
        } else {
            debug!("Could not determine parent directory");
            false
        }
    }

    // Simplified full disk access request - just check if we have it
    pub fn request_full_disk_access_once() -> bool {
        debug!("🔍 request_full_disk_access_once() called");
        
        // First check if we actually have Full Disk Access by testing a protected directory
        if test_full_disk_access() {
            debug!("Full Disk Access already granted");
            if let Ok(mut access) = FULL_DISK_ACCESS_GRANTED.lock() {
                *access = true;
            }
            return true;
        }
        
        debug!("No Full Disk Access detected, using NSOpenPanel for directory access");
        
        // Instead of showing Full Disk Access guidance, use NSOpenPanel
        // This is more user-friendly and works within the sandbox
        return request_directory_access_with_nsopenpanel("/Users");
    }

    // Helper function to test if we actually have Full Disk Access
    fn test_full_disk_access() -> bool {
        // Try to read a protected directory that requires Full Disk Access
        // We'll try to read the user's Desktop directory which is typically protected
        if let Some(home_dir) = dirs::home_dir() {
            let desktop_dir = home_dir.join("Desktop");
            match std::fs::read_dir(&desktop_dir) {
                Ok(_) => {
                    debug!("✅ Full Disk Access confirmed - can read Desktop directory");
                    return true;
                }
                Err(e) => {
                    debug!("❌ No Full Disk Access - cannot read Desktop directory: {}", e);
                }
            }
        }
        
        // Also try reading the user's Documents directory
        if let Some(home_dir) = dirs::home_dir() {
            let documents_dir = home_dir.join("Documents");
            match std::fs::read_dir(&documents_dir) {
                Ok(_) => {
                    debug!("✅ Full Disk Access confirmed - can read Documents directory");
                    return true;
                }
                Err(e) => {
                    debug!("❌ No Full Disk Access - cannot read Documents directory: {}", e);
                }
            }
        }
        
        false
    }

    unsafe extern "C" fn handle_open_files(
        _this: &mut AnyObject,
        _sel: Sel,
        _sender: &AnyObject,
        files: &NSArray<NSString>,
    ) {
        println!("application_open_files called with {} files", files.len());
        autoreleasepool(|pool| {
            for file in files.iter() {
                let path = file.as_str(pool).to_owned();
                println!("Received file path: {}", path);
                
                // Create NSURL from the path
                let url = NSURL::fileURLWithPath(&file);
                println!("Created NSURL for path: {}", path);
                
                // Try to start accessing security-scoped resource for the file
                let file_accessed: bool = unsafe { msg_send![&url, startAccessingSecurityScopedResource] };
                println!("File security-scoped resource access result: {}", file_accessed);
                
                // Store the file URL if access was granted
                if file_accessed {
                    store_security_scoped_url(&path, url.clone());
                    println!("Stored security-scoped access for file: {}", path);
                    
                    // IMPORTANT: Try to get parent directory access using the file's security scope
                    if let Some(parent_url) = url.URLByDeletingLastPathComponent() {
                        if let Some(parent_path) = parent_url.path() {
                            let parent_path_str = parent_path.as_str(pool).to_owned();
                            println!("Attempting to gain parent directory access: {}", parent_path_str);
                            
                            // Test if we can read the parent directory using the file's security scope
                            match std::fs::read_dir(std::path::Path::new(&parent_path_str)) {
                                Ok(_) => {
                                    println!("SUCCESS: Can read parent directory using file's security scope");
                                    // Store the parent directory access too
                                    store_security_scoped_url(&parent_path_str, url.clone());
                                    println!("Stored parent directory access using file's security scope");
                                }
                                Err(e) => {
                                    println!("FAILED: Cannot read parent directory using file's security scope: {}", e);
                                    println!("This means we have individual file access but not directory access");
                                }
                            }
                        }
                    }
                    
                    // Send the file path through the channel
                    unsafe {
                        if let Some(ref sender) = *(&raw const FILE_CHANNEL) {
                            if let Err(e) = sender.send(path.clone()) {
                                println!("Failed to send file path through channel: {}", e);
                            } else {
                                println!("Successfully sent file path through channel");
                            }
                        }
                    }
                    continue;
                }
                
                // If individual file access failed, try as fallback
                println!("Individual file access failed, sending path anyway as fallback");
                unsafe {
                    if let Some(ref sender) = *(&raw const FILE_CHANNEL) {
                        if let Err(e) = sender.send(path.clone()) {
                            println!("Failed to send file path through channel: {}", e);
                        } else {
                            println!("Successfully sent file path through channel (fallback)");
                        }
                    } else {
                        println!("FILE_CHANNEL is None when trying to send path");
                    }
                }
            }
        });
    }

    pub fn register_file_handler() {
        let mtm = MainThreadMarker::new().expect("Must be on main thread");
        unsafe {
            let app = NSApplication::sharedApplication(mtm);
            
            // Get the existing delegate
            let delegate = app.delegate().unwrap();
            
            // Find out class of the NSApplicationDelegate
            let class: &AnyClass = msg_send![&delegate, class];
            
            // Create a subclass of the existing delegate
            let mut my_class = ClassBuilder::new("ViewSkaterApplicationDelegate", class).unwrap();
            my_class.add_method(
                sel!(application:openFiles:),
                handle_open_files as unsafe extern "C" fn(_, _, _, _) -> _,
            );
            let class = my_class.register();
            
            // Cast and set the class
            let delegate_obj = Retained::cast::<AnyObject>(delegate);
            AnyObject::set_class(&delegate_obj, class);
            
            // Prevent AppKit from interpreting our command line
            let key = NSString::from_str("NSTreatUnknownArgumentsAsOpen");
            let keys = vec![key.as_ref()];
            let objects = vec![Retained::cast::<AnyObject>(NSString::from_str("NO"))];
            let dict = NSDictionary::from_vec(&keys, objects);
            NSUserDefaults::standardUserDefaults().registerDefaults(dict.as_ref());
        }
    }

    // Helper function to test if a security-scoped URL is working
    fn test_security_scoped_url_access(url: &Retained<NSURL>, path: &str) -> bool {
        debug!("Testing security-scoped access for: {}", path);
        
        let accessed = unsafe { url.startAccessingSecurityScopedResource() };
        debug!("startAccessingSecurityScopedResource result for {}: {}", path, accessed);
        
        if accessed {
            // Test actual file system access
            match std::fs::metadata(path) {
                Ok(_) => {
                    debug!("✅ Security-scoped access verified for: {}", path);
                    // Keep the access active - don't stop it
                    return true;
                }
                Err(e) => {
                    debug!("❌ File system access failed despite security scope for {}: {}", path, e);
                    unsafe { url.stopAccessingSecurityScopedResource() };
                    return false;
                }
            }
        } else {
            debug!("❌ Could not start security-scoped resource access for: {}", path);
            return false;
        }
    }

    // Function to remove a specific failed security-scoped URL
    fn remove_failed_security_scoped_url(failed_path: &str) {
        if let Ok(mut urls) = SECURITY_SCOPED_URLS.lock() {
            if urls.remove(failed_path).is_some() {
                debug!("Removed failed security-scoped URL for: {}", failed_path);
                println!("🗑️ Removed failed security-scoped URL for: {}", failed_path);
            }
        }
    }

    // Function to restore directory access from stored bookmarks
    pub fn restore_full_disk_access() -> bool {        
        debug!("🔍 Attempting to restore directory access from stored bookmarks");
        
        autoreleasepool(|_pool| {
            unsafe {
                let defaults = NSUserDefaults::standardUserDefaults();
                let key = NSString::from_str("ViewSkaterSecurityBookmark");
                
                if let Some(bookmark_data) = defaults.objectForKey(&key) {
                    debug!("Found stored security bookmark");
                    
                    // Cast the object to NSData
                    let bookmark_data: Retained<objc2_foundation::NSData> = 
                        Retained::cast(bookmark_data.clone());
                    
                    // Resolve the bookmark
                    use objc2_foundation::NSURLBookmarkResolutionOptions;
                    let resolution_options = NSURLBookmarkResolutionOptions(NSURL_BOOKMARK_RESOLUTION_WITH_SECURITY_SCOPE);
                    let mut is_stale = objc2::runtime::Bool::new(false);
                    
                    match NSURL::URLByResolvingBookmarkData_options_relativeToURL_bookmarkDataIsStale_error(
                        &bookmark_data,
                        resolution_options,
                        None,
                        &mut is_stale,
                    ) {
                        Ok(resolved_url) => {
                            debug!("Successfully resolved stored bookmark");
                            
                            // Start accessing the security-scoped resource
                            let access_granted = resolved_url.startAccessingSecurityScopedResource();
                            debug!("Security-scoped resource access result: {}", access_granted);
                            
                            if access_granted {
                                // Get the path from the resolved URL
                                if let Some(path_nsstring) = resolved_url.path() {
                                    let path = path_nsstring.as_str(_pool);
                                    
                                    // Store the resolved URL for future use
                                    store_security_scoped_url(path, resolved_url);
                                    
                                    debug!("✅ Successfully restored directory access for: {}", path);
                                    println!("✅ Restored directory access for: {}", path);
                                    
                                    return true;
                                }
                            } else {
                                debug!("❌ Failed to start accessing restored security-scoped resource");
                            }
                        }
                        Err(e) => {
                            debug!("❌ Failed to resolve stored bookmark: {:?}", e);
                        }
                    }
                } else {
                    debug!("No stored security bookmark found");
                }
            }
            
            false
        })
    }
}


