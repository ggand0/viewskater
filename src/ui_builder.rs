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
    container, row, column, horizontal_space, text, Text
};
//use iced::widget::{Image, Container};
use iced::widget::Container;
use iced::{Length, Color, alignment};
use iced::alignment::Horizontal;
use iced_aw::menu::{CloseCondition, ItemHeight, ItemWidth, PathHighlight};
use iced_aw::menu_bar;
use crate::dualslider::dualslider::DualSlider;
use crate::footer::footer;

//use crate::split::split::{Axis, Split};
use crate::pane;
//use crate::pane::{Pane};
use crate::menu;
use crate::{Message, PaneLayout, DataViewer};
use crate::viewer;

fn get_footer(footer_text: String) -> Container<'static, Message> {
    container(text(String::from(footer_text))
        .style(Color::from([0.8, 0.8, 0.8])).size(14) )
        //.style(Color::from_rgb8(220, 220, 220)).size(14) )
        .width(Length::Fill)
        .height(24)
        .padding(5)
        //.style(top_bar_style)
        .align_x(Horizontal::Right)
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

    /*let footer = container(text(String::from("footer text"))
        .style(Color::from([0.8, 0.8, 0.8])).size(14) )
        //.style(Color::from_rgb8(220, 220, 220)).size(14) )
        .width(Length::Fill)
        .height(24)
        .padding(5)
        //.style(top_bar_style)
        .align_x(Horizontal::Right);*/


    let container_all;
    match _app.pane_layout {
        PaneLayout::SinglePane => {
            // Create a footer text from the "current_index/total_files" info
            let footer_text = format!(
                "{}/{}",
                _app.panes[0].img_cache.current_index + 1,
                _app.panes[0].img_cache.num_files
            );
            let footer = get_footer(String::from(footer_text));

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

                    footer,
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
                let panes = pane::build_ui_dual_pane_slider2(&_app.panes, _app.ver_divider_position);
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
                        column![top_bar, panes, h_slider].spacing(25),
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