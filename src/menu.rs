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
use iced_widget::button::Style;
use iced_winit::core::Theme as WinitTheme;
use iced_winit::core::font::Font;
use iced_wgpu::Renderer;
use iced_wgpu::engine::CompressionStrategy;

use iced_aw::menu::{self, Item, Menu};
use iced_aw::{menu_bar, menu_items};
use iced_aw::MenuBar;
use iced_aw::style::{menu_bar::primary, Status};

use crate::app::VIEWER_MODE;
use crate::{app::Message, DataViewer};
use crate::widgets::toggler;
use crate::cache::img_cache::CacheStrategy;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaneLayout {
    SinglePane,
    DualPane,
}

const MENU_FONT_SIZE : u16 = 16;
const MENU_ITEM_FONT_SIZE : u16 = 14;
const _CARET_PATH : &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/svg/caret-right-fill.svg");

// Menu padding constants
const MENU_PADDING_VERTICAL: u16 = 4; // padding for top and bottom
const MENU_PADDING_HORIZONTAL: u16 = 8; // padding for left and right

// A constant for the menu bar height that other components can reference
pub const MENU_BAR_HEIGHT: f32 = (MENU_FONT_SIZE + MENU_PADDING_VERTICAL * 2) as f32; // 16px base height + 8px padding

pub fn button_style(theme: &WinitTheme, status: button::Status, style_type: &str) -> Style {
    match style_type {
        "transparent" => Style {
            text_color: theme.extended_palette().background.base.text,
            background: Some(iced::Color::TRANSPARENT.into()),
            border: iced::Border {
                color: iced::Color::TRANSPARENT,
                width: 0.0,
                radius: Radius::new(0.0),
            },
            ..Default::default()
        },
        "labeled" => match status {
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
                background: Some(theme.extended_palette().background.weak.color.into()),
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
        _ => Style::default(),
    }
}

fn _transparent_style(theme: &WinitTheme, status: button::Status) -> Style {
    button_style(theme, status, "transparent")
}

fn labeled_style(theme: &WinitTheme, status: button::Status) -> Style {
    button_style(theme, status, "labeled")
}

fn default_style(_theme: &WinitTheme, _status: button::Status) -> Style {
    Style::default()
}

fn base_button<'a>(
    content: impl Into<Element<'a, Message, WinitTheme, Renderer>>,
    msg: Message,
) -> button::Button<'a, Message, WinitTheme, Renderer> {
    button(content)
        .style(labeled_style)
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
    .style(labeled_style)
    .on_press(msg)
    .width(Length::Fill)
}

#[allow(dead_code)]
fn nothing_button<'a>(label: &'a str, text_size: u16) -> button::Button<'a, Message, WinitTheme, Renderer> {
    button(
        text(label)
            .size(text_size)
            .font(Font::with_name("Roboto"))
    )
    .style(default_style)
}

