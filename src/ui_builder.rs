#[cfg(target_os = "linux")]
mod other_os {
    pub use iced;
}

#[cfg(not(target_os = "linux"))]
mod macos {
    pub use iced_custom as iced;
}

#[cfg(target_os = "linux")]
use other_os::*;

#[cfg(not(target_os = "linux"))]
use macos::*;

use iced::widget::{container, row, column, horizontal_space, text, button};
use iced::{Length, Color, alignment, Element, Alignment};
use iced::alignment::Horizontal;
use iced::font::Font;

#[allow(unused_imports)]
use log::{Level, debug, info, warn, error};

use crate::widgets::{dualslider::DualSlider, viewer};
use crate::pane;
use crate::menu as app_menu;
use crate::{Message, PaneLayout, DataViewer};


fn icon<'a, Message>(codepoint: char) -> Element<'a, Message> {
    const ICON_FONT: Font = Font::with_name("viewskater-fonts");

    text(codepoint)
        .font(ICON_FONT)
        .size(18)
        .into()
}

fn file_copy_icon<'a, Message>() -> Element<'a, Message> {
    icon('\u{E804}')
}

fn folder_copy_icon<'a, Message>() -> Element<'a, Message> {
    icon('\u{E805}')
}

pub fn get_footer(footer_text: String, pane_index: usize) -> container::Container<'static, Message> {
    let copy_filename_button = button(file_copy_icon())
        .padding( iced::padding::all(2) )
        .class(crate::menu::ButtonClass::Labeled)
        .on_press(Message::CopyFilename(pane_index));

    let copy_filepath_button = button(folder_copy_icon())
        .padding( iced::padding::all(2) )
        .class(crate::menu::ButtonClass::Labeled)
        .on_press(Message::CopyFilePath(pane_index));

    container(
        row![
            copy_filepath_button,
            copy_filename_button,
            text(footer_text)
                .font(Font::MONOSPACE)
                .style(|_theme| iced::widget::text::Style {
                    color: Some(Color::from([0.8, 0.8, 0.8])), // Wrap Color in a style configuration
                    ..Default::default()
                })
                .size(14)
        ]
        .align_y(Alignment::Center)
        .spacing(3),
    )
    .width(Length::Fill)
    .height(32)
    .padding(3)
    .align_x(Horizontal::Right)
}


/// Build the main UI layout
pub fn build_ui(app: &DataViewer) -> container::Container<Message> {
    // Create the menu bar
    let mb = app_menu::build_menu(app);

    // Create the top bar
    let top_bar = container(
        row!(mb, horizontal_space())
            .align_y(alignment::Vertical::Center)

    )
    .align_y(alignment::Vertical::Center)
    .width(Length::Fill);

    // Handle layout based on pane configuration
    let container_all = match app.pane_layout {
        PaneLayout::SinglePane => {
            let num_digits = app.panes[0].img_cache.num_files.to_string().len();
            let footer_text = format!(
                "{:>num_digits$}/{:>num_digits$}",
                app.panes[0].img_cache.current_index + 1,
                app.panes[0].img_cache.num_files
            );

            let footer = if app.show_footer {
                get_footer(footer_text, 0)
            } else {
                container(text("")).height(0)
            };

            let first_img = if app.panes[0].dir_loaded {
                container(
                    column![
                        viewer::Viewer::new(app.panes[0].current_image.clone())
                            .width(Length::Fill)
                            .height(Length::Fill),
                        DualSlider::new(
                            0..=(app.panes[0].img_cache.num_files - 1) as u16,
                            app.slider_value,
                            -1,
                            Message::SliderChanged,
                            Message::SliderReleased
                        )
                        .width(Length::Fill),
                        footer
                    ],
                )
            } else {
                container(text(""))
            };

            container(
                column![top_bar, first_img.width(Length::Fill)],
            )
        }
        PaneLayout::DualPane => {
            if app.is_slider_dual {
                let panes = pane::build_ui_dual_pane_slider2(
                    &app.panes,
                    app.ver_divider_position,
                    app.show_footer,
                );
                container(column![top_bar, panes]).center_y(Length::Fill)
            } else {
                let panes = pane::build_ui_dual_pane_slider1(&app.panes, app.ver_divider_position);

                let footer_texts = vec![
                    format!(
                        "{}/{}",
                        app.panes[0].img_cache.current_index + 1,
                        app.panes[0].img_cache.num_files
                    ),
                    format!(
                        "{}/{}",
                        app.panes[1].img_cache.current_index + 1,
                        app.panes[1].img_cache.num_files
                    ),
                ];
                let footer = row![
                    get_footer(footer_texts[0].clone(), 0),
                    get_footer(footer_texts[1].clone(), 1)
                ];

                let max_num_files = app.panes.iter().map(|pane| pane.img_cache.num_files).max().unwrap_or(0);

                let h_slider = DualSlider::new(
                    0..=(max_num_files - 1) as u16,
                    app.slider_value,
                    -1,
                    Message::SliderChanged,
                    Message::SliderReleased,
                );

                if app.panes[0].dir_loaded || app.panes[1].dir_loaded {
                    container(
                        column![
                            top_bar,
                            panes,
                            h_slider,
                            if app.show_footer { footer } else { row![] }
                        ],
                    )
                    .center_y(Length::Fill)
                } else {
                    container(column![top_bar, panes].spacing(25)).center_y(Length::Fill)
                }
            }
        }
    };

    container_all
}
