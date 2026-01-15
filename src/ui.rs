#[cfg(target_os = "linux")]
mod other_os {
    //pub use iced;
    pub use iced_custom as iced;
}

#[cfg(not(target_os = "linux"))]
mod macos {
    pub use iced_custom as iced;
}

#[cfg(target_os = "linux")]
use other_os::*;

#[cfg(not(target_os = "linux"))]
use macos::*;

#[allow(unused_imports)]
use log::{Level, debug, info, warn, error};

use iced_widget::{container, Container, row, column, horizontal_space, text, button, center};
#[cfg(feature = "coco")]
use iced_widget::Stack;
use iced_winit::core::{Color, Element, Length, Alignment};
use iced_winit::core::alignment;
use iced_winit::core::alignment::Horizontal;
use iced_winit::core::font::Font;
use iced_winit::core::Theme as WinitTheme;
use iced_wgpu::Renderer;
use iced_wgpu::graphics::text::{font_system, cosmic_text, measure as measure_buffer, to_attributes, to_shaping};
use iced_winit::core::text::Shaping;

use crate::pane::Pane;
use crate::{menu as app_menu};
use app_menu::button_style;
use crate::menu::PaneLayout;
use crate::{app::Message, DataViewer};
use crate::widgets::shader::image_shader::ImageShader;
use crate::widgets::{split::Axis, viewer, dualslider::DualSlider};
use crate::{CURRENT_FPS, CURRENT_MEMORY_USAGE, pane::IMAGE_RENDER_FPS};
use crate::menu::MENU_BAR_HEIGHT;
use iced_widget::tooltip;
use crate::widgets::synced_image_split::SyncedImageSplit;
use crate::widgets::circular::mini_circular;
use crate::settings::SpinnerLocation;
#[cfg(feature = "selection")]
use crate::selection_manager::ImageMark;


fn icon<'a, Message>(codepoint: char) -> Element<'a, Message, WinitTheme, Renderer> {
    const ICON_FONT: Font = Font::with_name("viewskater-fonts");

    text(codepoint)
        .font(ICON_FONT)
        .size(18)
        .into()
}

fn file_copy_icon<'a, Message>() -> Element<'a, Message, WinitTheme, Renderer> {
    icon('\u{E804}')
}

fn folder_copy_icon<'a, Message>() -> Element<'a, Message, WinitTheme, Renderer> {
    icon('\u{E805}')
}


/// Helper struct to pass ML mark badge and COCO badge into footer function
pub struct FooterOptions {
    pub mark_badge: Option<Element<'static, Message, WinitTheme, Renderer>>,
    pub coco_badge: Option<Element<'static, Message, WinitTheme, Renderer>>,
}

impl FooterOptions {
    pub fn new() -> Self {
        Self {
            mark_badge: None,
            coco_badge: None,
        }
    }

    #[cfg(feature = "selection")]
    pub fn with_mark(mut self, mark: crate::selection_manager::ImageMark) -> Self {
        self.mark_badge = Some(crate::widgets::selection_widget::mark_badge(mark));
        self
    }

    #[cfg(feature = "coco")]
    #[allow(dead_code)]
    pub fn with_coco(mut self, has_annotations: bool, num_annotations: usize) -> Self {
        self.coco_badge = Some(crate::coco::widget::coco_badge(has_annotations, num_annotations));
        self
    }

    #[allow(dead_code)]
    pub fn get_mark_badge(self) -> Element<'static, Message, WinitTheme, Renderer> {
        self.mark_badge.unwrap_or_else(|| {
            #[cfg(feature = "selection")]
            {
                crate::widgets::selection_widget::empty_badge()
            }
            #[cfg(not(feature = "selection"))]
            {
                container(text("")).width(0).height(0).into()
            }
        })
    }

    #[allow(dead_code)]
    pub fn get_coco_badge(self) -> Element<'static, Message, WinitTheme, Renderer> {
        self.coco_badge.unwrap_or_else(|| {
            #[cfg(feature = "coco")]
            {
                crate::coco::widget::empty_badge()
            }
            #[cfg(not(feature = "coco"))]
            {
                container(text("")).width(0).height(0).into()
            }
        })
    }
}

/// Responsive footer layout state
struct ResponsiveFooterState {
    metadata: Option<String>,
    show_spinner: bool,
    show_copy_buttons: bool,
    footer_text: String,
}

/// Measures the width of text using the actual font system
/// Uses Font::MONOSPACE at size 14 to match footer text rendering
fn measure_text_width(text: &str) -> f32 {
    const FONT_SIZE: f32 = 14.0;
    const LINE_HEIGHT: f32 = 1.3; // Default line height multiplier

    let mut font_system_guard = font_system()
        .write()
        .expect("Failed to acquire font system lock");

    let mut buffer = cosmic_text::Buffer::new(
        font_system_guard.raw(),
        cosmic_text::Metrics::new(FONT_SIZE, FONT_SIZE * LINE_HEIGHT),
    );

    // Set a large width so text doesn't wrap
    buffer.set_size(font_system_guard.raw(), Some(10000.0), Some(100.0));

    buffer.set_text(
        font_system_guard.raw(),
        text,
        to_attributes(Font::MONOSPACE),
        to_shaping(Shaping::Basic),
    );

    measure_buffer(&buffer).width
}

