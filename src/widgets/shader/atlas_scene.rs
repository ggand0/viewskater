use iced_widget::shader::{self, Viewport};
use iced_winit::core::{Color, Element, Rectangle, Length::*, Theme, mouse};
use iced_wgpu::wgpu;
use std::sync::Arc;
use std::sync::RwLock;

use crate::atlas::atlas::Atlas;
use crate::atlas::entry::Entry;
use crate::cache::img_cache::CachedData;
use crate::widgets::shader::atlas_pipeline::AtlasPipeline;

#[derive(Debug)]
pub struct AtlasScene {
    atlas: Arc<RwLock<Atlas>>,
    entry: Option<Entry>,        // Current image's location in atlas
    image_size: (u32, u32),     // Original image dimensions
}

impl AtlasScene {
    pub fn new(atlas: Arc<RwLock<Atlas>>) -> Self {
        Self {
            atlas,
            entry: None,
            image_size: (0, 0),
        }
    }

    pub fn update_image(&mut self, entry: Entry, width: u32, height: u32) {
        self.entry = Some(entry);
        self.image_size = (width, height);
    }
}

#[derive(Debug)]
pub struct AtlasPrimitive {
    atlas: Arc<RwLock<Atlas>>,
    entry: Entry,
    image_size: (u32, u32),
    bounds: Rectangle,
}

impl shader::Primitive for AtlasPrimitive {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        storage: &mut shader::Storage,
        bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        if let Ok(atlas_guard) = self.atlas.read() {
            if !storage.has::<AtlasPipeline>() {
                storage.store(AtlasPipeline::new(
                    device,
                    format,
                    &atlas_guard,
                    self.image_size,
                    bounds,
                    viewport,
                ));
            } else {
                let pipeline = storage.get_mut::<AtlasPipeline>().unwrap();
                pipeline.update_vertices(device, bounds, viewport);
                pipeline.update_uniforms(queue, &self.entry, self.image_size, &atlas_guard);
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
        if let Some(pipeline) = storage.get::<AtlasPipeline>() {
            pipeline.render(target, encoder, clip_bounds);
        }
    }
}

impl<Message> shader::Program<Message> for AtlasScene {
    type State = ();
    type Primitive = AtlasPrimitive;

    fn draw(
        &self,
        _state: &Self::State,
        _cursor: mouse::Cursor,
        bounds: Rectangle,
    ) -> Self::Primitive {
        AtlasPrimitive {
            atlas: Arc::clone(&self.atlas),
            entry: self.entry.clone().expect("No atlas entry set"),
            image_size: self.image_size,
            bounds,
        }
    }
}