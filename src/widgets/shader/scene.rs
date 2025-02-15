use iced_widget::shader::{self, Viewport};
use iced_winit::core::{Color, Element, Rectangle, Length::*, Theme, mouse};
use iced_wgpu::wgpu;
use crate::widgets::shader::pipeline::Pipeline;
use image::GenericImageView;
use std::sync::Arc;

pub struct Scene {
    pub current_image_index: usize,
    pub atlas_size: (u32, u32),
    pub textures: Vec<(Arc<wgpu::Texture>, (u32, u32))>, // Store textures only
    pub image_data: Vec<(Vec<u8>, (u32, u32))>, // Store the image data
    pub pending_window_size: Option<(u32, u32)>
}

impl Scene {
    pub fn new(textures: Vec<(Arc<wgpu::Texture>, (u32, u32))>,
    image_data: Vec<(Vec<u8>, (u32, u32))>,
) -> Self {
        let atlas_size = (8192, 8192);
        Self {
            current_image_index: 0,
            atlas_size,
            textures,
            image_data,
            pending_window_size: None,
        }
    }

    pub fn set_current_image(&mut self, index: usize) {
        self.current_image_index = index;
    }

    pub fn create_texture_from_image(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        image_path: &str,
    ) -> (wgpu::Texture, (u32, u32)) {
        let image = image::open(image_path).expect("Failed to load image");
        let rgba_image = image.to_rgba8();
        let dimensions = image.dimensions();

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Image Texture"),
            size: wgpu::Extent3d {
                width: dimensions.0,
                height: dimensions.1,
                //width: 8192,
                //height: 8192,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rgba_image,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * dimensions.0),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: dimensions.0,
                height: dimensions.1,
                depth_or_array_layers: 1,
            },
        );

        (texture, dimensions)
    }

}

#[derive(Debug)]
pub struct Primitive {
    current_texture_index: usize,
    atlas_size: (u32, u32),
    textures: Vec<(Arc<wgpu::Texture>, (u32, u32))>, // Include dimensions
    bounds: Rectangle,
}

impl Primitive {
    pub fn new(
        current_texture_index: usize,
        atlas_size: (u32, u32),
        textures: Vec<(Arc<wgpu::Texture>, (u32, u32))>,
        bounds: Rectangle,
    ) -> Self {
        Self {
            current_texture_index,
            atlas_size,
            textures,
            bounds,
        }
    }
}

impl shader::Primitive for Primitive {
    fn prepare(
        &self,
        device: &iced_wgpu::wgpu::Device,
        queue: &iced_wgpu::wgpu::Queue,
        format: iced_wgpu::wgpu::TextureFormat,
        storage: &mut shader::Storage,
        bounds: &Rectangle,  // Use this to get the actual shader widget size
        viewport: &Viewport, // No longer used for size calculation
    ) {
        let scale_factor = viewport.scale_factor() as f32;
        let window_size = viewport.physical_size();
        let viewport_size = viewport.physical_size();

        let shader_size = (
            (bounds.width * viewport.scale_factor() as f32) as u32,
            (bounds.height * viewport.scale_factor() as f32) as u32,
        );
        
        let bounds_physical = (
            (bounds.x * scale_factor) as f32,
            (bounds.y * scale_factor) as f32,
            (bounds.width * scale_factor) as f32,
            (bounds.height * scale_factor) as f32,
        );
    
        let bounds_relative = (
            bounds_physical.0 / viewport_size.width as f32,
            bounds_physical.1 / viewport_size.height as f32,
            bounds_physical.2 / viewport_size.width as f32,
            bounds_physical.3 / viewport_size.height as f32,
        );

        if !storage.has::<Pipeline>() {
            let dim = self.textures[self.current_texture_index].1;
            let texture = self.textures[self.current_texture_index].0.clone();

            storage.store(Pipeline::new(
                device,
                queue,
                format,
                texture,
                self.atlas_size,
                shader_size,  // Use shader_size instead of full window size
                dim,
                bounds_relative,
            ));
        } else {
            /*let dim = self.textures[self.current_texture_index].1;
            let texture = self.textures[self.current_texture_index].0.clone();

            storage.store(Pipeline::new(
                device,
                queue,
                format,
                texture,
                self.atlas_size,
                shader_size,  // Use shader_size instead of full window size
                dim,
                bounds_relative,
            ));*/

            let pipeline = storage.get_mut::<Pipeline>().unwrap();
            let texture = self.textures[self.current_texture_index].0.clone();
            let dim = self.textures[self.current_texture_index].1;
            println!("Primitive::prepare: self.current_texture_index: {}", self.current_texture_index);

            pipeline.update_vertices(device, bounds_relative);
            pipeline.update_texture(device, queue, texture);
            pipeline.update_screen_uniforms(queue, dim, shader_size, bounds_relative);

            // TODO: Update the pipeline with the new texture
            // let pipeline = storage.get::<Pipeline>().unwrap();

            // Calculate the scaled dimensions for maintaining aspect ratio
            // ...

            // Update uniforms for texture sampling
            /*pipeline.update_uniforms(
                queue,
                (0, 0),                   // Offset (no atlas usage here)
                (img_width, img_height),  // Actual image dimensions
                shader_size,              // Actual shader widget size instead of full window
                self.atlas_size,
            );

            // Update uniforms for screen scaling and centering
            pipeline.update_screen_uniforms(
                queue,
                (img_width, img_height), // Actual image dimensions
                shader_size,             // Shader widget size
                (window_size.width, window_size.height), // Full window size
            );*/
        }
    }

    fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        storage: &shader::Storage,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        let pipeline = storage.get::<Pipeline>().unwrap();
        pipeline.render(target, encoder, clip_bounds);
    }
}

impl<Message> shader::Program<Message> for Scene {
    type State = ();
    type Primitive = Primitive;

    fn draw(
        &self,
        _state: &Self::State,
        _cursor: mouse::Cursor,
        bounds: Rectangle,
    ) -> Self::Primitive {
        Primitive::new(
            self.current_image_index,
            self.atlas_size,
            self.textures.clone(),
            bounds,
        )
    }
}