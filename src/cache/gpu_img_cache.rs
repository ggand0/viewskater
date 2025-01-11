use std::path::PathBuf;
use std::io;
use wgpu;
use crate::cache::img_cache::{ImageCache, CachedData, ImageCacheBackend};
use crate::cache::cache_utils::{shift_cache_left, shift_cache_right, load_pos};
use wgpu::Texture;
use wgpu::util::DeviceExt;

use std::path::Path;

#[allow(unused_imports)]
use log::{debug, info, warn, error};


use crate::loading_status::LoadingStatus;
use crate::cache::img_cache::{LoadOperation, LoadOperationType};

pub struct GpuImageCache {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

impl GpuImageCache {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        Self {
            device,
            queue,
        }
    }
}

impl ImageCacheBackend for GpuImageCache {
    fn load_image(&self, index: usize, image_paths: &[PathBuf]) -> Result<CachedData, io::Error> {
        if let Some(image_path) = image_paths.get(index) {
            let image_data = std::fs::read(image_path)?;

            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("CacheTexture"),
                size: wgpu::Extent3d {
                    width: 256, // Example width
                    height: 256, // Example height
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });

            let buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("ImageBuffer"),
                contents: &image_data,
                usage: wgpu::BufferUsages::COPY_SRC,
            });

            let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("ImageUpload"),
            });

            encoder.copy_buffer_to_texture(
                wgpu::ImageCopyBuffer {
                    buffer: &buffer,
                    layout: wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row: Some(4 * 256),
                        rows_per_image: Some(256),
                    },
                },
                wgpu::ImageCopyTexture {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: 256,
                    height: 256,
                    depth_or_array_layers: 1,
                },
            );

            self.queue.submit(Some(encoder.finish()));

            Ok(CachedData::Gpu(texture))
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid image index",
            ))
        }
    }

    fn load_initial_images(
        &mut self,
        image_paths: &[PathBuf],
        cache_count: usize,
        current_index: usize,
        cached_data: &mut Vec<Option<CachedData>>,
        cached_image_indices: &mut Vec<isize>,
        current_offset: &mut isize,
    ) -> Result<(), io::Error> {
        let start_index: isize;
        let end_index: isize;
        if current_index <= cache_count {
            start_index = 0;
            end_index = (cache_count * 2 + 1) as isize;
            *current_offset = -(cache_count as isize - current_index as isize);
        } else if current_index > (image_paths.len() - 1) - cache_count {
            start_index = image_paths.len() as isize - cache_count as isize * 2 - 1;
            end_index = image_paths.len() as isize;
            *current_offset = cache_count as isize - ((image_paths.len() - 1) as isize - current_index as isize);
        } else {
            start_index = current_index as isize - cache_count as isize;
            end_index = current_index as isize + cache_count as isize + 1;
        }

        for (i, cache_index) in (start_index..end_index).enumerate() {
            if cache_index < 0 {
                continue;
            }
            if cache_index > image_paths.len() as isize - 1 {
                break;
            }
            let image = self.load_image(cache_index as usize, image_paths)?;
            cached_data[i] = Some(image);
            cached_image_indices[i] = cache_index;
        }

        Ok(())
    }


    fn load_pos(&mut self, new_image: Option<CachedData>, pos: usize, image_index: isize) -> Result<bool, io::Error> {
        println!("GpuCache: Setting image at position {}", pos);
        // Placeholder logic for setting position in GPU cache
        Err(io::Error::new(io::ErrorKind::Unsupported, "GPU load_pos not implemented"))
    }

}
