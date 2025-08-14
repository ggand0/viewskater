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
use chrono;
use std::io::Write;

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

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

/// Sets up a signal handler to catch low-level crashes that bypass Rust panic hooks
/// This is critical for Objective-C interop crashes that might cause segfaults
#[cfg(unix)]
fn setup_signal_crash_handler() {
    extern "C" fn signal_handler(signal: libc::c_int) {
        let signal_name = match signal {
            libc::SIGSEGV => "SIGSEGV (segmentation fault)",
            libc::SIGBUS => "SIGBUS (bus error)",
            libc::SIGILL => "SIGILL (illegal instruction)",
            libc::SIGFPE => "SIGFPE (floating point exception)",
            libc::SIGABRT => "SIGABRT (abort)",
            _ => "UNKNOWN SIGNAL",
        };
        
        // Use the most basic logging possible since we're in a signal handler
        let _ = std::panic::catch_unwind(|| {
            eprintln!("CRASH_DEBUG: SIGNAL CAUGHT: {} ({})", signal_name, signal);
            println!("CRASH_DEBUG: SIGNAL CAUGHT: {} ({})", signal_name, signal);
        });
        
        // Try to write to NSUserDefaults if possible
        #[cfg(target_os = "macos")]
        {
            unsafe {
                use objc2_foundation::{NSUserDefaults, NSString};
                use objc2::{msg_send};
                
                let message = format!("SIGNAL_CRASH: {} ({})", signal_name, signal);
                let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.6fZ");
                let formatted_message = format!("{} CRASH_DEBUG: {}", timestamp, message);
                
                let defaults = NSUserDefaults::standardUserDefaults();
                let key = NSString::from_str("ViewSkaterLastCrashLog");
                let value = NSString::from_str(&formatted_message);
                let _: () = msg_send![&*defaults, setObject: &*value forKey: &*key];
            }
        }
        
        // Exit after logging
        std::process::exit(128 + signal);
    }
    
    unsafe {
        libc::signal(libc::SIGSEGV, signal_handler as libc::sighandler_t);
        libc::signal(libc::SIGBUS, signal_handler as libc::sighandler_t);
        libc::signal(libc::SIGILL, signal_handler as libc::sighandler_t);
        libc::signal(libc::SIGFPE, signal_handler as libc::sighandler_t);
        libc::signal(libc::SIGABRT, signal_handler as libc::sighandler_t);
    }
}

#[cfg(not(unix))]
fn setup_signal_crash_handler() {
    // Signal handling not implemented for non-Unix platforms
}

