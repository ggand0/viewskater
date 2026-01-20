/*
================================================================================
                            ViewSkater Logging System
================================================================================

This module provides comprehensive logging infrastructure for ViewSkater, handling
both standard application logging and low-level crash diagnostics. It contains
two main functional groups:

## 1. Standard Application Logging

**Purpose**: Normal application logging using Rust's `log` crate (debug!, info!, etc.)
**Components**:
- `BufferLogger`: Captures log messages in memory buffer for export
- `CompositeLogger`: Combines console output with buffer capture
- `setup_logger()`: Initializes the logging system with appropriate filters
- `setup_panic_hook()`: Handles Rust panics with detailed backtraces
- `export_debug_logs()`: Exports captured log messages to debug.log
- `setup_stdout_capture()`: Captures println! output for export (Unix only)
- `export_stdout_logs()`: Exports captured stdout to stdout.log

**Log Levels**:
- Debug builds: Shows DEBUG and above
- Release builds: Shows ERROR only (unless RUST_LOG is set)
- All logs are captured in circular buffer (last 1000 entries)

## 2. Low-Level Crash Diagnostics

**Purpose**:
Crash logging that works even when Rust panic handling fails.
When Objective-C code crashes, it bypasses Rust's panic system and goes straight to Unix signals.

**Use Cases**:
- Objective-C interop crashes (segfaults, bus errors)
- App Store sandbox crashes where console isn't available
- Signal-level crashes that bypass normal Rust error handling

**Components**:
- `write_crash_debug_log()`: Multi-method crash logging (stderr, stdout, NSUserDefaults)
- `write_immediate_crash_log()`: Synchronous disk logging with maximum reliability
- `setup_signal_crash_handler()`: Unix signal handler for SIGSEGV, SIGBUS, etc.
- `get_crash_debug_logs_from_userdefaults()`: Retrieve crash logs from macOS preferences

**Storage Methods**:
1. **Immediate file writes**: Multiple locations with O_SYNC for reliability
2. **NSUserDefaults**: macOS preferences system (survives crashes)
3. **Console output**: stderr/stdout for development debugging

## 3. Log Export & Management

**User-Facing Features**:
- "Export debug logs": Exports log buffer to debug.log
- "Export all logs": Exports both debug and stdout logs
- "Show logs": Opens log directory in file explorer
- Automatic log directory creation and management

**File Locations**:
- macOS: ~/Library/Application Support/viewskater/logs/
- Other: Uses dirs crate for appropriate data directory

## Thread Safety

All shared state is protected by Mutex:
- Log buffers use Arc<Mutex<VecDeque<String>>>
- Circular buffer management prevents memory growth
- Signal handlers use minimal, async-signal-safe operations

## Platform Differences

**macOS**: NSUserDefaults integration + Unix signal handling + pipe-based stdout capture
**Linux**: Unix signal handling + O_SYNC file writes + pipe-based stdout capture
**Windows**: File-based crash logging + manual stdout capture (no signal handling)

================================================================================
*/

use std::panic;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::Command;
use once_cell::sync::Lazy;
use env_logger::fmt::Color;
use log::{LevelFilter, Metadata, Record};
use env_logger::fmt::Formatter;
use chrono::Utc;

#[allow(unused_imports)]
use log::{Level, debug, info, warn, error};





const MAX_LOG_LINES: usize = 1000;

// Global buffer for stdout capture
static STDOUT_BUFFER: Lazy<Arc<Mutex<VecDeque<String>>>> = Lazy::new(|| {
    Arc::new(Mutex::new(VecDeque::with_capacity(1000)))
});

// Global flag to control stdout capture
#[allow(dead_code)]
static STDOUT_CAPTURE_ENABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);


struct BufferLogger {
    log_buffer: Arc<Mutex<VecDeque<String>>>,
}

impl BufferLogger {
    #[allow(dead_code)]
    fn new() -> Self {
        Self {
            log_buffer: Arc::new(Mutex::new(VecDeque::with_capacity(MAX_LOG_LINES))),
        }
    }

