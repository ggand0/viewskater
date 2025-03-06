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
use std::collections::HashMap;

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

// Add this struct to hold multiple pipeline instances
#[derive(Debug, Default)]
pub struct PipelineRegistry {
    pipelines: std::collections::HashMap<String, TexturePipeline>,
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
        let debug = true;
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

        // Create a unique key for this pipeline instance
        let pipeline_key = format!("pipeline_{}_{}_{}_{}", 
                                  bounds.x, bounds.y, bounds.width, bounds.height);

        if debug {
            println!("Preparing pipeline with key {}", pipeline_key);
        }

        // Get or create the registry
        if !storage.has::<PipelineRegistry>() {
            storage.store(PipelineRegistry::default());
        }
        
        // Get a mutable reference to our registry
        let registry = storage.get_mut::<PipelineRegistry>().unwrap();
        
        // Create or update the pipeline for this specific position
        if !registry.pipelines.contains_key(&pipeline_key) {
            if debug {
                println!("Creating new TexturePipeline for key {} - bounds: {:?}, texture_size: {:?}", 
                        pipeline_key, self.bounds, self.texture_size);
            }
            
            // Create a new pipeline for this specific position
            let pipeline = TexturePipeline::new(
                device,
                queue,
                format,
                self.texture.clone(),
                shader_size,
                self.texture_size,
                bounds_relative,
            );
            
            // Store it with its unique key
            registry.pipelines.insert(pipeline_key.clone(), pipeline);
        } else {
            // Update existing pipeline
            let pipeline = registry.pipelines.get_mut(&pipeline_key).unwrap();
            
            let start = Instant::now();
            pipeline.update_vertices(device, bounds_relative);
            pipeline.update_texture(device, queue, self.texture.clone());
            pipeline.update_screen_uniforms(queue, self.texture_size, shader_size, bounds_relative);
            let duration = start.elapsed();
            SHADER_UPDATE_STATS.lock().unwrap().add_measurement(duration);
        }

        if debug {
            println!("SHADER_DEBUG: Key: {}", pipeline_key);
            println!("SHADER_DEBUG: Initial bounds: {:?}", bounds);
            println!("SHADER_DEBUG: Image size: {:?}", self.texture_size);
            println!("SHADER_DEBUG: Viewport size: {:?}, scale_factor: {}", viewport_size, scale_factor);
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
        // Generate the same unique key for retrieval
        let pipeline_key = format!("pipeline_{}_{}_{}_{}", 
                                  self.bounds.x, self.bounds.y, self.bounds.width, self.bounds.height);
        
        // Debug what we're about to render
        println!("RENDER: About to render pipeline {} for bounds {:?}", pipeline_key, self.bounds);
        
        // Get the registry and find our pipeline
        if let Some(registry) = storage.get::<PipelineRegistry>() {
            if let Some(pipeline) = registry.pipelines.get(&pipeline_key) {
                // We found our pipeline, render it
                println!("RENDER: Found pipeline for key {}", pipeline_key);
                
                // IMPORTANT: Change how we render
                // Create our own render pass with specific settings for this pipeline
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some(&format!("Pass for {}", pipeline_key)),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: target,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            // CRUCIAL: Keep using Load to preserve previous content
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                });
                
                // Set scissor rect to only draw in our area
                pass.set_scissor_rect(
                    clip_bounds.x,
                    clip_bounds.y,
                    clip_bounds.width,
                    clip_bounds.height,
                );
                
                pass.set_pipeline(&pipeline.pipeline);
                pass.set_bind_group(0, &pipeline.bind_group, &[]);
                pass.set_vertex_buffer(0, pipeline.vertex_buffer.slice(..));
                pass.set_index_buffer(pipeline.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(0..pipeline.num_indices, 0, 0..1);
                
                println!("RENDER: Successfully rendered pipeline {} at clip_bounds {:?}", 
                        pipeline_key, clip_bounds);
            } else {
                println!("ERROR: Pipeline not found for key {}", pipeline_key);
            }
        } else {
            println!("ERROR: PipelineRegistry not found in storage");
        }
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