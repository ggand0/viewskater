// Port of iced_wgpu's atlas.rs
// Main atlas structure for efficient 2D texture allocation and management

use iced_core::Size;
use iced_wgpu::wgpu;
use iced_wgpu::engine::CompressionStrategy;
use std::sync::Arc;

use crate::slider_atlas::{Allocator, Allocation, Entry, Layer};
use crate::slider_atlas::entry::Fragment;

use texpresso::{Format, Algorithm, Params, COLOUR_WEIGHTS_PERCEPTUAL};

#[allow(unused_imports)]
use log::{debug, info, warn};

pub const SIZE: u32 = 2048;

// Gamma correction flag - match iced_wgpu's behavior
// For now, we'll use sRGB which handles gamma correction
const GAMMA_CORRECTION: bool = true;

#[derive(Debug)]
pub struct Atlas {
    texture: wgpu::Texture,
    texture_view: wgpu::TextureView,
    texture_bind_group: wgpu::BindGroup,
    texture_layout: Arc<wgpu::BindGroupLayout>,
    layers: Vec<Layer>,
    compression_strategy: CompressionStrategy,
}

impl Atlas {
    pub fn new(
        device: &wgpu::Device,
        backend: wgpu::Backend,
        texture_layout: Arc<wgpu::BindGroupLayout>,
        compression_strategy: CompressionStrategy,
    ) -> Self {
        info!("Creating new slider atlas with compression strategy: {:?}", compression_strategy);

        let layers = match backend {
            // On the GL backend we start with 2 layers, to help wgpu figure
            // out that this texture is `GL_TEXTURE_2D_ARRAY` rather than `GL_TEXTURE_2D`
            wgpu::Backend::Gl => vec![Layer::Empty, Layer::Empty],
            _ => vec![Layer::Empty],
        };

        let extent = wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: layers.len() as u32,
        };

