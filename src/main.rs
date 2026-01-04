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
mod settings;
mod app;
mod utils;
mod build_info;
mod logging;
#[cfg(feature = "selection")]
mod selection_manager;
#[cfg(feature = "coco")]
mod coco;
mod settings_modal;
mod replay;
mod exif_utils;

#[cfg(target_os = "macos")]
mod macos_file_access;
mod archive_cache;

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

pub static FRAME_TIMES: Lazy<Mutex<Vec<Instant>>> = Lazy::new(|| {
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

// Fullscreen UI detection zones
#[cfg(any(target_os = "macos", target_os = "windows"))]
const FULLSCREEN_TOP_ZONE_HEIGHT: f64 = 200.0;  // Larger zone for menu interactions in fullscreen mode

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const FULLSCREEN_TOP_ZONE_HEIGHT: f64 = 50.0;   // Standard zone for other platforms

const FULLSCREEN_BOTTOM_ZONE_HEIGHT: f64 = 100.0;  // Standard bottom zone for all platforms

// Store the actual shared log buffer from the file_io module
#[allow(clippy::type_complexity)]
static SHARED_LOG_BUFFER: Lazy<Arc<Mutex<Option<Arc<Mutex<VecDeque<String>>>>>>> = Lazy::new(|| {
    Arc::new(Mutex::new(None))
});

// Store the stdout buffer for global access
#[allow(clippy::type_complexity)]
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

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "viewskater")]
#[command(about = "A fast image viewer for browsing large collections of images")]
#[command(version)]
struct Args {
    /// Path to image file or directory to open
    path: Option<PathBuf>,

    /// Path to custom settings file
    #[arg(long = "settings")]
    settings_path: Option<String>,

    /// Enable replay/benchmark mode
    #[arg(long)]
    replay: bool,

    /// Test directories for replay mode (can be specified multiple times)
    #[arg(long = "test-dir", value_name = "DIR")]
    test_directories: Vec<PathBuf>,

    /// Duration to test each directory in seconds
    #[arg(long, default_value = "10")]
    duration: u64,

    /// Navigation interval in milliseconds
    #[arg(long, default_value = "50")]
    nav_interval: u64,

    /// Test directions: right, left, both
    #[arg(long, default_value = "both")]
    directions: String,

    /// Output file for benchmark results
    #[arg(long, value_name = "FILE")]
    output: Option<PathBuf>,

    /// Output format: text, json, markdown
    #[arg(long, default_value = "text")]
    output_format: String,

    /// Number of complete iterations/cycles to run
    #[arg(long, default_value = "1")]
    iterations: u32,

    /// Verbose output during replay
    #[arg(long)]
    verbose: bool,

    /// Exit automatically after replay completes
    #[arg(long)]
    auto_exit: bool,

    /// Skip first N images for metrics (to exclude pre-cached images with inflated FPS)
    #[arg(long, default_value = "0")]
    skip_initial: usize,

    /// Navigation mode: keyboard (continuous skating) or slider (stepped position changes)
    #[arg(long, default_value = "keyboard")]
    nav_mode: String,

    /// Step size for slider navigation mode (how many images to skip per navigation)
    #[arg(long, default_value = "1")]
    slider_step: u16,
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
    // CRITICAL: Write to crash log IMMEDIATELY - before any other operations
    crate::logging::write_crash_debug_log("MAIN: App startup initiated");

    // Set up signal handler FIRST to catch low-level crashes
    crate::logging::write_crash_debug_log("MAIN: About to setup signal handler");
    crate::logging::setup_signal_crash_handler();
    crate::logging::write_crash_debug_log("MAIN: Signal handler setup completed");

    // Set up stdout capture FIRST, before any println! statements
    crate::logging::write_crash_debug_log("MAIN: About to setup stdout capture");
    let shared_stdout_buffer = crate::logging::setup_stdout_capture();
    set_shared_stdout_buffer(Arc::clone(&shared_stdout_buffer));

    crate::logging::write_crash_debug_log("MAIN: Stdout capture setup completed");

    println!("ViewSkater starting...");
    crate::logging::write_crash_debug_log("MAIN: ViewSkater starting message printed");


    // Set up panic hook to log to a file
    crate::logging::write_crash_debug_log("MAIN: About to setup logger");
    let app_name = "viewskater";
    let shared_log_buffer = crate::logging::setup_logger(app_name);

    // Store the log buffer reference for global access
    set_shared_log_buffer(Arc::clone(&shared_log_buffer));
    crate::logging::write_crash_debug_log("MAIN: Logger setup completed");

    crate::logging::write_crash_debug_log("MAIN: About to setup panic hook");
    crate::logging::setup_panic_hook(app_name, shared_log_buffer);
    crate::logging::write_crash_debug_log("MAIN: Panic hook setup completed");

