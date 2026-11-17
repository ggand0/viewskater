use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use eframe::egui;

use crate::animation::{AnimationPlayer, AnimationPoll};
use crate::cache::{self, Decoded, Loaded};
use crate::decode::image_to_color_image;
use crate::file_io;
use crate::metadata::MetadataRecord;
use crate::settings::{ImageDiscoveryOptions};
use crate::view_animation::{Easing, ViewAnimation, ViewTransform};

const MIN_ZOOM: f32 = 0.05;
const MAX_ZOOM: f32 = 100.0;

const IMAGE_DOUBLE_CLICK_MAX_DELAY: f64 = 0.30;
const IMAGE_DOUBLE_CLICK_MAX_DISTANCE: f32 = 8.0;
const DOUBLE_CLICK_ZOOM_ANIMATION_DURATION: f64 = 0.12;

/// 当前图片的手动旋转方向，仅影响显示。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Rotation {
    #[default]
    None,
    Clockwise90,
    Clockwise180,
    Clockwise270,
}

impl Rotation {
    fn clockwise(self) -> Self {
        match self {
            Self::None => Self::Clockwise90,
            Self::Clockwise90 => Self::Clockwise180,
            Self::Clockwise180 => Self::Clockwise270,
            Self::Clockwise270 => Self::None,
        }
    }

    fn counter_clockwise(self) -> Self {
        match self {
            Self::None => Self::Clockwise270,
            Self::Clockwise90 => Self::None,
            Self::Clockwise180 => Self::Clockwise90,
            Self::Clockwise270 => Self::Clockwise180,
        }
    }

    /// 旋转后的显示尺寸；90° / 270° 时宽高互换。
    fn size(self, size: egui::Vec2) -> egui::Vec2 {
        match self {
            Self::None | Self::Clockwise180 => size,
            Self::Clockwise90 | Self::Clockwise270 => egui::vec2(size.y, size.x),
        }
    }

    /// 把未旋转图片上的偏移量映射到旋转后的屏幕坐标。
    fn rotate_vec(self, v: egui::Vec2) -> egui::Vec2 {
        match self {
            Self::None => v,
            Self::Clockwise90 => egui::vec2(-v.y, v.x),
            Self::Clockwise180 => egui::vec2(-v.x, -v.y),
            Self::Clockwise270 => egui::vec2(v.y, -v.x),
        }
    }
}

#[derive(Clone, Copy)]
struct ImageClick {
    time: f64,
    pos: egui::Pos2,
    image_index: usize,
}

pub(crate) struct Pane {
    /// Top level directory from which the pane loaded files
    pub(crate) dir_path: Option<PathBuf>,
    pub(crate) image_paths: Vec<PathBuf>,
    pub(crate) current_index: usize,
    pub(crate) current_texture: Option<egui::TextureHandle>,
    rotation: Rotation,
    /// File facts and EXIF of the image on screen, from the same decode
    /// that produced `current_texture`. Set when the image changes, not
    /// when an animation frame swaps the texture. Present without a
    /// texture when the pixels failed to decode.
    pub(crate) current_record: Option<Arc<MetadataRecord>>,
    animation: Option<AnimationPlayer>,
    pub(crate) zoom: f32,
    pub(crate) pan: egui::Vec2,
    pub(crate) cache: Option<cache::SlidingWindowCache>,
    pub(crate) thumbnail_cache: Option<cache::ThumbnailCache>,
    slider_loader: Option<cache::SliderLoader>,
    pub(crate) decode_cache: cache::DecodeLruCache,
    pub(crate) cache_count: usize,
    pub(crate) lru_budget_mb: usize,
    pub(crate) decode_threads: usize,
    pub(crate) selected: bool,
    pub(crate) mouse_wheel_zoom: bool,
    pub(crate) reset_zoom_pan_on_navigation: bool,
    pub(crate) preview_budget_mb: usize,
    last_image_click: Option<ImageClick>,
    /// A primary click on the image since `take_image_click` last ran.
    image_clicked: bool,
    view_animation: Option<ViewAnimation>,
    /// How long each synchronous load in `load_sync` took, plus the LRU hit
    /// count, recorded only while `--bench-slider` asks
    /// (`record_sync_load_times`).
    sync_load_times: Option<(Vec<crate::bench::slider::SyncLoadTiming>, usize)>,
}

