use iced_winit::core::{Element, Length, Alignment, Color};
use iced_winit::core::font::Font;
use iced_widget::{row, column, container, text, button, Space, scrollable};
use iced_winit::core::Theme as WinitTheme;
use iced_wgpu::Renderer;
use iced_wgpu::engine::CompressionStrategy;
use iced_aw::widget::tab_bar::tab_label::TabLabel;
use iced_aw::tabs::Tabs;

use crate::app::{Message, DataViewer};
use crate::cache::img_cache::CacheStrategy;
use crate::widgets;

/// Builds the settings modal dialog with tabs
pub fn view_settings_modal<'a>(viewer: &'a DataViewer) -> Element<'a, Message, WinitTheme, Renderer> {
    // Create the tabs with compact styling
    let tabs = Tabs::new(Message::SettingsTabSelected)
        .push(
            0,  // Tab ID
            TabLabel::Text("General".to_string()),  // Label
            view_general_tab(viewer)  // Content
        )
        .push(
            1,  // Tab ID
            TabLabel::Text("Advanced".to_string()),  // Label
            view_advanced_tab(viewer)  // Content
        )
        .set_active_tab(&viewer.active_settings_tab)
        .tab_bar_style(|theme: &WinitTheme, _status| {
            iced_aw::style::tab_bar::Style {
                background: Some(theme.extended_palette().background.weak.color.into()),
                border_color: Some(theme.extended_palette().background.strong.color),
                border_width: 0.0,
                tab_label_background: iced_winit::core::Background::Color(Color::TRANSPARENT),
                tab_label_border_color: theme.extended_palette().background.strong.color,
                tab_label_border_width: 1.0,
                icon_background: Some(iced_winit::core::Background::Color(Color::TRANSPARENT)),
                icon_border_radius: 0.0.into(),
                icon_color: theme.palette().text,
                text_color: theme.palette().text,
            }
        })
        .tab_label_spacing(0)
        //.tab_label_padding(5.0)
        .tab_label_padding(2.0)
        .text_size(13.0)
        .width(Length::Fill)
        .height(Length::Fill);

    let content = column![
        // Title row
        row![
            text("Settings").size(18)
                .font(Font {
                    family: iced_winit::core::font::Family::Name("Roboto"),
                    weight: iced_winit::core::font::Weight::Bold,
                    stretch: iced_winit::core::font::Stretch::Normal,
                    style: iced_winit::core::font::Style::Normal,
                }),
        ]
        .align_y(Alignment::Center),

        // Tabs
        container(tabs)
            .height(Length::Fixed(270.0))
            .padding(0),

        // Status message (always reserve space to prevent layout jump)
        container(
            text(viewer.settings_save_status.as_deref().unwrap_or(" ")).size(14)
        )
        .style(|theme: &WinitTheme| container::Style {
            text_color: Some(theme.extended_palette().success.strong.color),
            ..container::Style::default()
        })
        .height(Length::Fixed(18.0)),

        // Buttons row
        row![
            button(text("Save Settings"))
                .padding([3, 10])
                .on_press(Message::SaveSettings),
            button(text("Close"))
                .padding([3, 10])
                .on_press(Message::HideOptions),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
    ]
    .spacing(5)
    .align_x(iced_winit::core::alignment::Horizontal::Left)
    .width(Length::Fixed(700.0))
    .height(Length::Fixed(360.0));

    container(content)
        .padding(15)
        .style(|theme: &WinitTheme| {
            iced_widget::container::Style {
                background: Some(theme.extended_palette().background.base.color.into()),
                text_color: Some(theme.extended_palette().primary.weak.text),
                border: iced_winit::core::Border {
                    color: theme.extended_palette().background.strong.color,
                    width: 1.0,
                    radius: iced_winit::core::border::Radius::from(8.0),
                },
                ..Default::default()
            }
        })
        .into()
}

