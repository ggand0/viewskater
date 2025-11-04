// Slider Image Shader Widget
// Renders images from atlas during slider navigation

#[allow(unused_imports)]
use log::{debug, info, warn, error};

use std::marker::PhantomData;
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::sync::Mutex;
use std::collections::VecDeque;
use once_cell::sync::Lazy;
use iced_core::ContentFit;
use iced_core::layout::Layout;
use iced_winit::core::{layout, mouse, renderer, widget, Element, Length, Rectangle, Size};
use iced_winit::core::widget::Tree;
use iced_widget::shader::{self, Viewport, Storage};
use iced_wgpu::{wgpu, primitive};
use iced_wgpu::engine::CompressionStrategy;
use iced_wgpu::wgpu::util::DeviceExt;

use crate::slider_atlas::{Atlas, AtlasPipeline, Entry};

/// Simplified widget for rendering images from atlas during slider movement
pub struct SliderImageShader<Message> {
    pane_idx: usize,
    image_idx: usize,
    image_bytes: Vec<u8>,  // RGBA8 image data
    image_size: (u32, u32),
    width: Length,
    height: Length,
    content_fit: ContentFit,
    _phantom: PhantomData<Message>,
}

impl<Message> SliderImageShader<Message> {
    /// Create a new SliderImageShader
    pub fn new(
        pane_idx: usize,
        image_idx: usize,
        image_bytes: Vec<u8>,
        image_size: (u32, u32),
    ) -> Self {
        Self {
            pane_idx,
            image_idx,
            image_bytes,
            image_size,
            width: Length::Fill,
            height: Length::Fill,
            content_fit: ContentFit::Contain,
            _phantom: PhantomData,
        }
    }

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
}

// Implement Widget trait
impl<Message, Theme, R> widget::Widget<Message, Theme, R> for SliderImageShader<Message>
where
    R: primitive::Renderer,
{
    fn size(&self) -> Size<Length> {
        Size {
            width: self.width,
            height: self.height,
        }
    }

    fn layout(
        &self,
        _tree: &mut Tree,
        _renderer: &R,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::atomic(limits, self.width, self.height)
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut R,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();

        let primitive = SliderImagePrimitive {
            pane_idx: self.pane_idx,
            image_idx: self.image_idx,
            image_bytes: self.image_bytes.clone(),
            image_size: self.image_size,
            bounds,
            content_fit: self.content_fit,
        };

        renderer.draw_primitive(bounds, primitive);
    }
}

// Convert to Element
impl<'a, Message, Theme, R> From<SliderImageShader<Message>> for Element<'a, Message, Theme, R>
where
    Message: 'a,
    R: primitive::Renderer + 'a,
{
    fn from(shader: SliderImageShader<Message>) -> Self {
        Element::new(shader)
    }
}

// Primitive for rendering
#[derive(Debug)]
struct SliderImagePrimitive {
    pane_idx: usize,
    image_idx: usize,
    image_bytes: Vec<u8>,
    image_size: (u32, u32),
    bounds: Rectangle,
    content_fit: ContentFit,
}

impl shader::Primitive for SliderImagePrimitive {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        storage: &mut Storage,
        _bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        // Start timing
        if let Ok(mut tracker) = SLIDER_PERF_TRACKER.lock() {
            tracker.start_prepare();
        }
        
        // Create atlas if not exists
        if !storage.has::<SliderAtlasState>() {
            debug!("Creating new SliderAtlasState");
            let state = SliderAtlasState::new(device, wgpu::Backend::Vulkan);  // TODO: Get actual backend
            storage.store(state);
        }

        // Create pipeline if not exists
        if !storage.has::<AtlasPipeline>() {
            debug!("Creating new AtlasPipeline");
            let pipeline = AtlasPipeline::new(device, format);
            storage.store(pipeline);
        }

        // Get mutable access to atlas state
        let state = storage.get_mut::<SliderAtlasState>().unwrap();
        
        // Upload image to atlas (or get cached entry)
        let key = AtlasKey {
            pane_idx: self.pane_idx,
            image_idx: self.image_idx,
        };