    fn log_to_buffer(&self, message: &str, target: &str, line: Option<u32>, _module_path: Option<&str>) {
        if target.starts_with("viewskater") {
            let mut buffer = self.log_buffer.lock().unwrap();
            if buffer.len() == MAX_LOG_LINES {
                buffer.pop_front();
            }

            // Format the log message to include only line number to avoid duplication
            // The module is already in the target in most cases
            let formatted_message = if let Some(line_num) = line {
                format!("{target}:{line_num} {message}")
            } else {
                format!("{target} {message}")
            };

            buffer.push_back(formatted_message);
        }
    }

    #[allow(dead_code)]
    fn dump_logs(&self) -> Vec<String> {
        let buffer = self.log_buffer.lock().unwrap();
        buffer.iter().cloned().collect()
    }

    #[allow(dead_code)]
    fn get_shared_buffer(&self) -> Arc<Mutex<VecDeque<String>>> {
        Arc::clone(&self.log_buffer)
    }
}

impl log::Log for BufferLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.target().starts_with("viewskater") && metadata.level() <= LevelFilter::Debug
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let message = format!("{:<5} {}", record.level(), record.args());
            self.log_to_buffer(
                &message,
                record.target(),
                record.line(),
                record.module_path()
            );
        }
    }

    fn flush(&self) {}
}

#[allow(dead_code)]
struct CompositeLogger {
    console_logger: env_logger::Logger,
    buffer_logger: BufferLogger,
}

impl log::Log for CompositeLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        self.console_logger.enabled(metadata) || self.buffer_logger.enabled(metadata)
    }

    fn log(&self, record: &Record) {
        if self.console_logger.enabled(record.metadata()) {
            self.console_logger.log(record);
        }
        if self.buffer_logger.enabled(record.metadata()) {
            self.buffer_logger.log(record);
        }
    }

    fn flush(&self) {
        self.console_logger.flush();
        self.buffer_logger.flush();
    }
}


#[allow(dead_code)]
pub fn setup_logger(_app_name: &str) -> Arc<Mutex<VecDeque<String>>> {
    let buffer_logger = BufferLogger::new();
    let shared_buffer = buffer_logger.get_shared_buffer();

    let mut builder = env_logger::Builder::new();

    // First check if RUST_LOG is set - if so, use that configuration
    if std::env::var("RUST_LOG").is_ok() {
        builder.parse_env("RUST_LOG");
    } else {
        // If RUST_LOG is not set, use different defaults for debug/release builds
        if cfg!(debug_assertions) {
            // In debug mode, show debug logs and above
            builder.filter(Some("viewskater"), LevelFilter::Debug);
        } else {
            // In release mode, only show errors by default
            builder.filter(Some("viewskater"), LevelFilter::Error);
        }
    }

    // Filter out all other crates' logs
    builder.filter(None, LevelFilter::Off);

    builder.format(|buf: &mut Formatter, record: &Record| {
        let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%S%.6fZ");

        // Create the module:line part
        let module_info = if let (Some(module), Some(line)) = (record.module_path(), record.line()) {
            format!("{module}:{line}")
        } else if let Some(module) = record.module_path() {
            module.to_string()
        } else if let Some(line) = record.line() {
            format!("line:{line}")
        } else {
            "unknown".to_string()
        };

        let mut level_style = buf.style();
        let mut meta_style = buf.style();

        // Set level colors
        match record.level() {
            Level::Error => level_style.set_color(Color::Red).set_bold(true),
            Level::Warn => level_style.set_color(Color::Yellow).set_bold(true),
            Level::Info => level_style.set_color(Color::Green).set_bold(true),
            Level::Debug => level_style.set_color(Color::Blue).set_bold(true),
            Level::Trace => level_style.set_color(Color::White),
        };

        // Set meta style color based on platform
        #[cfg(target_os = "macos")]
        {
            // Color::Rgb does not work on macOS, so we use Color::Blue as a workaround
            meta_style.set_color(Color::Blue);
        }

        #[cfg(not(target_os = "macos"))]
        {
            // Color formatting with Color::Rgb works fine on Windows/Linux
            meta_style.set_color(Color::Rgb(120, 120, 120));
        }

        writeln!(
            buf,
            "{} {} {} {}",
            meta_style.value(timestamp),
            level_style.value(record.level()),
            meta_style.value(module_info),
            record.args()
        )
    });

    let console_logger = builder.build();

    let composite_logger = CompositeLogger {
        console_logger,
        buffer_logger,
    };

    log::set_boxed_logger(Box::new(composite_logger)).expect("Failed to set logger");

    // Always set the maximum level to Trace so that filtering works correctly
    log::set_max_level(LevelFilter::Trace);

    shared_buffer
}

