use iced_widget::shader::{self, Viewport};
use iced_winit::core::{Color, Element, Rectangle, Length::*, Theme, mouse};
use iced_wgpu::wgpu;
use crate::widgets::shader::pipeline::Pipeline;
use image::GenericImageView;
use std::sync::Arc;

use crate::cache::img_cache::CachedData;

pub struct Scene {
    pub texture: Option<Arc<wgpu::Texture>>, // Store the active texture
    pub texture_size: (u32, u32),            // Store texture dimensions
}

impl Scene {
    pub fn new(initial_image: Option<&CachedData>) -> Self {
        let (texture, texture_size) = match initial_image {
            Some(CachedData::Gpu(tex)) => (
                Some(Arc::clone(tex)), (tex.width(), tex.height())
            ),
            _ => (None, (0, 0)), // Default to (0,0) if no texture
        };
        println!("Scene::new: texture_size: {:?}", texture_size);

        Scene { texture, texture_size }
    }

    pub fn update_texture(&mut self, new_texture: Arc<wgpu::Texture>) {
        self.texture = Some(new_texture);
    }


}

#[derive(Debug)]
pub struct Primitive {
    texture: Arc<wgpu::Texture>,
    texture_size: (u32, u32),
    bounds: Rectangle,
}

impl Primitive {
    pub fn new(
        texture: Arc<wgpu::Texture>,
        texture_size: (u32, u32),
        bounds: Rectangle,
    ) -> Self {
        Self {
            texture,
            texture_size,
            bounds,
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
        bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        let scale_factor = viewport.scale_factor() as f32;
        let viewport_size = viewport.physical_size();

        let shader_size = (
            (bounds.width * scale_factor) as u32,
            (bounds.height * scale_factor) as u32,
        );

        let bounds_relative = (
            (bounds.x * scale_factor) / viewport_size.width as f32,
            (bounds.y * scale_factor) / viewport_size.height as f32,
            (bounds.width * scale_factor) / viewport_size.width as f32,
            (bounds.height * scale_factor) / viewport_size.height as f32,
        );

        if !storage.has::<Pipeline>() {
            storage.store(Pipeline::new(
                device,
                queue,
                format,
                self.texture.clone(), // ✅ Use the current texture
                //elf.atlas_size,
                shader_size,
                self.texture_size,
                bounds_relative,
            ));
        } else {
            let pipeline = storage.get_mut::<Pipeline>().unwrap();

            pipeline.update_vertices(device, bounds_relative);
            pipeline.update_texture(device, queue, self.texture.clone()); // ✅ Update with current texture
            pipeline.update_screen_uniforms(queue, self.texture_size, shader_size, bounds_relative);
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
        if let Some(texture) = &self.texture {
            Primitive::new(
                Arc::clone(texture),  // ✅ Pass the current GPU texture
                self.texture_size,    // ✅ Pass the correct dimensions
                bounds,
            )
        } else {
            panic!("No texture available for rendering in Scene!");
        }
    }
}



/*impl shader::Primitive for Primitive {
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
            //println!("Primitive::prepare: self.current_texture_index: {}", self.current_texture_index);

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
}*/