/// General tab content: Display, Performance, and Controls
fn view_general_tab<'a>(viewer: &'a DataViewer) -> Element<'a, Message, WinitTheme, Renderer> {
    // Left column - Display & Performance
    let left_column = column![
        text("Display").size(16)
            .font(Font {
                family: iced_winit::core::font::Family::Name("Roboto"),
                weight: iced_winit::core::font::Weight::Medium,
                stretch: iced_winit::core::font::Stretch::Normal,
                style: iced_winit::core::font::Style::Normal,
            }),

        container(
            widgets::toggler::Toggler::new(
                Some("Show FPS Display".into()),
                viewer.show_fps,
                Message::ToggleFpsDisplay,
            ).width(Length::Fill)
        ).style(|_theme: &WinitTheme| container::Style {
            text_color: Some(Color::from_rgb(0.878, 0.878, 0.878)),
            ..container::Style::default()
        }),

        container(
            widgets::toggler::Toggler::new(
                Some("Show Footer".into()),
                viewer.show_footer,
                Message::ToggleFooter,
            ).width(Length::Fill)
        ).style(|_theme: &WinitTheme| container::Style {
            text_color: Some(Color::from_rgb(0.878, 0.878, 0.878)),
            ..container::Style::default()
        }),

        container(
            widgets::toggler::Toggler::new(
                Some("Horizontal Split".into()),
                viewer.is_horizontal_split,
                Message::ToggleSplitOrientation,
            ).width(Length::Fill)
        ).style(|_theme: &WinitTheme| container::Style {
            text_color: Some(Color::from_rgb(0.878, 0.878, 0.878)),
            ..container::Style::default()
        }),

        Space::with_height(10),

        text("Performance").size(16)
            .font(Font {
                family: iced_winit::core::font::Family::Name("Roboto"),
                weight: iced_winit::core::font::Weight::Medium,
                stretch: iced_winit::core::font::Stretch::Normal,
                style: iced_winit::core::font::Style::Normal,
            }),

        container(
            widgets::toggler::Toggler::new(
                Some("BC1 Texture Compression".into()),
                viewer.compression_strategy == CompressionStrategy::Bc1,
                |enabled| {
                    if enabled {
                        Message::SetCompressionStrategy(CompressionStrategy::Bc1)
                    } else {
                        Message::SetCompressionStrategy(CompressionStrategy::None)
                    }
                },
            ).width(Length::Fill)
        ).style(|_theme: &WinitTheme| container::Style {
            text_color: Some(Color::from_rgb(0.878, 0.878, 0.878)),
            ..container::Style::default()
        }),

        container(
            widgets::toggler::Toggler::new(
                Some("GPU Cache (vs CPU)".into()),
                viewer.cache_strategy == CacheStrategy::Gpu,
                |enabled| {
                    if enabled {
                        Message::SetCacheStrategy(CacheStrategy::Gpu)
                    } else {
                        Message::SetCacheStrategy(CacheStrategy::Cpu)
                    }
                },
            ).width(Length::Fill)
        ).style(|_theme: &WinitTheme| container::Style {
            text_color: Some(Color::from_rgb(0.878, 0.878, 0.878)),
            ..container::Style::default()
        }),
    ]
    .spacing(3)
    .width(Length::FillPortion(1));

    // Right column - Controls
    let right_column = column![
        text("Controls").size(16)
            .font(Font {
                family: iced_winit::core::font::Family::Name("Roboto"),
                weight: iced_winit::core::font::Weight::Medium,
                stretch: iced_winit::core::font::Stretch::Normal,
                style: iced_winit::core::font::Style::Normal,
            }),

        container(
            widgets::toggler::Toggler::new(
                Some("Sync Zoom/Pan".into()),
                viewer.synced_zoom,
                Message::ToggleSyncedZoom,
            ).width(Length::Fill)
        ).style(|_theme: &WinitTheme| container::Style {
            text_color: Some(Color::from_rgb(0.878, 0.878, 0.878)),
            ..container::Style::default()
        }),

        container(
            widgets::toggler::Toggler::new(
                Some("Mouse Wheel Zoom".into()),
                viewer.mouse_wheel_zoom,
                Message::ToggleMouseWheelZoom,
            ).width(Length::Fill)
        ).style(|_theme: &WinitTheme| container::Style {
            text_color: Some(Color::from_rgb(0.878, 0.878, 0.878)),
            ..container::Style::default()
        }),

        container(
            widgets::toggler::Toggler::new(
                Some("Dual Slider".into()),
                viewer.is_slider_dual,
                Message::ToggleSliderType,
            ).width(Length::Fill)
        ).style(|_theme: &WinitTheme| container::Style {
            text_color: Some(Color::from_rgb(0.878, 0.878, 0.878)),
            ..container::Style::default()
        }),
    ]
    .spacing(3)
    .width(Length::FillPortion(1));

    scrollable(
        container(
            row![left_column, right_column]
                .spacing(30)
        )
        .padding([5, 10])
    )
    .height(Length::Fill)
    .into()
}

/// Advanced tab content: Config constants (placeholder for now)
fn view_advanced_tab<'a>(_viewer: &'a DataViewer) -> Element<'a, Message, WinitTheme, Renderer> {
    scrollable(
        container(
            column![
                text("Advanced Settings").size(16)
                    .font(Font {
                        family: iced_winit::core::font::Family::Name("Roboto"),
                        weight: iced_winit::core::font::Weight::Medium,
                        stretch: iced_winit::core::font::Stretch::Normal,
                        style: iced_winit::core::font::Style::Normal,
                    }),
                Space::with_height(10),
                text("Coming soon: Cache size, window dimensions, atlas size, etc.").size(14)
                    .style(|theme: &WinitTheme| {
                        iced_widget::text::Style {
                            color: Some(theme.extended_palette().background.weak.color)
                        }
                    }),
            ]
            .spacing(8)
        )
        .padding([5, 10])
    )
    .height(Length::Fill)
    .into()
}