pub fn get_log_directory(app_name: &str) -> PathBuf {
    dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join(app_name).join("logs")
}


/// Exports the current log buffer to a debug log file.
///
/// This function writes the last 1,000 lines of logs (captured via the log macros like debug!, info!, etc.)
/// to a separate debug log file. This is useful for troubleshooting issues without waiting for a crash.
///
/// NOTE: This currently captures logs from the Rust `log` crate macros (debug!, info!, warn!, error!)
/// but does NOT capture raw `println!` statements. To capture println! statements, stdout redirection
/// would be needed, which is more complex and may interfere with normal console output.
///
/// # Arguments
/// * `app_name` - The application name used for the log directory
/// * `log_buffer` - The shared log buffer containing the recent log messages
///
/// # Returns
/// * `Ok(PathBuf)` - The path to the created debug log file
/// * `Err(std::io::Error)` - An error if the export fails
pub fn export_debug_logs(app_name: &str, log_buffer: Arc<Mutex<VecDeque<String>>>) -> Result<PathBuf, std::io::Error> {
    // NOTE: Use println! instead of debug! to avoid circular logging
    // (debug! calls would be added to the same buffer we're trying to export)
    println!("DEBUG: export_debug_logs called");

    let log_dir_path = get_log_directory(app_name);
    println!("DEBUG: Log directory path: {}", log_dir_path.display());

    std::fs::create_dir_all(&log_dir_path)?;
    println!("DEBUG: Created log directory");

    let debug_log_path = log_dir_path.join("debug.log");
    println!("DEBUG: Debug log path: {}", debug_log_path.display());

    println!("DEBUG: About to open file for writing");
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&debug_log_path)?;
    println!("DEBUG: File opened successfully");

    // Write formatted timestamp
    let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.6fZ");
    println!("DEBUG: About to write header");

    writeln!(file, "{timestamp} [DEBUG EXPORT] =====================================")?;
    writeln!(file, "{timestamp} [DEBUG EXPORT] ViewSkater Debug Log Export")?;
    writeln!(file, "{timestamp} [DEBUG EXPORT] Export timestamp: {timestamp}")?;
    writeln!(file, "{timestamp} [DEBUG EXPORT] =====================================")?;
    writeln!(file, "{timestamp} [DEBUG EXPORT] ")?;
    writeln!(file, "{timestamp} [DEBUG EXPORT] IMPORTANT: This log captures output from Rust log macros")?;
    writeln!(file, "{timestamp} [DEBUG EXPORT] (debug!, info!, warn!, error!) but NOT raw println! statements.")?;
    writeln!(file, "{timestamp} [DEBUG EXPORT] Maximum captured entries: {MAX_LOG_LINES}")?;
    writeln!(file)?; // Empty line for readability
    println!("DEBUG: Header written");

    // Export all log entries from the buffer
    println!("DEBUG: About to lock log buffer");
    let buffer_size;
    let buffer_empty;
    let log_entries: Vec<String>;

    {
        let buffer = log_buffer.lock().unwrap();
        println!("DEBUG: Log buffer locked, size: {}", buffer.len());
        buffer_size = buffer.len();
        buffer_empty = buffer.is_empty();
        log_entries = buffer.iter().cloned().collect();
        println!("DEBUG: Copied {buffer_size} entries, releasing lock");
    } // Lock is dropped here

    println!("DEBUG: Buffer lock released");

    if buffer_empty {
        println!("DEBUG: Buffer is empty, writing empty message");
        writeln!(file, "{timestamp} [DEBUG EXPORT] No log entries found in buffer")?;
        writeln!(file, "{timestamp} [DEBUG EXPORT] This may indicate that:")?;
        writeln!(file, "{timestamp} [DEBUG EXPORT] 1. No log macros have been called yet")?;
        writeln!(file, "{timestamp} [DEBUG EXPORT] 2. All logs were filtered out by log level settings")?;
        writeln!(file, "{timestamp} [DEBUG EXPORT] 3. The app just started and no logs have been generated")?;
    } else {
        println!("DEBUG: Writing {buffer_size} log entries");
        writeln!(file, "{timestamp} [DEBUG EXPORT] Found {buffer_size} log entries (showing last {MAX_LOG_LINES} max):")?;
        writeln!(file, "{timestamp} [DEBUG EXPORT] =====================================")?;
        writeln!(file)?; // Empty line for readability

        for log_entry in log_entries.iter() {
            writeln!(file, "{timestamp} {log_entry}")?;
        }
        println!("DEBUG: All entries written");
    }

    println!("DEBUG: Writing footer");
    writeln!(file)?; // Final empty line
    writeln!(file, "{timestamp} [DEBUG EXPORT] =====================================")?;
    writeln!(file, "{timestamp} [DEBUG EXPORT] Export completed successfully")?;
    writeln!(file, "{timestamp} [DEBUG EXPORT] Total entries exported: {buffer_size}")?;
    writeln!(file, "{timestamp} [DEBUG EXPORT] =====================================")?;

    println!("DEBUG: About to flush file");
    file.flush()?;
    println!("DEBUG: File flushed");

    println!("DEBUG: About to call info! macro");
    info!("Debug logs exported to: {}", debug_log_path.display());
    println!("DEBUG: info! macro completed");

    println!("DEBUG: export_debug_logs completed successfully");

    Ok(debug_log_path)
}

