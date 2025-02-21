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

//use iced::widget::{container, row, column, horizontal_space, text, button, shader, center};
use iced_widget::{slider, container, row, column, Row, Column, horizontal_space, text, button, shader, center};
//use iced::{Length, Color, alignment, Element, Alignment, Fill};
//use iced::alignment::Horizontal;
//use iced::font::Font;
use iced_winit::core::{Color, Element, Length, Length::*, Alignment};
use iced_winit::core::alignment;
use iced_winit::core::alignment::Horizontal;
use iced_winit::core::font::Font;


#[allow(unused_imports)]
use log::{Level, debug, info, warn, error};

use crate::widgets::{dualslider::DualSlider, viewer};
use crate::pane;
use crate::menu as app_menu;
use crate::{app::Message, PaneLayout, DataViewer};
use crate::Scene;
use iced_wgpu::Renderer;
use iced_winit::core::Theme as WinitTheme;
use iced_widget::Container;


//fn icon<'a, Message>(codepoint: char) -> Element<'a, Message> {
fn icon<'a, Message>(codepoint: char) -> Element<'a, Message, WinitTheme, Renderer> {
    const ICON_FONT: Font = Font::with_name("viewskater-fonts");

    text(codepoint)
        .font(ICON_FONT)
        .size(18)
        .into()
}

//fn file_copy_icon<'a, Message>() -> Element<'a, Message> {
fn file_copy_icon<'a, Message>() -> Element<'a, Message, WinitTheme, Renderer> {
    icon('\u{E804}')
}

//fn folder_copy_icon<'a, Message>() -> Element<'a, Message> {
fn folder_copy_icon<'a, Message>() -> Element<'a, Message, WinitTheme, Renderer> {
    icon('\u{E805}')
}

