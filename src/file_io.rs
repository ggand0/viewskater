use std::fs;
use std::path::Path;
use std::path::PathBuf;
use tokio::io::AsyncReadExt;
use futures::future::join_all;
use crate::cache::img_cache::LoadOperation;
use tokio::time::Instant;

#[allow(unused_imports)]
use log::{Level, debug, info, warn, error};

use std::error::Error as StdError;
use std::io;
use std::io::Cursor;
use std::sync::{Arc, Mutex};
use once_cell::sync::Lazy;
use image::{GenericImageView, ImageReader};
use iced_wgpu::wgpu;

use crate::cache::img_cache::CachedData;
use crate::utils::timing::TimingStats;
use crate::cache::img_cache::CacheStrategy;
use iced_wgpu::engine::CompressionStrategy;
use image::DynamicImage;

const ALLOWED_EXTENSIONS: [&str; 15] = ["jpg", "jpeg", "png", "gif", "bmp", "ico", "tiff", "tif",
        "webp", "pnm", "pbm", "pgm", "ppm", "qoi", "tga"];

/// Check if the given bytes represent a JPEG 2000 file by checking magic bytes
#[cfg(feature = "jp2")]
fn is_jp2_format(bytes: &[u8]) -> bool {
    // JP2 file format: starts with 0x0000000C 6A502020 0D0A870A
    // or JPEG 2000 codestream: starts with 0xFF4FFF51
    if bytes.len() < 12 {
        return false;
    }

    // JP2 container format magic
    let jp2_magic = [0x00, 0x00, 0x00, 0x0C, 0x6A, 0x50, 0x20, 0x20, 0x0D, 0x0A, 0x87, 0x0A];
    if bytes.starts_with(&jp2_magic) {
        return true;
    }

    // Raw JPEG 2000 codestream (j2k/j2c)
    if bytes.len() >= 4 && bytes[0] == 0xFF && bytes[1] == 0x4F && bytes[2] == 0xFF && bytes[3] == 0x51 {
        return true;
    }

    false
}

/// Decode JPEG 2000 image from bytes
#[cfg(feature = "jp2")]
fn decode_jp2(bytes: &[u8]) -> Result<DynamicImage, std::io::ErrorKind> {
    use jpeg2k::Image as Jp2Image;

    let jp2_image = Jp2Image::from_bytes(bytes)
        .map_err(|e| {
            error!("Failed to decode JPEG 2000 image: {}", e);
            std::io::ErrorKind::InvalidData
        })?;

    // TryFrom is implemented for &Image, not Image
    DynamicImage::try_from(&jp2_image)
        .map_err(|e: jpeg2k::error::Error| {
            error!("Failed to convert JPEG 2000 to DynamicImage: {}", e);
            std::io::ErrorKind::InvalidData
        })
}

/// Decode image from bytes, handling both standard formats and JPEG 2000.
/// Applies EXIF orientation correction for supported formats (primarily JPEG).
pub fn decode_image_from_bytes(bytes: &[u8]) -> Result<DynamicImage, std::io::ErrorKind> {
    // Check for JPEG 2000 format first when feature is enabled
    // Note: JP2 doesn't use EXIF orientation, so decode directly
    #[cfg(feature = "jp2")]
    if is_jp2_format(bytes) {
        return decode_jp2(bytes);
    }

    // Use EXIF-aware decoding for standard formats
    crate::exif_utils::decode_with_exif_orientation(bytes)
}

/// Check if a file extension is a supported image format
fn is_supported_extension(ext: &str) -> bool {
    let ext_lower = ext.to_lowercase();

    if ALLOWED_EXTENSIONS.contains(&ext_lower.as_str()) {
        return true;
    }

    #[cfg(feature = "jp2")]
    if ALLOWED_EXTENSIONS_JP2.contains(&ext_lower.as_str()) {
        return true;
    }

    false
}
#[cfg(feature = "jp2")]
const ALLOWED_EXTENSIONS_JP2: [&str; 3] = ["jp2", "j2k", "j2c"];
pub const ALLOWED_COMPRESSED_FILES: [&str; 3] = ["zip", "rar", "7z"];

pub fn supported_image(name: &str) -> bool {
    // Filter out macOS metadata files
    if name.starts_with("__MACOSX/") {
        return false;
    }

    let ext = name.split('.').next_back().unwrap_or("").to_lowercase();

    if ALLOWED_EXTENSIONS.contains(&ext.as_str()) {
        return true;
    }

    #[cfg(feature = "jp2")]
    if ALLOWED_EXTENSIONS_JP2.contains(&ext.as_str()) {
        return true;
    }

    false
}

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

