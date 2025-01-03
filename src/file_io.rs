use std::fs;
use std::path::Path;
use std::path::PathBuf;
use tokio::io::AsyncReadExt;
use std::ffi::OsStr;
use rfd;
use futures::future::join_all;
use crate::image_cache::LoadOperation;
use tokio::fs::File;
use tokio::time::Instant;

#[allow(unused_imports)]
use log::{Level, debug, info, warn, error};

use std::panic;
use std::fs::{OpenOptions};
use std::io::Write;
use std::sync::Mutex;
use std::collections::VecDeque;
use lazy_static::lazy_static;
use env_logger::{fmt::Color, Builder};

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
        match File::open(file_path).await {
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
lazy_static! {
    static ref LOG_BUFFER: Mutex<VecDeque<String>> = Mutex::new(VecDeque::with_capacity(MAX_LOG_LINES));
}

pub fn setup_logger() {
    Builder::from_default_env()
        .format(|buf, record| {
            let level_color = match record.level() {
                Level::Trace => Color::White,
                Level::Debug => Color::Blue,
                Level::Info => Color::Green,
                Level::Warn => Color::Yellow,
                Level::Error => Color::Red,
            };
            let mut level_style = buf.style();
            level_style.set_color(level_color);

            let log_entry = format!("{} {}", record.level(), record.args());
            // Write to the ring buffer
            {
                let mut log_buffer = LOG_BUFFER.lock().unwrap();
                if log_buffer.len() == MAX_LOG_LINES {
                    log_buffer.pop_front(); // Remove the oldest entry
                }
                log_buffer.push_back(log_entry.clone());
            }

            // Write to stdout
            writeln!(buf, "{} {}", level_style.value(record.level()), record.args())
        })
        .init();
}


fn get_log_directory(app_name: &str) -> PathBuf {
    if cfg!(target_os = "linux") {
        dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join(app_name).join("logs")
    } else if cfg!(target_os = "macos") {
        dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join(app_name).join("logs")
    } else if cfg!(target_os = "windows") {
        dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join(app_name).join("logs")
    } else {
        PathBuf::from(".").join(app_name).join("logs")
    }
}

pub fn setup_log_file(app_name: &str) -> PathBuf {
    let log_dir = get_log_directory(app_name);

    // Ensure the directory exists
    std::fs::create_dir_all(&log_dir).expect("Failed to create log directory");

    // Construct the log file path
    log_dir.join("runtime_error.log")
}


pub fn setup_panic_hook(log_file: PathBuf) {
    panic::set_hook(Box::new(move |info| {
        let backtrace = backtrace::Backtrace::new();
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true) // Truncate the file to clear previous content
            .open(&log_file)
            .expect("Failed to open log file");

        writeln!(file, "Panic occurred: {}", info)
            .expect("Failed to write panic info");
        writeln!(file, "Backtrace:\n{:?}\n", backtrace)
            .expect("Failed to write backtrace");

        // Write the last `MAX_LOG_LINES` log entries
        writeln!(file, "Last {} log entries:\n", MAX_LOG_LINES).expect("Failed to write log header");
        let log_buffer = LOG_BUFFER.lock().unwrap();
        for log in log_buffer.iter() {
            writeln!(file, "{}", log).expect("Failed to write log entry");
        }
    }));
}