impl Pane {
    pub(crate) fn new(
        ctx: &egui::Context,
        cache_count: usize,
        lru_budget_mb: usize,
        decode_threads: usize,
        mouse_wheel_zoom: bool,
        reset_zoom_pan_on_navigation: bool,
        preview_budget_mb: usize,
    ) -> Self {
        Self {
            dir_path: None,
            image_paths: Vec::new(),
            current_index: 0,
            current_texture: None,
            rotation: Rotation::None,
            current_record: None,
            animation: None,
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
            cache: None,
            thumbnail_cache: None,
            slider_loader: None,
            decode_cache: cache::DecodeLruCache::new(ctx, lru_budget_mb),
            cache_count,
            lru_budget_mb,
            decode_threads,
            selected: true,
            mouse_wheel_zoom,
            reset_zoom_pan_on_navigation,
            preview_budget_mb,
            last_image_click: None,
            image_clicked: false,
            view_animation: None,
            sync_load_times: None,
        }
    }

    pub(crate) fn close(&mut self) {
        self.image_paths.clear();
        self.current_index = 0;
        self.current_texture = None;
        self.rotation = Rotation::None;
        self.current_record = None;
        self.animation = None;
        self.zoom = 1.0;
        self.pan = egui::Vec2::ZERO;
        self.cache = None;
        self.thumbnail_cache = None;
        self.slider_loader = None;
        self.decode_cache.clear();
    }

    pub(crate) fn open_path(
        &mut self,
        path: &std::path::Path,
        ctx: &egui::Context,
        discovery_options: ImageDiscoveryOptions,
    ) {
        if !path.exists() {
            log::error!("Path does not exist: {}", path.display());
            return;
        }

        let (dir, target_filename) = file_io::resolve_path(path);
        self.image_paths = file_io::enumerate_images(&dir, discovery_options);

        if self.image_paths.is_empty() {
            log::warn!("No supported images found in {}", dir.display());
            return;
        }
        self.dir_path = Some(dir);

        self.current_index = target_filename
            .and_then(|name| {
                self.image_paths.iter().position(|p| {
                    p.file_name().map(|f| f.to_string_lossy().into_owned()) == Some(name.clone())
                })
            })
            .unwrap_or(0);

        self.zoom = 1.0;
        self.pan = egui::Vec2::ZERO;
        self.rotation = Rotation::None;
        self.decode_cache.clear();
        self.animation = None;

        let mut c = cache::SlidingWindowCache::new(ctx, self.cache_count, self.decode_threads);
        let record = c.initialize(self.current_index, &self.image_paths);
        self.show_sync_result(c.loaded_for(self.current_index), record, ctx);
        self.cache = Some(c);
        self.thumbnail_cache = Some(cache::ThumbnailCache::new(ctx, self.preview_budget_mb));
        self.slider_loader = Some(cache::SliderLoader::new(ctx));
    }

