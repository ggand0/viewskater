use std::io;
use crate::cache::img_cache::CachedData;
use std::path::Path;
use image::GenericImageView;
use image::{DynamicImage, ImageOutputFormat};
use std::io::Cursor;
use std::sync::Arc;
use wgpu::{Device, Queue, Texture, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages};
use iced_wgpu::wgpu;

use image::{io::Reader as ImageReader, ImageFormat};

#[allow(unused_imports)]
use log::{debug, info, warn, error};

/// Shift the cache array to the left, adding a new item at the end.
/// Updates the indices accordingly.
pub fn shift_cache_left<T>(
    cached_items: &mut Vec<Option<T>>,
    cached_indices: &mut Vec<isize>,
    new_item: Option<T>,
    current_offset: &mut isize,
) {
    cached_items.remove(0);
    cached_items.push(new_item);

    // Update indices
    cached_indices.remove(0);
    let next_index = cached_indices[cached_indices.len() - 1] + 1;
    cached_indices.push(next_index);

    *current_offset -= 1;
    debug!("shift_cache_left - current_offset: {}", current_offset);
}

/// Shift the cache array to the right, adding a new item at the front.
/// Updates the indices accordingly.
pub fn shift_cache_right<T>(
    cached_items: &mut Vec<Option<T>>,
    cached_indices: &mut Vec<isize>,
    new_item: Option<T>,
    current_offset: &mut isize,
) {
    cached_items.pop();
    cached_items.insert(0, new_item);

    // Update indices
    cached_indices.pop();
    let prev_index = cached_indices[0] - 1;
    cached_indices.insert(0, prev_index);

    *current_offset += 1;
    debug!("shift_cache_right - current_offset: {}", current_offset);
}

/// Load an item into a specific position in the cache.
/// Returns `true` if the position corresponds to the center of the cache.
pub fn load_pos<T>(
    cached_items: &mut Vec<Option<T>>,
    cached_indices: &mut Vec<isize>,
    pos: usize,
    item: Option<T>,
    image_index: isize,
    cache_count: usize,
) -> Result<bool, io::Error> {
    if pos >= cached_items.len() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "Position out of bounds",
        ));
    }

    cached_items[pos] = item;
    cached_indices[pos] = image_index;

    if pos == cache_count {
        Ok(true) // Center of the cache
    } else {
        Ok(false)
    }
}

/// Load an image from disk, resize it, and return raw RGBA8 pixel data.
pub fn load_image_cpu(
    img_path: &Path,
    target_width: u32,
    target_height: u32,
) -> Result<Vec<u8>, io::Error> {
    // Load the image
    let img = ImageReader::open(img_path)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("Failed to open image: {}", e)))?
        .decode()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("Failed to decode image: {}", e)))?;

    let (original_width, original_height) = img.dimensions();
    debug!("Original image size: {}x{}", original_width, original_height);

    // Resize image to target dimensions
    let resized = img.resize_exact(target_width, target_height, image::imageops::FilterType::Triangle);

    let (resized_width, resized_height) = resized.dimensions();
    debug!("Resized image size: {}x{}", resized_width, resized_height);

    // Convert the resized image to raw RGBA8 format
    let rgba_image = resized.to_rgba8();
    let raw_data = rgba_image.into_vec();

    debug!("Final pixel data length: {}", raw_data.len());

    // Sanity check: Ensure that the raw data length is correct
    let expected_size = (resized_width * resized_height * 4) as usize;
    if raw_data.len() != expected_size {
        error!(
            "Mismatch in image data size! Expected: {}, Got: {}",
            expected_size, raw_data.len()
        );
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Image data size mismatch"));
    }

    Ok(raw_data)
}