        // Choose texture format based on compression strategy
        let format = match compression_strategy {
            CompressionStrategy::None => {
                if GAMMA_CORRECTION {
                    wgpu::TextureFormat::Rgba8UnormSrgb
                } else {
                    wgpu::TextureFormat::Rgba8Unorm
                }
            },
            CompressionStrategy::Bc1 => {
                wgpu::TextureFormat::Bc1RgbaUnormSrgb
            },
        };
        debug!("Slider atlas texture format: {:?}", format);

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("viewskater::slider_atlas texture"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });

        let texture_bind_group =
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("viewskater::slider_atlas bind group"),
                layout: &texture_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                }],
            });

        Atlas {
            texture,
            texture_view,
            texture_bind_group,
            texture_layout,
            layers,
            compression_strategy,
        }
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.texture_bind_group
    }

    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        width: u32,
        height: u32,
        data: &[u8],
    ) -> Option<Entry> {
        let entry = {
            let current_size = self.layers.len();
            let entry = self.allocate(width, height)?;

            // We grow the internal texture after allocating if necessary
            let new_layers = self.layers.len() - current_size;
            self.grow(new_layers, device, encoder);

            entry
        };

        debug!("Allocated slider atlas entry: {entry:?}");

        match self.compression_strategy {
            CompressionStrategy::None => {
                debug!("Uploading uncompressed image to slider atlas");

                // Original uncompressed upload path
                // It is a webgpu requirement that:
                //   BufferCopyView.layout.bytes_per_row % wgpu::COPY_BYTES_PER_ROW_ALIGNMENT == 0
                // So we calculate padded_width by rounding width up to the next
                // multiple of wgpu::COPY_BYTES_PER_ROW_ALIGNMENT.
                let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
                let padding = (align - (4 * width) % align) % align;
                let padded_width = (4 * width + padding) as usize;
                let padded_data_size = padded_width * height as usize;

                let mut padded_data = vec![0; padded_data_size];

                for row in 0..height as usize {
                    let offset = row * padded_width;

                    padded_data[offset..offset + 4 * width as usize].copy_from_slice(
                        &data[row * 4 * width as usize..(row + 1) * 4 * width as usize],
                    );
                }

                match &entry {
                    Entry::Contiguous(allocation) => {
                        self.upload_allocation(
                            &padded_data,
                            width,
                            height,
                            padding,
                            0,
                            allocation,
                            device,
                            encoder,
                        );
                    }
                    Entry::Fragmented { fragments, .. } => {
                        for fragment in fragments {
                            let (x, y) = fragment.position;
                            let offset = (y * padded_width as u32 + 4 * x) as usize;

                            self.upload_allocation(
                                &padded_data,
                                width,
                                height,
                                padding,
                                offset,
                                &fragment.allocation,
                                device,
                                encoder,
                            );
                        }
                    }
                }
            },
            CompressionStrategy::Bc1 => {
                debug!("Uploading BC1 compressed image to slider atlas");
                // New compressed upload path
                self.upload_compressed(device, encoder, width, height, data, &entry);
            }
        }

        if log::log_enabled!(log::Level::Debug) {
            debug!(
                "Slider atlas layers: {} (busy: {}, allocations: {})",
                self.layer_count(),
                self.layers.iter().filter(|layer| !layer.is_empty()).count(),
                self.layers.iter().map(Layer::allocations).sum::<usize>(),
            );
        }

        Some(entry)
    }

    pub fn remove(&mut self, entry: &Entry) {
        debug!("Removing slider atlas entry: {entry:?}");

        match entry {
            Entry::Contiguous(allocation) => {
                self.deallocate(allocation);
            }
            Entry::Fragmented { fragments, .. } => {
                for fragment in fragments {
                    self.deallocate(&fragment.allocation);
                }
            }
        }
    }

    fn allocate(&mut self, width: u32, height: u32) -> Option<Entry> {
        // Allocate one layer if texture fits perfectly
        if width == SIZE && height == SIZE {
            let mut empty_layers = self
                .layers
                .iter_mut()
                .enumerate()
                .filter(|(_, layer)| layer.is_empty());

            if let Some((i, layer)) = empty_layers.next() {
                *layer = Layer::Full;

                return Some(Entry::Contiguous(Allocation::Full { layer: i }));
            }

            self.layers.push(Layer::Full);

            return Some(Entry::Contiguous(Allocation::Full {
                layer: self.layers.len() - 1,
            }));
        }

        // Split big textures across multiple layers
        if width > SIZE || height > SIZE {
            let mut fragments = Vec::new();
            let mut y = 0;

            while y < height {
                let height = std::cmp::min(height - y, SIZE);
                let mut x = 0;

                while x < width {
                    let width = std::cmp::min(width - x, SIZE);

                    let allocation = self.allocate(width, height)?;

                    if let Entry::Contiguous(allocation) = allocation {
                        fragments.push(Fragment {
                            position: (x, y),
                            allocation,
                        });
                    }

                    x += width;
                }

                y += height;
            }

            return Some(Entry::Fragmented {
                size: Size::new(width, height),
                fragments,
            });
        }

        // Try allocating on an existing layer
        for (i, layer) in self.layers.iter_mut().enumerate() {
            match layer {
                Layer::Empty => {
                    let mut allocator = Allocator::new(SIZE);

                    if let Some(region) = allocator.allocate(width, height) {
                        *layer = Layer::Busy(allocator);

                        return Some(Entry::Contiguous(Allocation::Partial {
                            region,
                            layer: i,
                        }));
                    }
                }
                Layer::Busy(allocator) => {
                    if let Some(region) = allocator.allocate(width, height) {
                        return Some(Entry::Contiguous(Allocation::Partial {
                            region,
                            layer: i,
                        }));
                    }
                }
                Layer::Full => {}
            }
        }

        // Create new layer with atlas allocator
        let mut allocator = Allocator::new(SIZE);

        if let Some(region) = allocator.allocate(width, height) {
            self.layers.push(Layer::Busy(allocator));

            return Some(Entry::Contiguous(Allocation::Partial {
                region,
                layer: self.layers.len() - 1,
            }));
        }

        // We ran out of memory (?)
        None
    }

    fn deallocate(&mut self, allocation: &Allocation) {
        debug!("Deallocating slider atlas: {allocation:?}");

        match allocation {
            Allocation::Full { layer } => {
                self.layers[*layer] = Layer::Empty;
            }
            Allocation::Partial { layer, region } => {
                let layer = &mut self.layers[*layer];

                if let Layer::Busy(allocator) = layer {
                    allocator.deallocate(region);

                    if allocator.is_empty() {
                        *layer = Layer::Empty;
                    }
                }
            }
        }
    }

    fn upload_allocation(
        &mut self,
        data: &[u8],
        image_width: u32,
        image_height: u32,
        padding: u32,
        offset: usize,
        allocation: &Allocation,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        use wgpu::util::DeviceExt;

        let (x, y) = allocation.position();
        let Size { width, height } = allocation.size();
        let layer = allocation.layer();

        let extent = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        let buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("slider atlas image upload buffer"),
                contents: data,
                usage: wgpu::BufferUsages::COPY_SRC,
            });

        encoder.copy_buffer_to_texture(
            wgpu::ImageCopyBuffer {
                buffer: &buffer,
                layout: wgpu::ImageDataLayout {
                    offset: offset as u64,
                    bytes_per_row: Some(4 * image_width + padding),
                    rows_per_image: Some(image_height),
                },
            },
            wgpu::ImageCopyTexture {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x,
                    y,
                    z: layer as u32,
                },
                aspect: wgpu::TextureAspect::default(),
            },
            extent,
        );
    }

    fn grow(
        &mut self,
        amount: usize,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        if amount == 0 {
            return;
        }

        // Choose format based on compression strategy
        let format = match self.compression_strategy {
            CompressionStrategy::None => {
                if GAMMA_CORRECTION {
                    wgpu::TextureFormat::Rgba8UnormSrgb
                } else {
                    wgpu::TextureFormat::Rgba8Unorm
                }
            },
            CompressionStrategy::Bc1 => {
                wgpu::TextureFormat::Bc1RgbaUnormSrgb
            },
        };

        let new_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("viewskater::slider_atlas texture"),
            size: wgpu::Extent3d {
                width: SIZE,
                height: SIZE,
                depth_or_array_layers: self.layers.len() as u32,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let amount_to_copy = self.layers.len() - amount;

        for (i, layer) in
            self.layers.iter_mut().take(amount_to_copy).enumerate()
        {
            if layer.is_empty() {
                continue;
            }

            encoder.copy_texture_to_texture(
                wgpu::ImageCopyTexture {
                    texture: &self.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: i as u32,
                    },
                    aspect: wgpu::TextureAspect::default(),
                },
                wgpu::ImageCopyTexture {
                    texture: &new_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: i as u32,
                    },
                    aspect: wgpu::TextureAspect::default(),
                },
                wgpu::Extent3d {
                    width: SIZE,
                    height: SIZE,
                    depth_or_array_layers: 1,
                },
            );
        }

        self.texture = new_texture;
        self.texture_view =
            self.texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            });

        self.texture_bind_group =
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("viewskater::slider_atlas bind group"),
                layout: &self.texture_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &self.texture_view,
                    ),
                }],
            });
    }

    // New method to handle compressed uploads
    fn upload_compressed(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        width: u32,
        height: u32,
        data: &[u8],
        entry: &Entry,
    ) {
        match &entry {
            Entry::Contiguous(allocation) => {
                self.upload_compressed_allocation(
                    device,
                    encoder,
                    width,
                    height, 
                    data,
                    allocation,
                );
            }
            Entry::Fragmented { fragments, .. } => {
                for fragment in fragments {
                    let (x, y) = fragment.position;
                    let fragment_width = fragment.allocation.size().width;
                    let fragment_height = fragment.allocation.size().height;
                    
                    // Extract fragment data from the original image
                    let mut fragment_data = Vec::with_capacity((fragment_width * fragment_height * 4) as usize);
                    for fy in 0..fragment_height {
                        for fx in 0..fragment_width {
                            let src_x = x + fx;
                            let src_y = y + fy;
                            if src_x < width && src_y < height {
                                let src_idx = ((src_y * width + src_x) * 4) as usize;
                                fragment_data.extend_from_slice(&data[src_idx..src_idx+4]);
                            } else {
                                // Padding for fragments that extend beyond the original image
                                fragment_data.extend_from_slice(&[0, 0, 0, 0]);
                            }
                        }
                    }
                    
                    self.upload_compressed_allocation(
                        device,
                        encoder,
                        fragment_width,
                        fragment_height,
                        &fragment_data,
                        &fragment.allocation,
                    );
                }
            }
        }
    }

    fn upload_compressed_allocation(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        width: u32,
        height: u32,
        data: &[u8],
        allocation: &Allocation,
    ) {
        use wgpu::util::DeviceExt;

        let (x, y) = allocation.position();
        let layer = allocation.layer();
        
        // BC1 requires coordinates to be aligned to 4-pixel blocks
        // Round down to nearest multiple of 4
        let aligned_x = (x / 4) * 4;
        let aligned_y = (y / 4) * 4;
        
        // Convert dimensions to usize for texpresso
        let width_usize = width as usize;
        let height_usize = height as usize;
        
        // Calculate blocks and output size
        let blocks_x = (width_usize + 3) / 4;
        let blocks_y = (height_usize + 3) / 4;
        let block_size = Format::Bc1.block_size();
        let output_size = blocks_x * blocks_y * block_size;
        
        // Create output buffer
        let mut compressed_data = vec![0u8; output_size];
        
        // Set up compression parameters
        let params = Params {
            algorithm: Algorithm::RangeFit, // Fast and good quality
            weights: COLOUR_WEIGHTS_PERCEPTUAL,
            weigh_colour_by_alpha: true,
        };
        
        // Compress the image with texpresso
        Format::Bc1.compress(
            data, 
            width_usize, 
            height_usize, 
            params, 
            &mut compressed_data
        );
        
        // Calculate bytes per row
        let bytes_per_row = blocks_x * block_size;
        
        // Align to wgpu requirements
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize;
        let padding = (align - (bytes_per_row % align)) % align;
        let padded_bytes_per_row = bytes_per_row + padding;
        
        // Create padded data if needed
        let upload_data = if padding == 0 {
            compressed_data
        } else {
            let mut padded_data = Vec::with_capacity(padded_bytes_per_row * blocks_y);
            for i in 0..blocks_y {
                let start = i * bytes_per_row;
                let end = start + bytes_per_row;
                padded_data.extend_from_slice(&compressed_data[start..end]);
                padded_data.extend(std::iter::repeat(0).take(padding));
            }
            padded_data
        };
        
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("slider atlas compressed image upload buffer"),
            contents: &upload_data,
            usage: wgpu::BufferUsages::COPY_SRC,
        });
        
        encoder.copy_buffer_to_texture(
            wgpu::ImageCopyBuffer {
                buffer: &buffer,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row as u32),
                    rows_per_image: Some(blocks_y as u32),
                },
            },
            wgpu::ImageCopyTexture {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: aligned_x,
                    y: aligned_y,
                    z: layer as u32,
                },
                aspect: wgpu::TextureAspect::default(),
            },
            wgpu::Extent3d {
                width: blocks_x as u32 * 4,  // Convert back to pixels for the extent
                height: blocks_y as u32 * 4,
                depth_or_array_layers: 1,
            },
        );
    }
}

