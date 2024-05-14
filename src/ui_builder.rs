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
    container, row, column, horizontal_space, text, Text, button
};
//use iced::widget::{Image, Container};
use iced::widget::Container;
use iced::{Length, Color, alignment, Element};
use iced::alignment::{Horizontal, Vertical};
use iced::font::{self, Font};
use iced_aw::menu::{CloseCondition, ItemHeight, ItemWidth, PathHighlight};
use iced_aw::menu_bar;
use iced_aw::{graphics::icons, Icon};
use crate::dualslider::dualslider::DualSlider;
//use crate::footer::footer;


//use crate::split::split::{Axis, Split};
use crate::pane;
//use crate::pane::{Pane};
use crate::menu;
use crate::{Message, PaneLayout, DataViewer};
use crate::viewer;


// Fonts


/*fn icon(unicode: char) -> Text<'static> {
    const ICONS: Font = Font::with_name("vs-icons");

    //text(unicode.to_string())
    text(unicode)
        .font(ICONS)
        //.width(20)
        //.height(14)
        .horizontal_alignment(alignment::Horizontal::Center)
        .vertical_alignment(alignment::Vertical::Center)
}

fn copy_icon() -> Text<'static> {
    icon('\u{F0C5}')
}*/

fn icon<'a, Message>(codepoint: char) -> Element<'a, Message> {
    const ICON_FONT: Font = Font::with_name("viewskater-fonts");

    text(codepoint)
        .font(ICON_FONT)
        //.width(20)
        //.height(16)
        .into()
}
fn copy_icon<'a, Message>() -> Element<'a, Message> {
    //icon('\u{0f0c5}')
    icon('\u{F0C5}')
}
fn file_icon<'a, Message>() -> Element<'a, Message> {
    icon('\u{E800}')
}
fn folder_icon<'a, Message>() -> Element<'a, Message> {
    icon('\u{F114}')
}
fn image_icon<'a, Message>() -> Element<'a, Message> {
    icon('\u{F1C5}')
}

pub fn get_footer(footer_text: String) -> Container<'static, Message> {
    //let copy_button = button(text(String::from("Copy")).size(16)).on_press(Message::CopyFilename);
    ////let copy_button = button(text(String::from("Copy")).size(12)).on_press(Message::CopyFilename).padding(2); // kind of workds
    //let copy_button = button(text(String::from("Copy"))).on_press(Message::CopyFilename);

    //let copy_button = button(text(String::from("Copy")).size(12)).on_press(Message::CopyFilename).padding(2); // kind of workds
    //let copy_button = button(copy_icon()).on_press(Message::CopyFilename).padding(2);
    //let copy_button = button(copy_icon().size(24)).on_press(Message::CopyFilename).padding(1);
    let copy_button = button(copy_icon()).on_press(Message::CopyFilename).padding(2);
    //let copy_button = button(icon_text(icons::icon_to_char(Icon::Person)).size(12)).on_press(Message::CopyFilename).padding(2);

    let copy_filename_button = button(file_icon()).on_press(Message::CopyFilename).padding(2);
    let copy_filepath_button = button(folder_icon()).on_press(Message::CopyFilename).padding(2);
    let copy_image_button = button(image_icon()).on_press(Message::CopyFilename).padding(2);

    container(row![
            //copy_button,
            copy_filename_button,
            //copy_filepath_button,
            //copy_image_button,
            text(String::from(footer_text))
                .style(Color::from([0.8, 0.8, 0.8])).size(14) 
                //.style(Color::from_rgb8(220, 220, 220)).size(14) )
        ].align_items(alignment::Alignment::Center)
        .spacing(3)
    )
    .width(Length::Fill)
    //.height(24)
    .height(32)
    //.padding(5)
    .padding(3)
    //.style(top_bar_style)
    .align_x(Horizontal::Right)
    //.align_y(Vertical::Center)
    //.center_y()
}



//panes: &[Pane], ver_divider_position: Option<u16>, slider_value: u16, pane_layout: PaneLayout
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
            // Create a footer text from the "current_index/total_files" info
            let footer_text = format!(
                "{}/{}",
                _app.panes[0].img_cache.current_index + 1,
                _app.panes[0].img_cache.num_files
            );
            
            let footer = if _app.show_footer {
                get_footer(String::from(footer_text)) 
                } else { container(text(String::from(""))).height(0) };
            

            // let first_img: iced::widget::Container<Message> = _app.panes[0].build_ui();
            let first_img: iced::widget::Container<Message>  = if _app.panes[0].dir_loaded {
                container(column![
                    //Image::new(_app.panes[0].current_image.clone())
                    viewer::Viewer::new(_app.panes[0].current_image.clone())
                    //viewer::Viewer::new(handle=_app.panes[0].current_image.clone(), height=600.0)
                    .width(Length::Fill)
                    .height(Length::Fill),
                    //.height(Length::Fixed(600.0)),
                    DualSlider::new(
                        0..= (_app.panes[0].img_cache.num_files - 1) as u16,
                        // _app.slider_values[0],
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
            //.center_y();
        }
        PaneLayout::DualPane => {
            if _app.is_slider_dual {
                //let panes = _app.build_ui_dual_pane_slider2();
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
                //let panes = _app.build_ui_dual_pane_slider1();
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
                    get_footer(footer_texts[0].clone()),
                    get_footer(footer_texts[1].clone())
                ];

                let max_num_files = _app.panes.iter().fold(0, |max, pane| {
                    if pane.img_cache.num_files > max {
                        pane.img_cache.num_files
                    } else {
                        max
                    }
                });
                
                if _app.panes[0].dir_loaded || _app.panes[1].dir_loaded {
                    // println!("_app.slider_value at draw: {}", _app.slider_value);
                
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
                            ]//.spacing(25)
                        } else {
                            column![
                                top_bar,
                                panes,
                                h_slider,
                            ]//.spacing(25)
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