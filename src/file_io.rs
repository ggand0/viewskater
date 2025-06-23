use std::fs;
use std::path::Path;
use std::path::PathBuf;
use tokio::io::AsyncReadExt;
use std::ffi::OsStr;
use rfd;
use futures::future::join_all;
use crate::cache::img_cache::LoadOperation;
use tokio::time::Instant;

#[allow(unused_imports)]
use log::{Level, debug, info, warn, error};

use std::panic;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::collections::VecDeque;
use std::error::Error as StdError;
use std::io;
use std::process::Command;
use once_cell::sync::Lazy;
use env_logger::fmt::Color;
use log::{LevelFilter, Metadata, Record};
use image::GenericImageView;
use iced_wgpu::wgpu;

use crate::cache::img_cache::CachedData;
use crate::utils::timing::TimingStats;
use crate::cache::img_cache::CacheStrategy;
use iced_wgpu::engine::CompressionStrategy;
use std::thread;

static IMAGE_LOAD_STATS: Lazy<Mutex<TimingStats>> = Lazy::new(|| {
    Mutex::new(TimingStats::new("Image Load"))
});
static GPU_UPLOAD_STATS: Lazy<Mutex<TimingStats>> = Lazy::new(|| {
    Mutex::new(TimingStats::new("GPU Upload"))
});

// Global buffer for stdout capture
static STDOUT_BUFFER: Lazy<Arc<Mutex<VecDeque<String>>>> = Lazy::new(|| {
    Arc::new(Mutex::new(VecDeque::with_capacity(1000)))
});

// Global flag to control stdout capture
static STDOUT_CAPTURE_ENABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum Error {
    DialogClosed,
    InvalidSelection,
    InvalidExtension,
}


pub fn get_filename(path: &str) -> Option<String> {
    std::path::Path::new(path)
        .file_name()
        .and_then(|os_str| os_str.to_str())
        .map(|s| s.to_string())
}

/// Reads an image file into a byte vector.
/// 
/// This function reads raw bytes from a file using memory mapping for
/// improved performance with large files.
/// 
/// # Arguments
/// * `path` - The path to the image file
/// 
/// # Returns
/// * `Ok(Vec<u8>)` - The raw bytes of the image file
/// * `Err(io::Error)` - An error if reading fails
pub fn read_image_bytes(path: &PathBuf) -> Result<Vec<u8>, std::io::Error> {
    use std::fs::File;
    use std::io::{self, Read};
    use memmap2::Mmap;
    
    // Verify the file exists before attempting to read
    if !path.exists() {
        return Err(io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("File not found: {}", path.display())
        ));
    }
    
    // Use memory mapping for efficient file reading
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    let file_size = metadata.len() as usize;
    
    // Only use mmap for files over a certain size (e.g., 1MB)
    // For smaller files, regular reading is often faster
    if file_size > 1_048_576 {
        // Memory map the file for faster access
        let mmap = unsafe { Mmap::map(&file)? };
        let bytes = mmap.to_vec();
        debug!("Read {} bytes from {} using mmap", bytes.len(), path.display());
        Ok(bytes)
    } else {
        // For smaller files, regular reading is fine
        let mut buffer = Vec::with_capacity(file_size);
        let mut file = File::open(path)?;
        file.read_to_end(&mut buffer)?;
        debug!("Read {} bytes from {}", buffer.len(), path.display());
        Ok(buffer)
    }
}

#[allow(dead_code)]
pub async fn async_load_image(path: impl AsRef<Path>, operation: LoadOperation) -> Result<(Option<Vec<u8>>, Option<LoadOperation>), std::io::ErrorKind> {
    let file_path = path.as_ref();

    match tokio::fs::File::open(file_path).await {
        Ok(mut file) => {
            let mut buffer = Vec::new();
            if file.read_to_end(&mut buffer).await.is_ok() {
                Ok((Some(buffer), Some(operation) ))
            } else {
                Err(std::io::ErrorKind::InvalidData)
            }
        }
        Err(e) => Err(e.kind()),
    }
}

