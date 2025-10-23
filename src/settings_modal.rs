use iced_winit::core::{Element, Length, Alignment, Color};
use iced_winit::core::font::Font;
use iced_widget::{row, column, container, text, button, Space, scrollable, text_input};
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
        .tab_bar_style(|theme: &WinitTheme, status| {
            use iced_aw::style::status::Status;

            // Highlight active tab with a tinted background, show hover feedback
            let tab_bg = match status {
                Status::Active => iced_winit::core::Background::Color(
                    theme.extended_palette().primary.weak.color
                ),
                Status::Hovered => iced_winit::core::Background::Color(
                    theme.extended_palette().background.strong.color
                ),
                _ => iced_winit::core::Background::Color(Color::TRANSPARENT),
            };

            iced_aw::style::tab_bar::Style {
                background: Some(theme.extended_palette().background.weak.color.into()),
                border_color: Some(theme.extended_palette().background.strong.color),
                border_width: 0.0,
                tab_label_background: tab_bg,
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
        // Use red for errors, green for success
        {
            let status_text = viewer.settings_save_status.as_deref().unwrap_or(" ");
            // Catch both "Error:" and "Error parsing" messages
            let is_error = status_text.contains("Error");

            container(
                text(status_text).size(14)
            )
            .style(move |theme: &WinitTheme| container::Style {
                text_color: Some(if is_error {
                    theme.extended_palette().danger.strong.color
                } else {
                    theme.extended_palette().success.strong.color
                }),
                ..container::Style::default()
            })
            .height(Length::Fixed(18.0))
        },

        // Buttons row
        row![
            button(text("Reset to Defaults"))
                .padding([3, 10])
                .on_press(Message::ResetAdvancedSettings),
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
                Some("Show Copy Buttons".into()),
                viewer.show_copy_buttons,
                Message::ToggleCopyButtons,
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

/// Helper function to create a labeled text input row (editable)
fn labeled_text_input_row<'a>(
    label: &'a str,
    field_name: &'a str,
    value: String,
) -> iced_widget::Row<'a, Message, WinitTheme, Renderer> {
    let field_name_owned = field_name.to_string();
    row![
        text(label).size(14).width(Length::Fixed(250.0)),
        text_input("", &value)
            .size(14)
            .width(Length::Fixed(150.0))
            .on_input(move |new_value| {
                Message::AdvancedSettingChanged(field_name_owned.clone(), new_value)
            }),
    ]
    .spacing(10)
    .align_y(Alignment::Center)
}

/// Advanced tab content: Editable config constants
fn view_advanced_tab<'a>(viewer: &'a DataViewer) -> Element<'a, Message, WinitTheme, Renderer> {
    // Helper to get value from HashMap with fallback
    let get_value = |key: &str| -> String {
        viewer.advanced_settings_input
            .get(key)
            .cloned()
            .unwrap_or_default()
    };

    let content = column![
        text("Advanced Settings").size(16)
            .font(Font {
                family: iced_winit::core::font::Family::Name("Roboto"),
                weight: iced_winit::core::font::Weight::Medium,
                stretch: iced_winit::core::font::Stretch::Normal,
                style: iced_winit::core::font::Style::Normal,
            }),
        Space::with_height(5),
        text("Note: Changes take effect after saving and restarting the application.").size(12)
            .style(|theme: &WinitTheme| {
                iced_widget::text::Style {
                    color: Some(theme.extended_palette().background.weak.color)
                }
            }),
        Space::with_height(10),

        // Cache settings
        text("Cache Settings").size(14)
            .font(Font {
                family: iced_winit::core::font::Family::Name("Roboto"),
                weight: iced_winit::core::font::Weight::Medium,
                stretch: iced_winit::core::font::Stretch::Normal,
                style: iced_winit::core::font::Style::Normal,
            }),
        labeled_text_input_row("Cache Size:", "cache_size", get_value("cache_size")),
        labeled_text_input_row("Max Loading Queue Size:", "max_loading_queue_size", get_value("max_loading_queue_size")),
        labeled_text_input_row("Max Being Loaded Queue Size:", "max_being_loaded_queue_size", get_value("max_being_loaded_queue_size")),

        Space::with_height(10),

        // Window settings
        text("Window Settings").size(14)
            .font(Font {
                family: iced_winit::core::font::Family::Name("Roboto"),
                weight: iced_winit::core::font::Weight::Medium,
                stretch: iced_winit::core::font::Stretch::Normal,
                style: iced_winit::core::font::Style::Normal,
            }),
        labeled_text_input_row("Default Window Width (px):", "window_width", get_value("window_width")),
        labeled_text_input_row("Default Window Height (px):", "window_height", get_value("window_height")),
        labeled_text_input_row("Texture Atlas Size:", "atlas_size", get_value("atlas_size")),

        Space::with_height(10),

        // Other settings
        text("Other Settings").size(14)
            .font(Font {
                family: iced_winit::core::font::Family::Name("Roboto"),
                weight: iced_winit::core::font::Weight::Medium,
                stretch: iced_winit::core::font::Stretch::Normal,
                style: iced_winit::core::font::Style::Normal,
            }),
        labeled_text_input_row("Double-Click Threshold (ms):", "double_click_threshold_ms", get_value("double_click_threshold_ms")),
        labeled_text_input_row("Archive Cache Size (MB):", "archive_cache_size", get_value("archive_cache_size")),
        labeled_text_input_row("Archive Warning Threshold (MB):", "archive_warning_threshold_mb", get_value("archive_warning_threshold_mb")),
    ]
    .spacing(3);

    // Center the content with fixed width, scrollbar on right edge
    let centered_content = container(
        container(content)
            .width(Length::Fixed(480.0))  // Fixed width for content
            .padding([5, 10])
    )
    .width(Length::Fill)
    .center_x(Length::Fill);

    scrollable(centered_content)
        .height(Length::Fill)
        .into()
}