/// Exports debug logs and opens the log directory in the file explorer.
///
/// This is a convenience function that combines exporting debug logs and opening
/// the log directory for easy access to the exported files.
///
/// # Arguments
/// * `app_name` - The application name used for the log directory
/// * `log_buffer` - The shared log buffer containing the recent log messages
pub fn export_and_open_debug_logs(app_name: &str, log_buffer: Arc<Mutex<VecDeque<String>>>) {
    // NOTE: Use println! to avoid circular logging during export operations
    println!("DEBUG: About to export debug logs...");
    if let Ok(buffer) = log_buffer.lock() {
        println!("DEBUG: Buffer size at export time: {}", buffer.len());
        if !buffer.is_empty() {
            println!("DEBUG: First few entries:");
            for (i, entry) in buffer.iter().take(3).enumerate() {
                println!("DEBUG: Entry {i}: {entry}");
            }
        }
    }

    match export_debug_logs(app_name, log_buffer) {
        Ok(debug_log_path) => {
            info!("Debug logs successfully exported to: {}", debug_log_path.display());
            println!("Debug logs exported to: {}", debug_log_path.display());

            // Temporarily disable automatic directory opening to prevent hangs
            // let log_dir = debug_log_path.parent().unwrap_or_else(|| Path::new("."));
            // open_in_file_explorer(&log_dir.to_string_lossy().to_string());
        }
        Err(e) => {
            error!("Failed to export debug logs: {e}");
            eprintln!("Failed to export debug logs: {e}");
        }
    }
}

