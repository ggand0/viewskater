use std::sync::Arc;
use std::sync::Mutex;
use once_cell::sync::Lazy;
use iced_core::{Length, Size, Point, ContentFit};
use iced_widget::shader::{self, Viewport};
use iced_winit::core::{Rectangle, mouse};
use iced_wgpu::wgpu;
use crate::widgets::shader::texture_pipeline::TexturePipeline;
use crate::cache::img_cache::CachedData;
use crate::utils::timing::TimingStats;

static _SHADER_UPDATE_STATS: Lazy<Mutex<TimingStats>> = Lazy::new(|| {
    Mutex::new(TimingStats::new("Shader Update"))
});

#[derive(Debug, Clone)]
pub struct TextureScene {
    pub texture: Option<Arc<wgpu::Texture>>,
    pub texture_size: (u32, u32),
    pub width: Length,
    pub height: Length,
    pub content_fit: ContentFit,  // Use Iced's ContentFit enum
}

impl TextureScene {
    pub fn new(initial_image: Option<&CachedData>) -> Self {
        let (texture, texture_size) = match initial_image {
            Some(CachedData::Gpu(tex)) => (
                Some(Arc::clone(tex)), (tex.width(), tex.height())
            ),
            Some(CachedData::BC1(tex)) => (
                Some(Arc::clone(tex)), (tex.width(), tex.height())
            ),
            _ => (None, (0, 0)),
        };
        
        TextureScene { 
            texture, 
            texture_size,
            width: Length::Fill,
            height: Length::Fill,
            content_fit: ContentFit::Contain,
        }
    }
    
    // Add builder methods like Iced's Image widget
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }
    
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }
    
    pub fn content_fit(mut self, content_fit: ContentFit) -> Self {
        self.content_fit = content_fit;
        self
    }
    
    pub fn update_texture(&mut self, new_texture: Arc<wgpu::Texture>) {
        // Get width and height before moving the Arc
        let width = new_texture.width();
        let height = new_texture.height();
        
        self.texture = Some(new_texture);
        self.texture_size = (width, height);
    }
}

// Simplified primitive that just stores the layout rectangle and texture
#[derive(Debug)]
pub struct TexturePrimitive {
    pub texture: Arc<wgpu::Texture>,
    pub texture_size: (u32, u32),
    pub bounds: Rectangle,         // Full widget bounds
    pub content_bounds: Rectangle, // Bounds that maintain aspect ratio
}

impl TexturePrimitive {
    pub fn new(
        texture: Arc<wgpu::Texture>,
        texture_size: (u32, u32),
        bounds: Rectangle,
        content_bounds: Rectangle,
    ) -> Self {
        Self {
            texture,
            texture_size,
            bounds,
            content_bounds,
        }
    }
    
    pub fn placeholder(_bounds: Rectangle) -> Self {
        // Create a 1x1 white texture as placeholder
        // Simplified implementation - you'd create a real placeholder texture
        unimplemented!("Need to create a placeholder texture")
    }
}

// Struct to hold multiple pipeline instances
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
        _bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        let debug = true; // CRITICAL: Enable debugging
        let scale_factor = viewport.scale_factor() as f32;
        let viewport_size = viewport.physical_size();
        
        // CRUCIAL: The content_bounds preserve aspect ratio, we need to use these precisely
        let content_bounds = self.content_bounds;
        
        if debug {
            println!("###############PREPARE: Original bounds: {:?}", self.bounds);
            println!("###############PREPARE: Content bounds (aspect-preserved): {:?}", content_bounds);
            println!("###############PREPARE: Viewport size: {:?}", viewport_size);
            println!("###############PREPARE: Scale factor: {}", scale_factor);
        }
        
        // CRITICAL FIX: Calculate normalized device coordinates properly
        // These are percentages of the viewport, not percentages of the bounds
        let x_rel = content_bounds.x * scale_factor / viewport_size.width as f32;
        let y_rel = content_bounds.y * scale_factor / viewport_size.height as f32;
        let width_rel = content_bounds.width * scale_factor / viewport_size.width as f32;
        let height_rel = content_bounds.height * scale_factor / viewport_size.height as f32;
        
        let bounds_relative = (x_rel, y_rel, width_rel, height_rel);
        
        if debug {
            println!("PREPARE: Relative bounds: {:?}", bounds_relative);
        }

        // Create a pipeline with exactly these bounds
        let pipeline_key = format!("pipeline_{:.2}_{:.2}_{:.2}_{:.2}",
                                 bounds_relative.0, bounds_relative.1,
                                 bounds_relative.2, bounds_relative.3);
        
        // Registry setup
        if !storage.has::<PipelineRegistry>() {
            storage.store(PipelineRegistry::default());
        }
        
        let registry = storage.get_mut::<PipelineRegistry>().unwrap();
        
        // Create or update pipeline
        if !registry.pipelines.contains_key(&pipeline_key) {
            if debug {
                println!("Creating new TexturePipeline with bounds_relative: {:?}", bounds_relative);
            }
            
            let pipeline = TexturePipeline::new(
                device,
                queue,
                format,
                self.texture.clone(),
                (viewport_size.width, viewport_size.height),
                self.texture_size,
                bounds_relative,
            );
            
            registry.pipelines.insert(pipeline_key.clone(), pipeline);
        } else {
            // Only update the texture if needed
            let pipeline = registry.pipelines.get_mut(&pipeline_key).unwrap();
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
        let content_bounds = self.content_bounds;
        
        // Calculate the same key used in prepare
        // This needs to match exactly what we used in prepare
        let x_rel = content_bounds.x / 1.0; // We don't have viewport size here
        let y_rel = content_bounds.y / 1.0;
        let width_rel = content_bounds.width / 1.0;
        let height_rel = content_bounds.height / 1.0;
        
        // Create a pipeline with exactly these bounds - need to match prepare exactly
        let pipeline_key = format!("pipeline_{:.2}_{:.2}_{:.2}_{:.2}",
                                 x_rel, y_rel, width_rel, height_rel);
        
        // Simply retrieve the pipeline and call its render method
        let registry = storage.get::<PipelineRegistry>().unwrap();
        
        if let Some(pipeline) = registry.pipelines.get(&pipeline_key) {
            pipeline.render(target, encoder, clip_bounds);
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
        if let Some(texture) = &self.texture {
            // Calculate the content bounds based on content_fit
            let image_size = Size::new(self.texture_size.0 as f32, self.texture_size.1 as f32);
            let container_size = bounds.size();
            
            // Apply content_fit to maintain aspect ratio
            let fitted_size = self.content_fit.fit(image_size, container_size);
            
            // Calculate position (centered in the bounds)
            let x = bounds.x + (bounds.width - fitted_size.width) / 2.0;
            let y = bounds.y + (bounds.height - fitted_size.height) / 2.0;
            
            // These are the actual bounds where the image should be drawn
            let content_bounds = Rectangle::new(Point::new(x, y), fitted_size);
            
            TexturePrimitive::new(
                Arc::clone(texture),
                self.texture_size,
                bounds,            // Original layout bounds
                content_bounds,    // Calculated content bounds
            )
        } else {
            // Return a placeholder primitive if no texture
            TexturePrimitive::placeholder(bounds)
        }
    }
}