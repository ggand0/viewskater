#[cfg(target_os = "linux")]
mod other_os {
    pub use iced;
    pub use iced_aw;
    pub use iced_widget;
}

#[cfg(not(target_os = "linux"))]
mod macos {
    pub use iced_custom as iced;
    pub use iced_aw_custom as iced_aw;
    pub use iced_widget_custom as iced_widget;
}

#[cfg(target_os = "linux")]
use other_os::*;

#[cfg(not(target_os = "linux"))]
use macos::*;

use iced::widget::{
    container, row, column, horizontal_space, text, button
};
use iced::widget::Container;
use iced::{Length, Color, alignment, Element, theme, Theme};

#[allow(unused_imports)]
use iced::alignment::{Horizontal, Vertical};
use iced::font::Font;
use iced_aw::menu::{CloseCondition, ItemHeight, ItemWidth, PathHighlight};
use iced_aw::menu_bar;
use crate::dualslider::dualslider::DualSlider;

use crate::pane;
use crate::menu;
use crate::{Message, PaneLayout, DataViewer};
use crate::viewer;


struct CustomButtonStyle {
    theme: theme::Button,
}

impl CustomButtonStyle {
    pub fn new(theme: theme::Button) -> Self {
        Self { theme }
    }
}

impl button::StyleSheet for CustomButtonStyle {
    type Style = Theme;

    fn active(&self, style: &Self::Style) -> button::Appearance {
        let mut appearance = style.active(&self.theme);
        appearance.background = Some(Color::from_rgb(0.1, 0.1, 0.1).into());

        appearance
    }

    fn hovered(&self, _style: &Self::Style) -> button::Appearance {
        button::Appearance {
            background: Some(Color::from_rgba8(20, 148, 163, 1.0).into()),
            text_color: Color::WHITE,
            ..button::Appearance::default()
        }
    }
}
fn icon<'a, Message>(codepoint: char) -> Element<'a, Message> {
    const ICON_FONT: Font = Font::with_name("viewskater-fonts");

    text(codepoint)
        .font(ICON_FONT)
        .into()
}
#[allow(dead_code)]
fn copy_icon<'a, Message>() -> Element<'a, Message> {
    icon('\u{F0C5}')
}
#[allow(dead_code)]
fn file_icon<'a, Message>() -> Element<'a, Message> {
    icon('\u{E800}')
}
#[allow(dead_code)]
fn folder_icon<'a, Message>() -> Element<'a, Message> {
    icon('\u{F114}')
}
#[allow(dead_code)]
fn image_icon<'a, Message>() -> Element<'a, Message> {
    icon('\u{F1C5}')
}
fn file_copy_icon<'a, Message>() -> Element<'a, Message> {
    icon('\u{E804}')
}
fn folder_copy_icon<'a, Message>() -> Element<'a, Message> {
    icon('\u{E805}')
}

pub fn get_footer(footer_text: String, pane_index: usize) -> Container<'static, Message> {
    let copy_filename_button: iced_widget::Button<'_, Message> = button(file_copy_icon()) //: Element<Message> 
        .style(theme::Button::Custom(Box::new(
            CustomButtonStyle::new(theme::Button::Primary),
        )))
        .on_press(Message::CopyFilename(pane_index)).padding(2).into();
    let copy_filepath_button: iced_widget::Button<'_, Message> = button(folder_copy_icon())
        .style(theme::Button::Custom(Box::new(
            CustomButtonStyle::new(theme::Button::Primary),
        )))
        .on_press(Message::CopyFilePath(pane_index)).padding(2).into();
    

    container(row![
            copy_filepath_button,
            copy_filename_button,
            text(String::from(footer_text))
                .font(Font {
                    family: iced::font::Family::Name("Iosevka"),
                    weight: iced::font::Weight::Normal,
                    stretch: iced::font::Stretch::Normal,
                    monospaced: true,
                })
                .style(Color::from([0.8, 0.8, 0.8])).size(14) 
        ].align_items(alignment::Alignment::Center)
        .spacing(3)
    )
    .width(Length::Fill)
    .height(32)
    .padding(3)
    .align_x(Horizontal::Right)
}


