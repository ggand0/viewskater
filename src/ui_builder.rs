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


use iced_widget::{container, Container, row, column, horizontal_space, text, button, center};
use iced_widget::button::Style;
use iced_winit::core::{Color, Element, Length, Alignment};
use iced_winit::core::alignment;
use iced_winit::core::alignment::Horizontal;
use iced_winit::core::font::Font;
use iced_winit::core::Theme as WinitTheme;
use iced_wgpu::Renderer;

#[allow(unused_imports)]
use log::{Level, debug, info, warn, error};

use crate::widgets::{dualslider::DualSlider, viewer};
use crate::pane;
use crate::menu as app_menu;
use app_menu::button_style;
use crate::{app::Message, PaneLayout, DataViewer};
use crate::widgets::shader::image_shader::ImageShader;
use crate::{CURRENT_FPS, pane::IMAGE_RENDER_FPS};


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
    // Get both FPS values
    let ui_fps = {
        if let Ok(fps) = CURRENT_FPS.lock() {
            *fps
        } else {
            0.0
        }
    };
    
    let image_fps = {
        if let Ok(fps) = IMAGE_RENDER_FPS.lock() {
            *fps
        } else {
            0.0
        }
    };

    let fps_display = if app.show_fps {
        container(
            text(format!("UI: {:.1} FPS | Image: {:.1} FPS", ui_fps, image_fps))
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
                let panes = pane::build_ui_dual_pane_slider2(
                    &app.panes, 
                    app.ver_divider_position,
                    app.show_footer,
                    app.is_slider_moving
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
                let panes = pane::build_ui_dual_pane_slider1(
                    &app.panes, 
                    app.ver_divider_position,
                    app.is_slider_moving
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
