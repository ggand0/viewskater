use iced_widget::shader::{self, Viewport};
use iced_winit::core::{Rectangle, mouse};
use iced_wgpu::wgpu;
use crate::widgets::shader::texture_pipeline::TexturePipeline;
use std::sync::Arc;

use crate::cache::img_cache::CachedData;
use std::time::Instant;
use crate::utils::timing::TimingStats;
use once_cell::sync::Lazy;
use std::sync::Mutex;

static SHADER_UPDATE_STATS: Lazy<Mutex<TimingStats>> = Lazy::new(|| {
    Mutex::new(TimingStats::new("Shader Update"))
});

//#[derive(Clone)]

#[derive(Debug)]
pub struct TextureScene {
    pub texture: Option<Arc<wgpu::Texture>>, // Store the active texture
    pub texture_size: (u32, u32),            // Store texture dimensions
}

impl TextureScene {
    pub fn new(initial_image: Option<&CachedData>) -> Self {
        let (texture, texture_size) = match initial_image {
            Some(CachedData::Gpu(tex)) => (
                Some(Arc::clone(tex)), (tex.width(), tex.height())
            ),
            _ => (None, (0, 0)), // Default to (0,0) if no texture
        };
        println!("Scene::new: texture_size: {:?}", texture_size);

        TextureScene { texture, texture_size }
    }

    pub fn update_texture(&mut self, new_texture: Arc<wgpu::Texture>) {
        self.texture = Some(new_texture);
    }


}

#[derive(Debug)]
pub struct TexturePrimitive {
    texture: Arc<wgpu::Texture>,
    texture_size: (u32, u32),
    bounds: Rectangle,
}

impl TexturePrimitive {
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

impl shader::Primitive for TexturePrimitive {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        storage: &mut shader::Storage,
        bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        let debug = false;
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

        if !storage.has::<TexturePipeline>() {
            storage.store(TexturePipeline::new(
                device,
                queue,
                format,
                self.texture.clone(), // Use the current texture
                //self.atlas_size,
                shader_size,
                self.texture_size,
                bounds_relative,
            ));
        } else {
            let pipeline = storage.get_mut::<TexturePipeline>().unwrap();

            let start = Instant::now();
            pipeline.update_vertices(device, bounds_relative);
            pipeline.update_texture(device, queue, self.texture.clone()); // Update with current texture
            pipeline.update_screen_uniforms(queue, self.texture_size, shader_size, bounds_relative);
            let duration = start.elapsed();
            SHADER_UPDATE_STATS.lock().unwrap().add_measurement(duration);
        }

        if debug {
            println!("SHADER_DEBUG: Initial bounds: {:?}", bounds);
            println!("SHADER_DEBUG: Image size: {:?}", self.texture_size);
            println!("SHADER_DEBUG: Viewport size: {:?}, scale_factor: {}", viewport_size, scale_factor);
            println!("SHADER_DEBUG: Shader size (physical): {:?}", shader_size);
            println!("SHADER_DEBUG: Bounds relative (normalized): {:?}", bounds_relative);
            println!("SHADER_DEBUG: ==============================================");
        }
    }

    fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        storage: &shader::Storage,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        let pipeline = storage.get::<TexturePipeline>().unwrap();
        pipeline.render(target, encoder, clip_bounds);
    }
}

impl<Message> shader::Program<Message> for TextureScene {
    type State = ();
    type Primitive = TexturePrimitive;

    fn draw(
        &self,
        _state: &Self::State,
        _cursor: mouse::Cursor,
        bounds: Rectangle,
    ) -> Self::Primitive {
        println!("TEXTURE_SCENE_DEBUG: Bounds in TextureScene.draw(): {:?}", bounds);
        println!("TEXTURE_SCENE_DEBUG: Texture size: {:?}", self.texture_size);
        if let Some(texture) = &self.texture {
            TexturePrimitive::new(
                Arc::clone(texture),  // Pass the current GPU texture
                self.texture_size,    // Pass the correct dimensions
                bounds,
            )
        } else {
            panic!("No texture available for rendering in Scene!");
        }
    }
}