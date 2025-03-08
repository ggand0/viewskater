#[cfg(target_os = "linux")]
mod other_os {
    pub use iced_custom as iced;
    pub use iced_aw as iced_aw; // TODO: Change this to iced_aw_custom
}

#[cfg(not(target_os = "linux"))]
mod macos {
    pub use iced_custom as iced;
    pub use iced_aw as iced_aw;
}

#[cfg(target_os = "linux")]
use other_os::*;

#[cfg(not(target_os = "linux"))]
use macos::*;


use iced_widget::{container, row, button, text};
use iced_winit::core::alignment;
use iced_winit::core::{Padding, Element, Length, Border};
use iced_winit::core::border::Radius;
use iced_widget::button::{Style};
use iced_winit::core::Theme as WinitTheme;
use iced_winit::core::font::Font;
use iced_wgpu::Renderer;

use iced_aw::menu::{self, Item, Menu};
use iced_aw::{menu_bar, menu_items};
use iced_aw::MenuBar;
use iced_aw::style::{menu_bar::primary, Status};

use crate::{app::Message, DataViewer};
use crate::widgets::toggler;
use crate::cache::img_cache::CacheStrategy;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaneLayout {
    SinglePane,
    DualPane,
}

const _MENU_FONT_SIZE : u16 = 16;
const MENU_ITEM_FONT_SIZE : u16 = 14;
const _CARET_PATH : &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/svg/caret-right-fill.svg");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonClass {
    Transparent,
    Labeled,
    Nothing,
}

impl<'a> From<ButtonClass> for Box<dyn Fn(&WinitTheme, button::Status) -> Style + 'a> {
    fn from(class: ButtonClass) -> Self {
        Box::new(move |theme: &WinitTheme, status: button::Status| match class {
            ButtonClass::Transparent => Style {
                text_color: theme.extended_palette().background.base.text,
                background: Some(iced::Color::TRANSPARENT.into()),
                border: iced::Border {
                    color: iced::Color::TRANSPARENT,
                    width: 0.0,
                    radius: Radius::new(0.0),
                },
                ..Default::default()
            },
            ButtonClass::Labeled => match status {
                button::Status::Active => Style {
                    background: Some(theme.extended_palette().background.base.color.into()),
                    text_color: theme.extended_palette().primary.weak.text,
                    border: iced::Border {
                        color: iced::Color::TRANSPARENT,
                        width: 1.0,
                        radius: Radius::new(0.0),
                    },
                    ..Default::default()
                },
                button::Status::Hovered => Style {
                    background: Some(theme.extended_palette().background.weak.color.into()),
                    text_color: theme.extended_palette().primary.weak.text,
                    border: iced::Border {
                        color: iced::Color::TRANSPARENT,
                        width: 1.0,
                        radius: Radius::new(0.0),
                    },
                    ..Default::default()
                },
                button::Status::Pressed => Style {
                    background: Some(theme.extended_palette().primary.weak.color.into()),
                    text_color: theme.extended_palette().primary.weak.text,
                    border: iced::Border {
                        color: iced::Color::TRANSPARENT,
                        width: 1.0,
                        radius: Radius::new(0.0),
                    },
                    ..Default::default()
                },
                _ => Style::default(),
            },
            ButtonClass::Nothing => Style::default(),
        })
    }
}


fn base_button<'a>(
    content: impl Into<Element<'a, Message, WinitTheme, Renderer>>,
    msg: Message,
) -> button::Button<'a, Message, WinitTheme, Renderer> {
    button(content)
        .class(ButtonClass::Labeled)
        .on_press(msg)
}

fn labeled_button<'a>(
    label: &'a str,
    text_size: u16,
    msg: Message,
) -> button::Button<'a, Message, WinitTheme, Renderer> {
    button(
        text(label)
            .size(text_size)
            .font(Font::with_name("Roboto"))
    )
    .class(ButtonClass::Labeled)
    .on_press(msg)
    .width(Length::Fill)
}

#[allow(dead_code)]
fn nothing_button<'a>(label: &'a str, text_size: u16) -> button::Button<'a, Message> {
    button(
        text(label)
            .size(text_size)
            .font(Font::with_name("Roboto"))
    )
    //.padding([4, 8])
    .class(ButtonClass::Labeled)
    //.width(Length::Shrink)
}

fn submenu_button(label: &str, text_size: u16) -> button::Button<Message, WinitTheme, Renderer> {
    base_button(
        row![
            text(label)
                .size(text_size)
                .font(Font::with_name("Roboto"))
                .width(Length::Fill)
                .align_y(alignment::Vertical::Center),
            //text(icon_to_string(RequiredIcons::CaretRightFill))
            //.font(REQUIRED_FONT)
            text(">")
                .size(text_size)
                .width(Length::Shrink)
                .align_y(alignment::Vertical::Center),
        ]
        .align_y(iced::Alignment::Center),
        Message::Debug(label.into()),
    )
    .width(Length::Fill)
}