/// Reads an image file into a byte vector with dispatch based on PathSource.
///
/// This function uses type-safe routing for optimal performance:
/// - Filesystem: Direct filesystem I/O with mmap optimization
/// - Preloaded: Direct HashMap lookup in ArchiveCache
/// - Archive: Direct archive reading without unnecessary checks
///
/// # Arguments
/// * `path_source` - The typed path indicating source and loading strategy
/// * `archive_cache` - The archive cache for archive/preloaded content
///
/// # Returns
/// * `Ok(Vec<u8>)` - The raw bytes of the image file
/// * `Err(io::Error)` - An error if reading fails
pub fn read_image_bytes(path_source: &crate::cache::img_cache::PathSource, archive_cache: Option<&mut crate::archive_cache::ArchiveCache>) -> Result<Vec<u8>, std::io::Error> {
    use std::fs::File;
    use std::io::{self, Read};
    use memmap2::Mmap;
    use crate::cache::img_cache::PathSource;

    // Dispatch based on PathSource type
    match path_source {
        PathSource::Filesystem(path) => {
            // Direct filesystem reading with mmap optimization
            if !path.exists() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("Filesystem file not found: {}", path.display())
                ));
            }

            let file = File::open(path)?;
            let metadata = file.metadata()?;
            let file_size = metadata.len() as usize;

            // Use mmap for files over 1MB, regular reading for smaller files
            if file_size > 1_048_576 {
                let mmap = unsafe { Mmap::map(&file)? };
                let bytes = mmap.to_vec();
                debug!("Read {} bytes from filesystem using mmap: {}", bytes.len(), path.display());
                Ok(bytes)
            } else {
                // For smaller files, regular reading is often faster
                let mut buffer = Vec::with_capacity(file_size);
                let mut file = File::open(path)?;
                file.read_to_end(&mut buffer)?;
                debug!("Read {} bytes from filesystem: {}", buffer.len(), path.display());
                Ok(buffer)
            }
        },

        PathSource::Preloaded(path) => {
            // Direct HashMap lookup - fastest path for preloaded content
            let cache = archive_cache.ok_or_else(|| io::Error::new(
                io::ErrorKind::InvalidInput,
                "Archive cache required for preloaded content"
            ))?;

            let path_str = path.to_string_lossy();
            if let Some(data) = cache.get_preloaded_data(&path_str) {
                debug!("Using preloaded data for: {}", path_str);
                Ok(data.to_vec())
            } else {
                Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("Preloaded data not found: {}", path_str)
                ))
            }
        },

        PathSource::Archive(path) => {
            // Direct archive reading - no filesystem checks
            let cache = archive_cache.ok_or_else(|| io::Error::new(
                io::ErrorKind::InvalidInput,
                "Archive cache required for archive content"
            ))?;

            let path_str = path.to_string_lossy();
            debug!("Reading from archive: {}", path_str);
            cache.read_from_archive(&path_str)
                .map_err(|e| io::Error::other(format!("Failed to read from archive: {}", e)))
        }
    }
}

/// Reads image bytes and returns (bytes, file_size_in_bytes)
pub fn read_image_bytes_with_size(path_source: &crate::cache::img_cache::PathSource, archive_cache: Option<&mut crate::archive_cache::ArchiveCache>) -> Result<(Vec<u8>, u64), std::io::Error> {
    use std::fs::File;
    use std::io::{self, Read};
    use memmap2::Mmap;
    use crate::cache::img_cache::PathSource;

    // Dispatch based on PathSource type
    match path_source {
        PathSource::Filesystem(path) => {
            // Direct filesystem reading with mmap optimization
            if !path.exists() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("Filesystem file not found: {}", path.display())
                ));
            }

            let file = File::open(path)?;
            let metadata = file.metadata()?;
            let file_size = metadata.len();

            // Use mmap for files over 1MB, regular reading for smaller files
            if file_size > 1_048_576 {
                let mmap = unsafe { Mmap::map(&file)? };
                let bytes = mmap.to_vec();
                debug!("Read {} bytes from filesystem using mmap: {}", bytes.len(), path.display());
                Ok((bytes, file_size))
            } else {
                // For smaller files, regular reading is often faster
                let mut buffer = Vec::with_capacity(file_size as usize);
                let mut file = File::open(path)?;
                file.read_to_end(&mut buffer)?;
                debug!("Read {} bytes from filesystem: {}", buffer.len(), path.display());
                Ok((buffer, file_size))
            }
        },

        PathSource::Preloaded(path) => {
            // Direct HashMap lookup - fastest path for preloaded content
            let cache = archive_cache.ok_or_else(|| io::Error::new(
                io::ErrorKind::InvalidInput,
                "Archive cache required for preloaded content"
            ))?;

            let path_str = path.to_string_lossy();
            if let Some(data) = cache.get_preloaded_data(&path_str) {
                debug!("Using preloaded data for: {}", path_str);
                let file_size = data.len() as u64;
                Ok((data.to_vec(), file_size))
            } else {
                Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("Preloaded data not found: {}", path_str)
                ))
            }
        },

        PathSource::Archive(path) => {
            // Direct archive reading - no filesystem checks
            let cache = archive_cache.ok_or_else(|| io::Error::new(
                io::ErrorKind::InvalidInput,
                "Archive cache required for archive content"
            ))?;

            let path_str = path.to_string_lossy();
            debug!("Reading from archive: {}", path_str);
            let bytes = cache.read_from_archive(&path_str)
                .map_err(|e| io::Error::other(format!("Failed to read from archive: {}", e)))?;
            let file_size = bytes.len() as u64;
            Ok((bytes, file_size))
        }
    }
}