/// Determines responsive footer layout based on available width
/// Phases:
/// 1. Full metadata (resolution + file size) + spinner + buttons + index/total
/// 2. Resolution with "pixels" + spinner + buttons + index/total
/// 3. Dimensions only + spinner + buttons + index/total
/// 4. No metadata + spinner + buttons + index/total
/// 5. No metadata + no spinner + buttons + index/total
/// 6. No metadata + no spinner + no buttons + index/total
/// 7. No metadata + no spinner + no buttons + index only
/// 8. Nothing (empty footer)
fn get_responsive_footer_state(
    available_width: f32,
    metadata_text: &Option<String>,
    footer_text: &str,
    show_spinner: bool,
    show_copy_buttons: bool,
) -> ResponsiveFooterState {
    // Fixed widths for non-text elements
    const BUTTON_WIDTH: f32 = 26.0;  // Each copy button: 18px icon + padding
    const BUTTON_SPACING: f32 = 3.0; // Spacing between buttons
    const SPINNER_WIDTH: f32 = 18.0; // Mini spinner size
    const FOOTER_PADDING: f32 = 6.0; // Footer container padding (3px each side)
    const ELEMENT_SPACING: f32 = 3.0; // Spacing between row elements
    const MIN_MARGIN: f32 = 5.0;     // Minimum margin before hiding

    // Parse footer_text to get index and total (format: "index/total")
    let (index_str, _total_str) = footer_text.split_once('/').unwrap_or((footer_text, ""));
    let index_only = index_str.to_string();

    // Measure actual text widths dynamically
    let full_footer_width = measure_text_width(footer_text);
    let index_only_width = measure_text_width(&index_only);
    let buttons_width = if show_copy_buttons {
        BUTTON_WIDTH * 2.0 + BUTTON_SPACING
    } else {
        0.0
    };
    let spinner_width = if show_spinner {
        SPINNER_WIDTH + ELEMENT_SPACING
    } else {
        0.0
    };

    // Phase 8: Nothing fits - hide everything
    if available_width < index_only_width + FOOTER_PADDING + MIN_MARGIN {
        return ResponsiveFooterState {
            metadata: None,
            show_spinner: false,
            show_copy_buttons: false,
            footer_text: String::new(),
        };
    }

    // Phase 7: Only index (no total)
    if available_width < full_footer_width + FOOTER_PADDING + MIN_MARGIN {
        return ResponsiveFooterState {
            metadata: None,
            show_spinner: false,
            show_copy_buttons: false,
            footer_text: index_only,
        };
    }

    // Phase 6: Index/total but no buttons or spinner
    if available_width < full_footer_width + buttons_width + FOOTER_PADDING + ELEMENT_SPACING + MIN_MARGIN {
        return ResponsiveFooterState {
            metadata: None,
            show_spinner: false,
            show_copy_buttons: false,
            footer_text: footer_text.to_string(),
        };
    }

    // Phase 5: Buttons + index/total but no spinner or metadata
    if available_width < full_footer_width + buttons_width + spinner_width + FOOTER_PADDING + ELEMENT_SPACING + MIN_MARGIN {
        return ResponsiveFooterState {
            metadata: None,
            show_spinner: false,
            show_copy_buttons,
            footer_text: footer_text.to_string(),
        };
    }

    // Phase 4: Spinner + buttons + index/total but no metadata
    let right_side_width = spinner_width + buttons_width + full_footer_width + ELEMENT_SPACING;

    let Some(meta) = metadata_text else {
        return ResponsiveFooterState {
            metadata: None,
            show_spinner,
            show_copy_buttons,
            footer_text: footer_text.to_string(),
        };
    };

    // We need space for: left_content + horizontal_space + right_content
    // horizontal_space is flexible, but we need at least some gap
    let available_for_meta = available_width - right_side_width - FOOTER_PADDING - ELEMENT_SPACING;

    // Extract resolution parts for progressive display
    if let Some(pixels_pos) = meta.find(" pixels") {
        let resolution_with_pixels = &meta[..pixels_pos + 7]; // "1920 x 1080 pixels"
        let resolution_only = &meta[..pixels_pos]; // "1920 x 1080"

        let resolution_only_width = measure_text_width(resolution_only);
        let resolution_with_pixels_width = measure_text_width(resolution_with_pixels);
        let full_meta_width = measure_text_width(meta);

        // Not enough for even dimensions - no metadata
        if available_for_meta < resolution_only_width + MIN_MARGIN {
            return ResponsiveFooterState {
                metadata: None,
                show_spinner,
                show_copy_buttons,
                footer_text: footer_text.to_string(),
            };
        }

        // Phase 3: Dimensions only (e.g., "1920 x 1080")
        if available_for_meta < resolution_with_pixels_width + MIN_MARGIN {
            return ResponsiveFooterState {
                metadata: Some(resolution_only.to_string()),
                show_spinner,
                show_copy_buttons,
                footer_text: footer_text.to_string(),
            };
        }

        // Phase 2: Resolution with "pixels" (e.g., "1920 x 1080 pixels")
        if available_for_meta < full_meta_width + MIN_MARGIN {
            return ResponsiveFooterState {
                metadata: Some(resolution_with_pixels.to_string()),
                show_spinner,
                show_copy_buttons,
                footer_text: footer_text.to_string(),
            };
        }
    }

    // Phase 1: Full metadata fits
    ResponsiveFooterState {
        metadata: Some(meta.clone()),
        show_spinner,
        show_copy_buttons,
        footer_text: footer_text.to_string(),
    }
}

