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

static IMAGE_LOAD_STATS: Lazy<Mutex<TimingStats>> = Lazy::new(|| {
    Mutex::new(TimingStats::new("Image Load"))
});
static GPU_UPLOAD_STATS: Lazy<Mutex<TimingStats>> = Lazy::new(|| {
    Mutex::new(TimingStats::new("GPU Upload"))
});

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
    queue: &Arc<wgpu::Queue>
) -> Result<Option<CachedData>, std::io::ErrorKind> {
    if let Some(path) = path {
        let file_path = Path::new(path);
        let start = Instant::now();

        match image::open(file_path) {
            Ok(img) => {
                let rgba_image = img.to_rgba8();
                let (width, height) = img.dimensions();
                let duration = start.elapsed();
                IMAGE_LOAD_STATS.lock().unwrap().add_measurement(duration);

                let upload_start = Instant::now();
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("AsyncLoadedTexture"),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });

                queue.write_texture(
                    wgpu::ImageCopyTexture {
                        texture: &texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &rgba_image,
                    wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row: Some(4 * width),
                        rows_per_image: None,
                    },
                    wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                );
                let upload_duration = upload_start.elapsed();
                GPU_UPLOAD_STATS.lock().unwrap().add_measurement(upload_duration);

                return Ok(Some(CachedData::Gpu(Arc::new(texture))));
            }
            Err(_) => return Err(std::io::ErrorKind::InvalidData),
        }
    }

    Ok(None)
}


pub async fn load_images_async(
    paths: Vec<Option<String>>, 
    cache_strategy: CacheStrategy,
    device: &Arc<wgpu::Device>,
    queue: &Arc<wgpu::Queue>,
    load_operation: LoadOperation
) -> Result<(Vec<Option<CachedData>>, Option<LoadOperation>), std::io::ErrorKind> {
    let start = Instant::now();
    debug!("load_images_async - cache_strategy: {:?}", cache_strategy);

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
                    debug!("load_images_async - loading image with GPU strategy");
                    load_image_gpu_async(path_str, &device, &queue).await
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
    let allowed_extensions = ["jpg", "jpeg", "png", /* Add other image extensions */];

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
        if target.starts_with("view_skater") {
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
        metadata.target().starts_with("view_skater") && metadata.level() <= LevelFilter::Debug
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
            builder.filter(Some("view_skater"), LevelFilter::Debug);
        } else {
            // In release mode, only show errors by default
            builder.filter(Some("view_skater"), LevelFilter::Error);
        }
    }

    // Filter out all other crates' logs
    builder.filter(None, LevelFilter::Off);

    builder.format(|buf: &mut Formatter, record: &Record| {
        let mut level_style = buf.style();
        let mut meta_style = buf.style(); // For timestamp & filename/line number
    
        // Set colors
        match record.level() {
            Level::Error => level_style.set_color(Color::Red).set_bold(true),
            Level::Warn => level_style.set_color(Color::Yellow).set_bold(true),
            Level::Info => level_style.set_color(Color::Green).set_bold(true),
            Level::Debug => level_style.set_color(Color::Blue).set_bold(true),
            Level::Trace => level_style.set_color(Color::White),
        };
    
        meta_style.set_color(Color::Rgb(120, 120, 120)); // Dark grey for timestamps & filename/line numbers
    
        let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%S%.6fZ"); // ISO 8601 UTC format
        
        // Include module path and line number in formatted output
        let module_info = if let (Some(module), Some(line)) = (record.module_path(), record.line()) {
            format!("{}:{}", module, line)
        } else if let Some(module) = record.module_path() {
            module.to_string()
        } else if let Some(line) = record.line() {
            format!("line:{}", line)
        } else {
            "unknown".to_string()
        };
    
        writeln!(
            buf,
            "{} {} {} {}",
            meta_style.value(timestamp),                      // Dark grey timestamp
            level_style.value(record.level()),               // Colorized log level
            meta_style.value(module_info),                   // Module:line
            record.args()                                    // Log message
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
        Command::new("explorer")
            .arg(path)
            .spawn()
            .expect("Failed to open directory in File Explorer");
    } else if cfg!(target_os = "macos") {
        // macOS: Use "open" to open the directory
        Command::new("open")
            .arg(path)
            .spawn()
            .expect("Failed to open directory in Finder");
    } else if cfg!(target_os = "linux") {
        // Linux: Use "xdg-open" to open the directory (works with most desktop environments)
        Command::new("xdg-open")
            .arg(path)
            .spawn()
            .expect("Failed to open directory in File Explorer");
    } else {
        error!("Opening directories is not supported on this OS.");
    }
}