pub fn menu_3<'a>(app: &DataViewer) -> Menu<'a, Message, WinitTheme, Renderer> {
    let single_pane_text = if app.pane_layout == PaneLayout::SinglePane { "[x] Single Pane (Ctrl+1)" } else { "[  ] Single Pane (Ctrl+1)" };
    let dual_pane_text = if app.pane_layout == PaneLayout::DualPane { "[x] Dual Pane (Ctrl+2)" } else { "[  ] Dual Pane (Ctrl+2)" };

    let pane_layout_submenu = Menu::new(menu_items!(
        (labeled_button(
            single_pane_text,
            MENU_ITEM_FONT_SIZE,
            Message::TogglePaneLayout(PaneLayout::SinglePane)
        ))
        (labeled_button(
            dual_pane_text,
            MENU_ITEM_FONT_SIZE,
            Message::TogglePaneLayout(PaneLayout::DualPane)
        ))
    ))
    .max_width(180.0)
    .spacing(0.0);

    let controls_menu = Menu::new(menu_items!(
        (container(
            toggler::Toggler::new(
                Some("  Toggle Slider (Space)".into()),
                app.is_slider_dual,
                Message::ToggleSliderType,
            ).width(Length::Fill)
        ))
        (container(
            toggler::Toggler::new(
                Some("  Toggle Footer (Tab)".into()),
                app.show_footer,
                Message::ToggleFooter,
            ).width(Length::Fill)
        ))
    ))
    .max_width(200.0)
    .spacing(0.0);

    // Create the formatted strings first as owned values
    //let cpu_cache_text = if app.cache_strategy == CacheStrategy::Cpu { "✓ CPU cache" } else { "  CPU cache" };
    //let gpu_cache_text = if app.cache_strategy == CacheStrategy::Gpu { "✓ GPU cache" } else { "  GPU cache" };
    //let cpu_cache_text = if app.cache_strategy == CacheStrategy::Cpu { "-> CPU cache" } else { "   CPU cache" };
    //let gpu_cache_text = if app.cache_strategy == CacheStrategy::Gpu { "-> GPU cache" } else { "   GPU cache" };
    let cpu_cache_text = if app.cache_strategy == CacheStrategy::Cpu { "[x] CPU cache" } else { "[  ] CPU cache" };
    let gpu_cache_text = if app.cache_strategy == CacheStrategy::Gpu { "[x] GPU cache" } else { "[  ] GPU cache" };



    let cache_type_submenu = Menu::new(menu_items!(
        (labeled_button(
            cpu_cache_text,
            MENU_ITEM_FONT_SIZE,
            Message::SetCacheStrategy(CacheStrategy::Cpu)
        ))
        (labeled_button(
            gpu_cache_text,
            MENU_ITEM_FONT_SIZE,
            Message::SetCacheStrategy(CacheStrategy::Gpu)
        ))
    ))
    .max_width(180.0)
    .spacing(0.0);

    Menu::new(menu_items!(
        (submenu_button("Pane Layout", MENU_ITEM_FONT_SIZE), pane_layout_submenu)
        (submenu_button("Controls", MENU_ITEM_FONT_SIZE), controls_menu)
        (submenu_button("Cache Type", MENU_ITEM_FONT_SIZE), cache_type_submenu)
    ))
    .max_width(120.0)
    .spacing(0.0)
    .offset(5.0)
}

pub fn menu_1<'a>(_app: &DataViewer) -> Menu<'a, Message, WinitTheme, Renderer> {
    let menu_tpl_2 = |items| Menu::new(items).max_width(200.0).offset(5.0);
    menu_tpl_2(
        menu_items!(
            (labeled_button(
                "Open Folder (Alt+1 or 2)",
                MENU_ITEM_FONT_SIZE,
                Message::OpenFolder(0)
            ))
            (labeled_button(
                "Open File (Alt+Ctrl+1 or 2)",
                MENU_ITEM_FONT_SIZE,
                Message::OpenFile(0)
            ))
            (labeled_button("Close (Ctrl+W)", MENU_ITEM_FONT_SIZE, Message::Close))
            (labeled_button("Quit (Ctrl+Q)", MENU_ITEM_FONT_SIZE, Message::Quit))
        )
    )
}

pub fn menu_help<'a>(_app: &DataViewer) -> Menu<'a, Message, WinitTheme, Renderer> {
    let menu_tpl_2 = |items| Menu::new(items).max_width(200.0).offset(5.0);
    menu_tpl_2(
        menu_items!(
            (labeled_button("About", MENU_ITEM_FONT_SIZE, Message::ShowAbout))
            (labeled_button("Show logs", MENU_ITEM_FONT_SIZE, Message::ShowLogs))
        )
    )
}


pub fn build_menu(app: &DataViewer) -> MenuBar<Message, WinitTheme, Renderer> {
    menu_bar!(
        (
            container(
                text("File").size(16).font(Font::with_name("Roboto"))
            ).padding([4, 8]),
            menu_1(app)
        )

        (
            container(
                text("Controls").size(16).font(Font::with_name("Roboto")),//.align_y(alignment::Vertical::Center)
            ).padding([4, 8]),
            menu_3(app)
        )

        (
            container(
                text("Help").size(16).font(Font::with_name("Roboto"))
            ).padding([4, 8]),
            menu_help(app)
        )
    )
    //.spacing(10)
    // ref: https://github.com/iced-rs/iced_aw/blob/main/src/style/menu_bar.rs
    .draw_path(menu::DrawPath::Backdrop)
    .style(|theme: &WinitTheme, status: Status | menu::Style{
        //menu_background: theme.extended_palette().background.weak.color.into(),
        menu_border: Border{
            color: theme.extended_palette().background.weak.color,
            width: 1.0,
            radius: Radius::new(0.0),
            ..Default::default()
        },
        menu_background_expand: Padding::from(0.0),
        path_border: Border{
            radius: Radius::new(0.0),
            ..Default::default()
        },
        path: theme.extended_palette().background.weak.color.into(),
        ..primary(theme, status)
    })
}