pub fn setup_panic_hook(app_name: &str, log_buffer: Arc<Mutex<VecDeque<String>>>) {
    let log_file_path = get_log_directory(app_name).join("panic.log");
    std::fs::create_dir_all(log_file_path.parent().unwrap()).expect("Failed to create log directory");

    panic::set_hook(Box::new(move |info| {
        let backtrace = backtrace::Backtrace::new();
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&log_file_path)
            .expect("Failed to open panic log file");

        // Write formatted timestamp
        let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.6fZ");

        // Extract panic location information if available
        let location = if let Some(location) = info.location() {
            format!("{}:{}", location.file(), location.line())
        } else {
            "unknown location".to_string()
        };

        // Create formatted messages that we'll use for both console and file
        let header_msg = format!("[PANIC] at {location} - {info}");
        let backtrace_header = "[PANIC] Backtrace:";

        // Format backtrace lines
        let mut backtrace_lines = Vec::new();
        for line in format!("{backtrace:?}").lines() {
            backtrace_lines.push(format!("[BACKTRACE] {}", line.trim()));
        }

        // Log header to file
        writeln!(file, "{timestamp} {header_msg}").expect("Failed to write panic info");
        writeln!(file, "{timestamp} {backtrace_header}").expect("Failed to write backtrace header");

        // Log backtrace to file
        for line in &backtrace_lines {
            writeln!(file, "{timestamp} {line}").expect("Failed to write backtrace line");
        }

        // Add double linebreak between backtrace and log entries
        writeln!(file).expect("Failed to write newline");
        writeln!(file).expect("Failed to write second newline");

        // Dump the last N log lines from the buffer with timestamps
        writeln!(file, "{timestamp} [PANIC] Last {MAX_LOG_LINES} log entries:")
            .expect("Failed to write log header");

        let buffer = log_buffer.lock().unwrap();
        for log in buffer.iter() {
            writeln!(file, "{timestamp} {log}").expect("Failed to write log entry");
        }

        // ALSO PRINT TO CONSOLE (this is the new part)
        // Use eprintln! to print to stderr
        eprintln!("\n\n{header_msg}");
        eprintln!("{backtrace_header}");
        for line in &backtrace_lines {
            eprintln!("{line}");
        }
        eprintln!("\nA complete crash log has been written to: {}", log_file_path.display());
    }));
}



pub fn open_in_file_explorer(path: &str) {
    if cfg!(target_os = "windows") {
        // Windows: Use "explorer" to open the directory
        match Command::new("explorer")
            .arg(path)
            .spawn() {
                Ok(_) => println!("Opened directory in File Explorer: {path}"),
                Err(e) => eprintln!("Failed to open directory in File Explorer: {e}"),
            }
    } else if cfg!(target_os = "macos") {
        // macOS: Use "open" to open the directory
        match Command::new("open")
            .arg(path)
            .spawn() {
                Ok(_) => println!("Opened directory in Finder: {path}"),
                Err(e) => eprintln!("Failed to open directory in Finder: {e}"),
            }
    } else if cfg!(target_os = "linux") {
        // Linux: Use "xdg-open" to open the directory (works with most desktop environments)
        match Command::new("xdg-open")
            .arg(path)
            .spawn() {
                Ok(_) => println!("Opened directory in File Explorer: {path}"),
                Err(e) => eprintln!("Failed to open directory in File Explorer: {e}"),
            }
    } else {
        error!("Opening directories is not supported on this OS.");
    }
}