    // Initialize winit FIRST
    let event_loop = EventLoop::<Action<Message>>::with_user_event()
        .build()
        .unwrap();

    // Set up the file channel AFTER winit initialization
    let (file_sender, file_receiver) = mpsc::channel();

    // Parse command line arguments
    let args = Args::parse();
    let settings_path = args.settings_path.clone();
    #[cfg(not(target_os = "macos"))]
    let file_arg = args.path.as_ref().map(|p| p.to_string_lossy().to_string());

    // Create replay configuration if replay mode is enabled
    let replay_config = if args.replay {
        let test_dirs = if args.test_directories.is_empty() {
            // If no test directories specified, try to use the path argument
            if let Some(ref path) = args.path {
                vec![path.clone()]
            } else {
                eprintln!("Error: Replay mode requires at least one test directory. Use --test-dir or provide a path argument.");
                std::process::exit(1);
            }
        } else {
            args.test_directories.clone()
        };

        // Validate that all test directories exist
        for dir in &test_dirs {
            if !dir.exists() {
                eprintln!("Error: Test directory does not exist: {}", dir.display());
                std::process::exit(1);
            }
        }

        // Parse directions
        let directions = match args.directions.to_lowercase().as_str() {
            "right" => vec![replay::ReplayDirection::Right],
            "left" => vec![replay::ReplayDirection::Left],
            "both" => vec![replay::ReplayDirection::Both],
            _ => {
                eprintln!("Error: Invalid direction '{}'. Use 'right', 'left', or 'both'", args.directions);
                std::process::exit(1);
            }
        };

        // Parse navigation mode
        let navigation_mode = match args.nav_mode.to_lowercase().as_str() {
            "slider" => replay::NavigationMode::Slider,
            _ => replay::NavigationMode::Keyboard,
        };

        println!("Replay mode enabled:");
        println!("  Test directories: {:?}", test_dirs);
        println!("  Duration per directory: {}s", args.duration);
        println!("  Navigation interval: {}ms", args.nav_interval);
        println!("  Directions: {:?}", directions);
        println!("  Navigation mode: {:?}", navigation_mode);
        if navigation_mode == replay::NavigationMode::Slider {
            println!("  Slider step: {}", args.slider_step);
        }
        println!("  Iterations: {}", args.iterations);
        if let Some(ref output) = args.output {
            println!("  Output file: {}", output.display());
        }

        let output_format = match args.output_format.to_lowercase().as_str() {
            "json" => replay::OutputFormat::Json,
            "markdown" | "md" => replay::OutputFormat::Markdown,
            _ => replay::OutputFormat::Text,
        };

        Some(replay::ReplayConfig {
            test_directories: test_dirs,
            duration_per_directory: Duration::from_secs(args.duration),
            navigation_interval: Duration::from_millis(args.nav_interval),
            directions,
            output_file: args.output.clone(),
            output_format,
            verbose: args.verbose,
            iterations: args.iterations,
            auto_exit: args.auto_exit,
            skip_initial_images: args.skip_initial,
            navigation_mode,
            slider_step: args.slider_step,
        })
    } else {
        None
    };