fn submenu_button(label: &str, text_size: u16) -> button::Button<Message, WinitTheme, Renderer> {
    base_button(
        row![
            text(label)
                .size(text_size)
                .font(Font::with_name("Roboto"))
                .width(Length::Fill)
                .align_y(alignment::Vertical::Center),
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
    // Use platform-specific modifier text for menu items
    #[cfg(target_os = "macos")]
    let (single_pane_text, dual_pane_text) = (
        if app.pane_layout == PaneLayout::SinglePane { "[x] Single Pane (Cmd+1)" } else { "[  ] Single Pane (Cmd+1)" },
        if app.pane_layout == PaneLayout::DualPane { "[x] Dual Pane (Cmd+2)" } else { "[  ] Dual Pane (Cmd+2)" }
    );

    #[cfg(not(target_os = "macos"))]
    let (single_pane_text, dual_pane_text) = (
        if app.pane_layout == PaneLayout::SinglePane { "[x] Single Pane (Ctrl+1)" } else { "[  ] Single Pane (Ctrl+1)" },
        if app.pane_layout == PaneLayout::DualPane { "[x] Dual Pane (Ctrl+2)" } else { "[  ] Dual Pane (Ctrl+2)" }
    );

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
        ).style(|_theme: &WinitTheme| container::Style {
            text_color: Some(iced_core::Color::from_rgb(0.878, 0.878, 0.878)),
            ..container::Style::default()
        }))
        (container(
            toggler::Toggler::new(
                Some("  Toggle Footer (Tab)".into()),
                app.show_footer,
                Message::ToggleFooter,
            ).width(Length::Fill)
        ).style(|_theme: &WinitTheme| container::Style {
            text_color: Some(iced_core::Color::from_rgb(0.878, 0.878, 0.878)),
            ..container::Style::default()
        }))
        (container(
            toggler::Toggler::new(
                Some("  Horizontal Split (H)".into()),
                app.is_horizontal_split,
                Message::ToggleSplitOrientation,
            ).width(Length::Fill)
        ).style(|_theme: &WinitTheme| container::Style {
            text_color: Some(iced_core::Color::from_rgb(0.878, 0.878, 0.878)),
            ..container::Style::default()
        }))
        (container(
            toggler::Toggler::new(
                Some("  Toggle FPS Display".into()),
                app.show_fps,
                Message::ToggleFpsDisplay,
            ).width(Length::Fill)
        ).style(|_theme: &WinitTheme| container::Style {
            text_color: Some(iced_core::Color::from_rgb(0.878, 0.878, 0.878)),
            ..container::Style::default()
        }))
        (container(
            toggler::Toggler::new(
                Some("  Sync Zoom/Pan".into()),
                app.synced_zoom,
                Message::ToggleSyncedZoom,
            ).width(Length::Fill)
        ).style(|_theme: &WinitTheme| container::Style {
            text_color: Some(iced_core::Color::from_rgb(0.878, 0.878, 0.878)),
            ..container::Style::default()
        }))
        (container(
            toggler::Toggler::new(
                Some("  Toggle Viewer Mode".into()),
                *VIEWER_MODE.lock().unwrap(),
                Message::ToggleViewerMode,
            ).width(Length::Fill)
        ).style(|_theme: &WinitTheme| container::Style {
            text_color: Some(iced_core::Color::from_rgb(0.878, 0.878, 0.878)),
            ..container::Style::default()
        }))
    ))
    .max_width(200.0)
    .spacing(0.0);

    // Create the formatted strings first as owned values
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

    // Create the formatted strings for compression strategy menu
    let no_compression_text = if app.compression_strategy == CompressionStrategy::None { "[x] No compression" } else { "[  ] No compression" };
    let bc1_compression_text = if app.compression_strategy == CompressionStrategy::Bc1 { "[x] BC1 compression" } else { "[  ] BC1 compression" };

    let compression_submenu = Menu::new(menu_items!(
        (labeled_button(
            no_compression_text,
            MENU_ITEM_FONT_SIZE,
            Message::SetCompressionStrategy(CompressionStrategy::None)
        ))
        (labeled_button(
            bc1_compression_text,
            MENU_ITEM_FONT_SIZE,
            Message::SetCompressionStrategy(CompressionStrategy::Bc1)
        ))
    ))
    .max_width(180.0)
    .spacing(0.0);

    Menu::new(menu_items!(
        (submenu_button("Pane Layout", MENU_ITEM_FONT_SIZE), pane_layout_submenu)
        (submenu_button("Controls", MENU_ITEM_FONT_SIZE), controls_menu)
        (submenu_button("Cache Type", MENU_ITEM_FONT_SIZE), cache_type_submenu)
        (submenu_button("Compression", MENU_ITEM_FONT_SIZE), compression_submenu)
    ))
    .max_width(120.0)
    .spacing(0.0)
    .offset(5.0)
}