    /// Synchronous decode fallback for slider drag and jump.
    /// Checks the GPU-backed LRU first to skip both decode and re-upload on
    /// revisits. On miss, decodes from disk and uploads a new texture via
    /// `DecodeLruCache::insert`, which also handles budget eviction.
    fn load_sync(&mut self, ctx: &egui::Context) {
        let Some(path) = self.image_paths.get(self.current_index).cloned() else {
            return;
        };
        let file_index = self.current_index;

        // LRU hit — texture is already on the GPU, no upload.
        if let Some(cached) = self.decode_cache.get(file_index) {
            self.set_current(Some(cached), ctx);
            log::debug!("LRU hit [{}]", file_index);
            if let Some((_, hits)) = &mut self.sync_load_times {
                *hits += 1;
            }
            return;
        }

        let t0 = Instant::now();
        let loaded = file_io::load_image(&path);
        match loaded.image {
            Ok(img) => {
                let decode_ms = t0.elapsed().as_secs_f64() * 1000.0;

                let t1 = Instant::now();
                let color_image = image_to_color_image(img, loaded.orientation);
                let convert_ms = t1.elapsed().as_secs_f64() * 1000.0;

                let size = color_image.size;
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "slider_sync".into());

                let t2 = Instant::now();
                let cached = self.decode_cache.insert(file_index, name, color_image, loaded.record);
                let upload_ms = t2.elapsed().as_secs_f64() * 1000.0;
                self.set_current(Some(cached), ctx);
                if let Some((times, _)) = &mut self.sync_load_times {
                    times.push(crate::bench::slider::SyncLoadTiming { decode_ms, convert_ms, upload_ms });
                }

                log::debug!(
                    "load_sync [{}] ({}x{}): decode={:.1}ms convert={:.1}ms upload={:.1}ms total={:.1}ms [LRU: {} / {:.0} MB]",
                    file_index, size[0], size[1],
                    decode_ms, convert_ms, upload_ms,
                    t0.elapsed().as_secs_f64() * 1000.0,
                    self.decode_cache.len(),
                    self.decode_cache.total_mb(),
                );
            }
            Err(e) => {
                if !loaded.record.no_embedded_preview {
                    log::error!("Failed to load {}: {}", path.display(), e);
                }
                self.set_current_failed(loaded.record, ctx);
            }
        }
    }

    fn reset_view(&mut self) {
        self.zoom = 1.0;
        self.pan = egui::Vec2::ZERO;
    }

    /// 顺时针或逆时针旋转当前显示，不改动文件。
    pub(crate) fn rotate(&mut self, clockwise: bool) {
        if self.current_texture.is_none() {
            return;
        }
        self.rotation = if clockwise {
            self.rotation.clockwise()
        } else {
            self.rotation.counter_clockwise()
        };
    }

    /// Try to navigate by `delta` images. Returns true if the display advanced.
    pub(crate) fn navigate(&mut self, delta: isize, ctx: &egui::Context) -> bool {
        if self.image_paths.is_empty() {
            return false;
        }
        let new_index = (self.current_index as isize + delta)
            .clamp(0, self.image_paths.len() as isize - 1) as usize;
        if new_index == self.current_index {
            return false;
        }

        if let Some(decoded) = self
            .cache
            .as_ref()
            .and_then(|cache| cache.decoded_for(new_index))
        {
            self.current_index = new_index;
            self.rotation = Rotation::None;

            if let Some(cache) = &mut self.cache {
                if delta > 0 {
                    cache.navigate_forward(new_index, &self.image_paths);
                } else {
                    cache.navigate_backward(new_index, &self.image_paths);
                }

                let summary = cache.summary();

                if self.reset_zoom_pan_on_navigation {
                    self.reset_view();
                }

                let dir = if delta > 0 { "→" } else { "←" };
                log::debug!(
                    "nav {} {}/{} cache={} hit",
                    dir,
                    new_index,
                    self.image_paths.len(),
                    summary,
                );
            }
            self.show_decoded(decoded, ctx);
            return true;
        }
        false
    }

    pub(crate) fn jump_to(&mut self, index: usize, ctx: &egui::Context) {
        let index = index.min(self.image_paths.len().saturating_sub(1));
        if index == self.current_index {
            return;
        }

        self.current_index = index;
        self.rotation = Rotation::None;

        if self.reset_zoom_pan_on_navigation {
            self.reset_view();
        }

        if let Some(cache) = &mut self.cache {
            cache.jump_to(index, &self.image_paths);
            let decoded = cache.decoded_for(index);
            let summary = cache.summary();
            log::debug!(
                "jump {}/{} cache={} {}",
                index,
                self.image_paths.len(),
                summary,
                if decoded.is_some() { "hit" } else { "miss" },
            );
            match decoded {
                Some(decoded) => self.show_decoded(decoded, ctx),
                None => self.load_sync(ctx),
            }
        } else {
            self.load_sync(ctx);
        }
    }

    pub(crate) fn can_navigate_forward(&self) -> bool {
        !self.image_paths.is_empty() && self.current_index < self.image_paths.len() - 1
    }

    pub(crate) fn can_navigate_backward(&self) -> bool {
        !self.image_paths.is_empty() && self.current_index > 0
    }

    /// Check whether the next image in the given direction is cached and
    /// ready. A file whose decode failed is ready: there is nothing more
    /// to wait for, and navigation moves onto it and past it.
    pub(crate) fn is_next_cached(&self, delta: isize) -> bool {
        if self.image_paths.is_empty() {
            return false;
        }
        let new_index = (self.current_index as isize + delta)
            .clamp(0, self.image_paths.len() as isize - 1) as usize;
        if new_index == self.current_index {
            return true; // at boundary — nothing to advance to
        }
        self.cache
            .as_ref()
            .is_some_and(|c| c.decoded_for(new_index).is_some())
    }

    /// Benchmark hooks. The pane owns the cache, so these forward to it:
    /// record how long background decodes take, and report when the
    /// sliding window has nothing in flight.
    pub(crate) fn record_decode_times(&mut self, on: bool) {
        if let Some(cache) = &mut self.cache {
            cache.record_decode_times(on);
        }
    }

    pub(crate) fn take_decode_times(&mut self) -> Vec<f64> {
        self.cache.as_mut().map_or_else(Vec::new, |c| c.take_decode_times())
    }

    /// Empty the decode LRU so the next benchmark phase starts from the
    /// same state as the previous one, whatever it loaded.
    pub(crate) fn clear_decode_lru(&mut self) {
        self.decode_cache.clear();
    }

    pub(crate) fn record_sync_load_times(&mut self, on: bool) {
        self.sync_load_times = if on { Some((Vec::new(), 0)) } else { None };
    }

    /// Sync load timings and LRU hits since recording started or the last
    /// call.
    pub(crate) fn take_sync_load_times(&mut self) -> (Vec<crate::bench::slider::SyncLoadTiming>, usize) {
        match &mut self.sync_load_times {
            Some((times, hits)) => (std::mem::take(times), std::mem::take(hits)),
            None => (Vec::new(), 0),
        }
    }

    pub(crate) fn is_settled(&self) -> bool {
        self.cache.as_ref().is_none_or(|c| c.is_settled())
    }

    /// Returns (lru_mb, sliding_window_mb).
    pub(crate) fn cache_memory_mb(&self) -> (f64, f64) {
        let lru = self.decode_cache.total_mb();
        let sw = self.cache.as_ref().map_or(0.0, |c| c.total_mb());
        (lru, sw)
    }

    pub(crate) fn poll_cache(&mut self) {
        if let Some(cache) = &mut self.cache {
            cache.poll(&self.image_paths);
        }
        if let Some(tc) = &mut self.thumbnail_cache {
            tc.poll(&self.image_paths);
        }
    }

    /// Move the current image to the trash through `trasher` and drop it
    /// from the pane. Nothing in the pane changes unless `trasher` returns
    /// `Ok`, so a failed move leaves the list, index and caches as they
    /// were. Returns the path that was removed, or `None` for an empty pane.
    ///
    /// `trasher` is injected so the list bookkeeping can be tested without
    /// touching a real trash; the app passes `trash_bin::move_to_trash`.
    pub(crate) fn remove_current<E>(
        &mut self,
        ctx: &egui::Context,
        trasher: impl FnOnce(&Path) -> Result<(), E>,
    ) -> Result<Option<PathBuf>, E> {
        let Some(path) = self.image_paths.get(self.current_index).cloned() else {
            return Ok(None);
        };
        trasher(&path)?;
        self.remove_index(self.current_index, ctx);
        Ok(Some(path))
    }

    /// Drop the file at `index` from the pane after it left the directory.
    /// Every cache keyed by file index shifts with the list. If the current
    /// image was removed the pane shows the next one, or the previous one
    /// at the end of the list; an emptied pane shows the drop hint.
    pub(crate) fn remove_index(&mut self, index: usize, ctx: &egui::Context) {
        if index >= self.image_paths.len() {
            return;
        }
        self.image_paths.remove(index);
        if let Some(cache) = &mut self.cache {
            cache.remove_index(index, &self.image_paths);
        }
        self.decode_cache.remove_index(index);
        if let Some(tc) = &mut self.thumbnail_cache {
            tc.remove_index(index);
        }

        if self.image_paths.is_empty() {
            self.close();
            return;
        }

        if index < self.current_index {
            // A file before the current one left: same image, lower index.
            self.current_index -= 1;
            return;
        }
        if index > self.current_index {
            return;
        }

        // The current image itself left. Show the file that took its
        // index, or the last file when the removed one was last.
        self.current_index = index.min(self.image_paths.len() - 1);
        self.rotation = Rotation::None;
        if self.reset_zoom_pan_on_navigation {
            self.reset_view();
        }
        let cached = self
            .cache
            .as_ref()
            .and_then(|c| c.decoded_for(self.current_index));
        match cached {
            Some(decoded) => self.show_decoded(decoded, ctx),
            None => self.load_sync(ctx),
        }
    }

    /// Drag the slider to `idx`. Returns true if image was loaded.
    pub(crate) fn apply_slider_target(&mut self, idx: usize, ctx: &egui::Context) -> bool {
        let clamped = idx.min(self.image_paths.len().saturating_sub(1));
        if clamped == self.current_index {
            return false;
        }
        self.current_index = clamped;
        self.rotation = Rotation::None;

        if self.reset_zoom_pan_on_navigation {
            self.reset_view();
        }

        let found_in_cache = self
            .cache
            .as_ref()
            .and_then(|c| c.loaded_for(clamped));

        if let Some(loaded) = found_in_cache {
            self.set_current(Some(loaded), ctx);
            true
        } else if let Some(loader) = &mut self.slider_loader {
            if loader.should_load() {
                self.load_sync(ctx);
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Finalize after slider drag released: re-center cache.
    pub(crate) fn apply_slider_release(&mut self, ctx: &egui::Context) {
        let loaded = if let Some(cache) = &mut self.cache {
            cache.jump_to(self.current_index, &self.image_paths);
            let loaded = cache.loaded_for(self.current_index);
            log::debug!(
                "slider release {}/{} cache={}",
                self.current_index,
                self.image_paths.len(),
                cache.summary(),
            );
            loaded
        } else {
            None
        };
        if let Some(loaded) = loaded {
            self.set_current(Some(loaded), ctx);
        }
    }

    /// Show the pane content. Returns true if zoom/pan was changed by user interaction.
    pub(crate) fn show_content(&mut self, ui: &mut egui::Ui) -> bool {
        let tex = self.current_texture.clone();
        if let Some(tex) = tex {
            return self.show_image(ui, &tex);
        }
        if self.image_paths.is_empty() {
            let available = ui.available_width();
            let font = egui::TextStyle::Body.resolve(ui.style());
            let measure = |text: &str| -> f32 {
                ui.fonts(|f| {
                    f.layout_no_wrap(text.into(), font.clone(), egui::Color32::WHITE)
                        .size()
                        .x
                })
            };
            let full = "Drop an image or folder here";
            let short = "Drop image";
            let label = if available >= measure(full) {
                Some(full)
            } else if available >= measure(short) {
                Some(short)
            } else {
                None
            };
            if let Some(text) = label {
                ui.centered_and_justified(|ui| {
                    ui.label(text);
                });
            }
        } else {
            let no_preview = self.current_record.as_ref().is_some_and(|record| record.no_embedded_preview);
            ui.centered_and_justified(|ui| {
                ui.label(if no_preview { "No embedded preview in this RAW file" } else { "Failed to load image" });
            });
        }
        false
    }

    fn image_double_clicked(&mut self, response: &egui::Response, now: f64) -> Option<egui::Pos2> {
        if !response.clicked_by(egui::PointerButton::Primary) {
            return None;
        }

        let pos = response.interact_pointer_pos()?;
        let double_clicked = self.last_image_click.is_some_and(|last| {
            last.image_index == self.current_index
                && now - last.time <= IMAGE_DOUBLE_CLICK_MAX_DELAY
                && last.pos.distance(pos) <= IMAGE_DOUBLE_CLICK_MAX_DISTANCE
        });

        if double_clicked {
            self.last_image_click = None;
            Some(pos)
        } else {
            self.last_image_click = Some(ImageClick {
                time: now,
                pos,
                image_index: self.current_index,
            });
            None
        }
    }

    fn zoom_target(
        &self,
        zoom_factor: f32,
        anchor: egui::Pos2,
        available: &egui::Rect,
    ) -> (f32, egui::Vec2) {
        let target_zoom = (self.zoom * zoom_factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let old_center = available.center() + self.pan;
        let cursor_rel = anchor - old_center;
        let target_pan = self.pan + cursor_rel * (1.0 - target_zoom / self.zoom);
        (target_zoom, target_pan)
    }

    /// Zooms around a given anchor point, keeping that point fixed under the cursor.
    fn zoom_to(&mut self, zoom_factor: f32, anchor: egui::Pos2, available: &egui::Rect) {
        (self.zoom, self.pan) = self.zoom_target(zoom_factor, anchor, available);
    }

    fn advance_view_animation(&mut self, now: f64, ctx: &egui::Context) {
        let Some(animation) = self.view_animation else {
            return;
        };

        let sample = animation.sample(now);
        self.zoom = sample.transform.zoom;
        self.pan = sample.transform.pan;

        if sample.done {
            self.view_animation = None;
        } else {
            ctx.request_repaint();
        }
    }

    /// True once per primary click on the image since the last call. The
    /// metadata panel follows clicks between panes.
    pub(crate) fn take_image_click(&mut self) -> bool {
        std::mem::take(&mut self.image_clicked)
    }

    pub(crate) fn poll_animation(&mut self) {
        let Some(animation) = &mut self.animation else {
            return;
        };
        match animation.poll() {
            AnimationPoll::NewTexture(texture) => self.current_texture = Some(texture),
            AnimationPoll::Finished => self.animation = None,
            AnimationPoll::Unchanged => {}
        }
    }

    /// Put `loaded` on screen: its texture and its record together, or
    /// clear both.
    fn set_current(&mut self, loaded: Option<Loaded>, ctx: &egui::Context) {
        match loaded {
            Some(loaded) => {
                self.current_texture = Some(loaded.texture);
                self.current_record = Some(loaded.record);
            }
            None => {
                self.current_texture = None;
                self.current_record = None;
            }
        }
        self.start_animation(ctx);
    }

    /// Put what the sliding window has for the current file on screen.
    fn show_decoded(&mut self, decoded: Decoded, ctx: &egui::Context) {
        match decoded {
            Decoded::Image(loaded) => self.set_current(Some(loaded), ctx),
            Decoded::Failed(record) => self.set_current_failed(record, ctx),
        }
    }

    /// The pixels could not be decoded: nothing on screen, but the file
    /// facts still show in the panel and the footer.
    fn set_current_failed(&mut self, record: Arc<MetadataRecord>, ctx: &egui::Context) {
        self.current_texture = None;
        self.current_record = Some(record);
        self.start_animation(ctx);
    }

    /// After the cache decoded the centre image synchronously: show it,
    /// or when its pixels failed, keep the record it produced.
    fn show_sync_result(
        &mut self,
        loaded: Option<Loaded>,
        record: Option<Arc<MetadataRecord>>,
        ctx: &egui::Context,
    ) {
        match (loaded, record) {
            (Some(loaded), _) => self.set_current(Some(loaded), ctx),
            (None, Some(record)) => self.set_current_failed(record, ctx),
            (None, None) => self.set_current(None, ctx),
        }
    }

    fn start_animation(&mut self, ctx: &egui::Context) {
        self.animation = self
            .image_paths
            .get(self.current_index)
            .cloned()
            .filter(|path| self.current_texture.is_some() && file_io::may_have_animation(path))
            .map(|path| AnimationPlayer::new(path, ctx));
    }

    /// Draws the current image with zoom/pan applied and handles view input.
    /// Returns true if the user changed zoom or pan this frame.
    ///
    /// The frame runs in this order: (1) apply a running view animation,
    /// (2) let direct input override it, (3) start a new animation on
    /// double-click, (4) draw. Steps 1-3 only mutate `self.zoom`/`self.pan`;
    /// step 4 reads them once.
    fn show_image(&mut self, ui: &mut egui::Ui, tex: &egui::TextureHandle) -> bool {
        // 0. Setup: skip for an empty pane or texture and snapshot the transform
        let tex_size = tex.size_vec2();
        let available = ui.available_rect_before_wrap();

        if available.width() <= 0.0 || available.height() <= 0.0 {
            return false;
        }
        if tex_size.x <= 0.0 || tex_size.y <= 0.0 {
            return false;
        }

        let old_zoom = self.zoom;
        let old_pan = self.pan;
        let now = ui.input(|i| i.time); // frame clock for double-click timing and the animation

        // 1. Animation in progress: move zoom/pan toward its target for this frame.
        self.advance_view_animation(now, ui.ctx());

        let response = ui.allocate_rect(available, egui::Sense::click_and_drag());
        let shown_size = self.rotation.size(tex_size);
        let scale = (available.width() / shown_size.x).min(available.height() / shown_size.y);
        if response.clicked_by(egui::PointerButton::Primary) {
            self.image_clicked = true;
        }

        // 2. Direct input: applies immediately and cancels any running animation.
        //    Zoom: scroll wheel (when enabled) or Ctrl/Cmd+scroll, plus pinch.
        if response.hovered() && (self.mouse_wheel_zoom || ui.input(|i| i.modifiers.command)) {
            self.zoom_image(ui, &response, &available);
        }

        //    Pan: drag. Also clears a pending first click.
        if response.dragged() {
            self.last_image_click = None;
            self.view_animation = None;
            self.pan += response.drag_delta();
        }

        // 3. Double-click: toggle fit-to-screen / 1:1 by starting an animation (applied in step 1).
        if let Some(click_pos) = self.image_double_clicked(&response, now) {
            let is_fit_to_screen = (self.zoom - 1.0).abs() < f32::EPSILON;

            let (target_zoom, target_pan) = if is_fit_to_screen {
                let actual_size_zoom = 1.0 / scale;
                if actual_size_zoom >= 1.0 {
                    // Larger than the pane: 1:1 anchored at the click.
                    self.zoom_target(actual_size_zoom / self.zoom, click_pos, &available)
                } else {
                    // Smaller than the pane: 1:1 centered.
                    (actual_size_zoom, egui::Vec2::ZERO)
                }
            } else {
                // Back to fit-to-screen.
                (1.0, egui::Vec2::ZERO)
            };

            self.view_animation = Some(ViewAnimation::new(
                ViewTransform::new(self.zoom, self.pan),
                ViewTransform::new(target_zoom, target_pan),
                now,
                DOUBLE_CLICK_ZOOM_ANIMATION_DURATION,
                Easing::EaseOutCubic,
            ));
            ui.ctx().request_repaint();
        }

        // 4. Draw with this frame's final zoom/pan (zero-frame-delay).
        let center = available.center() + self.pan;
        let half = tex_size * (scale * self.zoom * 0.5);
        let corners = [
            egui::vec2(-half.x, -half.y),
            egui::vec2(half.x, -half.y),
            egui::vec2(half.x, half.y),
            egui::vec2(-half.x, half.y),
        ];
        let uvs = [
            egui::pos2(0.0, 0.0),
            egui::pos2(1.0, 0.0),
            egui::pos2(1.0, 1.0),
            egui::pos2(0.0, 1.0),
        ];

        // Clip to the pane rect so a zoomed image stays inside its own pane.
        let painter = ui.painter_at(available);
        let mut mesh = egui::Mesh::with_texture(tex.id());
        for (corner, uv) in corners.into_iter().zip(uvs) {
            mesh.vertices.push(egui::epaint::Vertex {
                pos: center + self.rotation.rotate_vec(corner),
                uv,
                color: egui::Color32::WHITE,
            });
        }
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        painter.add(egui::Shape::mesh(mesh));

        self.zoom != old_zoom || self.pan != old_pan
    }

    // Zoom: scroll wheel + pinch-to-zoom
    fn zoom_image(&mut self, ui: &mut egui::Ui, response: &egui::Response, available: &egui::Rect) {
        let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
        let scroll_zoom_speed = ui.ctx().options(|o| o.scroll_zoom_speed);
        let scroll_factor = (scroll * scroll_zoom_speed).exp();
        let zoom_factor = pinch * scroll_factor;

        if zoom_factor != 1.0 {
            if let Some(hover_pos) = response.hover_pos() {
                self.view_animation = None;
                self.zoom_to(zoom_factor, hover_pos, available);
            }
        }
    }
}

#[cfg(test)]
mod tests;
