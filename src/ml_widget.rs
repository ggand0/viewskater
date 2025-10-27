/// ML (Machine Learning) dataset curation widget and message handling
///
/// This module is only compiled when the "ml" feature is enabled.
/// It encapsulates all ML-related messages and UI components.
use std::path::PathBuf;
use iced_winit::core::{Element, Color};
use iced_winit::core::Theme as WinitTheme;
use iced_winit::runtime::Task;
use iced_wgpu::Renderer;
use iced_widget::{container, text};
use iced_core::padding;
use iced_core::keyboard::{self, Key};
use log::{info, error};

use crate::app::Message;
use crate::selection_manager::{ImageMark, SelectionManager};
use crate::pane::Pane;
use crate::menu::PaneLayout;

/// ML-specific messages grouped into a single enum variant
#[derive(Debug, Clone)]
pub enum MlMessage {
    MarkImageSelected(usize),      // pane_index
    MarkImageExcluded(usize),      // pane_index
    ClearImageMark(usize),         // pane_index
    ExportSelectionJson,
    ExportSelectionJsonToPath(PathBuf),
}

/// Convert MlMessage to the main Message type
impl From<MlMessage> for Message {
    fn from(ml_msg: MlMessage) -> Self {
        Message::MlAction(ml_msg)
    }
}

/// Creates a badge widget showing the image's mark status
pub fn mark_badge(mark: ImageMark) -> Element<'static, Message, WinitTheme, Renderer> {
    match mark {
        ImageMark::Selected => container(
            text("SELECTED")
                .size(12)
                .style(|_theme| iced_widget::text::Style {
                    color: Some(Color::from([1.0, 1.0, 1.0]))
                })
        )
        .padding(padding::all(4))
        .style(|_theme: &WinitTheme| container::Style {
            background: Some(Color::from([0.2, 0.8, 0.2]).into()), // Green
            border: iced_winit::core::Border {
                radius: 4.0.into(),
                width: 0.0,
                color: Color::TRANSPARENT,
            },
            ..container::Style::default()
        })
        .into(),
        ImageMark::Excluded => container(
            text("EXCLUDED")
                .size(12)
                .style(|_theme| iced_widget::text::Style {
                    color: Some(Color::from([1.0, 1.0, 1.0]))
                })
        )
        .padding(padding::all(4))
        .style(|_theme: &WinitTheme| container::Style {
            background: Some(Color::from([0.9, 0.2, 0.2]).into()), // Red
            border: iced_winit::core::Border {
                radius: 4.0.into(),
                width: 0.0,
                color: Color::TRANSPARENT,
            },
            ..container::Style::default()
        })
        .into(),
        ImageMark::Unmarked => container(text(""))
            .width(0)
            .height(0)
            .into(),
    }
}

/// Empty badge for when ML features are disabled
pub fn empty_badge() -> Element<'static, Message, WinitTheme, Renderer> {
    container(text("")).width(0).height(0).into()
}

/// Handle ML messages by delegating to the selection manager
///
/// This function encapsulates all ML-related message handling logic,
/// keeping it separate from the main app.rs update loop.
pub fn handle_ml_message(
    ml_msg: MlMessage,
    panes: &[Pane],
    selection_manager: &mut SelectionManager,
) -> Task<Message> {
    match ml_msg {
        MlMessage::MarkImageSelected(pane_index) => {
            if let Some(pane) = panes.get(pane_index) {
                if pane.dir_loaded {
                    let path = &pane.img_cache.image_paths[pane.img_cache.current_index];
                    let filename = path.file_name().to_string();
                    selection_manager.toggle_selected(&filename);
                    info!("Toggled selected: {}", filename);

                    // Save immediately
                    if let Err(e) = selection_manager.save() {
                        error!("Failed to save selection state: {}", e);
                    }
                }
            }
            Task::none()
        }

        MlMessage::MarkImageExcluded(pane_index) => {
            if let Some(pane) = panes.get(pane_index) {
                if pane.dir_loaded {
                    let path = &pane.img_cache.image_paths[pane.img_cache.current_index];
                    let filename = path.file_name().to_string();
                    selection_manager.toggle_excluded(&filename);
                    info!("Toggled excluded: {}", filename);

                    // Save immediately
                    if let Err(e) = selection_manager.save() {
                        error!("Failed to save selection state: {}", e);
                    }
                }
            }
            Task::none()
        }

        MlMessage::ClearImageMark(pane_index) => {
            if let Some(pane) = panes.get(pane_index) {
                if pane.dir_loaded {
                    let path = &pane.img_cache.image_paths[pane.img_cache.current_index];
                    let filename = path.file_name().to_string();
                    selection_manager.clear_mark(&filename);
                    info!("Cleared mark: {}", filename);

                    // Save immediately
                    if let Err(e) = selection_manager.save() {
                        error!("Failed to save selection state: {}", e);
                    }
                }
            }
            Task::none()
        }

        MlMessage::ExportSelectionJson => {
            // Use file picker to choose export location
            Task::perform(
                async {
                    rfd::AsyncFileDialog::new()
                        .set_file_name("selections.json")
                        .add_filter("JSON", &["json"])
                        .save_file()
                        .await
                },
                |file_handle| {
                    if let Some(file) = file_handle {
                        let path = file.path().to_path_buf();
                        Message::MlAction(MlMessage::ExportSelectionJsonToPath(path))
                    } else {
                        Message::Nothing
                    }
                }
            )
        }

        MlMessage::ExportSelectionJsonToPath(path) => {
            info!("Exporting selection to: {}", path.display());
            if let Err(e) = selection_manager.export_to_file(&path) {
                error!("Failed to export selection: {}", e);
            } else {
                info!("Successfully exported selections to: {}", path.display());
            }
            Task::none()
        }
    }
}

/// Handle ML-related keyboard events
///
/// Returns Some(Task) if the key was handled, None if not an ML key
pub fn handle_keyboard_event(
    key: &keyboard::Key,
    modifiers: keyboard::Modifiers,
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

    // Helper for platform-specific modifier key
    let is_platform_modifier = || {
        #[cfg(target_os = "macos")]
        return modifiers.logo(); // Command key on macOS

        #[cfg(not(target_os = "macos"))]
        return modifiers.control(); // Control key on other platforms
    };

    match key.as_ref() {
        Key::Character("s") | Key::Character("S") => {
            let pane_index = get_pane_index();
            Some(Task::done(Message::MlAction(
                MlMessage::MarkImageSelected(pane_index)
            )))
        }

        Key::Character("x") | Key::Character("X") => {
            let pane_index = get_pane_index();
            Some(Task::done(Message::MlAction(
                MlMessage::MarkImageExcluded(pane_index)
            )))
        }

        Key::Character("u") | Key::Character("U") => {
            let pane_index = get_pane_index();
            Some(Task::done(Message::MlAction(
                MlMessage::ClearImageMark(pane_index)
            )))
        }

        Key::Character("e") | Key::Character("E") => {
            if is_platform_modifier() {
                Some(Task::done(Message::MlAction(
                    MlMessage::ExportSelectionJson
                )))
            } else {
                None
            }
        }

        _ => None
    }
}