    // Test crash debug logging immediately at startup
    crate::logging::write_crash_debug_log("========== VIEWSKATER STARTUP ==========");
    crate::logging::write_crash_debug_log("Testing crash debug logging system at startup");
    crate::logging::write_crash_debug_log(&format!("App version: {}", env!("CARGO_PKG_VERSION")));
    crate::logging::write_crash_debug_log(&format!("Timestamp: {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")));
    crate::logging::write_crash_debug_log("If you can see this message, crash debug logging is working");
    crate::logging::write_crash_debug_log("=========================================");

    // Test all logging methods comprehensively
    #[cfg(target_os = "macos")]
    macos_file_access::test_crash_logging_methods();

    // Register file handler BEFORE creating the runner
    // This is required on macOS so the app can receive file paths
    // when launched by opening a file (e.g. double-clicking in Finder)
    // or using "Open With". Must be set up early in app lifecycle.
    #[cfg(target_os = "macos")]
    {
        crate::logging::write_crash_debug_log("MAIN: About to set file channel");
        macos_file_access::macos_file_handler::set_file_channel(file_sender);
        crate::logging::write_crash_debug_log("MAIN: File channel set");

        // NOTE: Automatic bookmark cleanup is DISABLED in production builds to avoid
        // wiping valid stored access. Use a special maintenance build or developer
        // tooling to invoke cleanup if ever needed.

        crate::logging::write_crash_debug_log("MAIN: About to register file handler");
        macos_file_access::macos_file_handler::register_file_handler();
        crate::logging::write_crash_debug_log("MAIN: File handler registered");

        // Try to restore full disk access from previous session
        crate::logging::write_crash_debug_log("MAIN: About to restore full disk access");
        debug!("🔍 Attempting to restore full disk access on startup");
        let restore_result = macos_file_access::macos_file_handler::restore_full_disk_access();
        debug!("🔍 Restore full disk access result: {}", restore_result);
        crate::logging::write_crash_debug_log(&format!("MAIN: Restore full disk access result: {}", restore_result));

        println!("macOS file handler registered");
        crate::logging::write_crash_debug_log("MAIN: macOS file handler registration completed");
    }

    // Handle command line arguments for Linux (and Windows)
    // This supports double-click and "Open With" functionality via .desktop files on Linux
    #[cfg(not(target_os = "macos"))]
    {
        if let Some(ref file_path) = file_arg {
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
            settings_path: Option<String>,
            replay_config: Option<replay::ReplayConfig>,
        },
        Ready {
            window: Arc<winit::window::Window>,
            device: Arc<wgpu::Device>,
            queue: Arc<wgpu::Queue>,
            surface: wgpu::Surface<'static>,
            format: wgpu::TextureFormat,
            engine: Arc<Mutex<Engine>>,
            renderer: std::rc::Rc<Mutex<Renderer>>,
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
            last_title: String,         // Track last set title to avoid unnecessary updates
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
                    last_title,
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
                                        // Update app's window width for responsive layout
                                        // Divide by scale factor to get logical pixels (important for macOS Retina)
                                        let logical_width = size.width as f32 / window.scale_factor() as f32;
                                        state.queue_message(Message::WindowResized(logical_width));
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
                                        macos_file_access::macos_file_handler::cleanup_all_security_scoped_access();
                                    }
                                    event_loop.exit();
                                }
                                WindowEvent::CursorMoved { position, .. } => {
                                    if state.program().is_fullscreen {
                                        state.queue_message(Message::CursorOnTop(position.y < FULLSCREEN_TOP_ZONE_HEIGHT));
                                        state.queue_message(Message::CursorOnFooter(
                                            position.y > (window.inner_size().height as f64 - FULLSCREEN_BOTTOM_ZONE_HEIGHT)));
                                    }
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
                                WindowEvent::KeyboardInput {
                                    event:
                                        winit::event::KeyEvent {
                                            physical_key: winit::keyboard::PhysicalKey::Code(
                                                winit::keyboard::KeyCode::F11),
                                            state: ElementState::Pressed,
                                            repeat: false,
                                            ..
                                        },
                                    ..
                                } => {
                                    #[cfg(target_os = "macos")] {
                                        // On macOS, window.fullscreen().is_some() doesn't work with set_simple_fullscreen()
                                        // so we need to use the application's internal state
                                        let fullscreen = if state.program().is_fullscreen {
                                            state.queue_message(Message::ToggleFullScreen(false));
                                            None
                                        } else {
                                            state.queue_message(Message::ToggleFullScreen(true));
                                            Some(winit::window::Fullscreen::Borderless(None))
                                        };
                                        use iced_winit::winit::platform::macos::WindowExtMacOS;
                                        window.set_simple_fullscreen(fullscreen.is_some());
                                    }
                                    #[cfg(not(target_os = "macos"))] {
                                        let fullscreen = if window.fullscreen().is_some() {
                                            state.queue_message(Message::ToggleFullScreen(false));
                                            None
                                        } else {
                                            state.queue_message(Message::ToggleFullScreen(true));
                                            Some(winit::window::Fullscreen::Borderless(None))
                                        };
                                        window.set_fullscreen(fullscreen);
                                    }
                                }
                                WindowEvent::KeyboardInput {
                                    event:
                                        winit::event::KeyEvent {
                                            physical_key: winit::keyboard::PhysicalKey::Code(
                                                winit::keyboard::KeyCode::Escape),
                                            state: ElementState::Pressed,
                                            repeat: false,
                                            ..
                                        },
                                    ..
                                } => {
                                    // Handle Escape key to exit fullscreen on macOS
                                    #[cfg(target_os = "macos")] {
                                        if window.fullscreen().is_some() || state.program().is_fullscreen {
                                            state.queue_message(Message::ToggleFullScreen(false));
                                            use iced_winit::winit::platform::macos::WindowExtMacOS;
                                            window.set_simple_fullscreen(false);
                                        }
                                    }
                                    #[cfg(not(target_os = "macos"))] {
                                        if window.fullscreen().is_some() {
                                            state.queue_message(Message::ToggleFullScreen(false));
                                            window.set_fullscreen(None);
                                        }
                                    }
                                }
                                WindowEvent::RedrawRequested => {
                                    // Queue RedrawRequested event for widgets (spinner animation)
                                    state.queue_event(iced_winit::core::Event::Window(
                                        iced_winit::core::window::Event::RedrawRequested(Instant::now())
                                    ));

                                    // Process the event so widgets get it
                                    if !state.is_queue_empty() {
                                        let (_, task) = state.update(
                                            viewport.logical_size(),
                                            cursor_position
                                                .map(|p| conversion::cursor_position(p, viewport.scale_factor()))
                                                .map(mouse::Cursor::Available)
                                                .unwrap_or(mouse::Cursor::Unavailable),
                                            &mut *renderer.lock().unwrap(),
                                            custom_theme,
                                            &renderer::Style { text_color: Color::WHITE },
                                            clipboard,
                                            debug_tool,
                                        );

                                        if let Some(t) = task {
                                            if let Some(stream) = into_stream(t) {
                                                runtime.run(stream);
                                            }
                                        }
                                    }

                                    // Handle resizing if needed
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

                                    // Actually render
                                    match surface.get_current_texture() {
                                        Ok(frame) => {
                                            let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
                                            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                                label: Some("Render Encoder"),
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

                                            window.set_cursor(
                                                iced_winit::conversion::mouse_interaction(state.mouse_interaction()),
                                            );

                                            // Check if spinner is animating and request next frame
                                            // This keeps the animation loop going
                                            if state.program().is_any_pane_loading() {
                                                window.request_redraw();
                                            }
                                        }
                                        Err(wgpu::SurfaceError::OutOfMemory) => {
                                            panic!("Swapchain error: OutOfMemory");
                                        }
                                        Err(_) => {
                                            window.request_redraw();
                                        }
                                    }

                                    *redraw = false;
                                    return; // Skip the normal redraw path below
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
                                {
                                    state.queue_message(Message::Event(event.clone()));
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
                                    custom_theme,
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
                                        renderer_guard.update_image_config(device, &mut engine_guard, config);

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

                                // Set window title when the title is actually changed
                                let new_title = state.program().title();
                                if new_title != *last_title {
                                    window.set_title(&new_title);
                                    *last_title = new_title;
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

                                            // Submit commands while still holding the lock
                                            engine_guard.submit(queue, encoder);
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

                                        // TODO: better way to track mouse on menu
                                        state.queue_message(Message::CursorOnMenu(
                                            !state.program().cursor_on_footer && state.mouse_interaction() == mouse::Interaction::Pointer));

                                        // Continue animation loop if spinner is active
                                        if state.program().is_any_pane_loading() {
                                            window.request_redraw();
                                        }

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
                                    log::info!("MAIN: Action::Output received - UserEvent is being processed");
                                    state.queue_message(message);
                                    // Queue RedrawRequested event so widgets receive it in on_event
                                    // This is required for spinner animation - widgets only see events, not messages
                                    state.queue_event(iced_winit::core::Event::Window(
                                        iced_winit::core::window::Event::RedrawRequested(Instant::now())
                                    ));

                                    // Process immediately - fix for spinner/async task animations
                                    if !state.is_queue_empty() {
                                        let (_, task) = state.update(
                                            viewport.logical_size(),
                                            cursor_position
                                                .map(|p| conversion::cursor_position(p, viewport.scale_factor()))
                                                .map(mouse::Cursor::Available)
                                                .unwrap_or(mouse::Cursor::Unavailable),
                                            &mut *renderer.lock().unwrap(),
                                            custom_theme,
                                            &renderer::Style { text_color: Color::WHITE },
                                            clipboard,
                                            debug_tool,
                                        );

                                        if let Some(t) = task {
                                            if let Some(stream) = into_stream(t) {
                                                runtime.run(stream);
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                            *redraw = true;
                            // Immediately request redraw for animation (e.g., SpinnerTick)
                            window.request_redraw();

                            // Also render directly if loading is active (for initial spinner animation)
                            if state.program().is_any_pane_loading() {
                                match surface.get_current_texture() {
                                    Ok(frame) => {
                                        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
                                        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
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
                                        *redraw = false;
                                    }
                                    Err(_) => {}
                                }
                            }
                        }
                        Event::EventLoopAwakened(winit::event::Event::AboutToWait) => {
                            // Process any pending control messages
                            while let Ok(control) = control_receiver.try_recv() {
                                match control {
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
                                            macos_file_access::macos_file_handler::cleanup_all_security_scoped_access();
                                        }
                                        event_loop.exit();
                                    }
                                    _ => {}
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
                Self::Loading { proxy, event_sender, control_receiver, file_receiver, settings_path, replay_config } => {
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

                    // Create renderer with Rc<Mutex>
                    let renderer = std::rc::Rc::new(Mutex::new(Renderer::new(
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
                        settings_path.as_deref(),
                        std::mem::take(replay_config),
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
                        last_title: String::new(),
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
        settings_path,
        replay_config,
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
