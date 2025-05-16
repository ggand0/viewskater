use std::io;
use std::path::Path;
#[allow(unused_imports)]
use image::GenericImageView;
use image::DynamicImage;
use std::sync::Arc;
use wgpu::{Device, Queue};
use iced_wgpu::wgpu;
use iced_wgpu::engine::CompressionStrategy;
use crate::cache::compression::{compress_image_bc1, CompressionAlgorithm};
use texpresso::{Format, Params, Algorithm, COLOUR_WEIGHTS_PERCEPTUAL};

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

/// Checks if BC1 compression should be used based on dimensions and strategy
pub fn should_use_compression(width: u32, height: u32, strategy: CompressionStrategy) -> bool {
    match strategy {
        CompressionStrategy::Bc1 => {
            // BC1 compression requires dimensions to be multiples of 4
            if width % 4 == 0 && height % 4 == 0 {
                debug!("Using BC1 compression for image ({} x {})", width, height);
                true
            } else {
                debug!("Image dimensions ({} x {}) not compatible with BC1. Using uncompressed format.", width, height);
                false
            }
        },
        CompressionStrategy::None => false,
    }
}

/// Creates a texture with the appropriate format based on compression settings
pub fn create_gpu_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    compression_strategy: CompressionStrategy,
) -> wgpu::Texture {
    let use_compression = should_use_compression(width, height, compression_strategy);

    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(if use_compression { "CompressedTexture" } else { "LoadedTexture" }),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: if use_compression { 
            wgpu::TextureFormat::Bc1RgbaUnormSrgb 
        } else {
            wgpu::TextureFormat::Rgba8UnormSrgb
        },
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

/// Compresses image data using BC1 algorithm
/// TODO: Remove this after confirming that the texpresso compression is stable
#[allow(dead_code)]
pub fn compress_image_data(
    rgba_data: &[u8],
    width: u32,
    height: u32,
) -> (Vec<u8>, u32) {
    // Compress the image data
    let compressed_blocks = compress_image_bc1(
        rgba_data,
        width as usize,
        height as usize,
        CompressionAlgorithm::RangeFit
    );

    // Calculate compressed data layout
    let blocks_x = (width + 3) / 4;
    let bytes_per_block = 8; // BC1 uses 8 bytes per 4x4 block
    let row_bytes = blocks_x * bytes_per_block;

    // Flatten the blocks into a single buffer
    let compressed_data: Vec<u8> = compressed_blocks.iter()
        .flat_map(|block| block.iter().copied())
        .collect();

    (compressed_data, row_bytes)
}

/// Uploads uncompressed image data to a texture
pub fn upload_uncompressed_texture(
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

/// Uploads compressed image data to a texture
pub fn upload_compressed_texture(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    compressed_data: &[u8],
    width: u32,
    height: u32,
    row_bytes: u32,
) {
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        compressed_data,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(row_bytes),
            rows_per_image: None,
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
}

/// Compresses an image using the texpresso library (BC1/DXT1 format)
pub fn compress_image_data_texpresso(image_data: &[u8], width: u32, height: u32) -> (Vec<u8>, u32) {
    // Create 4x4 blocks of RGBA data from the image
    let width_usize = width as usize;
    let height_usize = height as usize;
    
    // Calculate the output size
    let blocks_wide = (width_usize + 3) / 4;
    let blocks_tall = (height_usize + 3) / 4;
    let block_size = Format::Bc1.block_size();
    let output_size = blocks_wide * blocks_tall * block_size;
    
    // Create output buffer
    let mut compressed_data = vec![0u8; output_size];
    
    // Set up compression parameters
    let params = Params {
        //algorithm: Algorithm::ClusterFit, // Higher quality but still fast
        algorithm: Algorithm::RangeFit,
        weights: COLOUR_WEIGHTS_PERCEPTUAL,
        weigh_colour_by_alpha: true, // Better for images with transparency
    };
    
    // Compress the image
    Format::Bc1.compress(
        image_data, 
        width_usize, 
        height_usize, 
        params, 
        &mut compressed_data
    );
    
    // Calculate bytes per row
    let bytes_per_row = blocks_wide * block_size;
    
    (compressed_data, bytes_per_row as u32)
}

/// Creates and uploads a texture with the appropriate format and data
pub fn create_and_upload_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    image_data: &[u8],
    width: u32,
    height: u32,
    compression_strategy: CompressionStrategy,
) -> wgpu::Texture {
    let use_compression = should_use_compression(width, height, compression_strategy);
    
    let texture = create_gpu_texture(device, width, height, compression_strategy);

    if use_compression {
        // Use texpresso for compression when BC1 is selected
        match compression_strategy {
            CompressionStrategy::Bc1 => {
                let (compressed_data, bytes_per_row) = compress_image_data_texpresso(image_data, width, height);
                upload_compressed_texture(queue, &texture, &compressed_data, width, height, bytes_per_row);
            },
            _ => {
                // Raise an error if an unsupported compression strategy is used
                panic!("Unsupported compression strategy: {:?}", compression_strategy);
            }
        }
    } else {
        upload_uncompressed_texture(queue, &texture, image_data, width, height);
    }
    
    texture
}

pub fn load_image_resized_sync(
    img_path: &Path,
    is_slider_move: bool,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    existing_texture: &mut Arc<wgpu::Texture>,
    compression_strategy: CompressionStrategy,
) -> Result<(), io::Error> {
    let img = load_and_resize_image(img_path, is_slider_move)?;
    let (image_bytes, width, height) = convert_image_to_rgba(&img);

    // Use our new utility function to create and upload the texture
    let texture = Arc::new(
        create_and_upload_texture(device, queue, &image_bytes, width, height, compression_strategy)
    );
    
    // Replace the old texture
    *existing_texture = texture;

    Ok(())
}

/// Loads an image and resizes it to 720p if needed, then uploads it to GPU.
pub async fn _load_image_resized(
    img_path: &Path,
    is_slider_move: bool,
    device: &Device,
    queue: &Queue,
    existing_texture: &Arc<wgpu::Texture>,
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