/// Gets file size efficiently without reading the entire file content.
/// For filesystem files, uses std::fs::metadata() which only reads the inode.
/// For archive/preloaded content, reads from archive cache.
pub fn get_file_size(path_source: &crate::cache::img_cache::PathSource, archive_cache: Option<&mut crate::archive_cache::ArchiveCache>) -> u64 {
    use crate::cache::img_cache::PathSource;

    match path_source {
        PathSource::Filesystem(path) => {
            // Use fs::metadata() - only reads inode, not file content (O(1) operation)
            std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
        },
        PathSource::Preloaded(path) => {
            // Need to check preloaded data length
            if let Some(cache) = archive_cache {
                let path_str = path.to_string_lossy();
                cache.get_preloaded_data(&path_str)
                    .map(|data| data.len() as u64)
                    .unwrap_or(0)
            } else {
                0
            }
        },
        PathSource::Archive(path) => {
            // For archives, we need to read from cache to get size
            if let Some(cache) = archive_cache {
                let path_str = path.to_string_lossy();
                cache.read_from_archive(&path_str)
                    .map(|bytes| bytes.len() as u64)
                    .unwrap_or(0)
            } else {
                0
            }
        }
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
async fn load_image_cpu_async(path_source: Option<crate::cache::img_cache::PathSource>, archive_cache: Option<Arc<Mutex<crate::archive_cache::ArchiveCache>>>) -> Result<Option<(CachedData, crate::cache::img_cache::ImageMetadata)>, std::io::ErrorKind> {
    use crate::cache::img_cache::ImageMetadata;

    // Load a single image asynchronously
    if let Some(path_source) = path_source {
        let start = Instant::now();
        debug!("load_image_cpu_async - Starting to load: {:?}", path_source.file_name());

        // Dispatch based on PathSource type - get bytes and file size
        let (bytes, file_size) = match &path_source {
            crate::cache::img_cache::PathSource::Filesystem(path) => {
                // Direct filesystem reading - get file size from metadata
                let metadata = match tokio::fs::metadata(path).await {
                    Ok(m) => m,
                    Err(e) => return Err(e.kind()),
                };
                let file_size = metadata.len();

                match tokio::fs::read(path).await {
                    Ok(bytes) => (bytes, file_size),
                    Err(e) => return Err(e.kind()),
                }
            },
            crate::cache::img_cache::PathSource::Archive(_) | crate::cache::img_cache::PathSource::Preloaded(_) => {
                // Archive content requires archive cache
                if let Some(cache_arc) = archive_cache {
                    let cache_bytes_result = {
                        match cache_arc.lock() {
                            Ok(mut cache) => read_image_bytes_with_size(&path_source, Some(&mut *cache)),
                            Err(_) => Err(std::io::Error::other("Archive cache lock failed")),
                        }
                    };

                    match cache_bytes_result {
                        Ok((bytes, file_size)) => (bytes, file_size),
                        Err(e) => {
                            error!("Failed to read archive content: {}", e);
                            return Err(std::io::ErrorKind::Other);
                        }
                    }
                } else {
                    error!("Archive cache required for archive/preloaded content");
                    return Err(std::io::ErrorKind::InvalidInput);
                }
            }
        };

        // Get image dimensions efficiently using header-only read
        let (width, height) = ImageReader::new(Cursor::new(&bytes))
            .with_guessed_format()
            .ok()
            .and_then(|r| r.into_dimensions().ok())
            .unwrap_or((0, 0));

        let metadata = ImageMetadata::new(width, height, file_size);

        let total_time = start.elapsed();
        debug!("load_image_cpu_async - Total load time: {:?}", total_time);
        Ok(Some((CachedData::Cpu(bytes), metadata)))
    } else {
        Ok(None)
    }
}

#[allow(dead_code)]
async fn load_image_gpu_async(
    path_source: Option<crate::cache::img_cache::PathSource>,
    device: &Arc<wgpu::Device>,
    queue: &Arc<wgpu::Queue>,
    compression_strategy: CompressionStrategy,
    archive_cache: Option<Arc<Mutex<crate::archive_cache::ArchiveCache>>>
) -> Result<Option<(CachedData, crate::cache::img_cache::ImageMetadata)>, std::io::ErrorKind> {
    use crate::cache::img_cache::ImageMetadata;

    if let Some(path_source) = path_source {
        let start = Instant::now();

        // Dispatch based on PathSource type - get decoded image and file size
        let (img_result, file_size) = match &path_source {
            crate::cache::img_cache::PathSource::Filesystem(path) => {
                // Read bytes and use unified decode function for format detection
                // Get file size first
                let file_size = match std::fs::metadata(path) {
                    Ok(m) => m.len(),
                    Err(e) => {
                        error!("Failed to read filesystem metadata: {}", e);
                        return Err(e.kind());
                    }
                };
                match std::fs::read(path) {
                    Ok(bytes) => (decode_image_from_bytes(&bytes), file_size),
                    Err(e) => {
                        error!("Failed to read filesystem image: {}", e);
                        return Err(e.kind());
                    }
                }
            },
            crate::cache::img_cache::PathSource::Archive(_) | crate::cache::img_cache::PathSource::Preloaded(_) => {
                // Archive content requires archive cache
                if let Some(cache_arc) = &archive_cache {
                    let cache_bytes_result = {
                        match cache_arc.lock() {
                            Ok(mut cache) => read_image_bytes_with_size(&path_source, Some(&mut *cache)),
                            Err(e) => {
                                error!("Failed to lock archive cache: {}", e);
                                Err(std::io::Error::other("Archive cache lock failed"))
                            }
                        }
                    };

                    match cache_bytes_result {
                        Ok((bytes, file_size)) => (decode_image_from_bytes(&bytes), file_size),
                        Err(e) => {
                            error!("Failed to read archive content: {}", e);
                            return Err(std::io::ErrorKind::Other);
                        }
                    }
                } else {
                    error!("Archive cache required for archive/preloaded content");
                    return Err(std::io::ErrorKind::InvalidInput);
                }
            }
        };

        match img_result {
            Ok(img) => {
                // Apply size check and resize if image exceeds 8192px limit
                let img = crate::cache::cache_utils::check_and_resize_if_oversized(img);

                let (width, height) = img.dimensions();
                let rgba = img.to_rgba8();
                let rgba_data = rgba.as_raw();

                // Create metadata with original file size and current dimensions
                let metadata = ImageMetadata::new(width, height, file_size);

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
                        rgba_data, width, height
                    );

                    // Upload using the utility
                    crate::cache::cache_utils::upload_compressed_texture(
                        queue, &texture, &compressed_data, width, height, row_bytes
                    );

                    let upload_duration = upload_start.elapsed();
                    GPU_UPLOAD_STATS.lock().unwrap().add_measurement(upload_duration);

                    return Ok(Some((CachedData::BC1(Arc::new(texture)), metadata)));
                } else {
                    // Upload uncompressed
                    crate::cache::cache_utils::upload_uncompressed_texture(
                        queue, &texture, rgba_data, width, height
                    );

                    let upload_duration = upload_start.elapsed();
                    GPU_UPLOAD_STATS.lock().unwrap().add_measurement(upload_duration);

                    return Ok(Some((CachedData::Gpu(Arc::new(texture)), metadata)));
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
    paths: Vec<Option<crate::cache::img_cache::PathSource>>,
    cache_strategy: CacheStrategy,
    device: &Arc<wgpu::Device>,
    queue: &Arc<wgpu::Queue>,
    compression_strategy: CompressionStrategy,
    load_operation: LoadOperation,
    archive_caches: Vec<Option<Arc<Mutex<crate::archive_cache::ArchiveCache>>>>
) -> Result<(Vec<Option<CachedData>>, Vec<Option<crate::cache::img_cache::ImageMetadata>>, Option<LoadOperation>), std::io::ErrorKind> {
    let start = Instant::now();
    debug!("load_images_async - cache_strategy: {:?}, compression: {:?}", cache_strategy, compression_strategy);

    let futures = paths.into_iter().enumerate().map(|(i, path)| {
        let device = Arc::clone(device);
        let queue = Arc::clone(queue);
        let pane_archive_cache = archive_caches.get(i).cloned().flatten();

        async move {
            match cache_strategy {
                CacheStrategy::Cpu => {
                    debug!("load_images_async - loading image with CPU strategy");
                    load_image_cpu_async(path, pane_archive_cache).await
                },
                CacheStrategy::Gpu => {
                    debug!("load_images_async - loading image with GPU strategy and compression: {:?}", compression_strategy);
                    load_image_gpu_async(path, &device, &queue, compression_strategy, pane_archive_cache).await
                },
            }
        }
    });

    let results = join_all(futures).await;
    let duration = start.elapsed();
    debug!("Finished loading images in {:?}", duration);

    // Separate images and metadata from the results
    let mut images = Vec::new();
    let mut metadata_vec = Vec::new();

    for result in results {
        match result.ok().flatten() {
            Some((data, metadata)) => {
                images.push(Some(data));
                metadata_vec.push(Some(metadata));
            }
            None => {
                images.push(None);
                metadata_vec.push(None);
            }
        }
    }

    Ok((images, metadata_vec, Some(load_operation)))
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
    // https://stackoverflow.com/a/71194526
    #[cfg(feature = "jp2")]
    let extensions = [&ALLOWED_COMPRESSED_FILES[..], &ALLOWED_EXTENSIONS[..], &ALLOWED_EXTENSIONS_JP2[..]].concat();
    #[cfg(not(feature = "jp2"))]
    let extensions = [&ALLOWED_COMPRESSED_FILES[..], &ALLOWED_EXTENSIONS[..]].concat();
    let handle = rfd::FileDialog::new()
        .set_title("Open File")
        .add_filter("Supported Files", extensions.as_slice())
        .pick_file();

    match handle {
        Some(file_info) => {
            let path = file_info.as_path();
            // Convert the extension to lowercase for case-insensitive comparison
            if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
                if extensions.contains(&extension.to_lowercase().as_str()) {
                    Ok(path.to_string_lossy().to_string())
                } else {
                    Err(Error::InvalidExtension)
                }
            } else {
                Err(Error::InvalidExtension)
            }
        }
        None => Err(Error::DialogClosed),
    }
}

/// Show memory warning dialog for large solid 7z archives
/// Returns true if user wants to proceed, false if cancelled
pub fn show_memory_warning_sync(archive_size_mb: u64, available_gb: f64, is_recommended: bool) -> bool {
    let warning_level = if is_recommended {
        "Notice"
    } else {
        "Warning"
    };

    let memory_info = if available_gb > 0.0 {
        format!("Available memory: {:.1} GB\n\n", available_gb)
    } else {
        // Don't show memory size when it's 0.0 GB
        // related: https://github.com/GuillaumeGomez/sysinfo/issues/1030
        String::new()
    };

    let memory_note = if available_gb == 0.0 {
        "Memory information unavailable on this system."
    } else if is_recommended {
        "Sufficient memory available, but archive is large."
    } else {
        "Low available memory - may cause system slowdown."
    };

    let message = format!(
        "{}: Large Archive Detected\n\n\
        Archive size: {:.1} MB\n\
        {}{}\n\n\
        The application will load the archive into memory for optimal performance. \
        This may take a moment and use significant RAM.\n\n\
        Continue?",
        warning_level, archive_size_mb, memory_info, memory_note
    );

    let dialog_result = rfd::MessageDialog::new()
        .set_title("ViewSkater")
        .set_description(&message)
        .set_buttons(rfd::MessageButtons::YesNo)
        .set_level(if is_recommended {
            rfd::MessageLevel::Info
        } else {
            rfd::MessageLevel::Warning
        })
        .show();

    matches!(dialog_result, rfd::MessageDialogResult::Yes)
}


#[allow(dead_code)]
pub async fn empty_async_block(operation: LoadOperation) -> Result<(Option<CachedData>, Option<LoadOperation>), std::io::ErrorKind> {
    Ok((None, Some(operation)))
}

pub async fn empty_async_block_vec(operation: LoadOperation, count: usize) -> Result<(Vec<Option<CachedData>>, Vec<Option<crate::cache::img_cache::ImageMetadata>>, Option<LoadOperation>), std::io::ErrorKind> {
    Ok((vec![None; count], vec![None; count], Some(operation)))
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

pub fn get_file_index(files: &[PathBuf], file: &Path) -> Option<usize> {
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

/// Helper function to handle fallback when directory reading fails
/// This tries to treat the directory path as a single image file (useful for sandboxed apps)
#[cfg(target_os = "macos")]
fn handle_fallback_for_single_file(
    directory_path: &Path, 
    original_error: std::io::Error
) -> Result<Vec<PathBuf>, ImageError> {
    crate::logging::write_crash_debug_log("handle_fallback_for_single_file ENTRY");
    crate::logging::write_crash_debug_log(&format!("Directory path: {}", directory_path.display()));
    crate::logging::write_crash_debug_log(&format!("Original error: {}", original_error));
    
    debug!("🔄 FALLBACK: Attempting single file fallback due to directory access failure");
    debug!("Directory path: {}", directory_path.display());
    debug!("Original error: {}", original_error);
    
    // If we can't read the directory, check if the path itself is a valid image file
    if directory_path.is_file() {
        crate::logging::write_crash_debug_log("Path is a file, checking if it's a valid image");
        debug!("✅ Path is a file, checking if it's a valid image");
        if let Some(extension) = directory_path.extension().and_then(std::ffi::OsStr::to_str) {
            crate::logging::write_crash_debug_log(&format!("File extension: {}", extension));
            debug!("File extension: {}", extension);
            if is_supported_extension(extension) {
                crate::logging::write_crash_debug_log(&format!("✅ Valid image file found: {}", directory_path.display()));
                debug!("✅ Valid image file found: {}", directory_path.display());
                // Return just this single file
                return Ok(vec![directory_path.to_path_buf()]);
            } else {
                crate::logging::write_crash_debug_log(&format!("❌ File has unsupported extension: {}", extension));
                debug!("❌ File has unsupported extension: {}", extension);
            }
        } else {
            crate::logging::write_crash_debug_log("❌ File has no extension");
            debug!("❌ File has no extension");
        }
    } else {
        crate::logging::write_crash_debug_log("❌ Path is not a file");
        debug!("❌ Path is not a file");
        
        // Additional debugging for macOS sandboxing
        #[cfg(target_os = "macos")]
        {
            crate::logging::write_crash_debug_log("🔍 macOS-specific debugging:");
            crate::logging::write_crash_debug_log("  - This may be due to App Store sandboxing restrictions");
            crate::logging::write_crash_debug_log("  - The app may have individual file access but not directory access");
            
            debug!("🔍 macOS-specific debugging:");
            debug!("  - This may be due to App Store sandboxing restrictions");
            debug!("  - The app may have individual file access but not directory access");
            
            let path_str = directory_path.to_string_lossy();
            if crate::macos_file_access::macos_file_handler::has_security_scoped_access(&path_str) {
                crate::logging::write_crash_debug_log("  - Has security-scoped access for this path");
                debug!("  - Has security-scoped access for this path");
            } else {
                crate::logging::write_crash_debug_log("  - No security-scoped access for this path");
                debug!("  - No security-scoped access for this path");
            }
            
            if crate::macos_file_access::macos_file_handler::has_full_disk_access() {
                crate::logging::write_crash_debug_log("  - Has full disk access");
                debug!("  - Has full disk access");
            } else {
                crate::logging::write_crash_debug_log("  - No full disk access");
                debug!("  - No full disk access");
            }
        }
    }
    
    crate::logging::write_crash_debug_log("❌ FALLBACK FAILED: Cannot process as single file, returning original error");
    debug!("❌ FALLBACK FAILED: Cannot process as single file, returning original error");
    // If it's not a valid image file, return the original error
    Err(ImageError::DirectoryError(original_error))
}

/// Helper function to request directory access when bookmark restoration fails
/// This handles the permission dialog flow and fallbacks
#[cfg(target_os = "macos")]
fn request_directory_access_and_retry(
    directory_path: &Path, 
    original_error: std::io::Error
) -> Result<Vec<PathBuf>, ImageError> {
    crate::logging::write_crash_debug_log("request_directory_access_and_retry ENTRY");
    crate::logging::write_crash_debug_log(&format!("Directory path: {}", directory_path.display()));
    crate::logging::write_crash_debug_log(&format!("Original error: {}", original_error));
    
    #[cfg(target_os = "macos")]
    {
        crate::logging::write_crash_debug_log("macOS path - attempting to request new directory access");
        debug!("Attempting to request new directory access");
        
        // STEP 0: Try to restore directory access from stored bookmarks before prompting
        let path_str = directory_path.to_string_lossy();
        crate::logging::write_crash_debug_log("STEP 0 (retry): Attempting bookmark restoration before prompting user");
        if crate::macos_file_access::macos_file_handler::restore_directory_access_for_path(&path_str) {
            crate::logging::write_crash_debug_log("STEP 0 (retry): ✅ Restored directory access from bookmark, retrying read");
            
            // Use the same NSURL-based approach as the main function for consistency
            crate::logging::write_crash_debug_log("STEP 0 (retry): Attempting to read directory using resolved NSURL directly");
            if let Some(file_paths) = crate::macos_file_access::macos_file_handler::read_directory_with_security_scoped_url(&path_str) {
                crate::logging::write_crash_debug_log(&format!("STEP 0 (retry): ✅ Successfully read directory using NSURL, found {} files", file_paths.len()));
                
                // Convert to DirEntry-like structure for compatibility with existing code
                let mut image_paths = Vec::new();
                for file_path in file_paths {
                    let path = std::path::Path::new(&file_path);
                    if let Some(extension) = path.extension() {
                        if let Some(ext_str) = extension.to_str() {
                            if is_supported_extension(ext_str) {
                                image_paths.push(path.to_path_buf());
                            }
                        }
                    }
                }

                crate::logging::write_crash_debug_log(&format!("STEP 0 (retry): ✅ Found {} image files", image_paths.len()));
                return Ok(image_paths);
            } else {
                crate::logging::write_crash_debug_log("STEP 0 (retry): ❌ Failed to read directory using resolved NSURL");
            }
        } else {
            crate::logging::write_crash_debug_log("STEP 0 (retry): ❌ No stored bookmark or restoration failed");
        }

        // Try permission dialog first
        crate::logging::write_crash_debug_log("Getting accessible paths");
        let accessible_paths = crate::macos_file_access::macos_file_handler::get_accessible_paths();
        crate::logging::write_crash_debug_log(&format!("Got {} accessible paths", accessible_paths.len()));
        
        if let Some(file_path) = accessible_paths.first() {
            crate::logging::write_crash_debug_log(&format!("Using first accessible path: {}", file_path));
            crate::logging::write_crash_debug_log("About to call request_parent_directory_permission_dialog");
            if crate::macos_file_access::macos_file_handler::request_parent_directory_permission_dialog(file_path) {
                crate::logging::write_crash_debug_log("Permission dialog succeeded, retrying directory read");
                debug!("Permission dialog succeeded, retrying directory read");
                
                // CRITICAL FIX: Use the resolved NSURL directly for file operations, don't convert to path string
                let path_str = directory_path.to_string_lossy();
                crate::logging::write_crash_debug_log("Attempting to read directory using resolved NSURL directly after permission dialog");
                if let Some(file_paths) = crate::macos_file_access::macos_file_handler::read_directory_with_security_scoped_url(&path_str) {
                    crate::logging::write_crash_debug_log(&format!("✅ Successfully read directory using NSURL after permission dialog, found {} files", file_paths.len()));
                    
                    // Convert to DirEntry-like structure for compatibility with existing code
                    let mut image_paths = Vec::new();
                    for file_path in file_paths {
                        let path = std::path::Path::new(&file_path);
                        if let Some(extension) = path.extension() {
                            if let Some(ext_str) = extension.to_str() {
                                if is_supported_extension(ext_str) {
                                    image_paths.push(path.to_path_buf());
                                }
                            }
                        }
                    }

                    crate::logging::write_crash_debug_log(&format!("✅ Found {} image files after permission dialog", image_paths.len()));
                    return Ok(image_paths);
                } else {
                    crate::logging::write_crash_debug_log("❌ Failed to read directory using resolved NSURL after permission dialog");
                }
            } else {
                crate::logging::write_crash_debug_log("User declined permission dialog");
                debug!("User declined permission dialog");
            }
        } else {
            crate::logging::write_crash_debug_log("No accessible paths found");
        }

        // Fallback to single file handling if all else fails
        crate::logging::write_crash_debug_log("All directory access methods failed, falling back to single file handling");
        debug!("All directory access methods failed, falling back to single file handling");
        return handle_fallback_for_single_file(directory_path, original_error);
    }

    #[cfg(not(target_os = "macos"))]
    {
        crate::logging::write_crash_debug_log("Non-macOS platform - returning original error");
        return Err(ImageError::DirectoryError(original_error));
    }
}

/// Helper function to process directory entries and filter for image files
fn process_directory_entries(
    entries: std::fs::ReadDir,
    directory_path: &Path
) -> Result<Vec<PathBuf>, ImageError> {
    let mut image_paths: Vec<PathBuf> = Vec::new();

    for entry in entries.flatten() {
        if let Some(extension) = entry.path().extension().and_then(std::ffi::OsStr::to_str) {
            if is_supported_extension(extension) {
                image_paths.push(entry.path());
            }
        }
    }

    if image_paths.is_empty() {
        debug!("No image files found in directory: {}", directory_path.display());
        Err(ImageError::NoImagesFound)
    } else {
        debug!("Found {} image files", image_paths.len());
        // Sort paths like Nautilus file viewer for consistent ordering
        alphanumeric_sort::sort_path_slice(&mut image_paths);
        Ok(image_paths)
    }
}

/// Cross-platform image path discovery
/// Routes to OS-specific implementations based on compile target
pub fn get_image_paths(directory_path: &Path) -> Result<Vec<PathBuf>, ImageError> {
    #[cfg(target_os = "macos")]
    {
        get_image_paths_macos(directory_path)
    }
    
    #[cfg(not(target_os = "macos"))]
    {
        get_image_paths_standard(directory_path)
    }
}

/// Standard implementation for non-macOS platforms
/// Simple directory reading without sandbox considerations
#[cfg(not(target_os = "macos"))]
fn get_image_paths_standard(directory_path: &Path) -> Result<Vec<PathBuf>, ImageError> {
    debug!("Standard directory reading for path: {}", directory_path.display());
    
    let dir_entries = fs::read_dir(directory_path)
        .map_err(ImageError::DirectoryError)?;
    
    process_directory_entries(dir_entries, directory_path)
}

/// macOS implementation with App Store sandbox support
/// Handles security-scoped bookmarks and "Open With" scenarios
#[cfg(target_os = "macos")]
fn get_image_paths_macos(directory_path: &Path) -> Result<Vec<PathBuf>, ImageError> {
    crate::logging::write_crash_debug_log("======== get_image_paths_macos ENTRY ========");
    crate::logging::write_crash_debug_log(&format!("Directory path: {}", directory_path.display()));

    // Try standard directory reading first
    match fs::read_dir(directory_path) {
        Ok(entries) => {
            crate::logging::write_crash_debug_log("✅ Standard directory read successful");
            debug!("Successfully read directory normally (drag-and-drop or non-sandboxed): {}", directory_path.display());
            return process_directory_entries(entries, directory_path);
        }
        Err(e) => {
            crate::logging::write_crash_debug_log(&format!("❌ Standard directory read failed: {}", e));
            debug!("Failed to read directory normally: {} (error: {})", directory_path.display(), e);
            
            // Handle macOS App Store sandbox scenarios
            return handle_macos_sandbox_access(directory_path, e);
        }
    }
}

/// Handle macOS App Store sandbox directory access
/// This includes bookmark restoration and "Open With" permission dialogs
#[cfg(target_os = "macos")]
fn handle_macos_sandbox_access(
    directory_path: &Path, 
    original_error: std::io::Error
) -> Result<Vec<PathBuf>, ImageError> {
    let path_str = directory_path.to_string_lossy();
    crate::logging::write_crash_debug_log("macOS sandbox - checking for 'Open With' scenario");
    
    // STEP 1: Try to restore directory access from stored bookmarks
    crate::logging::write_crash_debug_log("STEP 1: Attempting bookmark restoration");
    let bookmark_restored = crate::macos_file_access::macos_file_handler::restore_directory_access_for_path(&path_str);
    
    if bookmark_restored {
        crate::logging::write_crash_debug_log("STEP 1: ✅ Bookmark restored, trying NSURL directory read");
        if let Some(file_paths) = crate::macos_file_access::macos_file_handler::read_directory_with_security_scoped_url(&path_str) {
            return convert_file_paths_to_image_paths(file_paths);
        } else {
            crate::logging::write_crash_debug_log("STEP 1: ❌ NSURL directory read failed");
        }
    } else {
        crate::logging::write_crash_debug_log("STEP 1: ❌ No bookmark found or restoration failed");
    }
    
    // STEP 2: Check if this is an "Open With" scenario
    let accessible_paths = crate::macos_file_access::macos_file_handler::get_accessible_paths();
    let has_individual_file_access = accessible_paths
        .iter()
        .any(|key| {
            let key_path = std::path::Path::new(key);
            key_path.is_file() && 
            key_path.parent()
                .map(|parent| parent.to_string_lossy() == path_str)
                .unwrap_or(false)
        });
    
    if has_individual_file_access {
        crate::logging::write_crash_debug_log("STEP 2: ✅ Confirmed 'Open With' scenario - requesting permission");
        debug!("Confirmed 'Open With' scenario");
        return request_directory_access_and_retry(directory_path, original_error);
    } else {
        crate::logging::write_crash_debug_log("STEP 2: ❌ Not an 'Open With' scenario - regular directory access failure");
        debug!("Not an 'Open With' scenario - regular directory access failure");
        return Err(ImageError::DirectoryError(original_error));
    }
}

/// Convert file paths from security-scoped URL reading to image paths
#[cfg(target_os = "macos")]
fn convert_file_paths_to_image_paths(
    file_paths: Vec<String>
) -> Result<Vec<PathBuf>, ImageError> {
    let mut image_paths = Vec::new();

    for file_path in file_paths {
        let path = std::path::Path::new(&file_path);
        if let Some(extension) = path.extension() {
            if let Some(ext_str) = extension.to_str() {
                if is_supported_extension(ext_str) {
                    image_paths.push(path.to_path_buf());
                }
            }
        }
    }

    if image_paths.is_empty() {
        crate::logging::write_crash_debug_log("❌ No image files found in security-scoped directory");
        Err(ImageError::NoImagesFound)
    } else {
        crate::logging::write_crash_debug_log(&format!("✅ Found {} image files via security-scoped access", image_paths.len()));
        // Sort paths for consistent ordering
        alphanumeric_sort::sort_path_slice(&mut image_paths);
        Ok(image_paths)
    }
}

// ============================================================================
// Async Directory Enumeration (Issue #73 - NFS Performance Fix)
// ============================================================================

use crate::app::{DirectoryEnumResult, DirectoryEnumError};

/// Async directory enumeration for non-blocking UI
/// Uses tokio::fs for async I/O to prevent UI freezes on slow filesystems (NFS)
pub async fn enumerate_directory_async(path: PathBuf) -> Result<DirectoryEnumResult, DirectoryEnumError> {
    use tokio::fs as async_fs;

    // Determine if path is a file or directory (sync metadata check is fast)
    let (dir_path, is_file_drop) = if is_file(&path) {
        let parent = path.parent()
            .ok_or(DirectoryEnumError::NotFound)?
            .to_path_buf();
        (parent, true)
    } else if is_directory(&path) {
        (path.clone(), false)
    } else {
        return Err(DirectoryEnumError::NotFound);
    };

    // Async directory enumeration
    let mut entries = async_fs::read_dir(&dir_path)
        .await
        .map_err(|e| DirectoryEnumError::DirectoryError(e.to_string()))?;

    let mut image_paths: Vec<PathBuf> = Vec::new();

    while let Some(entry) = entries.next_entry().await
        .map_err(|e| DirectoryEnumError::DirectoryError(e.to_string()))?
    {
        let entry_path = entry.path();
        if let Some(extension) = entry_path.extension().and_then(std::ffi::OsStr::to_str) {
            if is_supported_extension(extension) {
                image_paths.push(entry_path);
            }
        }
    }

    if image_paths.is_empty() {
        return Err(DirectoryEnumError::NoImagesFound);
    }

    // Sort paths for consistent ordering
    alphanumeric_sort::sort_path_slice(&mut image_paths);

    // Calculate initial index for file drops
    let initial_index = if is_file_drop {
        get_file_index(&image_paths, &path).unwrap_or(0)
    } else {
        0
    };

    Ok(DirectoryEnumResult {
        file_paths: image_paths,
        directory_path: dir_path.to_string_lossy().to_string(),
        initial_index,
    })
}