pub fn get_footer(
    footer_text: String,
    metadata_text: Option<String>,
    pane_index: usize,
    show_copy_buttons: bool,
    show_spinner: bool,
    spinner_location: SpinnerLocation,
    options: FooterOptions,
    available_width: f32,
) -> Container<'static, Message, WinitTheme, Renderer> {
    // Only show spinner in footer if spinner_location is Footer
    let show_spinner = show_spinner && spinner_location == SpinnerLocation::Footer;
    // Get responsive footer state based on available width
    let state = get_responsive_footer_state(
        available_width,
        &metadata_text,
        &footer_text,
        show_spinner,
        show_copy_buttons,
    );

    // Phase 6: Empty footer
    if state.footer_text.is_empty() {
        return container::<Message, WinitTheme, Renderer>(text(""))
            .width(Length::Fill)
            .height(32)
            .padding(3);
    }

    // Extract badges from options
    let mark_badge = options.mark_badge.unwrap_or_else(|| {
        #[cfg(feature = "selection")]
        {
            crate::widgets::selection_widget::empty_badge()
        }
        #[cfg(not(feature = "selection"))]
        {
            container(text("")).width(0).height(0).into()
        }
    });
    let coco_badge = options.coco_badge.unwrap_or_else(|| {
        #[cfg(feature = "coco")]
        {
            crate::coco::widget::empty_badge()
        }
        #[cfg(not(feature = "coco"))]
        {
            container(text("")).width(0).height(0).into()
        }
    });

    // Left side: metadata (resolution and file size) - EoG style
    let left_content: Element<'_, Message, WinitTheme, Renderer> = if let Some(meta) = state.metadata {
        text(meta)
            .font(Font::MONOSPACE)
            .style(|_theme| iced::widget::text::Style {
                color: Some(Color::from([0.8, 0.8, 0.8]))
            })
            .size(14)
            .into()
    } else {
        text("")
            .size(14)
            .into()
    };

    // Optional loading spinner (shown during background loading, hidden when footer is narrow)
    let spinner_element: Element<'_, Message, WinitTheme, Renderer> = if state.show_spinner {
        mini_circular()
    } else {
        // Empty placeholder to maintain spacing
        container(text("")).width(0).height(0).into()
    };

    // Right side: spinner, copy buttons, badges, and index
    let right_content: Element<'_, Message, WinitTheme, Renderer> = if state.show_copy_buttons {
        let copy_filename_button = tooltip(
            button(file_copy_icon())
                .padding(iced::padding::all(2))
                .style(|_theme: &WinitTheme, _status: button::Status| button_style(_theme, _status, "labeled"))
                .on_press(Message::CopyFilename(pane_index)),
            container(text("Copy filename").size(14))
                .padding(5)
                .style(|theme: &WinitTheme| container::Style {
                    text_color: Some(Color::from([1.0, 1.0, 1.0])),
                    background: Some(theme.extended_palette().background.strong.color.into()),
                    border: iced::Border {
                        radius: 4.0.into(),
                        width: 0.0,
                        color: Color::TRANSPARENT,
                    },
                    ..container::Style::default()
                }),
            tooltip::Position::Top,
        );

        let copy_filepath_button = tooltip(
            button(folder_copy_icon())
                .padding(iced::padding::all(2))
                .style(|_theme: &WinitTheme, _status: button::Status| button_style(_theme, _status, "labeled"))
                .on_press(Message::CopyFilePath(pane_index)),
            container(text("Copy file path").size(14))
                .padding(5)
                .style(|theme: &WinitTheme| container::Style {
                    text_color: Some(Color::from([1.0, 1.0, 1.0])),
                    background: Some(theme.extended_palette().background.strong.color.into()),
                    border: iced::Border {
                        radius: 4.0.into(),
                        width: 0.0,
                        color: Color::TRANSPARENT,
                    },
                    ..container::Style::default()
                }),
            tooltip::Position::Top,
        );

        row![
            spinner_element,
            copy_filepath_button,
            copy_filename_button,
            mark_badge,
            coco_badge,
            text(state.footer_text)
                .font(Font::MONOSPACE)
                .style(|_theme| iced::widget::text::Style {
                    color: Some(Color::from([0.8, 0.8, 0.8]))
                })
                .size(14)
        ]
        .align_y(Alignment::Center)
        .spacing(3)
        .into()
    } else {
        row![
            spinner_element,
            mark_badge,
            coco_badge,
            text(state.footer_text)
                .font(Font::MONOSPACE)
                .style(|_theme| iced::widget::text::Style {
                    color: Some(Color::from([0.8, 0.8, 0.8]))
                })
                .size(14)
        ]
        .align_y(Alignment::Center)
        .spacing(3)
        .into()
    };

    // Combine left (metadata) and right (index + buttons) with space between
    container::<Message, WinitTheme, Renderer>(
        row![
            left_content,
            horizontal_space(),
            right_content
        ]
        .align_y(Alignment::Center)
    )
    .width(Length::Fill)
    .height(32)
    .padding(3)
}


