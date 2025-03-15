use std::io;
use std::path::Path;
#[allow(unused_imports)]
use image::GenericImageView;
use image::DynamicImage;
use std::sync::Arc;
use wgpu::{Device, Queue};
use iced_wgpu::wgpu;

#[allow(unused_imports)]
use log::{debug, info, warn, error};

fn load_and_resize_image(img_path: &Path, is_slider_move: bool) -> Result<DynamicImage, io::Error> {
    let img = image::open(img_path)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("Failed to open image: {}", e)))?;

    let resized_img = if is_slider_move {
        const TARGET_WIDTH: u32 = 1280;
        const TARGET_HEIGHT: u32 = 720;
        img.resize_exact(TARGET_WIDTH, TARGET_HEIGHT, image::imageops::FilterType::Triangle)
    } else {
        img
    };

    Ok(resized_img)
}
fn convert_image_to_rgba(img: &DynamicImage) -> (Vec<u8>, u32, u32) {
    let rgba_image = img.to_rgba8();
    let (width, height) = rgba_image.dimensions();
    let rgba_bytes = rgba_image.into_raw();

    (rgba_bytes, width, height)
}
pub fn create_gpu_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("LoadedTexture"),
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
    })
}

fn upload_texture(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    image_bytes: &[u8],
    width: u32,
    height: u32,
) {
    let bytes_per_row = width * 4;
    
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        image_bytes,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(bytes_per_row),
            rows_per_image: None,
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
}

pub fn load_image_resized_sync(
    img_path: &Path,
    is_slider_move: bool,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    existing_texture: &mut Arc<wgpu::Texture>,
) -> Result<(), io::Error> {
    let img = load_and_resize_image(img_path, is_slider_move)?;
    let (image_bytes, width, height) = convert_image_to_rgba(&img);

    // Create a new texture if needed
    let texture = Arc::new(create_gpu_texture(device, width, height));
    
    // Upload the image data to GPU
    upload_texture(queue, &texture, &image_bytes, width, height);

    *existing_texture = texture; // Replace the old texture

    Ok(())
}


/// Loads an image and resizes it to 720p if needed, then uploads it to GPU.
pub async fn _load_image_resized(
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