/// Sets up stdout capture using Unix pipes to intercept println! and other stdout output.
///
/// This function creates a pipe, redirects stdout to the write end of the pipe,
/// and spawns a thread to read from the read end and capture the output.
///
/// # Returns
/// * `Arc<Mutex<VecDeque<String>>>` - The shared stdout buffer
#[cfg(unix)]
pub fn setup_stdout_capture() -> Arc<Mutex<VecDeque<String>>> {
    use std::os::unix::io::FromRawFd;
    use std::fs::File;
    use std::io::{BufReader, BufRead};
    use std::thread;

    // Create a pipe
    let mut pipe_fds = [0i32; 2];
    unsafe {
        if libc::pipe(pipe_fds.as_mut_ptr()) != 0 {
            eprintln!("Failed to create pipe for stdout capture");
            return Arc::clone(&STDOUT_BUFFER);
        }
    }

    let read_fd = pipe_fds[0];
    let write_fd = pipe_fds[1];

    // Duplicate the original stdout so we can restore it later
    let original_stdout_fd = unsafe { libc::dup(libc::STDOUT_FILENO) };
    if original_stdout_fd == -1 {
        eprintln!("Failed to duplicate original stdout");
        unsafe {
            libc::close(read_fd);
            libc::close(write_fd);
        }
        return Arc::clone(&STDOUT_BUFFER);
    }

    // Redirect stdout to the write end of the pipe
    unsafe {
        if libc::dup2(write_fd, libc::STDOUT_FILENO) == -1 {
            eprintln!("Failed to redirect stdout to pipe");
            libc::close(read_fd);
            libc::close(write_fd);
            libc::close(original_stdout_fd);
            return Arc::clone(&STDOUT_BUFFER);
        }
    }

    // Create a file from the read end of the pipe
    let pipe_reader = unsafe { File::from_raw_fd(read_fd) };
    let mut buf_reader = BufReader::new(pipe_reader);

    // Create a writer for the original stdout
    let original_stdout = unsafe { File::from_raw_fd(original_stdout_fd) };

    // Enable stdout capture
    STDOUT_CAPTURE_ENABLED.store(true, std::sync::atomic::Ordering::SeqCst);

    // Clone the buffer for the thread
    let buffer = Arc::clone(&STDOUT_BUFFER);

    // Spawn a thread to read from the pipe and capture output
    thread::spawn(move || {
        let mut line = String::new();
        let mut original_stdout = original_stdout;

        while STDOUT_CAPTURE_ENABLED.load(std::sync::atomic::Ordering::SeqCst) {
            line.clear();
            match buf_reader.read_line(&mut line) {
                Ok(0) => break, // EOF
                Ok(_) => {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        // Write to original stdout (console)
                        let _ = writeln!(original_stdout, "{trimmed}");
                        let _ = original_stdout.flush();

                        // Capture to buffer
                        if let Ok(mut buffer) = buffer.lock() {
                            if buffer.len() >= 1000 {
                                buffer.pop_front();
                            }
                            let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.6fZ");
                            buffer.push_back(format!("{timestamp} [STDOUT] {trimmed}"));
                        }
                    }
                }
                Err(_) => break,
            }
        }
    });

    // Close the write end of the pipe in this process (the duplicated stdout will handle writing)
    unsafe {
        libc::close(write_fd);
    }

    // Add initialization message to buffer
    if let Ok(mut buf) = STDOUT_BUFFER.lock() {
        let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.6fZ");
        buf.push_back(format!("{timestamp} [STDOUT] ViewSkater stdout capture initialized"));
    }

    // This println! should now be captured
    println!("Stdout capture initialized - all println! statements will be captured");

    Arc::clone(&STDOUT_BUFFER)
}

/// Sets up stdout capture (Windows/non-Unix fallback - manual capture only)
///
/// This function provides a fallback for non-Unix systems where stdout redirection
/// is more complex. It uses manual capture only.
///
/// # Returns
/// * `Arc<Mutex<VecDeque<String>>>` - The shared stdout buffer
#[cfg(not(unix))]
pub fn setup_stdout_capture() -> Arc<Mutex<VecDeque<String>>> {
    // Add initialization message to buffer
    if let Ok(mut buf) = STDOUT_BUFFER.lock() {
        let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.6fZ");
        buf.push_back(format!("{} [STDOUT] ViewSkater stdout capture initialized", timestamp));
    }

    println!("Stdout capture initialized (manual mode) - use capture_stdout() for important messages");

    Arc::clone(&STDOUT_BUFFER)
}