pub fn build_ui(app: &DataViewer) -> Container<'_, Message, WinitTheme, Renderer> {
    // Helper to get the current image mark for a pane (ML tools only)
    #[cfg(feature = "selection")]
    let get_mark_for_pane = |pane_index: usize| -> ImageMark {
        if let Some(pane) = app.panes.get(pane_index) {
            if pane.dir_loaded && !pane.img_cache.image_paths.is_empty() {
                let path = &pane.img_cache.image_paths[pane.img_cache.current_index];
                let filename = path.file_name().to_string();
                return app.selection_manager.get_mark(&filename);
            }
        }
        ImageMark::Unmarked
    };

    let mb = app_menu::build_menu(app);

    let is_fullscreen = app.is_fullscreen;
    let cursor_on_top = app.cursor_on_top;
    let cursor_on_menu = app.cursor_on_menu;
    let cursor_on_footer = app.cursor_on_footer;
    let show_option = app.settings.is_visible();

    // Check if spinner should be shown in menu bar
    let show_menu_bar_spinner = app.spinner_location == SpinnerLocation::MenuBar
        && app.panes.iter().any(|p| p.loading_started_at
            .map_or(false, |start| start.elapsed() > std::time::Duration::from_secs(1)));

    let menu_bar_spinner: Element<'_, Message, WinitTheme, Renderer> = if show_menu_bar_spinner {
        container(mini_circular()).padding([0, 5]).into()
    } else {
        container(text("")).width(0).height(0).into()
    };

    let top_bar = container(
        row![
            mb,
            horizontal_space(),
            menu_bar_spinner,
            if !is_fullscreen {
                get_fps_container(app)
            } else {
                container(text("")).width(0).height(0)
            }
        ]
            .align_y(alignment::Vertical::Center)
    )
    .align_y(alignment::Vertical::Center)
    .width(Length::Fill);

    // Menu bar spinner for fullscreen mode
    let fullscreen_menu_bar_spinner: Element<'_, Message, WinitTheme, Renderer> = if show_menu_bar_spinner {
        container(mini_circular()).padding([0, 5]).into()
    } else {
        container(text("")).width(0).height(0).into()
    };

    let fps_bar = if is_fullscreen {
        container (
            row![fullscreen_menu_bar_spinner, get_fps_container(app)]
        ).align_x(alignment::Horizontal::Right)
        .width(Length::Fill)
    } else {
        container(text("")).width(0).height(0)
    };

    match app.pane_layout {
        PaneLayout::SinglePane => {
            // Choose the appropriate widget based on slider movement state
            let first_img = if app.panes[0].dir_loaded {
                // First, create the base image widget (either slider or shader)
                let base_image_widget = if app.use_slider_image_for_render && app.panes[0].slider_image.is_some() {
                    // Use regular Image widget during slider movement (much faster)
                    let image_handle = app.panes[0].slider_image.clone().unwrap();

                    center({
                        #[cfg(feature = "coco")]
                        let mut viewer = viewer::Viewer::new(image_handle)
                            .width(Length::Fill)
                            .height(Length::Fill)
                            .content_fit(iced_winit::core::ContentFit::Contain);

                        #[cfg(not(feature = "coco"))]
                        let viewer = viewer::Viewer::new(image_handle)
                            .width(Length::Fill)
                            .height(Length::Fill)
                            .content_fit(iced_winit::core::ContentFit::Contain);

                        #[cfg(feature = "coco")]
                        {
                            viewer = viewer
                                .with_zoom_state(app.panes[0].zoom_scale, app.panes[0].zoom_offset)
                                .pane_index(0)
                                .on_zoom_change(|pane_idx, scale, offset| {
                                    Message::CocoAction(crate::coco::widget::CocoMessage::ZoomChanged(
                                        pane_idx, scale, offset
                                    ))
                                });
                        }

                        viewer
                    })
                } else if let Some(scene) = app.panes[0].scene.as_ref() {
                    // Fixed: Pass Arc<Scene> reference correctly
                    #[cfg(feature = "coco")]
                    let mut shader = ImageShader::new(Some(scene))
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .content_fit(iced_winit::core::ContentFit::Contain)
                        .horizontal_split(false)
                        .with_interaction_state(app.panes[0].mouse_wheel_zoom, app.panes[0].ctrl_pressed)
                        .double_click_threshold_ms(app.double_click_threshold_ms)
                        .use_nearest_filter(app.nearest_neighbor_filter);

                    #[cfg(not(feature = "coco"))]
                    let shader = ImageShader::new(Some(scene))
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .content_fit(iced_winit::core::ContentFit::Contain)
                        .horizontal_split(false)
                        .with_interaction_state(app.panes[0].mouse_wheel_zoom, app.panes[0].ctrl_pressed)
                        .double_click_threshold_ms(app.double_click_threshold_ms)
                        .use_nearest_filter(app.nearest_neighbor_filter);

                    #[cfg(feature = "coco")]
                    {
                        shader = shader.with_zoom_state(app.panes[0].zoom_scale, app.panes[0].zoom_offset);
                    }

                    // Set up zoom change callback for COCO bbox rendering
                    #[cfg(feature = "coco")]
                    {
                        shader = shader
                            .pane_index(0)
                            .image_index(app.panes[0].img_cache.current_index)
                            .on_zoom_change(|pane_idx, scale, offset| {
                                Message::CocoAction(crate::coco::widget::CocoMessage::ZoomChanged(
                                    pane_idx, scale, offset
                                ))
                            });
                    }

                    center(shader)
                } else {
                    return container(text("No image loaded"));
                };

                // Then, optionally add annotations overlay on top
                #[cfg(feature = "coco")]
                let with_annotations = {
                    if (app.panes[0].show_bboxes || app.panes[0].show_masks) && app.annotation_manager.has_annotations() {
                        // Determine which index to use for annotation lookup based on rendering mode
                        let annotation_index = if app.use_slider_image_for_render && app.panes[0].slider_image.is_some() {
                            // Slider mode: use slider_image_position
                            app.panes[0].slider_image_position
                                .or(app.panes[0].current_image_index)
                                .unwrap_or(app.panes[0].img_cache.current_index)
                        } else {
                            // Normal mode: use current_image_index
                            app.panes[0].current_image_index
                                .unwrap_or(app.panes[0].img_cache.current_index)
                        };

                        if let Some(path_source) = app.panes[0].img_cache.image_paths.get(annotation_index) {
                            let filename = path_source.file_name();

                            // Look up annotations for this image
                            if let Some(annotations) = app.annotation_manager.get_annotations(&filename) {
                                // Get image dimensions based on rendering mode
                                let image_size = if app.use_slider_image_for_render && app.panes[0].slider_image.is_some() {
                                    // Slider mode: use slider_image_dimensions
                                    app.panes[0].slider_image_dimensions
                                        .unwrap_or((app.panes[0].current_image.width(), app.panes[0].current_image.height()))
                                } else {
                                    // Normal mode: use current_image dimensions
                                    (app.panes[0].current_image.width(), app.panes[0].current_image.height())
                                };
                                // log::debug!("UI: Using dimensions for annotation_index={}: {:?} (slider_mode={})",
                                //     annotation_index, image_size, app.use_slider_image_for_render);

                                // Check if this image has invalid annotations
                                let has_invalid = app.annotation_manager.has_invalid_annotations(&filename);

                                // Create bbox/mask overlay
                                log::debug!("UI: Creating annotation overlay with zoom_scale={:.2}, zoom_offset=({:.1}, {:.1})",
                                    app.panes[0].zoom_scale, app.panes[0].zoom_offset.x, app.panes[0].zoom_offset.y);
                                let bbox_overlay = crate::coco::overlay::render_bbox_overlay(
                                    annotations,
                                    image_size,
                                    app.panes[0].zoom_scale,
                                    app.panes[0].zoom_offset,
                                    app.panes[0].show_bboxes,
                                    app.panes[0].show_masks,
                                    has_invalid,
                                    app.coco_mask_render_mode,
                                    app.coco_disable_simplification,
                                );

                                // Stack image and annotations
                                container(
                                    Stack::new()
                                        .push(base_image_widget)
                                        .push(bbox_overlay)
                                )
                                .width(Length::Fill)
                                .height(Length::Fill)
                                .padding(0)
                            } else {
                                // No annotations for this image
                                container(base_image_widget)
                                    .width(Length::Fill)
                                    .height(Length::Fill)
                                    .padding(0)
                            }
                        } else {
                            container(base_image_widget)
                                .width(Length::Fill)
                                .height(Length::Fill)
                                .padding(0)
                        }
                    } else {
                        // Annotations disabled or no annotations loaded
                        container(base_image_widget)
                            .width(Length::Fill)
                            .height(Length::Fill)
                            .padding(0)
                    }
                };

                #[cfg(not(feature = "coco"))]
                let with_annotations = container(base_image_widget)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .padding(0);

                with_annotations.into()
            } else {
                // Use build_ui_container even when dir not loaded to show loading spinner
                app.panes[0].build_ui_container(
                    app.use_slider_image_for_render,
                    app.is_horizontal_split,
                    app.double_click_threshold_ms,
                    app.nearest_neighbor_filter
                )
            };

            let footer = if app.show_footer && app.panes[0].dir_loaded {
                // Use slider position during slider movement, otherwise use current_image_index
                let display_index = if app.use_slider_image_for_render && app.panes[0].slider_image_position.is_some() {
                    app.panes[0].slider_image_position.unwrap()
                } else {
                    app.panes[0].current_image_index.unwrap_or(app.panes[0].img_cache.current_index)
                };
                let footer_text = format!("{}/{}", display_index + 1, app.panes[0].img_cache.num_files);

                // Generate metadata text for footer (EoG style: "1920x1080 pixels  2.5 MB")
                let metadata_text = if app.show_metadata {
                    app.panes[0].current_image_metadata.as_ref().map(|m|
                        format!("{} pixels  {}", m.resolution_string(), m.file_size_string(app.use_binary_size))
                    )
                } else {
                    None
                };

                // Show spinner after 1 second of loading
                let show_spinner = app.panes[0].loading_started_at
                    .map_or(false, |start| start.elapsed() > std::time::Duration::from_secs(1));

                let options = {
                    #[cfg(feature = "selection")]
                    {
                        FooterOptions::new().with_mark(get_mark_for_pane(0))
                    }
                    #[cfg(not(feature = "selection"))]
                    {
                        FooterOptions::new()
                    }
                };
                get_footer(footer_text, metadata_text, 0, app.show_copy_buttons, show_spinner, app.spinner_location, options, app.window_width)
            } else {
                container(text("")).height(0)
            };

            let slider = if app.panes[0].dir_loaded && app.panes[0].img_cache.num_files > 1 {
                container(DualSlider::new(
                    0..=(app.panes[0].img_cache.num_files - 1) as u16,
                    app.slider_value,
                    -1,
                    Message::SliderChanged,
                    Message::SliderReleased,
                )
                .width(Length::Fill))
            } else {
                container(text("")).height(0)
            };

            let slider_controls = slider
                .width(Length::Fill)
                .height(Length::Shrink)
                .align_x(Horizontal::Center);

            // Create the column WITHOUT converting to Element first
            center(
                container(
                    if is_fullscreen && !show_option &&(cursor_on_top || cursor_on_menu) {
                        column![top_bar, fps_bar, first_img]
                    } else if is_fullscreen && cursor_on_footer {
                        column![fps_bar, first_img, slider_controls, footer]
                    } else if is_fullscreen {
                        column![fps_bar, first_img]
                    } else {column![
                        top_bar,
                        first_img,
                        slider_controls,
                        footer
                    ]}
                )
                .style(|theme| container::Style {
                    background: Some(theme.extended_palette().background.base.color.into()),
                    ..container::Style::default()
                })
                .width(Length::Fill)
                .height(Length::Fill)
            ).align_x(Horizontal::Center)
        },
        PaneLayout::DualPane => {
            if app.is_slider_dual {
                // Prepare footer options for both panes
                let footer_options = [
                    {
                        #[cfg(feature = "selection")]
                        {
                            FooterOptions::new().with_mark(get_mark_for_pane(0))
                        }
                        #[cfg(not(feature = "selection"))]
                        {
                            FooterOptions::new()
                        }
                    },
                    {
                        #[cfg(feature = "selection")]
                        {
                            FooterOptions::new().with_mark(get_mark_for_pane(1))
                        }
                        #[cfg(not(feature = "selection"))]
                        {
                            FooterOptions::new()
                        }
                    },
                ];

                debug!("build_ui (dual_pane_slider2): app.nearest_neighbor_filter = {}", app.nearest_neighbor_filter);
                let panes = build_ui_dual_pane_slider2(
                    &app.panes,
                    app.divider_position,
                    app.show_footer,
                    app.use_slider_image_for_render,
                    app.is_horizontal_split,
                    app.synced_zoom,
                    app.show_copy_buttons,
                    app.show_metadata,
                    app.double_click_threshold_ms,
                    footer_options,
                    app.nearest_neighbor_filter,
                    app.use_binary_size,
                    app.spinner_location,
                    app.window_width,
                );

                container(
                    column![
                        top_bar,
                        panes
                    ]
                )
                .style(|theme| container::Style {
                    background: Some(theme.extended_palette().background.base.color.into()),
                    ..container::Style::default()
                })
                .width(Length::Fill)
                .height(Length::Fill)
            } else {
                // Pass synced_zoom parameter
                debug!("build_ui (dual_pane_slider1): app.nearest_neighbor_filter = {}", app.nearest_neighbor_filter);
                let panes = build_ui_dual_pane_slider1(
                    &app.panes,
                    app.divider_position,
                    app.use_slider_image_for_render,
                    app.is_horizontal_split,
                    app.synced_zoom,
                    app.double_click_threshold_ms,
                    app.nearest_neighbor_filter,
                );

                // Use slider position during slider movement, otherwise use current_image_index
                let display_index_0 = if app.use_slider_image_for_render && app.panes[0].slider_image_position.is_some() {
                    app.panes[0].slider_image_position.unwrap()
                } else {
                    app.panes[0].current_image_index.unwrap_or(app.panes[0].img_cache.current_index)
                };
                let display_index_1 = if app.use_slider_image_for_render && app.panes[1].slider_image_position.is_some() {
                    app.panes[1].slider_image_position.unwrap()
                } else {
                    app.panes[1].current_image_index.unwrap_or(app.panes[1].img_cache.current_index)
                };
                let footer_texts = [
                    format!("{}/{}", display_index_0 + 1, app.panes[0].img_cache.num_files),
                    format!("{}/{}", display_index_1 + 1, app.panes[1].img_cache.num_files)
                ];

                // Generate metadata text for each pane (EoG style)
                let metadata_texts = if app.show_metadata {
                    [
                        app.panes[0].current_image_metadata.as_ref().map(|m|
                            format!("{} pixels  {}", m.resolution_string(), m.file_size_string(app.use_binary_size))
                        ),
                        app.panes[1].current_image_metadata.as_ref().map(|m|
                            format!("{} pixels  {}", m.resolution_string(), m.file_size_string(app.use_binary_size))
                        ),
                    ]
                } else {
                    [None, None]
                };

                let footer = if app.show_footer && (app.panes[0].dir_loaded || app.panes[1].dir_loaded) {
                    // Show spinner after 1 second of loading
                    let show_spinner_0 = app.panes[0].loading_started_at
                        .map_or(false, |start| start.elapsed() > std::time::Duration::from_secs(1));
                    let show_spinner_1 = app.panes[1].loading_started_at
                        .map_or(false, |start| start.elapsed() > std::time::Duration::from_secs(1));

                    let options0 = {
                        #[cfg(feature = "selection")]
                        {
                            FooterOptions::new().with_mark(get_mark_for_pane(0))
                        }
                        #[cfg(not(feature = "selection"))]
                        {
                            FooterOptions::new()
                        }
                    };
                    let options1 = {
                        #[cfg(feature = "selection")]
                        {
                            FooterOptions::new().with_mark(get_mark_for_pane(1))
                        }
                        #[cfg(not(feature = "selection"))]
                        {
                            FooterOptions::new()
                        }
                    };
                    // Each pane gets half the window width in dual mode
                    let pane_width = app.window_width / 2.0;
                    row![
                        get_footer(footer_texts[0].clone(), metadata_texts[0].clone(), 0, app.show_copy_buttons, show_spinner_0, app.spinner_location, options0, pane_width),
                        get_footer(footer_texts[1].clone(), metadata_texts[1].clone(), 1, app.show_copy_buttons, show_spinner_1, app.spinner_location, options1, pane_width)
                    ]
                } else {
                    row![]
                };

                let max_num_files = app.panes.iter().map(|p| p.img_cache.num_files).max().unwrap_or(0);

                let slider = if app.panes.iter().any(|p| p.dir_loaded) && max_num_files > 1 {
                    container(
                        DualSlider::new(
                            0..=(max_num_files - 1) as u16,
                            app.slider_value,
                            -1,
                            Message::SliderChanged,
                            Message::SliderReleased,
                        ).width(Length::Fill)
                    )
                    .width(Length::Fill)
                    .height(Length::Shrink)
                } else {
                    container(text("")).height(0)
                };

                container(
                    if is_fullscreen && !show_option &&(cursor_on_top || cursor_on_menu) {
                        column![top_bar, fps_bar, panes]
                    } else if is_fullscreen && cursor_on_footer {
                        column![fps_bar, panes, slider, footer]
                    } else if is_fullscreen  {
                        column![fps_bar, panes]
                    } else {
                        column![
                            top_bar,
                            panes,
                            slider,
                            footer
                        ]
                    }
                ).style(|theme| container::Style {
                    background: Some(theme.extended_palette().background.base.color.into()),
                    ..container::Style::default()
                })
                .width(Length::Fill)
                .height(Length::Fill)
            }
        }
    }
}



