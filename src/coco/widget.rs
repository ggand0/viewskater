/// COCO dataset visualization widget and message handling
///
/// This module is only compiled when the "coco" feature is enabled.
/// It encapsulates all COCO-related messages and UI components.
use std::path::PathBuf;
use iced_winit::core::{Element, Color};
use iced_winit::core::Theme as WinitTheme;
use iced_winit::runtime::Task;
use iced_wgpu::Renderer;
use iced_widget::{container, text};
use iced_core::padding;
use iced_core::keyboard::{self, Key};
use iced_core::Vector;
use log::{info, error, warn};

use crate::app::Message;
use super::annotation_manager::AnnotationManager;
use crate::pane::Pane;
use crate::menu::PaneLayout;
use super::parser::CocoDataset;

/// Result type for COCO file loading: (dataset, path, skipped_count, warnings, invalid_image_ids)
type CocoLoadResult = Result<(CocoDataset, PathBuf, usize, Vec<String>, std::collections::HashSet<u64>), String>;

/// COCO-specific messages grouped into a single enum variant
#[derive(Debug, Clone)]
pub enum CocoMessage {
    /// Load COCO JSON file from path
    LoadCocoFile(PathBuf),

    /// COCO file loaded (with result: dataset, path, skipped_count, warnings, images_with_invalid)
    CocoFileLoaded(CocoLoadResult),

    /// User selected image directory (with pending dataset, json path, and invalid images)
    ImageDirectorySelected(Option<PathBuf>, CocoDataset, PathBuf, std::collections::HashSet<u64>),

    /// Toggle bounding box visibility for a pane
    ToggleBoundingBoxes(usize),  // pane_index

    /// Toggle bounding boxes for all panes
    ToggleAllBoundingBoxes,

    /// Toggle segmentation masks for a pane
    ToggleSegmentationMasks(usize),  // pane_index

    /// Toggle segmentation masks for all panes
    ToggleAllSegmentationMasks,

    /// Clear loaded annotations
    ClearAnnotations,

    /// Image zoom/pan changed (pane_index, scale, offset)
    ZoomChanged(usize, f32, Vector),
}

/// Convert CocoMessage to the main Message type
impl From<CocoMessage> for Message {
    fn from(coco_msg: CocoMessage) -> Self {
        Message::CocoAction(coco_msg)
    }
}

/// Creates a badge widget showing COCO annotation status
#[allow(dead_code)]
pub fn coco_badge(has_annotations: bool, num_annotations: usize) -> Element<'static, Message, WinitTheme, Renderer> {
    if !has_annotations {
        return container(text(""))
            .width(0)
            .height(0)
            .into();
    }

    container(
        text(format!("COCO ({})", num_annotations))
            .size(12)
            .style(|_theme| iced_widget::text::Style {
                color: Some(Color::from([1.0, 1.0, 1.0]))
            })
    )
    .padding(padding::all(4))
    .style(|_theme: &WinitTheme| container::Style {
        background: Some(Color::from([0.2, 0.5, 0.8]).into()), // Blue
        border: iced_winit::core::Border {
            radius: 4.0.into(),
            width: 0.0,
            color: Color::TRANSPARENT,
        },
        ..container::Style::default()
    })
    .into()
}

/// Empty badge for when COCO features are disabled
pub fn empty_badge() -> Element<'static, Message, WinitTheme, Renderer> {
    container(text("")).width(0).height(0).into()
}

