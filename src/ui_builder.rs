#[cfg(target_os = "linux")]
mod other_os {
    pub use iced;
    pub use iced_aw;
}

#[cfg(not(target_os = "linux"))]
mod macos {
    pub use iced_custom as iced;
    pub use iced_aw_custom as iced_aw;
}

#[cfg(target_os = "linux")]
use other_os::*;

#[cfg(not(target_os = "linux"))]
use macos::*;

use iced::widget::{container, row, column, horizontal_space, text, button};
use iced::{Length, Color, alignment, Element, Theme, Alignment};
use iced::alignment::{Horizontal, Vertical};
use iced::font::Font;
//use iced_aw::menu::{CloseCondition, ItemHeight, ItemWidth, PathHighlight};
//use iced_aw::menu_bar;
use iced_aw::menu::{Item, Menu};
//use iced_aw::{menu, menu_bar, menu_items};
use iced_aw::{menu_bar, menu_items};
#[allow(unused_imports)]
use log::{Level, debug, info, warn, error};


use crate::dualslider::dualslider::DualSlider;

use crate::pane;
use crate::menu as app_menu;
use crate::{Message, PaneLayout, DataViewer};
use crate::viewer;

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


/*pub fn build_ui(app: &DataViewer) -> container::Container<Message> {
    //let menu_bar = menu_bar!(menu::menu_1(app), menu::menu_3(app))
    let menu_bar = menu_bar!(menu::menu_3(app))
        .item_width(ItemWidth::Uniform(180))
        .item_height(ItemHeight::Uniform(27))
        .spacing(4.0)
        .bounds_expand(30)
        .main_offset(13)
        .cross_offset(16)
        .path_highlight(Some(PathHighlight::MenuActive))
        .close_condition(CloseCondition {
            leave: true,
            click_outside: false,
            click_inside: false,
        });

    let top_bar = container(
        row!(menu_bar, horizontal_space(Length::Fill))
            .padding([2, 8])
            .align_items(Alignment::Center),
    )
    .width(Length::Fill)
    .style(|_theme| container::Appearance {
        background: Some(Color::TRANSPARENT.into()),
        ..Default::default()
    });

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
                container(column![top_bar, panes]).center_y()
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
                    .center_y()
                } else {
                    container(column![top_bar, panes].spacing(25)).center_y()
                }
            }
        }
    };

    container_all
}*/

/// Build the main UI layout
pub fn build_ui(app: &DataViewer) -> container::Container<Message> {
    // Create the menu bar
    let menu_bar = menu_bar!(
        //(menu::menu_1(app), Menu::new(menu::build_menu_items_v1()).width(180.0))
        ("Main Menu", app_menu::menu_3(app).width(180.0))
    );

    // Create the top bar
    let top_bar = container(
        //row!(menu_bar, horizontal_space(Length::Fill))
        row!(menu_bar, horizontal_space())
            .padding([2, 8])
            //.align_items(Alignment::Center),
            .align_y(Alignment::Center)
    )
    .width(Length::Fill)
    /*.style(|_theme| container::Appearance {
        background: Some(Color::TRANSPARENT.into()),
        ..Default::default()
    })*/
    .style(|_theme| container::Style {
        text_color: None,
        background: Some(Color::TRANSPARENT.into()),
        border: iced::Border::default(),
        shadow: iced::Shadow::default(),
    });

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