/*pub fn load_image_cpu(
    img_path: &Path,
    target_width: u32,
    target_height: u32,
) -> Result<Vec<u8>, io::Error> {
    // Load the image
    let img = ImageReader::open(img_path)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("Failed to open image: {}", e)))?
        .decode()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("Failed to decode image: {}", e)))?;

    // Resize image to target dimensions while preserving aspect ratio
    let resized = img.resize_exact(target_width, target_height, image::imageops::FilterType::Triangle);

    // Convert the resized image to RGBA8 format
    let rgba_image = resized.to_rgba8();

    // Convert image data to raw Vec<u8>
    Ok(rgba_image.into_vec())
}*/


/// Loads an image and resizes it to 720p if needed, then uploads it to GPU.
pub async fn load_image_resized(
    img_path: &Path,
    is_slider_move: bool,
    device: &Device,
    queue: &Queue,
    existing_texture: &mut Arc<wgpu::Texture>,
) -> Result<(), io::Error> {
    let img = image::open(img_path)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("Failed to open image: {}", e)))?;

    let resized_img = if is_slider_move {
        const TARGET_WIDTH: u32 = 1280;
        const TARGET_HEIGHT: u32 = 720;

        img.resize_exact(TARGET_WIDTH, TARGET_HEIGHT, image::imageops::FilterType::Triangle)
    } else {
        img
    };

    let rgba_image = resized_img.to_rgba8();
    let (width, height) = rgba_image.dimensions();

    // 🔹 Ensure resized image is exactly 1280×720
    assert_eq!(width, 1280, "Resized image width must be 1280");
    assert_eq!(height, 720, "Resized image height must be 720");

    let rgba_bytes = rgba_image.as_raw();

    // 🔹 Align `bytes_per_row` to 256 bytes
    let unaligned_bytes_per_row = width * 4;
    let aligned_bytes_per_row = (unaligned_bytes_per_row + 255) & !255;

    // 🔹 Staging buffer
    let buffer_size = (aligned_bytes_per_row * height) as wgpu::BufferAddress;
    let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Staging Buffer"),
        size: buffer_size,
        usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::MAP_WRITE,
        mapped_at_creation: true,
    });

    {
        let mut mapping = staging_buffer.slice(..).get_mapped_range_mut();
        for row in 0..height {
            let src_start = (row * width * 4) as usize;
            let dst_start = (row * aligned_bytes_per_row) as usize;
            mapping[dst_start..dst_start + (width * 4) as usize]
                .copy_from_slice(&rgba_bytes[src_start..src_start + (width * 4) as usize]);
        }
    }
    staging_buffer.unmap();

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Image Upload Encoder") });

    encoder.copy_buffer_to_texture(
        wgpu::ImageCopyBuffer {
            buffer: &staging_buffer,
            layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(aligned_bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::ImageCopyTexture {
            texture: existing_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );

    queue.submit(Some(encoder.finish()));

    Ok(())
}



/*pub fn load_image_resized(img_path: &Path, is_slider_move: bool) -> Result<CachedData, io::Error> {
    let img = image::open(img_path).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    let resized_img = if is_slider_move {
        const MAX_WIDTH: u32 = 1280;
        const MAX_HEIGHT: u32 = 720;

        let (width, height) = img.dimensions();
        let aspect_ratio = width as f32 / height as f32;

        let (new_width, new_height) = if width > MAX_WIDTH || height > MAX_HEIGHT {
            if aspect_ratio > 1.0 {
                (MAX_WIDTH, (MAX_WIDTH as f32 / aspect_ratio) as u32)
            } else {
                ((MAX_HEIGHT as f32 * aspect_ratio) as u32, MAX_HEIGHT)
            }
        } else {
            (width, height)
        };

        img.resize_exact(new_width, new_height, image::imageops::FilterType::Triangle)
    } else {
        img
    };

    // Convert `DynamicImage` to raw bytes (PNG format)
    let mut buffer = Cursor::new(Vec::new());
    resized_img.write_to(&mut buffer, ImageOutputFormat::Png)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    Ok(CachedData::Cpu(buffer.into_inner()))
}*/
