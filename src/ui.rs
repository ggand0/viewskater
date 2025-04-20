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
use iced_winit::core::{Color, Element, Length, Alignment};
use iced_winit::core::alignment;
use iced_winit::core::alignment::Horizontal;
use iced_winit::core::font::Font;
use iced_winit::core::Theme as WinitTheme;
use iced_wgpu::Renderer;

use crate::pane::Pane;
use crate::menu as app_menu;
use app_menu::button_style;
use crate::menu::PaneLayout;
use crate::{app::Message, DataViewer};
use crate::widgets::shader::image_shader::ImageShader;
use crate::widgets::{split::{Axis, Split}, viewer, dualslider::DualSlider};
use crate::{CURRENT_FPS, CURRENT_MEMORY_USAGE, pane::IMAGE_RENDER_FPS};
use crate::menu::MENU_BAR_HEIGHT;


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

pub fn get_footer(footer_text: String, pane_index: usize) -> Container<'static, Message, WinitTheme, Renderer> {
    let copy_filename_button = button(file_copy_icon())
        .padding( iced::padding::all(2) )
        .style(|_theme: &WinitTheme, _status: button::Status| button_style(_theme, _status, "labeled"))
        .on_press(Message::CopyFilename(pane_index));

    let copy_filepath_button = button(folder_copy_icon())
        .padding( iced::padding::all(2) )
        .style(|_theme: &WinitTheme, _status: button::Status| button_style(_theme, _status, "labeled"))
        .on_press(Message::CopyFilePath(pane_index));

    container::<Message, WinitTheme, Renderer>(
        row![
            copy_filepath_button,
            copy_filename_button,
            Element::<'_, Message, WinitTheme, Renderer>::from(
                text(footer_text)
                .font(Font::MONOSPACE)
                .style(|_theme| iced::widget::text::Style {
                    color: Some(Color::from([0.8, 0.8, 0.8])), // Wrap Color in a style configuration
                    ..Default::default()
                })
                .size(14)
            )
        ]
        .align_y(Alignment::Center)
        .spacing(3),
    )
    .width(Length::Fill)
    .height(32)
    .padding(3)
    .align_x(Horizontal::Right)
}


pub fn build_ui(app: &DataViewer) -> Container<'_, Message, WinitTheme, Renderer> {
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
    let image_fps = if app.is_slider_moving {
        iced_wgpu::get_image_fps()
    } else {
        IMAGE_RENDER_FPS.lock().map(|fps| *fps as f64).unwrap_or(0.0)
    };

    // Get memory usage in MB
    let memory_mb = {
        if let Ok(mem) = CURRENT_MEMORY_USAGE.lock() {
            *mem as f64 / 1024.0 / 1024.0
        } else {
            0.0
        }
    };

    let fps_display = if app.show_fps {
        container(
            text(format!("UI: {:.1} FPS | Image: {:.1} FPS | Mem: {:.1} MB", 
                         ui_fps, image_fps, memory_mb))
                .size(14)
                .style(|_theme| iced::widget::text::Style {
                    color: Some(Color::from([1.0, 1.0, 1.0])),
                    ..Default::default()
                })
        )
        .padding(5)
    } else {
        container(text("")).width(0).height(0)
    };

    let mb = app_menu::build_menu(app);
    
    let top_bar = container(
        row![
            mb,
            horizontal_space(),
            fps_display
        ]
            .align_y(alignment::Vertical::Center)
    )
    .align_y(alignment::Vertical::Center)
    .width(Length::Fill);

    match app.pane_layout {
        PaneLayout::SinglePane => {
            // Choose the appropriate widget based on slider movement state
            let first_img = if app.panes[0].dir_loaded {
                if app.is_slider_moving && app.panes[0].slider_image.is_some() {
                    // Use regular Image widget during slider movement (much faster)
                    let image_handle = app.panes[0].slider_image.clone().unwrap();
                    
                    container(
                        center(
                            viewer::Viewer::new(image_handle)
                                .content_fit(iced_winit::core::ContentFit::Contain)
                        )
                    )
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .padding(0)
                } else if let Some(scene) = app.panes[0].scene.as_ref() {
                    // Fixed: Pass Arc<Scene> reference correctly
                    let shader = ImageShader::new(Some(scene))
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .content_fit(iced_winit::core::ContentFit::Contain);
            
                    container(center(shader))
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .padding(0)
                } else {
                    container(text("No image loaded"))
                }
            } else {
                container(text("")).height(Length::Fill)
            };

            let footer = if app.show_footer && app.panes[0].dir_loaded {
                get_footer(format!("{}/{}", app.panes[0].img_cache.current_index + 1, app.panes[0].img_cache.num_files), 0)
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
                    column![
                        top_bar,
                        first_img,
                        slider_controls,
                        footer
                    ]
                )
                .style(|theme| container::Style {
                    background: Some(theme.extended_palette().background.base.color.into()),
                    ..container::Style::default()
                })
                .width(Length::Fill)
                .height(Length::Fill)
            ).align_x(Horizontal::Center)
            .into()
        },
        PaneLayout::DualPane => {
            if app.is_slider_dual {
                // Use individual sliders for each pane (build_ui_dual_pane_slider2)
                let panes = build_ui_dual_pane_slider2(
                    &app.panes, 
                    app.divider_position,
                    app.show_footer,
                    app.is_slider_moving,
                    app.is_horizontal_split
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
                .into()
            } else {
                // Use master slider for both panes (build_ui_dual_pane_slider1)
                // Build panes using the split component
                let panes = build_ui_dual_pane_slider1(
                    &app.panes, 
                    app.divider_position,
                    app.is_slider_moving,
                    app.is_horizontal_split
                );

                let footer_texts = vec![
                    format!("{}/{}", app.panes[0].img_cache.current_index + 1, app.panes[0].img_cache.num_files),
                    format!("{}/{}", app.panes[1].img_cache.current_index + 1, app.panes[1].img_cache.num_files),
                ];

                let footer = if app.show_footer && (app.panes[0].dir_loaded || app.panes[1].dir_loaded) {
                    row![
                        get_footer(footer_texts[0].clone(), 0),
                        get_footer(footer_texts[1].clone(), 1)
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
                    column![
                        top_bar,
                        panes,
                        slider,
                        footer
                    ]
                ).style(|theme| container::Style {
                    background: Some(theme.extended_palette().background.base.color.into()),
                    ..container::Style::default()
                })
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
            }
        }
    }
}