//pub fn get_footer(footer_text: String, pane_index: usize) -> container::Container<'static, Message> {
pub fn get_footer(footer_text: String, pane_index: usize) -> Container<'static, Message, WinitTheme, Renderer> {
    let copy_filename_button = button(file_copy_icon())
        .padding( iced::padding::all(2) )
        .class(crate::menu::ButtonClass::Labeled)
        .on_press(Message::CopyFilename(pane_index));

    let copy_filepath_button = button(folder_copy_icon())
        .padding( iced::padding::all(2) )
        .class(crate::menu::ButtonClass::Labeled)
        .on_press(Message::CopyFilePath(pane_index));

    container::<Message, WinitTheme, Renderer>(
        row![
        //Row::<Message, WinitTheme, Renderer>::new([
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
    let mb = app_menu::build_menu(app);
    
    let top_bar = container(
        row!(mb, horizontal_space())
            .align_y(alignment::Vertical::Center)
    )
    .align_y(alignment::Vertical::Center)
    .width(Length::Fill);

    let first_img = if app.panes[0].dir_loaded {
        if let Some(scene) = &app.panes[0].scene {
            let shader_widget = shader(scene)
                .width(Fill)
                .height(Fill);
    
            container(center(shader_widget))
                .width(Length::Fill)
                .height(Length::Fill)
        } else {
            container(text("No image loaded"))
        }
    } else {
        container(text("")).height(Length::Fill)
    };
    
    

    let footer = if app.show_footer {
        get_footer(format!("{}/{}", app.panes[0].img_cache.current_index + 1, app.panes[0].img_cache.num_files), 0)
    } else {
        container(text("")).height(0)
    };

    // Mockup slider
    /*let background_color = app.background_color;
    let slider_controls = container(
        column![
            text("UI is still broken").color(Color::WHITE),
            slider(0.0..=1.0, background_color.r, move |r| {
                Message::BackgroundColorChanged(Color {
                    r,
                    ..background_color
                })
            })
            .step(0.01),
        ]
        .width(Length::Fill)
        .height(Length::Shrink)
        .spacing(10)
        .padding(10)
        .align_x(Horizontal::Center)
    ).style(|_theme| container::Style {
        background: Some(Color::from_rgb(0.2, 0.2, 0.2).into()), // Dark gray background
        text_color: Some(Color::WHITE),
        ..container::Style::default()
    });*/

    /*
    DualSlider::new(
        0..=(app.panes[0].img_cache.num_files - 1) as u16,
        app.slider_value,
        -1,
        Message::SliderChanged,
        Message::SliderReleased
    )
    .width(Length::Fill)
    */

    let slider = if app.panes[0].dir_loaded && app.panes[0].img_cache.num_files > 1 {
        container(DualSlider::new(
            0..=(app.panes[0].img_cache.num_files - 1) as u16,
            app.slider_value,
            -1, // Assuming this was for marking inactive/unused handle
            Message::SliderChanged,
            Message::SliderReleased,
        )
        .width(Length::Fill))
    } else {
        container(text("")).height(0) // Empty when no images are loaded
    };

    let slider_controls = slider
        .width(Length::Fill)
        .height(Length::Shrink)
        .padding(10)
        .align_x(Horizontal::Center);

    center(
        container(
            column![
            //top_bar,
            first_img,
            slider_controls,
            footer
            ])
            .width(Length::Fill)
            .height(Length::Fill)
    ).align_x(Horizontal::Center)
    .into()
}


/*
/// Build the main UI layout
//pub fn build_ui(app: &DataViewer) -> container::Container<Message> {
pub fn build_ui(app: &DataViewer) -> Container<'_, Message, WinitTheme, Renderer> {

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

            ////let shader = shader(&app.scene).width(Fill).height(Fill);
            let shader: iced_widget::Shader<Message, &Scene> = shader(
                &app.panes[0].scene).width(Fill).height(Fill);

            let first_img = if app.panes[0].dir_loaded {
                container::<Message, WinitTheme, Renderer>(
                    //column![
                    column::<Message, WinitTheme, Renderer>([
                        //viewer::Viewer::new(app.panes[0].current_image.clone())
                        //    .width(Length::Fill)
                        //    .height(Length::Fill),

                        ////center(shader).into(),
                        //center::<Message, WinitTheme, Renderer>(shader).into(),
                        Element::<'_, Message, WinitTheme, Renderer>::from(
                            center(shader)),

                        /*DualSlider::new(
                            0..=(app.panes[0].img_cache.num_files - 1) as u16,
                            app.slider_value,
                            -1,
                            Message::SliderChanged,
                            Message::SliderReleased
                        )
                        .width(Length::Fill),*/

                        footer.into()
                    ]),
                )
            } else {
                ////container(text(""))
                container::<Message, WinitTheme, Renderer>(text(""))

            };

            //container(
            //    column![top_bar, first_img.width(Length::Fill)],
            //)
            let first_img_element: Element<'_, Message, WinitTheme, Renderer> = first_img.into(); 

            container::<Message, WinitTheme, Renderer>(
                column::<Message, WinitTheme, Renderer>([
                    container::<Message, WinitTheme, Renderer>(first_img_element)
                        .width(Length::Fill)
                        .into(),  // Convert to Element
                ])
            )


            
            
        }
        PaneLayout::DualPane => {
            /*if app.is_slider_dual {
                let panes = pane::build_ui_dual_pane_slider2(
                    &app.panes,
                    app.ver_divider_position,
                    app.show_footer,
                );
                //container(column![top_bar, panes]).center_y(Length::Fill)
                container::<Message, WinitTheme, Renderer>(
                    //column![top_bar, panes]
                    column::<Message, WinitTheme, Renderer>([top_bar, panes])
                ).center_y(Length::Fill)
                
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
                    container::<Message, WinitTheme, Renderer>(
                        column![
                            top_bar,
                            panes,
                            h_slider,
                            if app.show_footer { footer } else { row![] }
                        ],
                    )
                    .center_y(Length::Fill)
                } else {
                    //container(column![top_bar, panes].spacing(25)).center_y(Length::Fill)
                    container::<Message, WinitTheme, Renderer>(
                        column![top_bar, panes].spacing(25)
                    ).center_y(Length::Fill)
                
                }
            }*/

            // debug: render dummy stuff here
            container::<Message, WinitTheme, Renderer>(
                text("DualPane")
            )
        }
    };

    container_all
}

*/