pub fn menu_1<'a>(_app: &DataViewer) -> Menu<'a, Message, WinitTheme, Renderer> {
    #[cfg(target_os = "macos")]
    let menu_tpl_2 = |items| Menu::new(items).max_width(210.0).offset(5.0);

    #[cfg(not(target_os = "macos"))]
    let menu_tpl_2 = |items| Menu::new(items).max_width(200.0).offset(5.0);

    // Use platform-specific modifier text for menu items
    #[cfg(target_os = "macos")]
    let (open_folder_text, open_file_text, close_text, quit_text) =
        ("Open Folder (Cmd+Shift+O)", "Open File (Cmd+O)", "Close (Cmd+W)", "Quit (Cmd+Q)");

    #[cfg(not(target_os = "macos"))]
    let (open_folder_text, open_file_text, close_text, quit_text) =
        ("Open Folder (Ctrl+Shift+O)", "Open File (Ctrl+O)", "Close (Ctrl+W)", "Quit (Ctrl+Q)");

    // Create submenu for "Open Folder"
    let open_folder_submenu = Menu::new(menu_items!(
        (labeled_button(
            "Pane 1 (Alt+1)",
            MENU_ITEM_FONT_SIZE,
            Message::OpenFolder(0)
        ))
        (labeled_button(
            "Pane 2 (Alt+2)",
            MENU_ITEM_FONT_SIZE,
            Message::OpenFolder(1)
        ))
    ))
    .max_width(180.0)
    .spacing(0.0);

    // Create submenu for "Open File"
    let open_file_submenu = Menu::new(menu_items!(
        (labeled_button(
            "Pane 1 (Shift+Alt+1)",
            MENU_ITEM_FONT_SIZE,
            Message::OpenFile(0)
        ))
        (labeled_button(
            "Pane 2 (Shift+Alt+2)",
            MENU_ITEM_FONT_SIZE,
            Message::OpenFile(1)
        ))
    ))
    .max_width(180.0)
    .spacing(0.0);

    menu_tpl_2(
        menu_items!(
            (submenu_button(open_folder_text, MENU_ITEM_FONT_SIZE), open_folder_submenu)
            (submenu_button(open_file_text, MENU_ITEM_FONT_SIZE), open_file_submenu)
            (labeled_button(close_text, MENU_ITEM_FONT_SIZE, Message::Close))
            (labeled_button(quit_text, MENU_ITEM_FONT_SIZE, Message::Quit))
        )
    )
}

pub fn menu_help<'a>(_app: &DataViewer) -> Menu<'a, Message, WinitTheme, Renderer> {
    let menu_tpl_2 = |items| Menu::new(items).max_width(200.0).offset(5.0);
    menu_tpl_2(
        menu_items!(
            (labeled_button("About", MENU_ITEM_FONT_SIZE, Message::ShowAbout))
            (labeled_button("Show logs", MENU_ITEM_FONT_SIZE, Message::ShowLogs))
            (labeled_button("Export debug logs", MENU_ITEM_FONT_SIZE, Message::ExportDebugLogs))
            (labeled_button("Export all logs", MENU_ITEM_FONT_SIZE, Message::ExportAllLogs))
        )
    )
}


pub fn build_menu(app: &DataViewer) -> MenuBar<Message, WinitTheme, Renderer> {
    menu_bar!(
        (
            container(
                text("File").size(MENU_FONT_SIZE).font(Font::with_name("Roboto"))
            )
            .style(|_theme: &WinitTheme| container::Style {
                text_color: Some(iced_core::Color::from_rgb(0.878, 0.878, 0.878)),
                ..container::Style::default()
            })
            .padding([MENU_PADDING_VERTICAL, MENU_PADDING_HORIZONTAL]),
            menu_1(app)
        )

        (
            container(
                text("Controls").size(MENU_FONT_SIZE).font(Font::with_name("Roboto")),//.align_y(alignment::Vertical::Center)
            )
            .style(|_theme: &WinitTheme| container::Style {
                text_color: Some(iced_core::Color::from_rgb(0.878, 0.878, 0.878)),
                ..container::Style::default()
            })
            .padding([MENU_PADDING_VERTICAL, MENU_PADDING_HORIZONTAL]), // // [top/bottom, left/right
            menu_3(app)
        )

        (
            container(
                text("Help").size(MENU_FONT_SIZE).font(Font::with_name("Roboto"))
            )
            .style(|_theme: &WinitTheme| container::Style {
                text_color: Some(iced_core::Color::from_rgb(0.878, 0.878, 0.878)),
                ..container::Style::default()
            })
            .padding([MENU_PADDING_VERTICAL, MENU_PADDING_HORIZONTAL]),
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