pub fn main() -> Result<(), winit::error::EventLoopError> {
    // CRITICAL: Write to crash log IMMEDIATELY - before any other operations
    write_immediate_crash_log("MAIN: App startup initiated");
    
    // Set up signal handler FIRST to catch low-level crashes
    write_immediate_crash_log("MAIN: About to setup signal handler");
    setup_signal_crash_handler();
    write_immediate_crash_log("MAIN: Signal handler setup completed");
    
    // Set up stdout capture FIRST, before any println! statements
    write_immediate_crash_log("MAIN: About to setup stdout capture");
    let shared_stdout_buffer = file_io::setup_stdout_capture();
    set_shared_stdout_buffer(Arc::clone(&shared_stdout_buffer));
    write_immediate_crash_log("MAIN: Stdout capture setup completed");
    
    println!("ViewSkater starting...");
    write_immediate_crash_log("MAIN: ViewSkater starting message printed");
    
    // Set up panic hook to log to a file
    write_immediate_crash_log("MAIN: About to setup logger");
    let app_name = "viewskater";
    let shared_log_buffer = file_io::setup_logger(app_name);
    
    // Store the log buffer reference for global access
    set_shared_log_buffer(Arc::clone(&shared_log_buffer));
    write_immediate_crash_log("MAIN: Logger setup completed");
    
    write_immediate_crash_log("MAIN: About to setup panic hook");
    file_io::setup_panic_hook(app_name, shared_log_buffer);
    write_immediate_crash_log("MAIN: Panic hook setup completed");
    
    // Initialize winit FIRST
    let event_loop = EventLoop::<Action<Message>>::with_user_event()
        .build()
        .unwrap();

    // Set up the file channel AFTER winit initialization
    let (file_sender, file_receiver) = mpsc::channel();

    // Test crash debug logging immediately at startup
    write_crash_debug_log("========== VIEWSKATER STARTUP ==========");
    write_crash_debug_log("Testing crash debug logging system at startup");
    write_crash_debug_log(&format!("App version: {}", env!("CARGO_PKG_VERSION")));
    write_crash_debug_log(&format!("Timestamp: {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")));
    write_crash_debug_log("If you can see this message, crash debug logging is working");
    write_crash_debug_log("=========================================");
    
    // Test all logging methods comprehensively
    test_crash_logging_methods();

    // Register file handler BEFORE creating the runner
    // This is required on macOS so the app can receive file paths
    // when launched by opening a file (e.g. double-clicking in Finder)
    // or using "Open With". Must be set up early in app lifecycle.
    #[cfg(target_os = "macos")]
    {
        write_immediate_crash_log("MAIN: About to set file channel");
        macos_file_handler::set_file_channel(file_sender);
        write_immediate_crash_log("MAIN: File channel set");
        
        // NOTE: Automatic bookmark cleanup is DISABLED in production builds to avoid
        // wiping valid stored access. Use a special maintenance build or developer
        // tooling to invoke cleanup if ever needed.
        
        write_immediate_crash_log("MAIN: About to register file handler");
        macos_file_handler::register_file_handler();
        write_immediate_crash_log("MAIN: File handler registered");
        
        // Try to restore full disk access from previous session
        write_immediate_crash_log("MAIN: About to restore full disk access");
        debug!("🔍 Attempting to restore full disk access on startup");
        let restore_result = macos_file_handler::restore_full_disk_access();
        debug!("🔍 Restore full disk access result: {}", restore_result);
        write_immediate_crash_log(&format!("MAIN: Restore full disk access result: {}", restore_result));
        
        println!("macOS file handler registered");
        write_immediate_crash_log("MAIN: macOS file handler registration completed");
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
                                    #[cfg(target_os = "macos")]
                                    {
                                        // Clean up all active security-scoped access before shutdown
                                        macos_file_handler::cleanup_all_security_scoped_access();
                                    }
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
                                            #[cfg(target_os = "macos")]
                                            {
                                                // Clean up all active security-scoped access before shutdown
                                                macos_file_handler::cleanup_all_security_scoped_access();
                                            }
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

// ==================== CRASH DEBUG LOGGING ====================

/// Writes a crash debug log entry using multiple bulletproof methods for App Store sandbox
/// This ensures we can see what happened even if all file writing is blocked
pub fn write_crash_debug_log(message: &str) {
    // Simple immediate stderr logging
    let _ = std::panic::catch_unwind(|| {
        eprintln!("CRASH_DEBUG: {}", message);
    });
    
    // Simple immediate stdout logging  
    let _ = std::panic::catch_unwind(|| {
        println!("CRASH_DEBUG: {}", message);
    });
    
    // Simple NSUserDefaults logging
    #[cfg(target_os = "macos")]
    {
        use objc2_foundation::{NSUserDefaults, NSString};
        use objc2::{msg_send};
        
        unsafe {
            let defaults = NSUserDefaults::standardUserDefaults();
            let key = NSString::from_str("ViewSkaterLastCrashLog");
            let value = NSString::from_str(message);
            let _: () = msg_send![&*defaults, setObject: &*value forKey: &*key];
        }
    }
}

/// Writes crash debug info immediately to disk (synchronous, unbuffered)
/// This is specifically for crashes during "Open With" startup where console isn't available
pub fn write_immediate_crash_log(message: &str) {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    
    let formatted = format!("{} CRASH: {}\n", timestamp, message);
    
    // Use the same directory approach as file_io module
    let mut paths = Vec::new();
    
    // Primary location: Use dirs crate like file_io does
    let app_log_dir = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("viewskater")
        .join("logs");
    
    if std::fs::create_dir_all(&app_log_dir).is_ok() {
        paths.push(app_log_dir.join("crash.log"));
    }
    
    // Backup: Use cache directory  
    if let Some(cache_dir) = dirs::cache_dir() {
        let cache_log_dir = cache_dir.join("viewskater");
        if std::fs::create_dir_all(&cache_log_dir).is_ok() {
            paths.push(cache_log_dir.join("crash.log"));
        }
    }
    
    // Fallback: home directory
    if let Some(home) = dirs::home_dir() {
        paths.push(home.join("viewskater_crash.log"));
    }
    
    // Emergency fallback: /tmp
    paths.push("/tmp/viewskater_crash.log".into());
    
    // Write to all available locations with MAXIMUM reliability
    for path in &paths {
        // Create options with immediate disk writes on Unix systems
        let mut options = std::fs::OpenOptions::new();
        options.create(true).append(true);
        
        #[cfg(unix)]
        {
            options.custom_flags(0x80); // O_SYNC on Unix - immediate disk writes
        }
        
        if let Ok(mut file) = options.open(path) {
            let _ = file.write_all(formatted.as_bytes());
            let _ = file.sync_all(); // Force filesystem sync
            let _ = file.sync_data(); // Force data sync (faster than sync_all)
            // Don't close - let it drop naturally to avoid blocking
        }
    }
    
    // ALSO write to NSUserDefaults immediately as backup
    #[cfg(target_os = "macos")]
    {
        let _ = std::panic::catch_unwind(|| {
            use objc2_foundation::{NSUserDefaults, NSString};
            use objc2::{msg_send};
            
            unsafe {
                let defaults = NSUserDefaults::standardUserDefaults();
                let key = NSString::from_str("ViewSkaterImmediateCrashLog");
                let value = NSString::from_str(&formatted);
                let _: () = msg_send![&*defaults, setObject: &*value forKey: &*key];
                let _: bool = msg_send![&*defaults, synchronize];
            }
        });
    }
}

// ==================== END CRASH DEBUG LOGGING ====================

/// Retrieves crash debug logs from NSUserDefaults (bulletproof storage) - SIMPLIFIED VERSION
/// This allows accessing logs even if file writing was blocked by App Store sandbox
#[cfg(target_os = "macos")]
pub fn get_crash_debug_logs_from_userdefaults() -> Vec<String> {
    use objc2_foundation::{NSUserDefaults, NSString};
    use objc2::{msg_send};
    use objc2::rc::autoreleasepool;
    
    autoreleasepool(|pool| unsafe {
        let mut results = Vec::new();
        
        let defaults = NSUserDefaults::standardUserDefaults();
        
        // Get the crash counter
        let counter_key = NSString::from_str("ViewSkaterCrashCounter");
        let crash_count: i64 = msg_send![&*defaults, integerForKey: &*counter_key];
        results.push(format!("CRASH_COUNTER: {} crashes detected", crash_count));
        
        // Get the last crash log
        let log_key = NSString::from_str("ViewSkaterLastCrashLog");
        let last_log: *mut objc2::runtime::AnyObject = msg_send![&*defaults, objectForKey: &*log_key];
        
        if !last_log.is_null() {
            let log_nsstring = &*(last_log as *const NSString);
            let log_str = log_nsstring.as_str(pool).to_owned();
            results.push(format!("LAST_CRASH_LOG: {}", log_str));
        } else {
            results.push("LAST_CRASH_LOG: No crash log found".to_string());
        }
        
        results
    })
}

/// Export crash debug logs from NSUserDefaults to a file (non-macOS fallback)
#[cfg(not(target_os = "macos"))]
pub fn get_crash_debug_logs_from_userdefaults() -> Vec<String> {
    Vec::new() // Not supported on non-macOS
}

#[cfg(target_os = "macos")]
pub mod macos_file_handler {
    use std::sync::mpsc::Sender;
    use std::sync::Mutex;
    use std::collections::HashMap;
    use objc2::rc::autoreleasepool;
    use objc2::{msg_send, sel};
    use objc2::declare::ClassBuilder;
    use objc2::runtime::{AnyObject, Sel, AnyClass};
    use objc2_app_kit::{NSApplication, NSModalResponse, NSModalResponseOK};
    use objc2_foundation::{MainThreadMarker, NSArray, NSString, NSDictionary, NSUserDefaults, NSURL, NSData};
    use objc2::rc::Retained;
    use once_cell::sync::Lazy;
    use std::io::Write;
    
    #[allow(unused_imports)]
    use log::{debug, info, warn, error};

    static mut FILE_CHANNEL: Option<Sender<String>> = None;
    
    // Store security-scoped URLs globally for session access  
    // FIXED: Store both the URL and whether it has active security scope
    #[derive(Clone, Debug)]
    struct SecurityScopedURLInfo {
        url: Retained<NSURL>,
        has_active_scope: bool,
        original_path: String,
    }
    
    static SECURITY_SCOPED_URLS: Lazy<Mutex<HashMap<String, SecurityScopedURLInfo>>> = 
        Lazy::new(|| Mutex::new(HashMap::new()));

    // Constants for security-scoped bookmarks
    const NSURL_BOOKMARK_CREATION_WITH_SECURITY_SCOPE: u64 = 1 << 11;  // 0x800
    const NSURL_BOOKMARK_RESOLUTION_WITH_SECURITY_SCOPE: u64 = 1 << 8;  // 0x100
    
    // ENABLED: Re-enable bookmark restoration after cleanup
    const DISABLE_BOOKMARK_RESTORATION: bool = false;
    
    // ENABLED: Re-enable bookmark creation after implementing safer methods
    const DISABLE_BOOKMARK_CREATION: bool = false;

    // ==================== CRASH DEBUG LOGGING ====================
    
    /// Writes a crash debug log entry immediately to disk (not buffered)
    /// This ensures we can see what happened even if the process crashes immediately after
    fn write_crash_debug_log(message: &str) {
        // Use the public function from the parent module
        crate::write_crash_debug_log(message);
    }
    
    /// Build stable UserDefaults keys for storing bookmarks
    /// Modern: uses full absolute path to avoid collisions and truncation
    /// Legacy: previous sanitized/truncated format for backward compatibility
    fn make_bookmark_keys(directory_path: &str) -> (
        Retained<NSString>,
        Retained<NSString>,
    ) {
        // Modern key retains full path
        let modern_key = format!("VSBookmark|{}", directory_path);
        let modern_ns = NSString::from_str(&modern_key);
        
        // Legacy key: first 50 alnum/_ chars
        let legacy_simple: String = directory_path
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_')
            .take(50)
            .collect();
        let legacy_key = format!("VSBookmark_{}", legacy_simple);
        let legacy_ns = NSString::from_str(&legacy_key);
        
        (modern_ns, legacy_ns)
    }
    
    // ==================== END CRASH DEBUG LOGGING ====================

    pub fn set_file_channel(sender: Sender<String>) {
        debug!("Setting file channel for macOS file handler");
        unsafe {
            FILE_CHANNEL = Some(sender);
        }
    }

    /// Stores a security-scoped URL for session access
    /// FIXED: Store URL info with active scope status
    fn store_security_scoped_url(path: &str, url: Retained<NSURL>) {
        debug!("Storing security-scoped URL for path: {}", path);
        
        let info = SecurityScopedURLInfo {
            url: url.clone(),
            has_active_scope: true,  // Assume it has active scope when stored
            original_path: path.to_string(),
        };
        
        if let Ok(mut urls) = SECURITY_SCOPED_URLS.lock() {
            urls.insert(path.to_string(), info);
            debug!("Stored security-scoped URL (total count: {})", urls.len());
        } else {
            error!("Failed to lock security-scoped URLs mutex");
        }
    }

    /// FIXED: Get the actual resolved path from the security-scoped URL
    pub fn get_security_scoped_path(original_path: &str) -> Option<String> {
        if let Ok(urls) = SECURITY_SCOPED_URLS.lock() {
            if let Some(info) = urls.get(original_path) {
                if info.has_active_scope {
                    // Get the actual path from the resolved URL
                    autoreleasepool(|pool| unsafe {
                        if let Some(path_nsstring) = info.url.path() {
                            let resolved_path = path_nsstring.as_str(pool);
                            debug!("Resolved security-scoped path: {} -> {}", original_path, resolved_path);
                            Some(resolved_path.to_string())
                        } else {
                            debug!("No path available from security-scoped URL for: {}", original_path);
                            None
                        }
                    })
                } else {
                    debug!("Security-scoped URL exists but scope is not active for: {}", original_path);
                    None
                }
            } else {
                debug!("No security-scoped URL found for: {}", original_path);
                None
            }
        } else {
            error!("Failed to lock security-scoped URLs mutex");
            None
        }
    }

    /// Checks if we have security-scoped access to a path
    pub fn has_security_scoped_access(path: &str) -> bool {
        if let Ok(urls) = SECURITY_SCOPED_URLS.lock() {
            if let Some(info) = urls.get(path) {
                info.has_active_scope
            } else {
                false
            }
        } else {
            false
        }
    }

    /// FIXED: Stop security-scoped access for a path
    pub fn stop_security_scoped_access(path: &str) {
        if let Ok(mut urls) = SECURITY_SCOPED_URLS.lock() {
            if let Some(mut info) = urls.get_mut(path) {
                if info.has_active_scope {
                    unsafe {
                        let _: () = msg_send![&*info.url, stopAccessingSecurityScopedResource];
                        info.has_active_scope = false;
                        debug!("Stopped security-scoped access for: {}", path);
                    }
                }
            }
        }
    }

    /// Gets all accessible paths for debugging
    pub fn get_accessible_paths() -> Vec<String> {
        if let Ok(urls) = SECURITY_SCOPED_URLS.lock() {
            urls.keys().cloned().collect()
        } else {
            Vec::new()
        }
    }
    
    /// Clean up all active security-scoped access (call on app shutdown)
    /// ADDED: Proper lifecycle management
    pub fn cleanup_all_security_scoped_access() {
        debug!("Cleaning up all active security-scoped access");
        
        if let Ok(mut urls) = SECURITY_SCOPED_URLS.lock() {
            let mut stopped_count = 0;
            for (path, info) in urls.iter_mut() {
                if info.has_active_scope {
                    unsafe {
                        let _: () = msg_send![&*info.url, stopAccessingSecurityScopedResource];
                        info.has_active_scope = false;
                        stopped_count += 1;
                        debug!("Stopped security-scoped access for: {}", path);
                    }
                }
            }
            debug!("Cleaned up {} active security-scoped URLs", stopped_count);
        } else {
            error!("Failed to lock security-scoped URLs mutex during cleanup");
        }
    }

    /// Creates a security-scoped bookmark from a security-scoped URL and stores it persistently
    /// FIXED: Simplified and corrected implementation following Apple's documented pattern
    fn create_and_store_security_scoped_bookmark(url: &Retained<NSURL>, directory_path: &str) -> bool {
        if DISABLE_BOOKMARK_CREATION {
            eprintln!("BOOKMARK_CREATE_FIXED: disabled - skipping");
            return true;
        }
        
        write_crash_debug_log(&format!("BOOKMARK_CREATE_FIXED: Starting for path: {}", directory_path));
        debug!("Creating security-scoped bookmark for: {}", directory_path);
        
        let result = autoreleasepool(|_pool| unsafe {
            write_crash_debug_log("BOOKMARK_CREATE_FIXED: Entered autoreleasepool");
            
            // Validate input path
            if directory_path.is_empty() || directory_path.len() > 500 {
                write_crash_debug_log("BOOKMARK_CREATE_FIXED: ERROR - invalid directory path");
                return false;
            }
            
            // Create bookmark data from the security-scoped URL (from NSOpenPanel)
            let mut error: *mut AnyObject = std::ptr::null_mut();
            
            write_crash_debug_log("BOOKMARK_CREATE_FIXED: About to create bookmark data from NSOpenPanel URL");
            let bookmark_data: *mut AnyObject = msg_send![
                &**url,
                bookmarkDataWithOptions: NSURL_BOOKMARK_CREATION_WITH_SECURITY_SCOPE
                includingResourceValuesForKeys: std::ptr::null::<AnyObject>()
                relativeToURL: std::ptr::null::<AnyObject>()
                error: &mut error
            ];
            
            // Check for errors
            if !error.is_null() {
                write_crash_debug_log("BOOKMARK_CREATE_FIXED: ERROR - bookmark creation failed");
                return false;
            }
            
            if bookmark_data.is_null() {
                write_crash_debug_log("BOOKMARK_CREATE_FIXED: ERROR - bookmark data is null");
                return false;
            }
            
            // Verify it's NSData
            let nsdata_class = objc2::runtime::AnyClass::get("NSData").unwrap();
            let is_nsdata: bool = msg_send![bookmark_data, isKindOfClass: nsdata_class];
            
            if !is_nsdata {
                write_crash_debug_log("BOOKMARK_CREATE_FIXED: ERROR - bookmark data is not NSData");
                return false;
            }
            
            write_crash_debug_log("BOOKMARK_CREATE_FIXED: Bookmark data created successfully");
            
            // Store in NSUserDefaults with modern key (and legacy for back-compat)
            let defaults = NSUserDefaults::standardUserDefaults();
            let (modern_key, legacy_key) = make_bookmark_keys(directory_path);
            
            write_crash_debug_log("BOOKMARK_CREATE_FIXED: About to store in NSUserDefaults");
            let _: () = msg_send![&*defaults, setObject: bookmark_data forKey: &*modern_key];
            // Also store legacy key for back-compat migration
            let _: () = msg_send![&*defaults, setObject: bookmark_data forKey: &*legacy_key];
            
            // Synchronize to ensure it's persisted
            let sync_ok: bool = msg_send![&*defaults, synchronize];
            if sync_ok {
                write_crash_debug_log("BOOKMARK_CREATE_FIXED: SUCCESS - bookmark stored and synchronized");
                debug!("Successfully stored security-scoped bookmark");
                true
            } else {
                write_crash_debug_log("BOOKMARK_CREATE_FIXED: ERROR - failed to synchronize");
                false
            }
        });
        
        write_crash_debug_log(&format!("BOOKMARK_CREATE_FIXED: Final result: {}", result));
        result
    }
    
    /// Resolves a stored security-scoped bookmark back into a security-scoped URL
    /// FIXED: Proper implementation following Apple's documented pattern with lifecycle management
    fn resolve_security_scoped_bookmark(directory_path: &str) -> Option<Retained<NSURL>> {
        if DISABLE_BOOKMARK_RESTORATION {
            eprintln!("RESOLVE_FIXED: disabled - skipping");
            return None;
        }
        
        eprintln!("RESOLVE_FIXED: Starting for path: {}", directory_path);
        debug!("Resolving security-scoped bookmark for: {}", directory_path);
        
        let result = autoreleasepool(|_pool| unsafe {
            eprintln!("RESOLVE_FIXED: Entered autoreleasepool");
            
            // Validate input
            if directory_path.is_empty() || directory_path.len() > 500 {
                eprintln!("RESOLVE_FIXED: ERROR - invalid path");
                return None;
            }
            
            let defaults = NSUserDefaults::standardUserDefaults();
            
            // Build keys and try modern first, then legacy
            let (modern_key, legacy_key) = make_bookmark_keys(directory_path);
            eprintln!("RESOLVE_FIXED: Looking for modern key");
            let mut bookmark_data: *mut AnyObject = msg_send![&*defaults, objectForKey: &*modern_key];
            if bookmark_data.is_null() {
                eprintln!("RESOLVE_FIXED: Modern key not found, trying legacy");
                bookmark_data = msg_send![&*defaults, objectForKey: &*legacy_key];
            }
            
            if bookmark_data.is_null() {
                eprintln!("RESOLVE_FIXED: No bookmark found");
                return None;
            }
            
            // Verify it's NSData
            let nsdata_class = objc2::runtime::AnyClass::get("NSData").unwrap();
            let is_nsdata: bool = msg_send![bookmark_data, isKindOfClass: nsdata_class];
            
            if !is_nsdata {
                eprintln!("RESOLVE_FIXED: ERROR - not NSData, cleaning up");
                let _: () = msg_send![&*defaults, removeObjectForKey: &*modern_key];
                let _: () = msg_send![&*defaults, removeObjectForKey: &*legacy_key];
                return None;
            }
            
            eprintln!("RESOLVE_FIXED: Found valid bookmark data");
            
            // CRITICAL: Resolve bookmark to get NEW security-scoped URL instance
            let mut is_stale: objc2::runtime::Bool = objc2::runtime::Bool::new(false);
            let mut error: *mut AnyObject = std::ptr::null_mut();
            
            eprintln!("RESOLVE_FIXED: About to resolve bookmark data to NEW URL instance");
            let resolved_url: *mut AnyObject = msg_send![
                objc2::runtime::AnyClass::get("NSURL").unwrap(),
                URLByResolvingBookmarkData: bookmark_data
                options: NSURL_BOOKMARK_RESOLUTION_WITH_SECURITY_SCOPE
                relativeToURL: std::ptr::null::<AnyObject>()
                bookmarkDataIsStale: &mut is_stale
                error: &mut error
            ];
            
            if !error.is_null() {
                eprintln!("RESOLVE_FIXED: ERROR - bookmark resolution failed, removing stale bookmark");
                let _: () = msg_send![&*defaults, removeObjectForKey: &*modern_key];
                let _: () = msg_send![&*defaults, removeObjectForKey: &*legacy_key];
                return None;
            }
            
            if resolved_url.is_null() {
                eprintln!("RESOLVE_FIXED: ERROR - resolved URL is null, removing bookmark");
                let _: () = msg_send![&*defaults, removeObjectForKey: &*modern_key];
                let _: () = msg_send![&*defaults, removeObjectForKey: &*legacy_key];
                return None;
            }
            
            // Verify it's an NSURL
            let nsurl_class = objc2::runtime::AnyClass::get("NSURL").unwrap();
            let is_nsurl: bool = msg_send![resolved_url, isKindOfClass: nsurl_class];
            
            if !is_nsurl {
                eprintln!("RESOLVE_FIXED: ERROR - resolved object is not NSURL");
                return None;
            }
            
            eprintln!("RESOLVE_FIXED: Successfully resolved bookmark to security-scoped URL");
            
            // CRITICAL: Call startAccessingSecurityScopedResource on the RESOLVED URL instance
            eprintln!("RESOLVE_FIXED: About to start accessing security-scoped resource on RESOLVED URL");
            let access_granted: bool = msg_send![resolved_url, startAccessingSecurityScopedResource];
            eprintln!("RESOLVE_FIXED: Security access result: {}", access_granted);
            
            if access_granted {
                // Handle stale bookmarks by refreshing them
                if is_stale.as_bool() {
                    eprintln!("RESOLVE_FIXED: Bookmark is stale, refreshing it");
                    // Create fresh bookmark from the resolved URL
                    let fresh_bookmark_result: *mut AnyObject = msg_send![
                        resolved_url,
                        bookmarkDataWithOptions: NSURL_BOOKMARK_CREATION_WITH_SECURITY_SCOPE
                        includingResourceValuesForKeys: std::ptr::null::<AnyObject>()
                        relativeToURL: std::ptr::null::<AnyObject>()
                        error: std::ptr::null_mut::<*mut AnyObject>()
                    ];
                    
                    if !fresh_bookmark_result.is_null() {
                        eprintln!("RESOLVE_FIXED: Created fresh bookmark, storing it");
                        let _: () = msg_send![&*defaults, setObject: fresh_bookmark_result forKey: &*modern_key];
                        let _: () = msg_send![&*defaults, setObject: fresh_bookmark_result forKey: &*legacy_key];
                        let _: bool = msg_send![&*defaults, synchronize];
                        eprintln!("RESOLVE_FIXED: Fresh bookmark stored");
                    } else {
                        eprintln!("RESOLVE_FIXED: WARNING - failed to create fresh bookmark");
                    }
                }
                
                // Create Retained<NSURL> from the resolved URL (which already has security scope)
                // We need to retain it properly since we're taking ownership
                let _: *mut AnyObject = msg_send![resolved_url, retain];
                let nsurl_ptr = resolved_url as *mut NSURL;
                
                if let Some(retained_url) = Retained::from_raw(nsurl_ptr) {
                    eprintln!("RESOLVE_FIXED: SUCCESS - created Retained<NSURL> from resolved security-scoped URL");
                    
                    // Store the resolved URL for session use
                    store_security_scoped_url(directory_path, retained_url.clone());
                    debug!("Successfully restored directory access from bookmark");
                    Some(retained_url)
                } else {
                    eprintln!("RESOLVE_FIXED: ERROR - failed to create Retained<NSURL>");
                    // Clean up - stop accessing the resource since we can't return it
                    let _: () = msg_send![resolved_url, stopAccessingSecurityScopedResource];
                    None
                }
            } else {
                eprintln!("RESOLVE_FIXED: ERROR - failed to start accessing security-scoped resource");
                debug!("Failed to start accessing restored security-scoped resource");
                None
            }
        });
        
        match &result {
            Some(_) => eprintln!("RESOLVE_FIXED: FINAL SUCCESS"),
            None => eprintln!("RESOLVE_FIXED: FINAL FAILURE"),
        }
        
        result
    }
    
    /// Public function to restore directory access for a specific path using stored bookmarks
    /// This is called when the app needs to regain access to a previously granted directory
    pub fn restore_directory_access_for_path(directory_path: &str) -> bool {
        debug!("Restoring directory access for path: {}", directory_path);
        
        // Now enabled - try to restore from bookmark
        match resolve_security_scoped_bookmark(directory_path) {
            Some(_url) => {
                debug!("Successfully restored directory access via bookmark");
                true
            }
            None => {
                debug!("Failed to restore directory access via bookmark");
                false
            }
        }
    }
    
    /// Normalizes a directory path for use as a UserDefaults key - SIMPLIFIED VERSION
    fn normalize_path_for_key(path: &str) -> String {
        // Create a simple, safe key from the directory path
        path.chars()
            .filter(|c| c.is_alphanumeric() || *c == '_')
            .take(50)
            .collect::<String>()
    }

    /// Requests directory access via NSOpenPanel and creates persistent bookmark
    /// FIXED: Proper handling of NSOpenPanel security-scoped URLs
    fn request_directory_access_with_nsopenpanel(requested_path: &str) -> bool {
        eprintln!("PANEL_FIXED: Starting for path: {}", requested_path);
        debug!("Requesting directory access via NSOpenPanel for: {}", requested_path);
        
        let result = autoreleasepool(|_pool| unsafe {
            eprintln!("PANEL_FIXED: Entered autoreleasepool");
            
            let mtm = MainThreadMarker::new().expect("Must be on main thread");
            eprintln!("PANEL_FIXED: Main thread marker created");
                
            // Create NSOpenPanel
            eprintln!("PANEL_FIXED: Getting NSOpenPanel class");
            let panel_class = objc2::runtime::AnyClass::get("NSOpenPanel").expect("NSOpenPanel class not found");
            eprintln!("PANEL_FIXED: Creating NSOpenPanel instance");
            let panel: *mut AnyObject = msg_send![panel_class, openPanel];
            eprintln!("PANEL_FIXED: NSOpenPanel created");
                
            // Configure panel for directory selection
            eprintln!("PANEL_FIXED: Configuring panel");
            let _: () = msg_send![panel, setCanChooseDirectories: true];
            let _: () = msg_send![panel, setCanChooseFiles: false];
            let _: () = msg_send![panel, setAllowsMultipleSelection: false];
            let _: () = msg_send![panel, setCanCreateDirectories: false];
            
            // Set initial directory to the requested path's parent if possible
            if let Some(parent_dir) = std::path::Path::new(requested_path).parent() {
                eprintln!("PANEL_FIXED: Setting initial directory");
                let parent_str = parent_dir.to_string_lossy();
                let parent_nsstring = NSString::from_str(&parent_str);
                let parent_url = NSURL::fileURLWithPath(&parent_nsstring);
                let _: () = msg_send![panel, setDirectoryURL: &*parent_url];
                eprintln!("PANEL_FIXED: Initial directory set");
            }
                
            // Set dialog title and message
            eprintln!("PANEL_FIXED: Setting panel text");
            let title = NSString::from_str("Grant Directory Access");
            let _: () = msg_send![panel, setTitle: &*title];
                
            let message = NSString::from_str(&format!(
                "ViewSkater needs access to browse images in this directory:\n\n{}\n\nPlease select the directory to grant persistent access.",
                requested_path
            ));
            let _: () = msg_send![panel, setMessage: &*message];
                
            // Show the panel and get user response
            eprintln!("PANEL_FIXED: About to show modal");
            debug!("Showing NSOpenPanel...");
            let response: NSModalResponse = msg_send![panel, runModal];
            eprintln!("PANEL_FIXED: Modal completed with response: {:?}", response as i32);
                
            if response == NSModalResponseOK {
                eprintln!("PANEL_FIXED: User granted access");
                debug!("User granted directory access via NSOpenPanel");
                
                // Get the selected URLs array
                eprintln!("PANEL_FIXED: Getting selected URLs");
                let selected_urls: *mut AnyObject = msg_send![panel, URLs];
                
                if selected_urls.is_null() {
                    eprintln!("PANEL_FIXED: ERROR - URLs array is null");
                    return false;
                }
                
                // Cast to NSArray and get first URL
                let urls_array = &*(selected_urls as *const NSArray<NSURL>);
                if urls_array.len() == 0 {
                    eprintln!("PANEL_FIXED: ERROR - URLs array is empty");
                    return false;
                }
                
                let selected_url = &urls_array[0];
                
                // Get the path string
                if let Some(path_nsstring) = selected_url.path() {
                    let selected_path = path_nsstring.as_str(_pool);
                    eprintln!("PANEL_FIXED: Selected path: '{}'", selected_path);
                    debug!("Selected directory: {}", selected_path);
                    
                    // IMPORTANT: The URL from NSOpenPanel is already security-scoped for this session
                    // We don't need to call startAccessingSecurityScopedResource() now
                    eprintln!("PANEL_FIXED: URL from NSOpenPanel is already security-scoped for this session");
                    
                    // Convert &NSURL to Retained<NSURL>
                    let _: *mut AnyObject = msg_send![selected_url, retain];
                    let retained_url = Retained::from_raw(selected_url as *const NSURL as *mut NSURL).unwrap();
                    
                    // Store the URL for immediate session use
                    store_security_scoped_url(selected_path, retained_url.clone());
                    eprintln!("PANEL_FIXED: URL stored for session use");
                    
                    // Create and store persistent bookmark for future sessions
                    eprintln!("PANEL_FIXED: About to create persistent bookmark");
                    if create_and_store_security_scoped_bookmark(&retained_url, selected_path) {
                        eprintln!("PANEL_FIXED: SUCCESS - bookmark created and stored");
                        debug!("Successfully created persistent bookmark");
                        true
                    } else {
                        eprintln!("PANEL_FIXED: WARNING - bookmark creation failed, but have session access");
                        debug!("Failed to create persistent bookmark, but have session access");
                        true // Still have temporary access for this session
                    }
                } else {
                    eprintln!("PANEL_FIXED: ERROR - selected URL has no path");
                    debug!("No path returned from selected URL");
                    false
                }
            } else {
                eprintln!("PANEL_FIXED: User cancelled");
                debug!("User cancelled NSOpenPanel");
                false
            }
        });
        
        eprintln!("PANEL_FIXED: Final result: {}", result);
        result
    }

    /// Attempts to read a directory using existing security-scoped access
    /// FIXED: Use the resolved URL path for file operations
    pub fn read_directory_with_security_scoped_access(path: &str) -> Option<Result<std::fs::ReadDir, std::io::Error>> {
        debug!("Attempting to read directory with existing security-scoped access: {}", path);
        
        // First, try to restore access from stored bookmarks
        if !has_security_scoped_access(path) {
            debug!("No session access found, attempting to restore from bookmark");
            restore_directory_access_for_path(path);
        }
        
        // Check if we now have access after restoration attempt
        if has_security_scoped_access(path) {
            debug!("Have security-scoped access, attempting directory read");
            
            // CRITICAL FIX: Use the resolved URL path, not the original path
            if let Some(resolved_path) = get_security_scoped_path(path) {
                debug!("Using resolved security-scoped path: {}", resolved_path);
                Some(std::fs::read_dir(resolved_path))
            } else {
                error!("Failed to get resolved path from security-scoped URL for: {}", path);
                Some(std::fs::read_dir(path)) // Fallback to original path
            }
        } else {
            debug!("No security-scoped access available for directory read");
            
            // Check for "Open With" scenario - individual file access within this directory
            if let Ok(urls) = SECURITY_SCOPED_URLS.lock() {
                let has_file_in_directory = urls.keys().any(|key| {
                    let key_path = std::path::Path::new(key);
                    if key_path.is_file() {
                        if let Some(file_parent) = key_path.parent() {
                            return file_parent.to_string_lossy() == path;
                        }
                    }
                    false
                });
                
                if has_file_in_directory {
                    debug!("Detected 'Open With' scenario - requesting directory access");
                    drop(urls); // Release lock before calling request function
                    
                    if request_directory_access_with_nsopenpanel(path) {
                        debug!("Directory access granted, retrying read");
                        // After granting access, use the resolved path
                        if let Some(resolved_path) = get_security_scoped_path(path) {
                            debug!("Using newly resolved security-scoped path: {}", resolved_path);
                            return Some(std::fs::read_dir(resolved_path));
                        } else {
                            return Some(std::fs::read_dir(path)); // Fallback
                        }
                    }
                }
            }
            
            None
        }
    }

    /// Helper function to request parent directory access for a file
    pub fn request_parent_directory_permission_dialog(file_path: &str) -> bool {
        debug!("🔍 Requesting parent directory access for file: {}", file_path);
        
        if let Some(parent_dir) = std::path::Path::new(file_path).parent() {
            let parent_dir_str = parent_dir.to_string_lossy();
            request_directory_access_with_nsopenpanel(&parent_dir_str)
        } else {
            debug!("Could not determine parent directory for: {}", file_path);
            false
        }
    }

    /// Placeholder for full disk access - in a sandboxed environment, we use directory-specific access
    pub fn restore_full_disk_access() -> bool {
        debug!("🔍 restore_full_disk_access() called - deferring to directory-specific restoration");
        false // We handle restoration per-directory via restore_directory_access_for_path
    }

    /// Check if we have full disk access (simplified check)
    pub fn has_full_disk_access() -> bool {
        // Try to read a protected directory
        if let Some(home_dir) = dirs::home_dir() {
            let desktop_dir = home_dir.join("Desktop");
            match std::fs::read_dir(&desktop_dir) {
                Ok(_) => {
                    debug!("✅ Full disk access confirmed");
                    true
                }
                Err(_) => {
                    debug!("❌ No full disk access");
                    false
                }
            }
        } else {
            false
        }
    }

    /// Simplified full disk access request - actually requests directory access
    pub fn request_full_disk_access_once() -> bool {
        debug!("🔍 request_full_disk_access_once() - using directory access instead");
        
        // In a sandboxed environment, we can't get true "full disk access"
        // Instead, we request access to the user's home directory as a reasonable default
        if let Some(home_dir) = dirs::home_dir() {
            let home_path = home_dir.to_string_lossy();
            request_directory_access_with_nsopenpanel(&home_path)
        } else {
            false
        }
    }

    /// Handle opening a file via "Open With" from Finder
    unsafe extern "C" fn handle_opened_file(
        _this: &mut AnyObject,
        _sel: Sel,
        _sender: &AnyObject,
        files: &NSArray<NSString>,
    ) {
        crate::write_immediate_crash_log("HANDLE_OPENED_FILE: Function entry");
        write_crash_debug_log("FINDER_OPEN: handle_opened_file called");
        debug!("handle_opened_file called with {} files", files.len());
        
        if files.is_empty() {
            crate::write_immediate_crash_log("HANDLE_OPENED_FILE: Empty files array");
            write_crash_debug_log("FINDER_OPEN: Empty files array received");
            debug!("Empty files array received");
            return;
        }
        
        crate::write_immediate_crash_log(&format!("HANDLE_OPENED_FILE: Processing {} files", files.len()));
        write_crash_debug_log(&format!("FINDER_OPEN: Processing {} files", files.len()));
        
        autoreleasepool(|pool| {
            crate::write_immediate_crash_log("HANDLE_OPENED_FILE: Entered autoreleasepool");
            write_crash_debug_log("FINDER_OPEN: Entered autoreleasepool");
            
            for (i, file) in files.iter().enumerate() {
                crate::write_immediate_crash_log(&format!("HANDLE_OPENED_FILE: Processing file {} of {}", i + 1, files.len()));
                write_crash_debug_log(&format!("FINDER_OPEN: Processing file {} of {}", i + 1, files.len()));
                
                let path = file.as_str(pool).to_owned();
                crate::write_immediate_crash_log(&format!("HANDLE_OPENED_FILE: File path: {}", path));
                debug!("Processing file: {}", path);
                write_crash_debug_log(&format!("FINDER_OPEN: File path: {}", path));
                
                crate::write_immediate_crash_log("HANDLE_OPENED_FILE: About to create NSURL");
                write_crash_debug_log("FINDER_OPEN: About to create NSURL");
                // Create NSURL and try to get security-scoped access
                let url = NSURL::fileURLWithPath(&file);
                crate::write_immediate_crash_log("HANDLE_OPENED_FILE: NSURL created");
                write_crash_debug_log("FINDER_OPEN: NSURL created, about to call startAccessingSecurityScopedResource");
                let file_accessed: bool = msg_send![&*url, startAccessingSecurityScopedResource];
                crate::write_immediate_crash_log(&format!("HANDLE_OPENED_FILE: Security access result: {}", file_accessed));
                write_crash_debug_log(&format!("FINDER_OPEN: Security access result: {}", file_accessed));
                
                if file_accessed {
                    crate::write_immediate_crash_log("HANDLE_OPENED_FILE: Security access granted");
                    debug!("Gained security-scoped access to file: {}", path);
                    
                    crate::write_immediate_crash_log("HANDLE_OPENED_FILE: About to store file URL");
                    write_crash_debug_log("FINDER_OPEN: About to store file URL");
                    // Store the file URL
                    store_security_scoped_url(&path, url.clone());
                    crate::write_immediate_crash_log("HANDLE_OPENED_FILE: File URL stored");
                    write_crash_debug_log("FINDER_OPEN: File URL stored successfully");
                    
                    crate::write_immediate_crash_log("HANDLE_OPENED_FILE: About to get parent directory");
                    write_crash_debug_log("FINDER_OPEN: About to get parent directory");
                    // Try to get parent directory access
                    if let Some(parent_url) = url.URLByDeletingLastPathComponent() {
                        crate::write_immediate_crash_log("HANDLE_OPENED_FILE: Got parent URL");
                        write_crash_debug_log("FINDER_OPEN: Got parent URL, about to get path");
                        if let Some(parent_path) = parent_url.path() {
                            let parent_path_str = parent_path.as_str(pool).to_owned();
                            crate::write_immediate_crash_log(&format!("HANDLE_OPENED_FILE: Parent directory: {}", parent_path_str));
                            debug!("Checking parent directory: {}", parent_path_str);
                            write_crash_debug_log(&format!("FINDER_OPEN: Parent directory: {}", parent_path_str));
                            
                            crate::write_immediate_crash_log("HANDLE_OPENED_FILE: About to test directory access");
                            write_crash_debug_log("FINDER_OPEN: About to test directory access");
                            // Test if we already have directory access
                            match std::fs::read_dir(&parent_path_str) {
                                Ok(_) => {
                                    crate::write_immediate_crash_log("HANDLE_OPENED_FILE: Have directory access");
                                    debug!("Already have parent directory access");
                                    write_crash_debug_log("FINDER_OPEN: Have directory access, storing parent URL");
                                    store_security_scoped_url(&parent_path_str, parent_url.clone());
                                    crate::write_immediate_crash_log("HANDLE_OPENED_FILE: Parent URL stored");
                                    write_crash_debug_log("FINDER_OPEN: Parent URL stored successfully");
                                }
                                Err(_) => {
                                    crate::write_immediate_crash_log("HANDLE_OPENED_FILE: No directory access");
                                    debug!("No parent directory access - will restore from bookmark if available");
                                    write_crash_debug_log("FINDER_OPEN: No directory access - bookmark restoration needed");
                                }
                            }
                        } else {
                            crate::write_immediate_crash_log("HANDLE_OPENED_FILE: Parent URL has no path");
                            write_crash_debug_log("FINDER_OPEN: Parent URL has no path");
                        }
                    } else {
                        crate::write_immediate_crash_log("HANDLE_OPENED_FILE: Could not get parent URL");
                        write_crash_debug_log("FINDER_OPEN: Could not get parent URL");
                    }
                    
                    crate::write_immediate_crash_log("HANDLE_OPENED_FILE: About to send file path to main thread");
                    write_crash_debug_log("FINDER_OPEN: About to send file path to main thread");
                    // Send file path to main app
                    if let Some(ref sender) = FILE_CHANNEL {
                        match sender.send(path.clone()) {
                            Ok(_) => {
                                crate::write_immediate_crash_log("HANDLE_OPENED_FILE: Successfully sent to main thread");
                                debug!("Successfully sent file path to main thread");
                                write_crash_debug_log("FINDER_OPEN: Successfully sent to main thread");
                            },
                            Err(e) => {
                                crate::write_immediate_crash_log(&format!("HANDLE_OPENED_FILE: Failed to send: {}", e));
                                error!("Failed to send file path: {}", e);
                                write_crash_debug_log(&format!("FINDER_OPEN: Failed to send: {}", e));
                            },
                        }
                    } else {
                        crate::write_immediate_crash_log("HANDLE_OPENED_FILE: FILE_CHANNEL is None");
                        write_crash_debug_log("FINDER_OPEN: FILE_CHANNEL is None");
                    }
                } else {
                    crate::write_immediate_crash_log(&format!("HANDLE_OPENED_FILE: Failed security access for: {}", path));
                    debug!("Failed to get security-scoped access for file: {}", path);
                    write_crash_debug_log(&format!("FINDER_OPEN: Failed security access for: {}", path));
                }
                
                crate::write_immediate_crash_log(&format!("HANDLE_OPENED_FILE: Completed file {} of {}", i + 1, files.len()));
                write_crash_debug_log(&format!("FINDER_OPEN: Completed file {} of {}", i + 1, files.len()));
            }
            
            crate::write_immediate_crash_log("HANDLE_OPENED_FILE: About to exit autoreleasepool");
            write_crash_debug_log("FINDER_OPEN: About to exit autoreleasepool");
        });
        
        crate::write_immediate_crash_log("HANDLE_OPENED_FILE: Function completed successfully");
        write_crash_debug_log("FINDER_OPEN: handle_opened_file completed successfully");
    }

    /// Handle opening a single file via legacy "Open With" method (application:openFile:)
    unsafe extern "C" fn handle_opened_file_single(
        _this: &mut AnyObject,
        _sel: Sel,
        _sender: &AnyObject,
        filename: &NSString,
    ) {
        debug!("handle_opened_file_single called");
        
        autoreleasepool(|pool| {
            let path = filename.as_str(pool).to_owned();
            debug!("Processing single file: {}", path);
            
            // Create NSURL and try to get security-scoped access
            let url = NSURL::fileURLWithPath(&filename);
            let file_accessed: bool = msg_send![&*url, startAccessingSecurityScopedResource];
            
            if file_accessed {
                debug!("Gained security-scoped access to single file");
                store_security_scoped_url(&path, url);
                
                // Send the file path to the main app
                if let Some(ref sender) = FILE_CHANNEL {
                    match sender.send(path.clone()) {
                        Ok(_) => debug!("Successfully sent single file path to main thread"),
                        Err(e) => error!("Failed to send single file path: {}", e),
                    }
                }
            } else {
                debug!("Failed to get security-scoped access for single file: {}", path);
            }
        });
    }

    /// Handle app launch detection to see if we're launched with files
    unsafe extern "C" fn handle_will_finish_launching(
        _this: &mut AnyObject,
        _sel: Sel,
        _notification: &AnyObject,
    ) {
        debug!("App will finish launching");
        
        // Check command line arguments
        let args: Vec<String> = std::env::args().collect();
        debug!("Command line arguments count: {}", args.len());
        
        for (i, arg) in args.iter().enumerate() {
            if i > 0 && std::path::Path::new(arg).exists() {
                debug!("Found potential file argument: {}", arg);
            }
        }
    }

    pub fn register_file_handler() {
        crate::write_immediate_crash_log("REGISTER_HANDLER: Function entry");
        debug!("Registering file handler for macOS");
        
        crate::write_immediate_crash_log("REGISTER_HANDLER: About to create MainThreadMarker");
        let mtm = MainThreadMarker::new().expect("Must be on main thread");
        crate::write_immediate_crash_log("REGISTER_HANDLER: MainThreadMarker created");
        
        unsafe {
            crate::write_immediate_crash_log("REGISTER_HANDLER: About to get NSApplication");
            let app = NSApplication::sharedApplication(mtm);
            crate::write_immediate_crash_log("REGISTER_HANDLER: NSApplication obtained");
            
            // Get the existing delegate
            crate::write_immediate_crash_log("REGISTER_HANDLER: About to get delegate");
            let delegate = app.delegate().unwrap();
            crate::write_immediate_crash_log("REGISTER_HANDLER: Delegate obtained");
            
            // Find out class of the NSApplicationDelegate
            crate::write_immediate_crash_log("REGISTER_HANDLER: About to get delegate class");
            let class: &AnyClass = msg_send![&delegate, class];
            crate::write_immediate_crash_log("REGISTER_HANDLER: Delegate class obtained");
            
            // Create a subclass of the existing delegate
            crate::write_immediate_crash_log("REGISTER_HANDLER: About to create ClassBuilder");
            let mut my_class = ClassBuilder::new("ViewSkaterApplicationDelegate", class).unwrap();
            crate::write_immediate_crash_log("REGISTER_HANDLER: ClassBuilder created");
            
            // Add file handling methods
            crate::write_immediate_crash_log("REGISTER_HANDLER: About to add methods");
            my_class.add_method(
                sel!(application:openFiles:),
                handle_opened_file as unsafe extern "C" fn(_, _, _, _),
            );
            
            my_class.add_method(
                sel!(application:openFile:),
                handle_opened_file_single as unsafe extern "C" fn(_, _, _, _),
            );
            
            my_class.add_method(
                sel!(applicationWillFinishLaunching:),
                handle_will_finish_launching as unsafe extern "C" fn(_, _, _),
            );
            crate::write_immediate_crash_log("REGISTER_HANDLER: Methods added");
            
            crate::write_immediate_crash_log("REGISTER_HANDLER: About to register class");
            let class = my_class.register();
            crate::write_immediate_crash_log("REGISTER_HANDLER: Class registered");
            
            // Cast and set the class
            crate::write_immediate_crash_log("REGISTER_HANDLER: About to cast delegate");
            let delegate_obj = Retained::cast::<AnyObject>(delegate);
            crate::write_immediate_crash_log("REGISTER_HANDLER: About to set delegate class");
            AnyObject::set_class(&delegate_obj, class);
            crate::write_immediate_crash_log("REGISTER_HANDLER: Delegate class set");
            
            // Prevent AppKit from interpreting our command line
            crate::write_immediate_crash_log("REGISTER_HANDLER: About to configure AppKit");
            let key = NSString::from_str("NSTreatUnknownArgumentsAsOpen");
            let keys = vec![key.as_ref()];
            let objects = vec![Retained::cast::<AnyObject>(NSString::from_str("NO"))];
            let dict = NSDictionary::from_vec(&keys, objects);
            NSUserDefaults::standardUserDefaults().registerDefaults(dict.as_ref());
            crate::write_immediate_crash_log("REGISTER_HANDLER: AppKit configuration completed");
            
            debug!("File handler registration completed");
            crate::write_immediate_crash_log("REGISTER_HANDLER: Function completed successfully");
        }
    }

    /// CRITICAL FIX: Clear corrupted bookmark data that may be causing crashes
    pub fn clear_corrupted_bookmarks() {
        eprintln!("BOOKMARK_CLEANUP: Starting cleanup of potentially corrupted bookmarks");
        
        autoreleasepool(|_pool| unsafe {
            let defaults = NSUserDefaults::standardUserDefaults();
            
            // Safer cleanup: Only remove legacy/broken prefixes and debug counters
            // DO NOT remove new-format VSBookmark_ keys, as they hold valid bookmarks
            let bookmark_prefixes = [
                "ViewSkaterSecurityBookmark_", // legacy broken format
                // "VSBookmark_",               // KEEP new format intact
                "ViewSkaterLastCrashLog",
                "ViewSkaterCrashCounter",
                "ViewSkaterImmediateCrashLog",
            ];
            
            for prefix in &bookmark_prefixes {
                eprintln!("BOOKMARK_CLEANUP: Clearing keys with prefix: {}", prefix);
                
                // Create a simple test key to see if any exist
                for i in 0..50 { // Check up to 50 possible entries
                    let test_key_str = format!("{}{}", prefix, i);
                    let test_key = NSString::from_str(&test_key_str);
                    
                    let obj: *mut AnyObject = msg_send![&*defaults, objectForKey: &*test_key];
                    if !obj.is_null() {
                        eprintln!("BOOKMARK_CLEANUP: Removing key: {}", test_key_str);
                        let _: () = msg_send![&*defaults, removeObjectForKey: &*test_key];
                    }
                }
                
                // Also try to remove the base prefix key
                let base_key = NSString::from_str(prefix);
                let _: () = msg_send![&*defaults, removeObjectForKey: &*base_key];
            }
            
            // Also clean up any entries that match the exact pattern we use for the legacy format
            let common_paths = [
                "/Users", "/Applications", "/Documents", "/Desktop", "/Downloads",
                "/Pictures", "/Movies", "/Music", "/Library",
            ];
            
            for path in &common_paths {
                let normalized = normalize_path_for_key(path);
                let key_str = format!("ViewSkaterSecurityBookmark_{}", normalized);
                let key = NSString::from_str(&key_str);
                
                eprintln!("BOOKMARK_CLEANUP: Attempting to remove: {}", key_str);
                let _: () = msg_send![&*defaults, removeObjectForKey: &*key];
            }
            
            // Force synchronization to ensure cleanup is persisted
            let sync_result: bool = msg_send![&*defaults, synchronize];
            eprintln!("BOOKMARK_CLEANUP: Synchronization result: {}", sync_result);
            eprintln!("BOOKMARK_CLEANUP: Cleanup completed successfully");
        });
    }
}

/// Test function to verify all crash logging methods work
/// Call this during startup to confirm logs are being written
pub fn test_crash_logging_methods() {
    write_crash_debug_log("========== CRASH LOGGING TEST START ==========");
    write_crash_debug_log("Testing stderr output");
    write_crash_debug_log("Testing stdout output"); 
    write_crash_debug_log("Testing syslog output");
    write_crash_debug_log("Testing NSUserDefaults output");
    write_crash_debug_log("Testing file output");
    write_crash_debug_log("========== CRASH LOGGING TEST END ==========");
    
    // Test retrieval immediately
    #[cfg(target_os = "macos")]
    {
        let logs = get_crash_debug_logs_from_userdefaults();
        println!("Retrieved logs from UserDefaults:");
        for log in logs {
            println!("  {}", log);
        }
    }
}

// ==================== END CRASH DEBUG LOGGING ====================



