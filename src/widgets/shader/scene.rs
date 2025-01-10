use iced::widget::shader::{self, Viewport};
use iced::{Element, Rectangle, Size, mouse};
use iced_wgpu::wgpu;
use crate::widgets::shader::pipeline::Pipeline;


pub struct Scene {
    current_image_index: usize,
    atlas_size: (u32, u32),
    image_data: Option<Vec<(Vec<u8>, (u32, u32))>>, // Store the image data
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            current_image_index: 0,
            atlas_size: (0, 0),
            image_data: None,
        }
    }
}

impl Scene {
    pub fn new(
        image_data: Vec<(Vec<u8>, (u32, u32),
    )>) -> Self {
        ////let atlas_size = (8192, 8192); // Example: large enough for 3 4K images
        let atlas_size = image_data[0].1; // Use the image size directly

        Self {
            current_image_index: 0, // Start with image2.jpg
            atlas_size,
            image_data
        }
    }

    pub fn set_current_image(&mut self, index: usize) {
        // Update the current image index
        self.current_image_index = index;

        // Optional: Perform additional logic if needed, such as updating uniforms
    }

    pub fn update(&mut self) {
        // Any dynamic updates if needed
    }
}

#[derive(Debug)]
pub struct Primitive {
    image_offset: (u32, u32),
    atlas_size: (u32, u32),
    image_data: Vec<(Vec<u8>, (u32, u32))>,
}

impl Primitive {
    pub fn new(image_offset: (u32, u32), atlas_size: (u32, u32),
        image_data: Vec<(Vec<u8>, (u32, u32))>, 
    ) -> Self {
        Self {
            image_offset,
            atlas_size,
            image_data,
        }
    }
}

impl shader::Primitive for Primitive {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        storage: &mut shader::Storage,
        _bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        let limits = device.limits();
        let max_texture_size = limits.max_texture_dimension_2d;
        //println!("Maximum texture size: {} x {}", max_texture_size, max_texture_size);

        let max_texture_size = limits.max_texture_dimension_2d;
        let atlas_size = (max_texture_size, max_texture_size); // Use the maximum size supported

        // Get the window size from viewport
        let window_size = viewport.physical_size();
        let window_size_tuple = (window_size.width, window_size.height);
    
        if !storage.has::<Pipeline>() {
            // If no pipeline exists, create a new one
            storage.store(Pipeline::new(
                device,
                queue,
                format,
                self.image_data.clone(),
                self.atlas_size,
                window_size_tuple,
            ));
        } else {
            // Retrieve the pipeline
            let pipeline = storage.get::<Pipeline>().unwrap();
    
            // Scale the image dimensions proportionally to the window size
            //let (img_width, img_height) = self.image_data[self.current_image_index].1;
            let (img_width, img_height) = self.image_data[0].1;
    
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
    
            // Update uniforms for texture sampling
            pipeline.update_uniforms(
                queue,
                self.image_offset,
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
        _clip_bounds: &Rectangle<u32>,
    ) {
        let pipeline = storage.get::<Pipeline>().unwrap();
        pipeline.render(target, encoder);
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
        Primitive::new(
            (0, 0),
            self.atlas_size,
            self.image_data.clone(), // Pass the image data
        )
    }
}