        // Check if already uploaded
        if !state.entries.contains_key(&key) {
            debug!("Uploading image to atlas: pane={}, image={}", self.pane_idx, self.image_idx);
            
            // Upload to atlas
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Slider Atlas Upload"),
            });
            
            if let Some(entry) = state.atlas.upload(
                device,
                &mut encoder,
                self.image_size.0,
                self.image_size.1,
                &self.image_bytes,
            ) {
                queue.submit(Some(encoder.finish()));
                state.entries.insert(key, entry);
                debug!("Successfully uploaded to atlas");
            } else {
                warn!("Failed to upload image to atlas");
            }
        } else {
            debug!("Atlas entry already exists: pane={}, image={}", self.pane_idx, self.image_idx);
        }

        // Initialize registry if needed
        if !storage.has::<SliderResourceRegistry>() {
            debug!("Creating new SliderResourceRegistry");
            storage.store(SliderResourceRegistry::new());
        }

        // Check if we already have cached resources for this key
        let registry = storage.get_mut::<SliderResourceRegistry>().unwrap();
        if registry.get(&key).is_some() {
            debug!("Reusing cached GPU resources for pane={}, image={} (cache size={})", 
                   self.pane_idx, self.image_idx, registry.resources.len());
            return;
        }
        
        // New image - calculate content bounds and create GPU resources
        let content_bounds = self.calculate_content_bounds(viewport);
        
        // Create vertex buffer
        let viewport_size = viewport.physical_size();
        let (x, y, width, height) = (
            content_bounds.x / viewport_size.width as f32,
            content_bounds.y / viewport_size.height as f32,
            content_bounds.width / viewport_size.width as f32,
            content_bounds.height / viewport_size.height as f32,
        );
        
        let left = 2.0 * x - 1.0;
        let right = 2.0 * (x + width) - 1.0;
        let top = 1.0 - 2.0 * y;
        let bottom = 1.0 - 2.0 * (y + height);
        
        let vertices: [f32; 16] = [
            left, bottom, 0.0, 1.0,   // Bottom-left
            right, bottom, 1.0, 1.0,  // Bottom-right
            right, top, 1.0, 0.0,     // Top-right
            left, top, 0.0, 0.0,      // Top-left
        ];
        
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Slider Atlas Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        
        // Get pipeline first (immutable borrow)
        let pipeline = storage.get::<AtlasPipeline>().expect("Pipeline should exist");
        
        // Then get state again to access atlas and entry (separate scope)
        let state = storage.get::<SliderAtlasState>().expect("State should exist");
        let entry = state.entries.get(&key).expect("Entry should exist after upload");
        
        // Create uniform buffer and bind group in prepare() where we have device access
        debug!("Creating new GPU resources for pane={}, image={}", self.pane_idx, self.image_idx);
        let (uniform_buffer, bind_group) = pipeline.create_render_resources(
            device,
            &state.atlas,
            entry,
        );
        
        // Store in registry with LRU tracking
        let registry = storage.get_mut::<SliderResourceRegistry>().unwrap();
        registry.insert(key, SliderGpuResources {
            vertex_buffer,
            uniform_buffer,
            bind_group,
            content_bounds,
        });
        
        debug!("Cached new slider image: pane={}, image={}, cache_size={}", 
               self.pane_idx, self.image_idx, registry.resources.len());
        
        // End timing
        if let Ok(mut tracker) = SLIDER_PERF_TRACKER.lock() {
            tracker.end_prepare();
        }
    }

    fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        storage: &Storage,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        let Some(pipeline) = storage.get::<AtlasPipeline>() else {
            warn!("AtlasPipeline not found in storage");
            return;
        };

        let registry = storage.get::<SliderResourceRegistry>().expect("Registry should exist after prepare");
        
        let key = AtlasKey {
            pane_idx: self.pane_idx,
            image_idx: self.image_idx,
        };
        
        let Some(resources) = registry.resources.get(&key) else {
            warn!("GPU resources not found in cache for pane={}, image={}", self.pane_idx, self.image_idx);
            return;
        };

        // Render using cached resources
        pipeline.render_with_resources(
            &resources.vertex_buffer,
            &resources.bind_group,
            encoder,
            target,
            clip_bounds,
        );
    }
}

