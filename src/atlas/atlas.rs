/*pub mod entry;
mod allocation;
mod allocator;
mod layer;*/

pub use crate::atlas::allocation::Allocation;
pub use crate::atlas::entry::{self, Entry};
pub use crate::atlas::layer::Layer;

use crate::atlas::allocator::Allocator;

pub const SIZE: u32 = 2048;

//use crate::core::Size;
//use crate::graphics::color;
use iced_core::Size;
use iced_graphics::color;

use std::sync::Arc;
use std::time::Instant;
use log::debug;
use iced_wgpu::wgpu;

#[derive(Debug)]
pub struct Atlas {
    pub texture: wgpu::Texture,
    texture_view: wgpu::TextureView,
    texture_bind_group: wgpu::BindGroup,
    texture_layout: Arc<wgpu::BindGroupLayout>,
    layers: Vec<Layer>,
}

impl Atlas {
    pub fn new(
        device: &wgpu::Device,
        backend: wgpu::Backend,
        texture_layout: Arc<wgpu::BindGroupLayout>,
    ) -> Self {
        let layers = match backend {
            // On the GL backend we start with 2 layers, to help wgpu figure
            // out that this texture is `GL_TEXTURE_2D_ARRAY` rather than `GL_TEXTURE_2D`
            // https://github.com/gfx-rs/wgpu/blob/004e3efe84a320d9331371ed31fa50baa2414911/wgpu-hal/src/gles/mod.rs#L371
            wgpu::Backend::Gl => vec![Layer::Empty, Layer::Empty],
            _ => vec![Layer::Empty],
        };

        let extent = wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: layers.len() as u32,
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("iced_wgpu::image texture atlas"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: if color::GAMMA_CORRECTION {
                wgpu::TextureFormat::Rgba8UnormSrgb
            } else {
                wgpu::TextureFormat::Rgba8Unorm
            },
            usage: wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });

        /*let texture_bind_group =
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("iced_wgpu::image texture atlas bind group"),
                layout: &texture_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                }],
            });*/

        // Create a sampler for the atlas
        let atlas_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Atlas Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // Create a uniform buffer (Dummy for now, will be updated later)
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Atlas Uniform Buffer"),
            size: std::mem::size_of::<f32>() as wgpu::BufferAddress * 8, // Ensure correct size
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Fix the Bind Group to include all 3 expected bindings
        let texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("iced_wgpu::image texture atlas bind group"),
            layout: &texture_layout,
            entries: &[
                // Binding 0: Texture View
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                // Binding 1: Sampler
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&atlas_sampler),
                },
                // Binding 2: Uniform Buffer
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &uniform_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
            ],
        });


        Atlas {
            texture,
            texture_view,
            texture_bind_group,
            texture_layout,
            layers,
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
        device: Arc<wgpu::Device>,
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
            self.grow(new_layers, &device, encoder);

            entry
        };

        log::debug!("Allocated atlas entry: {entry:?}");

        // It is a webgpu requirement that:
        //   BufferCopyView.layout.bytes_per_row % wgpu::COPY_BYTES_PER_ROW_ALIGNMENT == 0
        // So we calculate padded_width by rounding width up to the next
        // multiple of wgpu::COPY_BYTES_PER_ROW_ALIGNMENT.
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padding = (align - (4 * width) % align) % align;
        let padded_width = (4 * width + padding) as usize;
        let padded_data_size = padded_width * height as usize;

        let mut padded_data = vec![0; padded_data_size];

        // Flip the image vertically during upload by copying rows in reverse order
        for row in 0..height as usize {
            let dst_offset = row * padded_width;
            let src_offset = (height as usize - 1 - row) * 4 * width as usize;
            
            padded_data[dst_offset..dst_offset + 4 * width as usize].copy_from_slice(
                &data[src_offset..src_offset + 4 * width as usize],
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
                    &device,
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
                        &device,
                        encoder,
                    );
                }
            }
        }

        if log::log_enabled!(log::Level::Debug) {
            log::debug!(
                "Atlas layers: {} (busy: {}, allocations: {})",
                self.layer_count(),
                self.layers.iter().filter(|layer| !layer.is_empty()).count(),
                self.layers.iter().map(Layer::allocations).sum::<usize>(),
            );
        }

        Some(entry)
    }

    pub fn remove(&mut self, entry: &Entry) {
        log::debug!("Removing atlas entry: {entry:?}");

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

    pub fn allocate(
        &mut self,
        width: u32,
        height: u32,
    ) -> Option<Entry> {
        let start = Instant::now();
        debug!("iced_wgpu: Attempting atlas allocation for size {:?}", Size::new(width, height));
        
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
                        fragments.push(entry::Fragment {
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

    pub fn deallocate(&mut self, allocation: &Allocation) {
        log::debug!("Deallocating atlas: {allocation:?}");

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
                label: Some("image upload buffer"),
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

        let new_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("iced_wgpu::image texture atlas"),
            size: wgpu::Extent3d {
                width: SIZE,
                height: SIZE,
                depth_or_array_layers: self.layers.len() as u32,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: if color::GAMMA_CORRECTION {
                wgpu::TextureFormat::Rgba8UnormSrgb
            } else {
                wgpu::TextureFormat::Rgba8Unorm
            },
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

        // Create a sampler for the atlas
        let atlas_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Atlas Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // Create a uniform buffer (Dummy for now, will be updated later)
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Atlas Uniform Buffer"),
            size: std::mem::size_of::<f32>() as wgpu::BufferAddress * 8, // Ensure correct size
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Update the bind group with all 3 required bindings
        self.texture_bind_group =
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("iced_wgpu::image texture atlas bind group"),
                layout: &self.texture_layout,
                entries: &[
                    // Binding 0: Texture View
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&self.texture_view),
                    },
                    // Binding 1: Sampler
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&atlas_sampler),
                    },
                    // Binding 2: Uniform Buffer
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &uniform_buffer,
                            offset: 0,
                            size: None,
                        }),
                    },
                ],
            });
    }
}