/// Exports stdout logs to a separate file.
///
/// This function writes the captured stdout output (from println! and other stdout writes)
/// to a separate stdout log file. This complements the debug log export.
///
/// # Arguments
/// * `app_name` - The application name used for the log directory
/// * `stdout_buffer` - The shared stdout buffer containing captured output
///
/// # Returns
/// * `Ok(PathBuf)` - The path to the created stdout log file
/// * `Err(std::io::Error)` - An error if the export fails
pub fn export_stdout_logs(app_name: &str, stdout_buffer: Arc<Mutex<VecDeque<String>>>) -> Result<PathBuf, std::io::Error> {
    let log_dir_path = get_log_directory(app_name);
    std::fs::create_dir_all(&log_dir_path)?;

    let stdout_log_path = log_dir_path.join("stdout.log");
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&stdout_log_path)?;

    // Write formatted timestamp
    let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.6fZ");

    writeln!(file, "{timestamp} [STDOUT EXPORT] =====================================")?;
    writeln!(file, "{timestamp} [STDOUT EXPORT] ViewSkater Stdout Log Export")?;
    writeln!(file, "{timestamp} [STDOUT EXPORT] Export timestamp: {timestamp}")?;
    writeln!(file, "{timestamp} [STDOUT EXPORT] =====================================")?;
    writeln!(file, "{timestamp} [STDOUT EXPORT] ")?;
    writeln!(file, "{timestamp} [STDOUT EXPORT] This log captures stdout output including println! statements")?;
    writeln!(file, "{timestamp} [STDOUT EXPORT] Maximum captured entries: 1000")?;
    writeln!(file)?; // Empty line for readability

    // Export all stdout entries from the buffer
    let buffer = stdout_buffer.lock().unwrap();
    if buffer.is_empty() {
        writeln!(file, "{timestamp} [STDOUT EXPORT] No stdout entries found in buffer")?;
        writeln!(file, "{timestamp} [STDOUT EXPORT] Note: Automatic stdout capture is disabled")?;
        writeln!(file, "{timestamp} [STDOUT EXPORT] Use debug logs (debug!, info!, etc.) for logging instead")?;
    } else {
        writeln!(file, "{} [STDOUT EXPORT] Found {} stdout entries:", timestamp, buffer.len())?;
        writeln!(file, "{timestamp} [STDOUT EXPORT] =====================================")?;
        writeln!(file)?; // Empty line for readability

        for stdout_entry in buffer.iter() {
            writeln!(file, "{stdout_entry}")?;
        }
    }

    writeln!(file)?; // Final empty line
    writeln!(file, "{timestamp} [STDOUT EXPORT] =====================================")?;
    writeln!(file, "{timestamp} [STDOUT EXPORT] Export completed successfully")?;
    writeln!(file, "{} [STDOUT EXPORT] Total entries exported: {}", timestamp, buffer.len())?;
    writeln!(file, "{timestamp} [STDOUT EXPORT] =====================================")?;

    file.flush()?;

    info!("Stdout logs exported to: {}", stdout_log_path.display());
    println!("Stdout logs exported to: {}", stdout_log_path.display());

    Ok(stdout_log_path)
}