impl SliderImagePrimitive {
    fn calculate_content_bounds(&self, viewport: &Viewport) -> Rectangle {
        let scale_factor = viewport.scale_factor() as f32;
        
        // Image size
        let image_size = Size::new(self.image_size.0 as f32, self.image_size.1 as f32);
        
        // Available bounds
        let bounds_size = self.bounds.size();
        
        // Calculate fitted size based on ContentFit
        let (width, height) = match self.content_fit {
            ContentFit::Contain => {
                let ratio = (bounds_size.width / image_size.width)
                    .min(bounds_size.height / image_size.height);
                (image_size.width * ratio, image_size.height * ratio)
            }
            ContentFit::Fill => (bounds_size.width, bounds_size.height),
            ContentFit::Cover => {
                let ratio = (bounds_size.width / image_size.width)
                    .max(bounds_size.height / image_size.height);
                (image_size.width * ratio, image_size.height * ratio)
            }
            ContentFit::ScaleDown => {
                let ratio = (bounds_size.width / image_size.width)
                    .min(bounds_size.height / image_size.height)
                    .min(1.0);
                (image_size.width * ratio, image_size.height * ratio)
            }
            ContentFit::None => (image_size.width, image_size.height),
        };
        
        // Center the image
        let x = self.bounds.x + (bounds_size.width - width) / 2.0;
        let y = self.bounds.y + (bounds_size.height - height) / 2.0;
        
        // Apply scale factor for physical coordinates
        Rectangle {
            x: x * scale_factor,
            y: y * scale_factor,
            width: width * scale_factor,
            height: height * scale_factor,
        }
    }
}

// Key for identifying atlas entries
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct AtlasKey {
    pane_idx: usize,
    image_idx: usize,
}

// Atlas state stored in shader Storage
#[derive(Debug)]
struct SliderAtlasState {
    atlas: Atlas,
    entries: std::collections::HashMap<AtlasKey, Entry>,
}

impl SliderAtlasState {
    fn new(device: &wgpu::Device, backend: wgpu::Backend) -> Self {
        let bind_group_layout = Arc::new(device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("slider_atlas_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                },
                count: None,
            }],
        }));
        
        let atlas = Atlas::new(
            device,
            backend,
            bind_group_layout,
            CompressionStrategy::None,  // No compression for slider - speed over memory
        );
        
        Self {
            atlas,
            entries: std::collections::HashMap::new(),
        }
    }
}

// GPU resources for a prepared slider image
struct SliderGpuResources {
    vertex_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    content_bounds: Rectangle,
}

impl std::fmt::Debug for SliderGpuResources {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SliderGpuResources")
            .field("content_bounds", &self.content_bounds)
            .field("vertex_buffer", &"wgpu::Buffer")
            .field("uniform_buffer", &"wgpu::Buffer")
            .field("bind_group", &"wgpu::BindGroup")
            .finish()
    }
}

// Registry for caching slider GPU resources with LRU eviction
#[derive(Default)]
struct SliderResourceRegistry {
    resources: std::collections::HashMap<AtlasKey, SliderGpuResources>,
    keys_order: std::collections::VecDeque<AtlasKey>,
    max_resources: usize,
}

impl std::fmt::Debug for SliderResourceRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SliderResourceRegistry")
            .field("cached_count", &self.resources.len())
            .field("max_resources", &self.max_resources)
            .finish()
    }
}

impl SliderResourceRegistry {
    fn new() -> Self {
        Self {
            resources: std::collections::HashMap::new(),
            keys_order: std::collections::VecDeque::new(),
            max_resources: 50,  // Cache up to 50 slider images
        }
    }
    
    fn insert(&mut self, key: AtlasKey, resource: SliderGpuResources) {
        // If key already exists, update its position
        if self.resources.contains_key(&key) {
            if let Some(pos) = self.keys_order.iter().position(|k| k == &key) {
                self.keys_order.remove(pos);
            }
        } else if self.resources.len() >= self.max_resources && !self.keys_order.is_empty() {
            // At capacity - evict oldest
            if let Some(oldest_key) = self.keys_order.pop_front() {
                self.resources.remove(&oldest_key);
                debug!("Evicted slider resources for pane={}, image={}", oldest_key.pane_idx, oldest_key.image_idx);
            }
        }
        
        // Add to end (most recently used)
        self.keys_order.push_back(key);
        self.resources.insert(key, resource);
    }
    