pub fn build_ui(_app: &DataViewer) -> Container<Message> {

    let mb =  { menu_bar!(menu::menu_1(_app), menu::menu_3(_app))
        .item_width(ItemWidth::Uniform(180))
        .item_height(ItemHeight::Uniform(27)) }
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
    let r = row!(mb, horizontal_space(Length::Fill))
    .padding([2, 8])
    .align_items(alignment::Alignment::Center);
    let top_bar_style: fn(&iced::Theme) -> container::Appearance =
    |_theme| container::Appearance {
        background: Some(Color::TRANSPARENT.into()),
        ..Default::default()
    };
    let top_bar = container(r).width(Length::Fill).style(top_bar_style);


    let container_all;
    match _app.pane_layout {
        PaneLayout::SinglePane => {
            let num_digits = _app.panes[0].img_cache.num_files.to_string().len();

            // Create a footer text from the "current_index/total_files" info
            let footer_text = format!(
                "{:>num_digits$}/{:>num_digits$}",
                _app.panes[0].img_cache.current_index + 1,
                _app.panes[0].img_cache.num_files
            );
            let footer = if _app.show_footer {
                    get_footer(String::from(footer_text), 0) 
                } else { container(text(String::from(""))).height(0) };
            

            let first_img: iced::widget::Container<Message>  = if _app.panes[0].dir_loaded {
                container(column![
                    viewer::Viewer::new(_app.panes[0].current_image.clone())
                    .width(Length::Fill)
                    .height(Length::Fill),
                    DualSlider::new(
                        0..= (_app.panes[0].img_cache.num_files - 1) as u16,
                        _app.slider_value,
                        -1,
                        Message::SliderChanged,
                        Message::SliderReleased
                    )
                    .width(Length::Fill),

                    footer
                    ]
                )
            } else {
                container(column![
                text(String::from(""))
                .width(Length::Fill)
                .height(Length::Fill)
                
                ])
            };

            container_all = container(
                column![
                    top_bar,
                    first_img
                    .width(Length::Fill)
                ]
            )
        }
        PaneLayout::DualPane => {
            if _app.is_slider_dual {
                let panes = pane::build_ui_dual_pane_slider2(
                    &_app.panes, _app.ver_divider_position, _app.show_footer);
                container_all = container(
                    column![
                        top_bar,
                        panes,
                    ]
                )
                .center_y();
            } else {
                let panes = pane::build_ui_dual_pane_slider1(&_app.panes, _app.ver_divider_position);

                let footer_texts = vec![
                    format!(
                        "{}/{}",
                        _app.panes[0].img_cache.current_index + 1,
                        _app.panes[0].img_cache.num_files
                    ),
                    format!(
                        "{}/{}",
                        _app.panes[1].img_cache.current_index + 1,
                        _app.panes[1].img_cache.num_files
                    )
                ];
                let footer = row![
                    get_footer(footer_texts[0].clone(), 0),
                    get_footer(footer_texts[1].clone(), 1)
                ];

                let max_num_files = _app.panes.iter().fold(0, |max, pane| {
                    if pane.img_cache.num_files > max {
                        pane.img_cache.num_files
                    } else {
                        max
                    }
                });
                
                if _app.panes[0].dir_loaded || _app.panes[1].dir_loaded {
                
                    let h_slider = DualSlider::new(
                        0..=(max_num_files - 1) as u16,
                        _app.slider_value,
                        -1, // -1 means all panes
                        Message::SliderChanged,
                        Message::SliderReleased
                    );
                
                    container_all = container(
                        if _app.show_footer {
                            column![
                                top_bar,
                                panes,
                                h_slider,
                                footer,
                            ]
                        } else {
                            column![
                                top_bar,
                                panes,
                                h_slider,
                            ]
                        }
                    )
                    .center_y();
                } else {
                    container_all = container(column![
                        top_bar,
                        panes,
                    ].spacing(25)).center_y();
                }
                    
            }
        }
    }
    
    container_all

}