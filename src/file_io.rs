use std::fs;
use std::path::Path;
use std::path::PathBuf;
use tokio::io::AsyncReadExt;
use std::ffi::OsStr;
use rfd;
use futures::future::join_all;
use crate::image_cache::LoadOperation;
use image::io::Reader as ImageReader;
use image::{DynamicImage, ImageOutputFormat};
use tokio::fs::File;
use tokio::time::Instant;
use futures::FutureExt;


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

/*async fn load_image_async(path: &str) -> image::DynamicImage {
    let start_time = Instant::now();
    println!("Starting to load image from: {}", path);

    let mut file = File::open(path).await.unwrap();
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).await.unwrap();
    let image = image::load_from_memory(&buf).unwrap();

    let duration = start_time.elapsed();
    println!("Finished loading image from: {} in {:?}", path, duration);

    image
}

pub async fn load_images_async(paths: Vec<&str>) -> Vec<image::DynamicImage> {
    let futures = paths.into_iter().map(|path| load_image_async(path));
    let results = join_all(futures).await;
    results
}*/



// v1
/*async fn load_image_async(path: &str) -> Result<DynamicImage, std::io::ErrorKind> {
    let mut file = File::open(path).await.map_err(|e| e.kind())?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).await.map_err(|e| e.kind())?;
    let image = image::load_from_memory(&buf).map_err(|_| std::io::ErrorKind::InvalidData)?;
    Ok(image)
}

pub async fn load_images_async(paths: Vec<&str>, load_operation: LoadOperation) -> Result<(Vec<Option<Vec<u8>>>, Option<LoadOperation>), std::io::ErrorKind> {
    let futures = paths.into_iter().map(|path| load_image_async(path));
    let results = join_all(futures).await;

    let mut images = Vec::new();
    for result in results {
        match result {
            Ok(image) => {
                let mut buf = Vec::new();
                image.write_to(&mut buf, ImageOutputFormat::Png).map_err(|_| std::io::ErrorKind::InvalidData)?;
                images.push(Some(buf));
            }
            Err(_) => images.push(None),
        }
    }

    Ok((images, Some(load_operation)))
}*/

// v2
/*async fn load_image_async(path: Option<&str>) -> Result<Option<Vec<u8>>, std::io::ErrorKind> {
    if let Some(path) = path {
        let mut file = File::open(path).await.map_err(|e| e.kind())?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).await.map_err(|e| e.kind())?;
        let image = image::load_from_memory(&buf).map_err(|_| std::io::ErrorKind::InvalidData)?;

        let mut img_buf = Vec::new();
        image.write_to(&mut img_buf, ImageOutputFormat::Png).map_err(|_| std::io::ErrorKind::InvalidData)?;
        Ok(Some(img_buf))
    } else {
        Ok(None)
    }
}

pub async fn load_images_async(paths: Vec<Option<&str>>, load_operation: LoadOperation) -> Result<(Vec<Option<Vec<u8>>>, Option<LoadOperation>), std::io::ErrorKind> {
    let futures = paths.into_iter().map(load_image_async);
    let results = join_all(futures).await;

    let mut images = Vec::new();
    for result in results {
        match result {
            Ok(image_data) => images.push(image_data),
            Err(_) => images.push(None),
        }
    }

    Ok((images, Some(load_operation)))
}*/

async fn load_image_async(path: Option<&str>) -> Result<Option<Vec<u8>>, std::io::ErrorKind> {
    if let Some(path) = path {
        let mut file = File::open(path).await.map_err(|e| e.kind())?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).await.map_err(|e| e.kind())?;
        let image = image::load_from_memory(&buf).map_err(|_| std::io::ErrorKind::InvalidData)?;

        let mut img_buf = Vec::new();
        image.write_to(&mut img_buf, ImageOutputFormat::Png).map_err(|_| std::io::ErrorKind::InvalidData)?;
        Ok(Some(img_buf))
    } else {
        Ok(None)
    }
}

pub async fn load_images_async(paths: Vec<Option<String>>, load_operation: LoadOperation) -> Result<(Vec<Option<Vec<u8>>>, Option<LoadOperation>), std::io::ErrorKind> {
    let futures = paths.into_iter().map(|path| {
        let future = async move {
            let path_str = path.as_deref();
            load_image_async(path_str).await
        };
        future
    });
    let results = join_all(futures).await;

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
    // let handle = rfd::AsyncFileDialog::new().set_title("Open Folder with images").pick_folder().await
    //     .ok_or(Error::DialogClosed);

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
