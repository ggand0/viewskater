use std::sync::Arc;
use std::sync::Mutex;
use once_cell::sync::Lazy;
use iced_widget::shader::{self, Viewport};
use iced_winit::core::{Rectangle, mouse};
use iced_wgpu::wgpu;

use crate::cache::img_cache::CachedData;
use crate::utils::timing::TimingStats;
use crate::widgets::shader::texture_scene::TextureScene;
use crate::widgets::shader::texture_scene::TexturePrimitive;
use crate::widgets::shader::texture_pipeline::TexturePipeline;
use crate::widgets::shader::cpu_scene::{CpuScene, CpuPrimitive};

static _SHADER_UPDATE_STATS: Lazy<Mutex<TimingStats>> = Lazy::new(|| {
    Mutex::new(TimingStats::new("Shader Update"))
});

#[derive(Debug, Clone)]
pub enum Scene {
    TextureScene(TextureScene),
    CpuScene(CpuScene),
}

#[derive(Debug)]
pub enum ScenePrimitive {
    Texture(TexturePrimitive),
    Cpu(CpuPrimitive),
}

impl Scene {
    pub fn new(initial_image: Option<&CachedData>) -> Self {
        match initial_image {
            Some(CachedData::Gpu(texture)) => {
                Scene::TextureScene(TextureScene::new(Some(&CachedData::Gpu(Arc::clone(texture)))))
            }
            Some(CachedData::Cpu(image_bytes)) => {
                Scene::CpuScene(CpuScene::new(image_bytes.clone(), true))
            }
            _ => {
                Scene::TextureScene(TextureScene::new(None))
            }
        }
    }

    pub fn get_texture(&self) -> Option<&Arc<wgpu::Texture>> {
        match self {
            Scene::TextureScene(scene) => scene.texture.as_ref(),
            Scene::CpuScene(scene) => scene.texture.as_ref(),
        }
    }

    pub fn update_texture(&mut self, texture: Arc<wgpu::Texture>) {
        match self {
            Scene::TextureScene(scene) => scene.update_texture(texture),
            Scene::CpuScene(_) => {
                // Not applicable for CPU scene
            }
        }
    }

    pub fn update_cpu_image(&mut self, image_bytes: Vec<u8>) {
        if let Scene::CpuScene(scene) = self {
            scene.update_image(image_bytes);
        }
    }

    pub fn has_valid_dimensions(&self) -> bool {
        match self {
            Scene::TextureScene(scene) => scene.texture_size.0 > 0 && scene.texture_size.1 > 0,
            Scene::CpuScene(scene) => scene.texture_size.0 > 0 && scene.texture_size.1 > 0,
        }
    }

    pub fn ensure_texture(&mut self, device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>, pane_id: usize) {
        match self {
            Scene::CpuScene(cpu_scene) => {
                cpu_scene.ensure_texture(&device, &queue, &format!("pane_{}", pane_id));
            }
            _ => {
                // Other scene types already have textures managed
            }
        }
    }
}

impl<Message> shader::Program<Message> for Scene {
    type State = ();
    type Primitive = ScenePrimitive;

    fn draw(
        &self,
        _state: &Self::State,
        cursor: mouse::Cursor,
        bounds: Rectangle,
    ) -> Self::Primitive {
        match self {
            Scene::TextureScene(scene) => {
                let texture_primitive = <TextureScene as iced_widget::shader::Program<Message>>::draw(scene, &(), cursor, bounds);
                ScenePrimitive::Texture(texture_primitive)
            }
            Scene::CpuScene(scene) => {
                let cpu_primitive = <CpuScene as iced_widget::shader::Program<Message>>::draw(scene, &(), cursor, bounds);
                ScenePrimitive::Cpu(cpu_primitive)
            }
        }
    }
}

impl shader::Primitive for ScenePrimitive {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        storage: &mut shader::Storage,
        bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        match self {
            ScenePrimitive::Texture(primitive) => {
                primitive.prepare(device, queue, format, storage, bounds, viewport)
            }
            ScenePrimitive::Cpu(primitive) => {
                primitive.prepare(device, queue, format, storage, bounds, viewport)
            }
        }
    }

    fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        storage: &shader::Storage,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        match self {
            ScenePrimitive::Texture(primitive) => {
                primitive.render(encoder, storage, target, clip_bounds)
            }
            ScenePrimitive::Cpu(primitive) => {
                primitive.render(encoder, storage, target, clip_bounds)
            }
        }
    }
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct Primitive {
    texture: Arc<wgpu::Texture>,
    texture_size: (u32, u32),
    bounds: Rectangle,
}

impl Primitive {
    #[allow(dead_code)]
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

        if !storage.has::<TexturePipeline>() {
            storage.store(TexturePipeline::new(
                device,
                queue,
                format,
                self.texture.clone(),
                shader_size,
                self.texture_size,
                bounds_relative,
            ));
        } else {
            let pipeline = storage.get_mut::<TexturePipeline>().unwrap();
            pipeline.update_texture(device, queue, self.texture.clone());
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