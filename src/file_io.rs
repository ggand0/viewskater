use std::fs;
use std::path::Path;
use std::path::PathBuf;
use tokio::io::AsyncReadExt;
use std::ffi::OsStr;
use rfd;
use futures::future::join_all;
use crate::image_cache::LoadOperation;
use tokio::time::Instant;

#[allow(unused_imports)]
use log::{Level, debug, info, warn, error};

use std::panic;
use std::fs::{OpenOptions};
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::collections::VecDeque;
use env_logger::{fmt::Color};
use log::{LevelFilter, Metadata, Record};
use backtrace::Backtrace;
use std::process::Command;

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
async fn load_image_async(path: Option<&str>) -> Result<Option<Vec<u8>>, std::io::ErrorKind> {
    // Load a single image asynchronously
    if let Some(path) = path {
        let file_path = Path::new(path);
        match tokio::fs::File::open(file_path).await {
            Ok(mut file) => {
                let mut buffer = Vec::new();
                if file.read_to_end(&mut buffer).await.is_ok() {
                    Ok(Some(buffer))
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

pub async fn load_images_async(paths: Vec<Option<String>>, load_operation: LoadOperation) -> Result<(Vec<Option<Vec<u8>>>, Option<LoadOperation>), std::io::ErrorKind> {
    let start = Instant::now();
    let futures = paths.into_iter().map(|path| {
        let future = async move {
            let path_str = path.as_deref();
            load_image_async(path_str).await
        };
        future
    });
    let results = join_all(futures).await;
    let duration = start.elapsed();
    debug!("Finished loading images in {:?}", duration);

    let mut images = Vec::new();
    for result in results {
        match result {
            Ok(image_data) => images.push(image_data),
            Err(_) => images.push(None),
        }
    }

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
pub async fn empty_async_block(operation: LoadOperation) -> Result<(Option<Vec<u8>>, Option<LoadOperation>), std::io::ErrorKind> {
    Ok((None, Some(operation)))
}

pub async fn empty_async_block_vec(operation: LoadOperation, count: usize) -> Result<(Vec<Option<Vec<u8>>>, Option<LoadOperation>), std::io::ErrorKind> {
    Ok((vec![None; count], Some(operation)))
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

pub fn get_image_paths(directory_path: &Path) -> Vec<PathBuf> {
    let mut image_paths: Vec<PathBuf> = Vec::new();
    let allowed_extensions = ["jpg", "jpeg", "png", /* Add other image extensions */];

    if let Ok(paths) = fs::read_dir(directory_path) {
        for entry in paths.flatten() {
            if let Some(extension) = entry.path().extension().and_then(OsStr::to_str) {
                // Check if the extension is among allowed extensions
                if allowed_extensions.contains(&extension.to_lowercase().as_str()) {
                    image_paths.push(entry.path());
                }
            }
        }
    }

    // Sort paths like Nautilus file viewer. `image_paths.sort()` does not work as expected
    alphanumeric_sort::sort_path_slice(&mut image_paths);
    image_paths
}


const MAX_LOG_LINES: usize = 1000;

struct BufferLogger {
    log_buffer: Arc<Mutex<VecDeque<String>>>,
}

impl BufferLogger {
    fn new() -> Self {
        Self {
            log_buffer: Arc::new(Mutex::new(VecDeque::with_capacity(MAX_LOG_LINES))),
        }
    }

    fn log_to_buffer(&self, message: &str, target: &str) {
        if target.starts_with("view_skater") {
            let mut buffer = self.log_buffer.lock().unwrap();
            if buffer.len() == MAX_LOG_LINES {
                buffer.pop_front();
            }
            buffer.push_back(message.to_string());
        }
    }

    #[allow(dead_code)]
    fn dump_logs(&self) -> Vec<String> {
        let buffer = self.log_buffer.lock().unwrap();
        buffer.iter().cloned().collect()
    }

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
            self.log_to_buffer(&message, record.target());
        }
    }

    fn flush(&self) {}
}

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

pub fn setup_logger(_app_name: &str) -> Arc<Mutex<VecDeque<String>>> {
    let buffer_logger = BufferLogger::new();
    let shared_buffer = buffer_logger.get_shared_buffer();

    let mut builder = env_logger::Builder::new();
    if std::env::var("RUST_LOG").is_ok() {
        builder.parse_env("RUST_LOG");
    } else if cfg!(debug_assertions) {
        builder.filter(Some("view_skater"), LevelFilter::Debug);
    } else {
        builder.filter(Some("view_skater"), LevelFilter::Info);
    }

    builder.filter(None, LevelFilter::Off);

    builder.format(|buf, record| {
        let mut style = buf.style();
        match record.level() {
            Level::Error => style.set_color(Color::Red),
            Level::Warn => style.set_color(Color::Yellow),
            Level::Info => style.set_color(Color::Green),
            Level::Debug => style.set_color(Color::Blue),
            Level::Trace => style.set_color(Color::White),
        };
        writeln!(buf, "{:<5} {}", style.value(record.level()), record.args())
    });

    let console_logger = builder.build();

    let composite_logger = CompositeLogger {
        console_logger,
        buffer_logger,
    };

    log::set_boxed_logger(Box::new(composite_logger)).expect("Failed to set logger");
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
        let backtrace = Backtrace::new();
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&log_file_path)
            .expect("Failed to open panic log file");

        writeln!(file, "Panic occurred: {}", info).expect("Failed to write panic info");
        writeln!(file, "Backtrace:\n{:?}\n", backtrace).expect("Failed to write backtrace");

        writeln!(file, "Last {} log entries:\n", MAX_LOG_LINES).expect("Failed to write log header");

        let buffer = log_buffer.lock().unwrap();
        for log in buffer.iter() {
            writeln!(file, "{}", log).expect("Failed to write log entry");
        }
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