#[allow(dead_code)]
async fn load_image_cpu_async(path: Option<&str>) -> Result<Option<CachedData>, std::io::ErrorKind> {
    // Load a single image asynchronously
    if let Some(path) = path {
        let file_path = Path::new(path);
        let start = Instant::now();
        debug!("load_image_cpu_async - Starting to load: {}", path);
        
        match tokio::fs::File::open(file_path).await {
            Ok(mut file) => {
                let file_open_time = start.elapsed();
                debug!("load_image_cpu_async - File opened in {:?}", file_open_time);
                
                let read_start = Instant::now();
                let mut buffer = Vec::new();
                if file.read_to_end(&mut buffer).await.is_ok() {
                    let read_time = read_start.elapsed();
                    debug!("load_image_cpu_async - Read {} bytes in {:?}", buffer.len(), read_time);
                    
                    let total_time = start.elapsed();
                    debug!("load_image_cpu_async - Total load time: {:?}", total_time);
                    
                    Ok(Some(CachedData::Cpu(buffer)))
                } else {
                    Err(std::io::ErrorKind::InvalidData)
                }
            }
            Err(e) => Err(e.kind()),
        }
    } else {
        Ok(None)
    }
}


#[allow(dead_code)]
async fn load_image_gpu_async(
    path: Option<&str>, 
    device: &Arc<wgpu::Device>, 
    queue: &Arc<wgpu::Queue>,
    compression_strategy: CompressionStrategy
) -> Result<Option<CachedData>, std::io::ErrorKind> {
    if let Some(path_str) = path {
        let start = Instant::now();

        match image::open(path_str) {
            Ok(img) => {
                let (width, height) = img.dimensions();
                let rgba = img.to_rgba8();
                let rgba_data = rgba.as_raw();
                
                let duration = start.elapsed();
                IMAGE_LOAD_STATS.lock().unwrap().add_measurement(duration);
                
                let upload_start = Instant::now();

                // Use our utility to check if compression is applicable
                let use_compression = crate::cache::cache_utils::should_use_compression(
                    width, height, compression_strategy
                );
                
                // Create texture with the appropriate format
                let texture = crate::cache::cache_utils::create_gpu_texture(
                    device, width, height, compression_strategy
                );
                
                if use_compression {
                    // Use utility to compress and upload
                    let (compressed_data, row_bytes) = crate::cache::cache_utils::compress_image_data(
                        &rgba_data, width, height
                    );
                    
                    // Upload using the utility
                    crate::cache::cache_utils::upload_compressed_texture(
                        queue, &texture, &compressed_data, width, height, row_bytes
                    );
                    
                    let upload_duration = upload_start.elapsed();
                    GPU_UPLOAD_STATS.lock().unwrap().add_measurement(upload_duration);
                    
                    return Ok(Some(CachedData::BC1(Arc::new(texture))));
                } else {
                    // Upload uncompressed
                    crate::cache::cache_utils::upload_uncompressed_texture(
                        queue, &texture, &rgba_data, width, height
                    );
                    
                    let upload_duration = upload_start.elapsed();
                    GPU_UPLOAD_STATS.lock().unwrap().add_measurement(upload_duration);
                    
                    return Ok(Some(CachedData::Gpu(Arc::new(texture))));
                }
            }
            Err(e) => {
                error!("Error opening image: {:?}", e);
                return Err(std::io::ErrorKind::InvalidData);
            }
        }
    }

    Ok(None)
}


pub async fn load_images_async(
    paths: Vec<Option<String>>, 
    cache_strategy: CacheStrategy,
    device: &Arc<wgpu::Device>,
    queue: &Arc<wgpu::Queue>,
    compression_strategy: CompressionStrategy,
    load_operation: LoadOperation
) -> Result<(Vec<Option<CachedData>>, Option<LoadOperation>), std::io::ErrorKind> {
    let start = Instant::now();
    debug!("load_images_async - cache_strategy: {:?}, compression: {:?}", cache_strategy, compression_strategy);

    let futures = paths.into_iter().map(|path| {
        let device = Arc::clone(device);
        let queue = Arc::clone(queue);
        
        async move {
            let path_str = path.as_deref();
            match cache_strategy {
                CacheStrategy::Cpu => {
                    debug!("load_images_async - loading image with CPU strategy");
                    load_image_cpu_async(path_str).await
                },
                CacheStrategy::Gpu => {
                    debug!("load_images_async - loading image with GPU strategy and compression: {:?}", compression_strategy);
                    load_image_gpu_async(path_str, &device, &queue, compression_strategy).await
                },
            }
        }
    });

    let results = join_all(futures).await;
    let duration = start.elapsed();
    debug!("Finished loading images in {:?}", duration);

    let images = results
        .into_iter()
        .map(|result| result.ok().flatten())
        .collect();

    Ok((images, Some(load_operation)))
}