/// Handle COCO messages by delegating to the annotation manager
///
/// This function encapsulates all COCO-related message handling logic,
/// keeping it separate from the main app.rs update loop.
pub fn handle_coco_message(
    coco_msg: CocoMessage,
    panes: &mut [Pane],
    annotation_manager: &mut AnnotationManager,
) -> Task<Message> {
    match coco_msg {
        CocoMessage::LoadCocoFile(path) => {
            info!("Loading COCO file: {}", path.display());

            // Load the file asynchronously
            Task::perform(
                async move {
                    // Parse the COCO file
                    match CocoDataset::from_file(&path) {
                        Ok(mut dataset) => {
                            // Validate and clean the dataset (filter invalid annotations)
                            let (skipped_count, warnings, images_with_invalid) = dataset.validate_and_clean();
                            Ok((dataset, path, skipped_count, warnings, images_with_invalid))
                        }
                        Err(e) => Err(e),
                    }
                },
                |result| Message::CocoAction(CocoMessage::CocoFileLoaded(result))
            )
        }

        CocoMessage::CocoFileLoaded(result) => {
            match result {
                Ok((dataset, json_path, skipped_count, warnings, images_with_invalid)) => {
                    info!("COCO dataset loaded: {} images, {} annotations",
                          dataset.images.len(), dataset.annotations.len());

                    if skipped_count > 0 {
                        warn!("Skipped {} invalid annotation(s)", skipped_count);
                        for warning in &warnings {
                            warn!("{}", warning);
                        }
                    }

                    // Try to find image directory automatically
                    let json_dir = json_path.parent()
                        .map(|p| p.to_path_buf())
                        .unwrap_or_else(|| PathBuf::from("."));

                    // Try common directory patterns
                    let mut candidates = vec![
                        json_dir.join("images"),
                        json_dir.join("img"),
                        json_dir.join("val2017"),
                        json_dir.join("train2017"),
                        json_dir.clone(),
                    ];

                    // If there's only a single directory in the JSON's parent directory,
                    // add it as a candidate (handles arbitrary directory names)
                    if let Ok(entries) = std::fs::read_dir(&json_dir) {
                        let dirs: Vec<PathBuf> = entries
                            .flatten()
                            .filter_map(|entry| {
                                if entry.file_type().ok()?.is_dir() {
                                    Some(entry.path())
                                } else {
                                    None
                                }
                            })
                            .collect();

                        if dirs.len() == 1 {
                            info!("Found single directory in JSON parent: {:?}", dirs[0]);
                            let single_dir = &dirs[0];
                            candidates.insert(0, single_dir.clone());

                            // Also check nested single subdirectory (depth 1)
                            // Handles structures like default/images or default/whatever_name
                            if let Ok(nested_entries) = std::fs::read_dir(single_dir) {
                                let nested_dirs: Vec<PathBuf> = nested_entries
                                    .flatten()
                                    .filter_map(|entry| {
                                        if entry.file_type().ok()?.is_dir() {
                                            Some(entry.path())
                                        } else {
                                            None
                                        }
                                    })
                                    .collect();

                                if nested_dirs.len() == 1 {
                                    info!("Found single nested directory: {:?}", nested_dirs[0]);
                                    candidates.insert(0, nested_dirs[0].clone());
                                }
                            }
                        }
                    }

                    // Check if we can find images
                    let test_filenames: Vec<_> = dataset.get_image_filenames()
                        .into_iter()
                        .take(3)
                        .collect();

                    let mut found_dir: Option<PathBuf> = None;
                    for candidate in candidates {
                        if candidate.exists() && candidate.is_dir() {
                            let mut count = 0;
                            for filename in &test_filenames {
                                if candidate.join(filename).exists() {
                                    count += 1;
                                }
                            }
                            if count >= 2 {
                                found_dir = Some(candidate);
                                break;
                            }
                        }
                    }

                    if let Some(dir) = found_dir {
                        // Found directory, set it and open the directory for viewing
                        if let Err(e) = annotation_manager.set_image_directory(
                            dataset,
                            json_path,
                            dir.clone(),
                            images_with_invalid,
                        ) {
                            error!("Failed to set image directory: {}", e);
                            Task::none()
                        } else {
                            info!("COCO annotations loaded successfully from directory: {}", dir.display());

                            // Enable bbox and mask rendering by default
                            for pane in panes.iter_mut() {
                                pane.show_bboxes = true;
                                pane.show_masks = true;
                            }

                            // Now open the image directory to actually load and display images
                            // We use FolderOpened message to trigger the standard directory loading
                            Task::done(Message::FolderOpened(
                                Ok(dir.to_string_lossy().to_string()),
                                0  // pane_index
                            ))
                        }
                    } else {
                        // Need to prompt user for directory
                        warn!("Could not auto-detect image directory, prompting user");

                        // Use native-dialog which has better Linux support
                        let initial_dir = json_path.parent()
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_else(|| "~".to_string());

                        Task::perform(
                            async move {
                                let result = tokio::task::spawn_blocking(move || {
                                    native_dialog::FileDialog::new()
                                        .set_title("Select image directory for COCO dataset")
                                        .set_location(&initial_dir)
                                        .show_open_single_dir()
                                }).await;

                                let dir_path = match result {
                                    Ok(Ok(Some(path))) => Some(path),
                                    _ => None,
                                };

                                (dir_path, dataset, json_path, images_with_invalid)
                            },
                            |(dir_path, dataset, json_path, images_with_invalid)| {
                                Message::CocoAction(CocoMessage::ImageDirectorySelected(dir_path, dataset, json_path, images_with_invalid))
                            }
                        )
                    }
                }
                Err(e) => {
                    error!("Failed to load COCO file: {}", e);
                    Task::none()
                }
            }
        }

        CocoMessage::ImageDirectorySelected(maybe_path, dataset, json_path, images_with_invalid) => {
            if let Some(dir_path) = maybe_path {
                info!("User selected image directory: {}", dir_path.display());

                // Set the image directory with the dataset
                if let Err(e) = annotation_manager.set_image_directory(
                    dataset,
                    json_path,
                    dir_path.clone(),
                    images_with_invalid,
                ) {
                    error!("Failed to set image directory: {}", e);
                    Task::none()
                } else {
                    info!("COCO annotations loaded successfully from directory: {}", dir_path.display());

                    // Enable bbox and mask rendering by default
                    for pane in panes.iter_mut() {
                        pane.show_bboxes = true;
                        pane.show_masks = true;
                    }

                    // Now open the image directory to actually load and display images
                    Task::done(Message::FolderOpened(
                        Ok(dir_path.to_string_lossy().to_string()),
                        0  // pane_index
                    ))
                }
            } else {
                warn!("User cancelled directory selection");
                Task::none()
            }
        }

        CocoMessage::ToggleBoundingBoxes(pane_index) => {
            if let Some(pane) = panes.get_mut(pane_index) {
                pane.show_bboxes = !pane.show_bboxes;
                info!("Toggled bounding boxes for pane {}: {}", pane_index, pane.show_bboxes);
            }
            Task::none()
        }

        CocoMessage::ToggleAllBoundingBoxes => {
            // Toggle all panes
            let new_state = panes.first()
                .map(|p| !p.show_bboxes)
                .unwrap_or(true);

            for pane in panes.iter_mut() {
                pane.show_bboxes = new_state;
            }
            info!("Toggled all bounding boxes: {}", new_state);
            Task::none()
        }

        CocoMessage::ToggleSegmentationMasks(pane_index) => {
            if let Some(pane) = panes.get_mut(pane_index) {
                pane.show_masks = !pane.show_masks;
                info!("Toggled segmentation masks for pane {}: {}", pane_index, pane.show_masks);
            }
            Task::none()
        }

        CocoMessage::ToggleAllSegmentationMasks => {
            // Toggle all panes
            let new_state = panes.first()
                .map(|p| !p.show_masks)
                .unwrap_or(true);

            for pane in panes.iter_mut() {
                pane.show_masks = new_state;
            }
            info!("Toggled all segmentation masks: {}", new_state);
            Task::none()
        }

        CocoMessage::ClearAnnotations => {
            annotation_manager.clear();

            // Clear bbox and mask visibility on all panes
            for pane in panes.iter_mut() {
                pane.show_bboxes = false;
                pane.show_masks = false;
            }

            info!("Cleared COCO annotations");
            Task::none()
        }

        CocoMessage::ZoomChanged(pane_index, scale, offset) => {
            // Update zoom state in the corresponding pane
            if let Some(pane) = panes.get_mut(pane_index) {
                pane.zoom_scale = scale;
                pane.zoom_offset = offset;
                log::debug!("ZoomChanged: pane={}, scale={:.2}, offset=({:.1}, {:.1})",
                    pane_index, scale, offset.x, offset.y);
            }
            Task::none()
        }
    }
}

/// Handle COCO-related keyboard events
///
/// Returns Some(Task) if the key was handled, None if not a COCO key
pub fn handle_keyboard_event(
    key: &keyboard::Key,
    _modifiers: keyboard::Modifiers,
    pane_layout: &PaneLayout,
    last_opened_pane: isize,
) -> Option<Task<Message>> {
    // Helper to determine current pane index
    let get_pane_index = || {
        if *pane_layout == PaneLayout::SinglePane {
            0
        } else {
            last_opened_pane as usize
        }
    };

    match key.as_ref() {
        Key::Character("b") | Key::Character("B") => {
            // Toggle bounding boxes for current pane
            let pane_index = get_pane_index();
            Some(Task::done(Message::CocoAction(
                CocoMessage::ToggleBoundingBoxes(pane_index)
            )))
        }
        Key::Character("m") | Key::Character("M") => {
            // Toggle segmentation masks for current pane
            let pane_index = get_pane_index();
            Some(Task::done(Message::CocoAction(
                CocoMessage::ToggleSegmentationMasks(pane_index)
            )))
        }
        _ => None
    }
}