pub fn build_ui_dual_pane_slider1(
    panes: &[Pane],
    divider_position: Option<u16>,
    use_slider_image_for_render: bool,
    is_horizontal_split: bool,
    synced_zoom: bool,
    double_click_threshold_ms: u16,
    use_nearest_filter: bool,
) -> Element<'_, Message, WinitTheme, Renderer> {
    let first_img = panes[0].build_ui_container(use_slider_image_for_render, is_horizontal_split, double_click_threshold_ms, use_nearest_filter);
    let second_img = panes[1].build_ui_container(use_slider_image_for_render, is_horizontal_split, double_click_threshold_ms, use_nearest_filter);

    let is_selected: Vec<bool> = panes.iter().map(|pane| pane.is_selected).collect();

    SyncedImageSplit::new(
        false,
        first_img,
        second_img,
        is_selected,
        divider_position,
        if is_horizontal_split { Axis::Horizontal } else { Axis::Vertical },
        Message::OnSplitResize,
        Message::ResetSplit,
        Message::FileDropped,
        Message::PaneSelected,
        MENU_BAR_HEIGHT,
        true,
    )
    .synced_zoom(synced_zoom)
    .min_scale(0.25)
    .max_scale(10.0)
    .scale_step(0.10)
    .double_click_threshold_ms(double_click_threshold_ms)
    .into()
}