pub async fn pick_folder() -> Result<String, Error> {
    let handle= rfd::AsyncFileDialog::new()
        .set_title("Open Folder with images")
        .pick_folder()
        .await;

    match handle {
        Some(selected_folder) => {
            // Convert the PathBuf to a String
            let selected_folder_string = selected_folder
                .path()
                .to_string_lossy()
                .to_string();

            Ok(selected_folder_string)
        }
        None => Err(Error::DialogClosed),
    }
}

pub async fn pick_file() -> Result<String, Error> {
    let handle = rfd::FileDialog::new()
        .set_title("Open File")
        .add_filter("JPEG and PNG Images", &["jpg", "jpeg", "png"])
        .pick_file();

    match handle {
        Some(file_info) => {
            let path = file_info.as_path();
            // Convert the extension to lowercase for case-insensitive comparison
            if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
                match extension.to_lowercase().as_str() {
                    "jpg" | "jpeg" | "png" => Ok(path.to_string_lossy().to_string()),
                    _ => Err(Error::InvalidExtension),
                }
            } else {
                Err(Error::InvalidExtension)
            }
        }
        None => Err(Error::DialogClosed),
    }
}


#[allow(dead_code)]
pub async fn empty_async_block(operation: LoadOperation) -> Result<(Option<CachedData>, Option<LoadOperation>), std::io::ErrorKind> {
    Ok((None, Some(operation)))
}

pub async fn empty_async_block_vec(operation: LoadOperation, count: usize) -> Result<(Vec<Option<CachedData>>, Option<LoadOperation>), std::io::ErrorKind> {
    Ok((vec![None; count], Some(operation)))
}

pub async fn _literal_empty_async_block() -> Result<(), std::io::ErrorKind> {
    Ok(())
}


pub fn is_file(path: &Path) -> bool {
    fs::metadata(path).map(|metadata| metadata.is_file()).unwrap_or(false)
}

pub fn is_directory(path: &Path) -> bool {
    fs::metadata(path).map(|metadata| metadata.is_dir()).unwrap_or(false)
}

pub fn get_file_index(files: &[PathBuf], file: &PathBuf) -> Option<usize> {
    let file_name = file.file_name()?;
    files.iter().position(|f| f.file_name() == Some(file_name))
}



#[derive(Debug)]
pub enum ImageError {
    NoImagesFound,
    DirectoryError(io::Error),
    // Add other error types as needed
}

impl std::fmt::Display for ImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImageError::NoImagesFound => write!(f, "No supported images found in directory"),
            ImageError::DirectoryError(e) => write!(f, "Directory error: {}", e),
        }
    }
}

impl StdError for ImageError {}

pub fn get_image_paths(directory_path: &Path) ->  Result<Vec<PathBuf>, ImageError> {
    let mut image_paths: Vec<PathBuf> = Vec::new();
    let allowed_extensions = [
        "jpg", "jpeg", "png", "gif", "bmp", "ico", "tiff", "tif",
        "webp", "pnm", "pbm", "pgm", "ppm", "qoi", "tga"
    ];

    let dir_entries = fs::read_dir(directory_path)
        .map_err(|e| ImageError::DirectoryError(e))?;

    for entry in dir_entries.flatten() {
        if let Some(extension) = entry.path().extension().and_then(OsStr::to_str) {
            if allowed_extensions.contains(&extension.to_lowercase().as_str()) {
                image_paths.push(entry.path());
            }
        }
    }

    // Sort paths like Nautilus file viewer. (`image_paths.sort()` won't achieve this)
    if !image_paths.is_empty() {
        alphanumeric_sort::sort_path_slice(&mut image_paths);
        Ok(image_paths)
    } else {
        Err(ImageError::NoImagesFound)
    }
}