    fn get(&mut self, key: &AtlasKey) -> Option<&SliderGpuResources> {
        if self.resources.contains_key(key) {
            // Update LRU: move to end
            if let Some(pos) = self.keys_order.iter().position(|k| k == key) {
                self.keys_order.remove(pos);
                self.keys_order.push_back(*key);
            }
            return self.resources.get(key);
        }
        None
    }
    
    fn clear(&mut self) {
        debug!("Clearing all slider resource cache ({} entries)", self.resources.len());
        self.resources.clear();
        self.keys_order.clear();
    }
}

// Performance tracking for slider rendering
static SLIDER_PERF_TRACKER: Lazy<Mutex<SliderPerfTracker>> = 
    Lazy::new(|| Mutex::new(SliderPerfTracker::new()));

#[derive(Debug)]
struct SliderPerfTracker {
    prepare_times: VecDeque<Duration>,
    frame_timestamps: VecDeque<Instant>,
    window_duration: Duration,
    current_prepare_start: Option<Instant>,
    total_frames: u64,
}

impl SliderPerfTracker {
    fn new() -> Self {
        Self {
            prepare_times: VecDeque::with_capacity(100),
            frame_timestamps: VecDeque::with_capacity(120),
            window_duration: Duration::from_secs(3),
            current_prepare_start: None,
            total_frames: 0,
        }
    }
    
    fn start_prepare(&mut self) {
        self.current_prepare_start = Some(Instant::now());
    }
    
    fn end_prepare(&mut self) {
        if let Some(start) = self.current_prepare_start.take() {
            let duration = start.elapsed();
            self.prepare_times.push_back(duration);
            if self.prepare_times.len() > 100 {
                self.prepare_times.pop_front();
            }
        }
    }
    
    fn record_frame(&mut self) {
        self.frame_timestamps.push_back(Instant::now());
        self.total_frames += 1;
        
        // Calculate and log FPS every 30 frames
        if self.total_frames % 30 == 0 {
            let fps = self.calculate_fps();
            let avg_prepare = self.get_avg_prepare_time();
            info!("SLIDER PERF: Frames: {}, FPS: {:.1}, Prepare: {:.2}ms", 
                  self.total_frames, fps, avg_prepare * 1000.0);
        }
    }
    
    fn calculate_fps(&mut self) -> f64 {
        let now = Instant::now();
        let cutoff = now - self.window_duration;
        
        // Remove old timestamps
        while !self.frame_timestamps.is_empty() && 
              self.frame_timestamps.front().unwrap() < &cutoff {
            self.frame_timestamps.pop_front();
        }
        
        if self.frame_timestamps.len() > 1 {
            let oldest = self.frame_timestamps.front().unwrap();
            let elapsed = now.duration_since(*oldest).as_secs_f64();
            if elapsed > 0.0 {
                return self.frame_timestamps.len() as f64 / elapsed;
            }
        }
        0.0
    }
    
    fn get_avg_prepare_time(&self) -> f64 {
        if self.prepare_times.is_empty() {
            0.0
        } else {
            self.prepare_times.iter().sum::<Duration>().as_secs_f64() 
                / self.prepare_times.len() as f64
        }
    }
}

/// Get current slider FPS for display in debug menu
pub fn get_slider_fps() -> f64 {
    if let Ok(mut tracker) = SLIDER_PERF_TRACKER.lock() {
        tracker.calculate_fps()
    } else {
        0.0
    }
}

/// Get current slider performance stats for display
pub fn get_slider_perf_stats() -> (f64, f64) {
    if let Ok(mut tracker) = SLIDER_PERF_TRACKER.lock() {
        let fps = tracker.calculate_fps();
        let avg_prepare = tracker.get_avg_prepare_time();
        (fps, avg_prepare * 1000.0)  // Convert to ms
    } else {
        (0.0, 0.0)
    }
}

/// Record a slider frame when image is loaded (called from message handler)
pub fn record_slider_frame() {
    if let Ok(mut tracker) = SLIDER_PERF_TRACKER.lock() {
        tracker.record_frame();
    }
}