pub fn build_ui_dual_pane_slider2<'a>(
    panes: &'a [Pane],
    divider_position: Option<u16>,
    show_footer: bool,
    use_slider_image_for_render: bool,
    is_horizontal_split: bool,
    _synced_zoom: bool,
    show_copy_buttons: bool,
    show_metadata: bool,
    double_click_threshold_ms: u16,
    footer_options: [FooterOptions; 2],
    use_nearest_filter: bool,
    use_binary_size: bool,
    spinner_location: SpinnerLocation,
    window_width: f32,
) -> Element<'a, Message, WinitTheme, Renderer> {
    // Each pane gets roughly half the window width
    let pane_width = window_width / 2.0;
    let footer_texts = [
        format!(
            "{}/{}",
            panes[0].img_cache.current_index + 1,
            panes[0].img_cache.num_files
        ),
        format!(
            "{}/{}",
            panes[1].img_cache.current_index + 1,
            panes[1].img_cache.num_files
        )
    ];

    // Generate metadata text for each pane (EoG style)
    let metadata_texts = if show_metadata {
        [
            panes[0].current_image_metadata.as_ref().map(|m|
                format!("{} pixels  {}", m.resolution_string(), m.file_size_string(use_binary_size))
            ),
            panes[1].current_image_metadata.as_ref().map(|m|
                format!("{} pixels  {}", m.resolution_string(), m.file_size_string(use_binary_size))
            ),
        ]
    } else {
        [None, None]
    };

    // Destructure footer_options array
    let [footer_opt0, footer_opt1] = footer_options;

    // Show spinner after 1 second of loading
    let show_spinner_0 = panes[0].loading_started_at
        .map_or(false, |start| start.elapsed() > std::time::Duration::from_secs(1));
    let show_spinner_1 = panes[1].loading_started_at
        .map_or(false, |start| start.elapsed() > std::time::Duration::from_secs(1));

    let first_img = if panes[0].dir_loaded {
        container(
            if show_footer {
                column![
                    panes[0].build_ui_container(use_slider_image_for_render, is_horizontal_split, double_click_threshold_ms, use_nearest_filter),
                    DualSlider::new(
                        0..=(panes[0].img_cache.num_files - 1) as u16,
                        panes[0].slider_value,
                        0,
                        Message::SliderChanged,
                        Message::SliderReleased
                    )
                    .width(Length::Fill),
                    get_footer(footer_texts[0].clone(), metadata_texts[0].clone(), 0, show_copy_buttons, show_spinner_0, spinner_location, footer_opt0, pane_width)
                ]
            } else {
                column![
                    panes[0].build_ui_container(use_slider_image_for_render, is_horizontal_split, double_click_threshold_ms, use_nearest_filter),
                    DualSlider::new(
                        0..=(panes[0].img_cache.num_files - 1) as u16,
                        panes[0].slider_value,
                        0,
                        Message::SliderChanged,
                        Message::SliderReleased
                    )
                    .width(Length::Fill),
                ]
            }
        )
    } else {
        // Use build_ui_container even when dir not loaded to show loading spinner
        container(column![
            panes[0].build_ui_container(use_slider_image_for_render, is_horizontal_split, double_click_threshold_ms, use_nearest_filter),
        ])
    };

    let second_img = if panes[1].dir_loaded {
        container(
            if show_footer {
                column![
                    panes[1].build_ui_container(use_slider_image_for_render, is_horizontal_split, double_click_threshold_ms, use_nearest_filter),
                    DualSlider::new(
                        0..=(panes[1].img_cache.num_files - 1) as u16,
                        panes[1].slider_value,
                        1,
                        Message::SliderChanged,
                        Message::SliderReleased
                    )
                    .width(Length::Fill),
                    get_footer(footer_texts[1].clone(), metadata_texts[1].clone(), 1, show_copy_buttons, show_spinner_1, spinner_location, footer_opt1, pane_width)
                ]
            } else {
                column![
                    panes[1].build_ui_container(use_slider_image_for_render, is_horizontal_split, double_click_threshold_ms, use_nearest_filter),
                    DualSlider::new(
                        0..=(panes[1].img_cache.num_files - 1) as u16,
                        panes[1].slider_value,
                        1,
                        Message::SliderChanged,
                        Message::SliderReleased
                    )
                    .width(Length::Fill),
                ]
            }
        )
    } else {
        // Use build_ui_container even when dir not loaded to show loading spinner
        container(column![
            panes[1].build_ui_container(use_slider_image_for_render, is_horizontal_split, double_click_threshold_ms, use_nearest_filter),
        ])
    };

    let is_selected: Vec<bool> = panes.iter().map(|pane| pane.is_selected).collect();

    SyncedImageSplit::new(
        true,
        first_img,
        second_img,
        is_selected,
        divider_position,
        if is_horizontal_split { Axis::Horizontal } else { Axis::Vertical },
        Message::OnSplitResize,
        Message::ResetSplit,
        Message::FileDropped,
        Message::PaneSelected,
        MENU_BAR_HEIGHT,
        true,
    )
    .synced_zoom(false)
    .min_scale(0.25)
    .max_scale(10.0)
    .scale_step(0.10)
    .double_click_threshold_ms(double_click_threshold_ms)
    .into()
}