pub fn build_ui_dual_pane_slider1(
    panes: &[Pane],
    divider_position: Option<u16>,
    is_slider_moving: bool,
    is_horizontal_split: bool
) -> Element<Message, WinitTheme, Renderer> {
    let first_img = panes[0].build_ui_container(is_slider_moving);
    let second_img = panes[1].build_ui_container(is_slider_moving);
    
    let is_selected: Vec<bool> = panes.iter().map(|pane| pane.is_selected).collect();
    Split::new(
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
    )
    .into()
}


pub fn build_ui_dual_pane_slider2(
    panes: &[Pane],
    divider_position: Option<u16>,
    show_footer: bool,
    is_slider_moving: bool,
    is_horizontal_split: bool
) -> Element<Message, WinitTheme, Renderer> {
    let footer_texts = vec![
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

    let first_img = if panes[0].dir_loaded {
        container(
            if show_footer { 
                column![
                    panes[0].build_ui_container(is_slider_moving),
                    DualSlider::new(
                        0..=(panes[0].img_cache.num_files - 1) as u16,
                        panes[0].slider_value,
                        0,
                        Message::SliderChanged,
                        Message::SliderReleased
                    )
                    .width(Length::Fill),
                    get_footer(footer_texts[0].clone(), 0)
                ]
            } else { 
                column![
                    panes[0].build_ui_container(is_slider_moving),
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
        container(column![
            text(String::from(""))
                .width(Length::Fill)
                .height(Length::Fill),
        ])
    };

    let second_img = if panes[1].dir_loaded {
        container(
            if show_footer { 
                column![
                    panes[1].build_ui_container(is_slider_moving),
                    DualSlider::new(
                        0..=(panes[1].img_cache.num_files - 1) as u16,
                        panes[1].slider_value,
                        1,
                        Message::SliderChanged,
                        Message::SliderReleased
                    )
                    .width(Length::Fill),
                    get_footer(footer_texts[1].clone(), 1)
                ]
            } else { 
                column![
                    panes[1].build_ui_container(is_slider_moving),
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
        container(column![
            text(String::from(""))
                .width(Length::Fill)
                .height(Length::Fill),
        ])
    };

    let is_selected: Vec<bool> = panes.iter().map(|pane| pane.is_selected).collect();
    Split::new(
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
    )
    .into()
}