const MAX_LOG_LINES: usize = 1000;

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
                format!("{}:{} {}", target, line_num, message)
            } else {
                format!("{} {}", target, message)
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

use env_logger::fmt::Formatter; // Correct import
use chrono::Utc;

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
            format!("{}:{}", module, line)
        } else if let Some(module) = record.module_path() {
            module.to_string()
        } else if let Some(line) = record.line() {
            format!("line:{}", line)
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
    
    writeln!(file, "{} [DEBUG EXPORT] =====================================", timestamp)?;
    writeln!(file, "{} [DEBUG EXPORT] ViewSkater Debug Log Export", timestamp)?;
    writeln!(file, "{} [DEBUG EXPORT] Export timestamp: {}", timestamp, timestamp)?;
    writeln!(file, "{} [DEBUG EXPORT] =====================================", timestamp)?;
    writeln!(file, "{} [DEBUG EXPORT] ", timestamp)?;
    writeln!(file, "{} [DEBUG EXPORT] IMPORTANT: This log captures output from Rust log macros", timestamp)?;
    writeln!(file, "{} [DEBUG EXPORT] (debug!, info!, warn!, error!) but NOT raw println! statements.", timestamp)?;
    writeln!(file, "{} [DEBUG EXPORT] Maximum captured entries: {}", timestamp, MAX_LOG_LINES)?;
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
        println!("DEBUG: Copied {} entries, releasing lock", buffer_size);
    } // Lock is dropped here
    
    println!("DEBUG: Buffer lock released");
    
    if buffer_empty {
        println!("DEBUG: Buffer is empty, writing empty message");
        writeln!(file, "{} [DEBUG EXPORT] No log entries found in buffer", timestamp)?;
        writeln!(file, "{} [DEBUG EXPORT] This may indicate that:", timestamp)?;
        writeln!(file, "{} [DEBUG EXPORT] 1. No log macros have been called yet", timestamp)?;
        writeln!(file, "{} [DEBUG EXPORT] 2. All logs were filtered out by log level settings", timestamp)?;
        writeln!(file, "{} [DEBUG EXPORT] 3. The app just started and no logs have been generated", timestamp)?;
    } else {
        println!("DEBUG: Writing {} log entries", buffer_size);
        writeln!(file, "{} [DEBUG EXPORT] Found {} log entries (showing last {} max):", timestamp, buffer_size, MAX_LOG_LINES)?;
        writeln!(file, "{} [DEBUG EXPORT] =====================================", timestamp)?;
        writeln!(file)?; // Empty line for readability
        
        for (_i, log_entry) in log_entries.iter().enumerate() {
            writeln!(file, "{} {}", timestamp, log_entry)?;
        }
        println!("DEBUG: All entries written");
    }
    
    println!("DEBUG: Writing footer");
    writeln!(file)?; // Final empty line
    writeln!(file, "{} [DEBUG EXPORT] =====================================", timestamp)?;
    writeln!(file, "{} [DEBUG EXPORT] Export completed successfully", timestamp)?;
    writeln!(file, "{} [DEBUG EXPORT] Total entries exported: {}", timestamp, buffer_size)?;
    writeln!(file, "{} [DEBUG EXPORT] =====================================", timestamp)?;
    
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
    // Debug: Check if this is the same buffer that should be receiving logs
    println!("DEBUG: About to export debug logs...");
    if let Ok(buffer) = log_buffer.lock() {
        println!("DEBUG: Buffer size at export time: {}", buffer.len());
        if buffer.len() > 0 {
            println!("DEBUG: First few entries:");
            for (i, entry) in buffer.iter().take(3).enumerate() {
                println!("DEBUG: Entry {}: {}", i, entry);
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
            error!("Failed to export debug logs: {}", e);
            eprintln!("Failed to export debug logs: {}", e);
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
        let header_msg = format!("[PANIC] at {} - {}", location, info);
        let backtrace_header = "[PANIC] Backtrace:";
        
        // Format backtrace lines
        let mut backtrace_lines = Vec::new();
        for line in format!("{:?}", backtrace).lines() {
            backtrace_lines.push(format!("[BACKTRACE] {}", line.trim()));
        }
        
        // Log header to file
        writeln!(file, "{} {}", timestamp, header_msg).expect("Failed to write panic info");
        writeln!(file, "{} {}", timestamp, backtrace_header).expect("Failed to write backtrace header");
        
        // Log backtrace to file
        for line in &backtrace_lines {
            writeln!(file, "{} {}", timestamp, line).expect("Failed to write backtrace line");
        }
        
        // Add double linebreak between backtrace and log entries
        writeln!(file).expect("Failed to write newline");
        writeln!(file).expect("Failed to write second newline");

        // Dump the last N log lines from the buffer with timestamps
        writeln!(file, "{} [PANIC] Last {} log entries:", timestamp, MAX_LOG_LINES)
            .expect("Failed to write log header");

        let buffer = log_buffer.lock().unwrap();
        for log in buffer.iter() {
            writeln!(file, "{} {}", timestamp, log).expect("Failed to write log entry");
        }
        
        // ALSO PRINT TO CONSOLE (this is the new part)
        // Use eprintln! to print to stderr
        eprintln!("\n\n{}", header_msg);
        eprintln!("{}", backtrace_header);
        for line in &backtrace_lines {
            eprintln!("{}", line);
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
                Ok(_) => println!("Opened directory in File Explorer: {}", path),
                Err(e) => eprintln!("Failed to open directory in File Explorer: {}", e),
            }
    } else if cfg!(target_os = "macos") {
        // macOS: Use "open" to open the directory
        match Command::new("open")
            .arg(path)
            .spawn() {
                Ok(_) => println!("Opened directory in Finder: {}", path),
                Err(e) => eprintln!("Failed to open directory in Finder: {}", e),
            }
    } else if cfg!(target_os = "linux") {
        // Linux: Use "xdg-open" to open the directory (works with most desktop environments)
        match Command::new("xdg-open")
            .arg(path)
            .spawn() {
                Ok(_) => println!("Opened directory in File Explorer: {}", path),
                Err(e) => eprintln!("Failed to open directory in File Explorer: {}", e),
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
                        let _ = writeln!(original_stdout, "{}", trimmed);
                        let _ = original_stdout.flush();
                        
                        // Capture to buffer
                        if let Ok(mut buffer) = buffer.lock() {
                            if buffer.len() >= 1000 {
                                buffer.pop_front();
                            }
                            let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.6fZ");
                            buffer.push_back(format!("{} [STDOUT] {}", timestamp, trimmed));
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
        buf.push_back(format!("{} [STDOUT] ViewSkater stdout capture initialized", timestamp));
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
    
    writeln!(file, "{} [STDOUT EXPORT] =====================================", timestamp)?;
    writeln!(file, "{} [STDOUT EXPORT] ViewSkater Stdout Log Export", timestamp)?;
    writeln!(file, "{} [STDOUT EXPORT] Export timestamp: {}", timestamp, timestamp)?;
    writeln!(file, "{} [STDOUT EXPORT] =====================================", timestamp)?;
    writeln!(file, "{} [STDOUT EXPORT] ", timestamp)?;
    writeln!(file, "{} [STDOUT EXPORT] This log captures stdout output including println! statements", timestamp)?;
    writeln!(file, "{} [STDOUT EXPORT] Maximum captured entries: 1000", timestamp)?;
    writeln!(file)?; // Empty line for readability

    // Export all stdout entries from the buffer
    let buffer = stdout_buffer.lock().unwrap();
    if buffer.is_empty() {
        writeln!(file, "{} [STDOUT EXPORT] No stdout entries found in buffer", timestamp)?;
        writeln!(file, "{} [STDOUT EXPORT] Note: Automatic stdout capture is disabled", timestamp)?;
        writeln!(file, "{} [STDOUT EXPORT] Use debug logs (debug!, info!, etc.) for logging instead", timestamp)?;
    } else {
        writeln!(file, "{} [STDOUT EXPORT] Found {} stdout entries:", timestamp, buffer.len())?;
        writeln!(file, "{} [STDOUT EXPORT] =====================================", timestamp)?;
        writeln!(file)?; // Empty line for readability
        
        for stdout_entry in buffer.iter() {
            writeln!(file, "{}", stdout_entry)?;
        }
    }
    
    writeln!(file)?; // Final empty line
    writeln!(file, "{} [STDOUT EXPORT] =====================================", timestamp)?;
    writeln!(file, "{} [STDOUT EXPORT] Export completed successfully", timestamp)?;
    writeln!(file, "{} [STDOUT EXPORT] Total entries exported: {}", timestamp, buffer.len())?;
    writeln!(file, "{} [STDOUT EXPORT] =====================================", timestamp)?;
    
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
    // Debug: Check both buffers
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
            let log_dir = debug_log_path.parent().unwrap_or_else(|| Path::new("."));
            open_in_file_explorer(&log_dir.to_string_lossy().to_string());
        }
        Err(e) => {
            error!("Failed to export debug logs: {}", e);
            eprintln!("Failed to export debug logs: {}", e);
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
                error!("Failed to export stdout logs: {}", e);
                eprintln!("Failed to export stdout logs: {}", e);
            }
        }
    } else {
        println!("Skipping stdout.log export - buffer is empty (stdout capture disabled)");
    }
}