/// Exports both debug logs and stdout logs, then opens the log directory.
///
/// This is a convenience function that exports both types of logs and opens
/// the log directory for easy access to all exported files.
///
/// # Arguments
/// * `app_name` - The application name used for the log directory
/// * `log_buffer` - The shared log buffer containing recent log messages
/// * `stdout_buffer` - The shared stdout buffer containing captured output
pub fn export_and_open_all_logs(app_name: &str, log_buffer: Arc<Mutex<VecDeque<String>>>, stdout_buffer: Arc<Mutex<VecDeque<String>>>) {
    // NOTE: Use println! to avoid circular logging during export operations
    println!("DEBUG: About to export all logs...");
    if let Ok(log_buf) = log_buffer.lock() {
        println!("DEBUG: Log buffer size: {}", log_buf.len());
    }
    if let Ok(stdout_buf) = stdout_buffer.lock() {
        println!("DEBUG: Stdout buffer size: {}", stdout_buf.len());
    }

    // Export debug logs
    match export_debug_logs(app_name, log_buffer) {
        Ok(debug_log_path) => {
            info!("Debug logs successfully exported to: {}", debug_log_path.display());

            // Open the log directory in file explorer (using debug log path)
            let log_dir = debug_log_path.parent().unwrap_or_else(|| std::path::Path::new("."));
            open_in_file_explorer(log_dir.to_string_lossy().as_ref());
        }
        Err(e) => {
            error!("Failed to export debug logs: {e}");
            eprintln!("Failed to export debug logs: {e}");
        }
    }

    // Only export stdout logs if there's actually something in the buffer
    let should_export_stdout = {
        if let Ok(stdout_buf) = stdout_buffer.lock() {
            !stdout_buf.is_empty()
        } else {
            false
        }
    };

    if should_export_stdout {
        match export_stdout_logs(app_name, stdout_buffer) {
            Ok(stdout_log_path) => {
                info!("Stdout logs successfully exported to: {}", stdout_log_path.display());
            }
            Err(e) => {
                error!("Failed to export stdout logs: {e}");
                eprintln!("Failed to export stdout logs: {e}");
            }
        }
    } else {
        println!("Skipping stdout.log export - buffer is empty (stdout capture disabled)");
    }
}

// ==================== CRASH DEBUG LOGGING ====================

/// Writes a crash debug log entry using multiple bulletproof methods for App Store sandbox
/// This ensures we can see what happened even if all file writing is blocked
pub fn write_crash_debug_log(message: &str) {
    // Simple immediate stderr logging
    let _ = std::panic::catch_unwind(|| {
        eprintln!("CRASH_DEBUG: {message}");
    });

    // Simple immediate stdout logging
    let _ = std::panic::catch_unwind(|| {
        println!("CRASH_DEBUG: {message}");
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
#[cfg(target_os = "macos")]
pub fn write_immediate_crash_log(message: &str) {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let formatted = format!("{timestamp} CRASH: {message}\n");

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
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.custom_flags(0x80); // O_SYNC on Unix - immediate disk writes
            }
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
        results.push(format!("CRASH_COUNTER: {crash_count} crashes detected"));

        // Get the last crash log
        let log_key = NSString::from_str("ViewSkaterLastCrashLog");
        let last_log: *mut objc2::runtime::AnyObject = msg_send![&*defaults, objectForKey: &*log_key];

        if !last_log.is_null() {
            let log_nsstring = &*(last_log as *const NSString);
            let log_str = log_nsstring.as_str(pool).to_owned();
            results.push(format!("LAST_CRASH_LOG: {log_str}"));
        } else {
            results.push("LAST_CRASH_LOG: No crash log found".to_string());
        }

        results
    })
}

/// Sets up a signal handler to catch low-level crashes that bypass Rust panic hooks
/// This is critical for Objective-C interop crashes that might cause segfaults
#[cfg(unix)]
pub fn setup_signal_crash_handler() {
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
            eprintln!("CRASH_DEBUG: SIGNAL CAUGHT: {signal_name} ({signal})");
            println!("CRASH_DEBUG: SIGNAL CAUGHT: {signal_name} ({signal})");
        });

        // Try to write to NSUserDefaults if possible
        #[cfg(target_os = "macos")]
        {
            unsafe {
                use objc2_foundation::{NSUserDefaults, NSString};
                use objc2::{msg_send};

                let message = format!("SIGNAL_CRASH: {signal_name} ({signal})");
                let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.6fZ");
                let formatted_message = format!("{timestamp} CRASH_DEBUG: {message}");

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
pub fn setup_signal_crash_handler() {
    // Signal handling not implemented for non-Unix platforms
}

