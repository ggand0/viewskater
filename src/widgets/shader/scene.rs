//use iced::widget::shader::{self, Viewport};
use iced_widget::shader::{self, Viewport};
//use iced::{Element, Rectangle, mouse};
use iced_winit::core::{Color, Element, Rectangle, Length::*, Theme, mouse};
use iced_wgpu::wgpu;
use crate::widgets::shader::pipeline::Pipeline;
use image::GenericImageView;
use std::sync::Arc;


impl Default for Scene {
    fn default() -> Self {
        Self {
            current_image_index: 0,
            atlas_size: (8192, 8192),
            textures: vec![],
            image_data: vec![],
            pending_window_size: None,
        }
    }
}

pub struct Scene {
    current_image_index: usize,
    atlas_size: (u32, u32),
    textures: Vec<(Arc<wgpu::Texture>, (u32, u32))>, // Store textures only
    image_data: Vec<(Vec<u8>, (u32, u32))>, // Store the image data
    pending_window_size: Option<(u32, u32)>
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

    
    pub fn update_screen_rect(&mut self, window_size: (u32, u32)) {
        println!("Scene received window size update: {:?}", window_size);
        self.pending_window_size = Some(window_size); // Store window size
    }
}

#[derive(Debug)]
pub struct Primitive {
    current_texture_index: usize,
    atlas_size: (u32, u32),
    textures: Vec<(Arc<wgpu::Texture>, (u32, u32))>, // Include dimensions
    //image_data: Vec<(Vec<u8>, (u32, u32))>,
}

impl Primitive {
    pub fn new(
        current_texture_index: usize,
        atlas_size: (u32, u32),
        textures: Vec<(Arc<wgpu::Texture>, (u32, u32))>,
        //image_data: Vec<(Vec<u8>, (u32, u32))>,
    ) -> Self {
        Self {
            current_texture_index,
            atlas_size,
            textures,
            //image_data,
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
        _bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        //println!("Preparing primitive");
        let window_size = viewport.physical_size();
        let window_size_tuple = (window_size.width, window_size.height);

        if !storage.has::<Pipeline>() {
            let dim = self.textures[self.current_texture_index].1;
            let texture = self.textures[self.current_texture_index].0.clone();

            storage.store(Pipeline::new(
                device,
                queue,
                format,
                texture,
                self.atlas_size,
                window_size_tuple,
                dim,
            ));
        } else {
            let pipeline = storage.get::<Pipeline>().unwrap();
            let (img_width, img_height) = self.textures[self.current_texture_index].1;

            // Calculate the scaled dimensions for maintaining aspect ratio
            let window_aspect_ratio = window_size.width as f32 / window_size.height as f32;
            let img_aspect_ratio = img_width as f32 / img_height as f32;
    
            let (scaled_width, scaled_height) = if img_aspect_ratio > window_aspect_ratio {
                (
                    window_size.width as f32,
                    window_size.width as f32 / img_aspect_ratio,
                )
            } else {
                (
                    window_size.height as f32 * img_aspect_ratio,
                    window_size.height as f32,
                )
            };
    
            // Calculate offsets to center the image in the window
            let offset_x = (window_size.width as f32 - scaled_width) / 2.0;
            let offset_y = (window_size.height as f32 - scaled_height) / 2.0;

            //println!("img_width: {}, img_height: {}", img_width, img_height); // img_width: 3590, img_height: 2396
            //println!("window_size_tuple: {:?}", window_size_tuple); // window_size_tuple: (1280, 960)
            //println!("atlas_size: {:?}", self.atlas_size); // atlas_size: (8192, 8192)
    
            // Update uniforms for texture sampling
            pipeline.update_uniforms(
                queue,
                (0, 0), // Offset
                (img_width, img_height), // Actual image dimensions
                window_size_tuple,
                self.atlas_size,
            );
    
            // Update uniforms for screen scaling and centering
            pipeline.update_screen_uniforms(
                queue,
                (img_width, img_height), // Actual image dimensions
                window_size_tuple,       // Current window size
            );
        }
    }

    fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        storage: &shader::Storage,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        //println!("Rendering primitive");
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
        _bounds: Rectangle,
    ) -> Self::Primitive {
        //println!("Drawing primitive");
        Primitive::new(
            0,
            self.atlas_size,
            self.textures.clone(),
            //self.image_data.clone(),
        )
    }
}