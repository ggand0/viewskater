use std::path::PathBuf;

use iced_core::Event;
use iced_core::image::Handle;
use iced_core::Color;

use crate::cache::img_cache::{CachedData, CacheStrategy, ImageMetadata, LoadOperation};
use crate::menu::PaneLayout;
use crate::file_io;
use iced_wgpu::engine::CompressionStrategy;

/// Result of async directory enumeration
#[derive(Debug, Clone)]
pub struct DirectoryEnumResult {
    pub file_paths: Vec<PathBuf>,
    pub directory_path: String,
    pub original_path: PathBuf,
    pub initial_index: usize,
}

/// Error type for async directory enumeration
#[derive(Debug, Clone)]
pub enum DirectoryEnumError {
    NoImagesFound,
    DirectoryError(String),
    NotFound,
}

/// Result type for slider image widget loading: (pane_idx, position, handle, dimensions, file_size)
pub type SliderImageWidgetResult = Result<(usize, usize, Handle, (u32, u32), u64), (usize, usize)>;

/// Result type for batch image loading: (cached_data, metadata, load_operation)
pub type ImagesLoadedResult = Result<(Vec<Option<CachedData>>, Vec<Option<ImageMetadata>>, Option<LoadOperation>), std::io::ErrorKind>;

#[derive(Debug, Clone)]
pub enum Message {
    Debug(String),
    Nothing,
    ShowAbout,
    HideAbout,
    ShowOptions,
    HideOptions,
    SaveSettings,
    ClearSettingsStatus,
    SettingsTabSelected(usize),
    ShowLogs,
    OpenSettingsDir,
    ExportDebugLogs,
    ExportAllLogs,
    OpenWebLink(String),
    // Note: Changed from font::Error to () since the error is never used
    #[allow(dead_code)]
    FontLoaded(Result<(), ()>),
    OpenFolder(usize),
    OpenFile(usize),
    FileDropped(isize, String),
    Close,
    Quit,
    ReplayKeepAlive,
    FolderOpened(Result<String, file_io::Error>, usize),
    DirectoryEnumerated(Result<DirectoryEnumResult, DirectoryEnumError>, usize),
    SliderChanged(isize, u16),
    SliderReleased(isize, u16),
    #[allow(dead_code)]
    SliderImageLoaded(Result<(usize, CachedData), usize>),
    SliderImageWidgetLoaded(SliderImageWidgetResult),
    Event(Event),
    ImagesLoaded(ImagesLoadedResult),
    OnSplitResize(u16),
    ResetSplit(u16),
    ToggleSliderType(bool),
    TogglePaneLayout(PaneLayout),
    ToggleFooter(bool),
    PaneSelected(usize, bool),
    CopyFilename(usize),
    CopyFilePath(usize),
    #[allow(dead_code)]
    BackgroundColorChanged(Color),
    #[allow(dead_code)]
    TimerTick,
    SetCacheStrategy(CacheStrategy),
    SetCompressionStrategy(CompressionStrategy),
    ToggleFpsDisplay(bool),
    ToggleSplitOrientation(bool),
    ToggleSyncedZoom(bool),
    ToggleMouseWheelZoom(bool),
    ToggleCopyButtons(bool),
    ToggleMetadataDisplay(bool),
    ToggleNearestNeighborFilter(bool),
    #[cfg(feature = "coco")]
    ToggleCocoSimplification(bool),
    #[cfg(feature = "coco")]
    SetCocoMaskRenderMode(crate::settings::CocoMaskRenderMode),
    ToggleFullScreen(bool),
    CursorOnTop(bool),
    CursorOnMenu(bool),
    CursorOnFooter(bool),
    #[cfg(feature = "selection")]
    SelectionAction(crate::widgets::selection_widget::SelectionMessage),
    #[cfg(feature = "coco")]
    CocoAction(crate::coco::widget::CocoMessage),
    // Advanced settings input
    AdvancedSettingChanged(String, String),  // (field_name, value)
    ResetAdvancedSettings,
    // Window resize
    WindowResized(f32),  // new width
}