fn get_fps_container(app: &DataViewer) -> Container<'_, Message, WinitTheme, Renderer> {
    // Get UI event loop FPS
    let ui_fps = {
        if let Ok(fps) = CURRENT_FPS.lock() {
            *fps
        } else {
            0.0
        }
    };

    // Get image render FPS (image content refresh rate)
    // During slider movement use iced_wgpu::get_image_fps()
    // Otherwise use IMAGE_RENDER_FPS
    let image_fps = if app.use_slider_image_for_render {
        iced_wgpu::get_image_fps()
    } else {
        IMAGE_RENDER_FPS.lock().map(|fps| *fps as f64).unwrap_or(0.0)
    };

    // Get memory usage in MB
    let memory_mb = {
        if let Ok(mem) = CURRENT_MEMORY_USAGE.lock() {
            if *mem == u64::MAX {
                // Special value indicating memory info is unavailable
                -1.0 // Use negative value as a marker
            } else {
                *mem as f64 / 1024.0 / 1024.0
            }
        } else {
            0.0
        }
    };

    if app.show_fps {
        let memory_text = if memory_mb < 0.0 {
            "Mem: N/A".to_string()
        } else {
            format!("Mem: {:.1} MB", memory_mb)
        };

        container(
            text(format!("UI: {:.1} FPS | Image: {:.1} FPS | {}",
                         ui_fps, image_fps, memory_text))
                .size(14)
                .style(|_theme| iced::widget::text::Style {
                    color: Some(Color::from([1.0, 1.0, 1.0]))
                })
        )
        .padding(5)
    } else {
        container(text("")).width(0).height(0)
    }
}