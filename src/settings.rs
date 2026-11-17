use std::path::PathBuf;

use eframe::egui;
use serde::{Deserialize, Serialize};

use crate::menu::toggle_switch;
use crate::theme::UiTheme;

/// Custom slider with accent-colored handle and two-tone rail.
///
/// egui's built-in Slider ties the idle handle color to the rail
/// background (`widgets.inactive.bg_fill`), making it impossible to
/// theme them independently.  This draws everything from scratch.
fn accent_slider(
    ui: &mut egui::Ui,
    value: &mut usize,
    range: std::ops::RangeInclusive<usize>,
    default: usize,
    theme: &UiTheme,
    editable: bool,
) {
    let lo = *range.start();
    let hi = *range.end();

    let slider_width = ui.spacing().slider_width;
    let thickness = ui
        .text_style_height(&egui::TextStyle::Body)
        .max(ui.spacing().interact_size.y);

    // Allocate rail + handle area, then value text to the right.
    let desired = egui::vec2(slider_width, thickness);
    let (rect, response) =
        ui.allocate_exact_size(desired, egui::Sense::click_and_drag());

    // Double-click resets to default.
    if response.double_clicked() {
        *value = default;
    } else if let Some(pos) = response.interact_pointer_pos() {
        // Handle dragging.
        let handle_radius = rect.height() / 2.5;
        let usable = rect.x_range().shrink(handle_radius);
        let t = ((pos.x - usable.min) / (usable.max - usable.min)).clamp(0.0, 1.0);
        *value = lo + ((hi - lo) as f64 * t as f64).round() as usize;
    }

    // Paint.
    let handle_radius = rect.height() / 2.5;
    let rail_radius = 4.0_f32;
    let cy = rect.center().y;
    let rail = egui::Rect::from_min_max(
        egui::pos2(rect.left(), cy - rail_radius),
        egui::pos2(rect.right(), cy + rail_radius),
    );

    let t = if hi > lo {
        (*value - lo) as f32 / (hi - lo) as f32
    } else {
        0.0
    };
    let handle_x = egui::lerp(
        (rect.left() + handle_radius)..=(rect.right() - handle_radius),
        t,
    );

    // Unfilled rail (full width, painted first).
    ui.painter()
        .rect_filled(rail, rail_radius, egui::Color32::from_gray(60));
    // Filled rail (left edge → handle center).
    let filled = egui::Rect::from_min_max(rail.min, egui::pos2(handle_x, rail.max.y));
    ui.painter().rect_filled(filled, rail_radius, theme.accent);
    // Handle circle.
    let center = egui::pos2(handle_x, cy);
    ui.painter().circle(
        center,
        handle_radius,
        theme.accent,
        egui::Stroke::NONE,
    );

    if editable {
        ui.add(egui::DragValue::new(value).range(range));
    } else {
        // Value text to the right.
        let text_rect = egui::Rect::from_min_size(
            egui::pos2(rect.right() + ui.spacing().item_spacing.x, rect.top()),
            egui::vec2(40.0, rect.height()),
        );
        ui.put(
            text_rect,
            egui::Label::new(
                egui::RichText::new(format!("{value}"))
                    .monospace()
                    .color(egui::Color32::from_gray(200)),
            ),
        );
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
enum SettingsTab {
    #[default]
    General,
    Performance,
}

impl SettingsTab {
    const ALL: [Self; 2] = [Self::General, Self::Performance];

    fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Performance => "Performance",
        }
    }
}

fn tab_bar(ui: &mut egui::Ui, active: &mut SettingsTab, theme: &UiTheme) {
    let font = egui::FontId::proportional(14.0);
    let padding = egui::vec2(6.0, 4.0);
    let underline_gap = 3.0;

    ui.horizontal(|ui| {
        for tab in SettingsTab::ALL {
            let is_active = *active == tab;

            let galley = ui.painter().layout_no_wrap(
                tab.label().to_string(),
                font.clone(),
                egui::Color32::PLACEHOLDER,
            );
            let desired = galley.size() + padding * 2.0 + egui::vec2(0.0, underline_gap);
            let (rect, response) =
                ui.allocate_exact_size(desired, egui::Sense::click());

            let color = if is_active {
                theme.accent
            } else if response.hovered() {
                egui::Color32::WHITE
            } else {
                egui::Color32::from_gray(160)
            };

            ui.painter().text(
                rect.min + padding,
                egui::Align2::LEFT_TOP,
                tab.label(),
                font.clone(),
                color,
            );

            if is_active {
                ui.painter().hline(
                    (rect.min.x + padding.x)..=(rect.max.x - padding.x),
                    rect.max.y - 1.0,
                    egui::Stroke::new(2.0_f32, theme.accent),
                );
            }

            if response.clicked() {
                *active = tab;
            }
        }
    });
    ui.add_space(2.0);
}

/// Custom radio row: an accent-colored circle indicator, a primary label with
/// an optional color swatch after it, and a muted description on the next
/// line. Used for the GPU memory mode and the accent presets.
fn radio_row<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    current: &mut T,
    value: T,
    label: &str,
    description: &str,
    swatch: Option<egui::Color32>,
    theme: &UiTheme,
) {
    let selected = *current == value;
    ui.horizontal(|ui| {
        let radius = 7.0_f32;
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(radius * 2.0 + 4.0, radius * 2.0 + 4.0),
            egui::Sense::click(),
        );
        let center = rect.center();
        ui.painter().circle_stroke(
            center,
            radius,
            egui::Stroke::new(1.5_f32, egui::Color32::from_gray(140)),
        );
        if selected {
            ui.painter()
                .circle_filled(center, radius - 3.0, theme.accent);
        }
        if response.clicked() {
            *current = value;
        }

        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                let label_response = ui.add(
                    egui::Label::new(egui::RichText::new(label).size(13.0))
                        .sense(egui::Sense::click()),
                );
                if label_response.clicked() {
                    *current = value;
                }
                if let Some(color) = swatch {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect.shrink(1.0), 3.0, color);
                }
            });
            if !description.is_empty() {
                ui.label(
                    egui::RichText::new(description)
                        .size(11.0)
                        .color(theme.muted),
                );
            }
        });
    });
    ui.add_space(4.0);
}

/// Accent color preset. Only the official build shows the picker; the free
/// build keeps the default teal and ignores the stored value, so a settings
/// file written by either build loads in the other.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccentPreset {
    /// The teal from the iced version and the free build.
    #[default]
    Teal,
    /// Bright cyan from the icon's foreground trails.
    Cyan,
    /// Warm amber, for contrast against photo-heavy screens.
    Amber,
    /// Soft violet.
    Violet,
    /// A color picked in the settings, stored in `AppSettings::custom_accent`.
    Custom,
}

#[cfg(feature = "official")]
impl AccentPreset {
    pub(crate) const ALL: [Self; 5] = [Self::Teal, Self::Cyan, Self::Amber, Self::Violet, Self::Custom];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Teal => "Teal",
            Self::Cyan => "Cyan",
            Self::Amber => "Amber",
            Self::Violet => "Violet",
            Self::Custom => "Custom",
        }
    }

    /// Fixed color of a preset. `Custom` has none; see [`AppSettings::color_of`].
    fn fixed_color(self) -> Option<egui::Color32> {
        let rgb = match self {
            Self::Teal => crate::theme::DEFAULT_ACCENT,
            Self::Cyan => [46, 230, 255],
            Self::Amber => [240, 160, 60],
            Self::Violet => [160, 130, 235],
            Self::Custom => return None,
        };
        Some(crate::theme::rgb(rgb))
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GpuMemoryMode {
    /// gpu_allocator default (~256 MB blocks). Highest navigation speed,
    /// largest GPU memory footprint.
    Performance,
    /// 64 MB device / 32 MB host blocks. Recommended balance.
    #[default]
    Balanced,
    /// 8 MB device / 4 MB host blocks. Lowest GPU memory, but a 4K texture
    /// no longer fits in a single block — degrades navigation performance.
    LowMemory,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageSortKey {
    #[default]
    Name,
    Modified,
    Created,
    Size,
    Extension,
}

impl ImageSortKey {
    pub(crate) const ALL: [Self; 5] = [
        Self::Name,
        Self::Modified,
        Self::Created,
        Self::Size,
        Self::Extension,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::Modified => "Modified Date",
            Self::Created => "Created Date",
            Self::Size => "File Size",
            Self::Extension => "Extension",
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    #[default]
    Ascending,
    Descending,
}

impl SortDirection {
    pub(crate) const ALL: [Self; 2] = [Self::Ascending, Self::Descending];

    pub fn label(self) -> &'static str {
        match self {
            Self::Ascending => "Ascending",
            Self::Descending => "Descending",
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageDiscoveryOptions {
    pub sort_order: ImageSortOrder,
    pub include_hidden: bool,
    pub recursive: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageSortOrder {
    pub key: ImageSortKey,
    pub direction: SortDirection,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub show_footer: bool,
    /// Action buttons at the right end of the footer (rotate and Move to Trash).
    /// Off for people who use the viewer as a plain reader.
    pub show_footer_buttons: bool,
    pub show_fps: bool,
    pub show_cache_overlay: bool,
    pub sync_zoom_pan: bool,
    pub cache_count: usize,
    pub lru_budget_mb: usize,
    pub decode_threads: usize,
    pub gpu_memory_mode: GpuMemoryMode,
    pub mouse_wheel_zoom: bool,
    pub reset_zoom_pan_on_navigation: bool,
    pub slider_preview: bool,
    pub preview_budget_mb: usize,
    pub image_discovery_options: ImageDiscoveryOptions,
    pub accent_preset: AccentPreset,
    /// Accent used when `accent_preset` is `Custom`, as sRGB bytes.
    pub custom_accent: [u8; 3],
    /// The metadata side panel (the I key). Off until someone turns it on.
    pub show_metadata_panel: bool,
    /// Width of the metadata panel in points, stored when a resize drag
    /// ends.
    pub metadata_panel_width: f32,
    /// Whether the panel's All EXIF list is expanded.
    pub metadata_all_exif_open: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            show_footer: true,
            show_footer_buttons: true,
            show_fps: true,
            show_cache_overlay: false,
            sync_zoom_pan: true,
            cache_count: 5,
            lru_budget_mb: 1024,
            decode_threads: 10,
            gpu_memory_mode: GpuMemoryMode::default(),
            mouse_wheel_zoom: false,
            reset_zoom_pan_on_navigation: true,
            slider_preview: true,
            preview_budget_mb: 200,
            image_discovery_options: ImageDiscoveryOptions::default(),
            accent_preset: AccentPreset::default(),
            custom_accent: crate::theme::DEFAULT_ACCENT,
            show_metadata_panel: false,
            metadata_panel_width: crate::metadata_panel::DEFAULT_WIDTH,
            metadata_all_exif_open: false,
        }
    }
}

#[cfg(feature = "official")]
impl AppSettings {
    /// The color a preset resolves to, using `custom_accent` for `Custom`.
    pub(crate) fn color_of(&self, preset: AccentPreset) -> egui::Color32 {
        preset
            .fixed_color()
            .unwrap_or_else(|| crate::theme::rgb(self.custom_accent))
    }

    /// The accent color the UI should use for the current preset.
    pub(crate) fn accent_color(&self) -> egui::Color32 {
        self.color_of(self.accent_preset)
    }
}

#[derive(Default)]
pub struct SettingsChanges {
    pub pane_settings: bool,
}

impl SettingsChanges {
    pub(crate) fn between(before: &AppSettings, after: &AppSettings) -> Self {
        Self {
            pane_settings: after.cache_count != before.cache_count
                || after.lru_budget_mb != before.lru_budget_mb
                || after.decode_threads != before.decode_threads
                || after.mouse_wheel_zoom != before.mouse_wheel_zoom
                || after.reset_zoom_pan_on_navigation != before.reset_zoom_pan_on_navigation
                || after.preview_budget_mb != before.preview_budget_mb
                || after.image_discovery_options != before.image_discovery_options
        }
    }
}

impl AppSettings {
    fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("viewskater-egui").join("settings.yaml"))
    }

    pub fn load() -> Self {
        let settings = Self::config_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_yaml::from_str(&s).ok())
            .unwrap_or_default();
        log::debug!("Loaded settings: {:?}", settings);
        settings
    }

    pub fn save(&self) {
        if let Some(path) = Self::config_path() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            match serde_yaml::to_string(self) {
                Ok(yaml) => {
                    if let Err(e) = std::fs::write(&path, yaml) {
                        log::error!("Failed to save settings to {}: {}", path.display(), e);
                    } else {
                        log::debug!("Settings saved to {}", path.display());
                    }
                }
                Err(e) => log::error!("Failed to serialize settings: {}", e),
            }
        }
    }
}

/// Show the settings modal and report which settings changed.
pub fn show_settings_modal(
    ctx: &egui::Context,
    settings: &mut AppSettings,
    show: &mut bool,
    theme: &UiTheme,
) -> SettingsChanges {
    if !*show {
        return SettingsChanges::default();
    }

    // Snapshot at start of frame; if anything changes we save immediately
    // and stamp the save time so the "Saved" indicator can fade in.
    let snapshot = settings.clone();

    let saved_at_id = egui::Id::new("settings_saved_at");
    let now = ctx.input(|i| i.time);

    // Semi-transparent backdrop
    let screen = ctx.screen_rect();
    egui::Area::new(egui::Id::new("settings_backdrop"))
        .fixed_pos(screen.min)
        .order(egui::Order::Middle)
        .show(ctx, |ui| {
            let response = ui.allocate_response(screen.size(), egui::Sense::click());
            ui.painter().rect_filled(screen, 0.0, theme.backdrop);
            if response.clicked() {
                *show = false;
            }
        });

    let max_modal_height = (screen.height() * 0.75).clamp(200.0, 600.0);

    // Modal card
    egui::Area::new(egui::Id::new("settings_modal"))
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::default()
                .fill(theme.card_bg)
                .stroke(egui::Stroke::new(1.0_f32, theme.card_stroke))
                .corner_radius(8.0)
                .inner_margin(20.0)
                .show(ui, |ui| {
                    ui.set_width(360.0);
                    ui.set_max_height(max_modal_height);

                    // Title (outside the scroll area so it stays pinned)
                    ui.label(egui::RichText::new("Preferences").size(20.0).strong());
                    ui.separator();
                    ui.add_space(4.0);

                    let tab_id = egui::Id::new("settings_active_tab");
                    let mut active_tab: SettingsTab =
                        ctx.data(|d| d.get_temp(tab_id)).unwrap_or_default();
                    tab_bar(ui, &mut active_tab, theme);
                    ui.separator();
                    ui.add_space(4.0);

                    let max_h_id = egui::Id::new("settings_max_tab_h");
                    let mut target_h: f32 =
                        ctx.data(|d| d.get_temp(max_h_id)).unwrap_or(0.0);

                    if target_h == 0.0 {
                        for tab in SettingsTab::ALL {
                            let rect = egui::Rect::from_min_size(
                                ui.cursor().min,
                                egui::vec2(ui.available_width(), 10000.0),
                            );
                            #[allow(deprecated)]
                            let mut child = ui.child_ui_with_id_source(
                                rect,
                                *ui.layout(),
                                ("settings_measure", tab as u8),
                                None,
                            );
                            child.set_invisible();
                            let mut tmp = settings.clone();
                            match tab {
                                SettingsTab::General => render_general_tab(&mut child, &mut tmp, theme),
                                SettingsTab::Performance => render_performance_tab(&mut child, &mut tmp, theme),
                            }
                            target_h = target_h.max(child.min_rect().height());
                        }
                        ctx.data_mut(|d| d.insert_temp(max_h_id, target_h));
                    }

                    egui::ScrollArea::vertical()
                        .id_salt(egui::Id::new("settings_scroll").with(active_tab))
                        .max_height(target_h)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            match active_tab {
                                SettingsTab::General => {
                                    render_general_tab(ui, settings, theme);
                                }
                                SettingsTab::Performance => {
                                    render_performance_tab(ui, settings, theme);
                                }
                            }
                        });

                    ctx.data_mut(|d| d.insert_temp(tab_id, active_tab));

                    // "Saved" indicator pinned below the scroll area
                    let saved_at: Option<f64> = ctx.data(|d| d.get_temp(saved_at_id));
                    if let Some(t) = saved_at {
                        let elapsed = now - t;
                        if elapsed < 2.0 {
                            let alpha = ((1.0 - elapsed / 2.0) as f32).clamp(0.0, 1.0);
                            let green = egui::Color32::from_rgba_unmultiplied(
                                120, 220, 120,
                                (alpha * 255.0) as u8,
                            );
                            let card_rect = ui.min_rect();
                            ui.painter().text(
                                egui::pos2(card_rect.right() - 10.0, card_rect.bottom() - 6.0),
                                egui::Align2::RIGHT_BOTTOM,
                                "✔ Saved",
                                egui::FontId::proportional(11.0),
                                green,
                            );
                            ctx.request_repaint();
                        }
                    }
                });
        });

    // Escape to close
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        *show = false;
    }

    // Auto-save on any change inside the modal and stamp the save time so
    // the green "Saved" indicator can show.
    if *settings != snapshot {
        settings.save();
        ctx.data_mut(|d| d.insert_temp(saved_at_id, now));
    }

    SettingsChanges::between(&snapshot, settings)
}

fn section(
    ui: &mut egui::Ui,
    heading: &str,
    subtitle: Option<&str>,
    theme: &UiTheme,
    content: impl FnOnce(&mut egui::Ui),
) {
    ui.label(
        egui::RichText::new(heading)
            .size(14.0)
            .color(theme.heading),
    );
    if let Some(sub) = subtitle {
        ui.label(
            egui::RichText::new(sub)
                .size(11.0)
                .color(theme.muted),
        );
    }
    ui.add_space(4.0);
    egui::Frame::default()
        .fill(theme.section_bg)
        .corner_radius(6.0)
        .inner_margin(10.0)
        .show(ui, content);
}

fn render_general_tab(ui: &mut egui::Ui, settings: &mut AppSettings, theme: &UiTheme) {
    section(ui, "Control", None, theme, |ui| {
        ui.horizontal(|ui| {
            toggle_switch(ui, &mut settings.mouse_wheel_zoom, "Mouse Wheel Zoom", theme);
        });
    });

    ui.add_space(12.0);

    // Files section
    section(ui, "Files", Some("Default sorting for newly opened folders"), theme, |ui| {
        ui.horizontal(|ui| {
            ui.label("Sort By");
            egui::ComboBox::from_id_salt("image_sort_order_key")
                .selected_text(settings.image_discovery_options.sort_order.key.label())
                .show_ui(ui, |ui| {
                    for sort_key in ImageSortKey::ALL {
                        ui.selectable_value(
                            &mut settings.image_discovery_options.sort_order.key,
                            sort_key,
                            sort_key.label(),
                        );
                    }
                });
        });

        ui.horizontal(|ui| {
            ui.label("Direction");
            egui::ComboBox::from_id_salt("image_sort_order_direction")
                .selected_text(settings.image_discovery_options.sort_order.direction.label())
                .show_ui(ui, |ui| {
                    for direction in SortDirection::ALL {
                        ui.selectable_value(
                            &mut settings.image_discovery_options.sort_order.direction,
                            direction,
                            direction.label(),
                        );
                    }
                });
        });
    });

    ui.add_space(4.0);
    ui.label(
        egui::RichText::new("Control which files are loaded")
            .size(11.0)
            .color(theme.muted),
    );
    ui.add_space(4.0);
    egui::Frame::default()
        .fill(theme.section_bg)
        .corner_radius(6.0)
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                toggle_switch(ui, &mut settings.image_discovery_options.recursive, "Discover recursively", theme);
            });
            ui.horizontal(|ui| {
                toggle_switch(ui, &mut settings.image_discovery_options.include_hidden, "Include hidden files (& directories)", theme);
            });
        });

    ui.add_space(12.0);

    section(ui, "Display", None, theme, |ui| {
        ui.horizontal(|ui| {
            toggle_switch(ui, &mut settings.show_footer, "Footer", theme);
        });
        ui.horizontal(|ui| {
            toggle_switch(ui, &mut settings.show_footer_buttons, "Footer Buttons", theme);
        });
        ui.horizontal(|ui| {
            toggle_switch(ui, &mut settings.show_metadata_panel, "Metadata Panel", theme);
        });
        ui.horizontal(|ui| {
            toggle_switch(ui, &mut settings.show_fps, "FPS Overlay", theme);
        });
        ui.horizontal(|ui| {
            toggle_switch(ui, &mut settings.show_cache_overlay, "Cache Overlay", theme);
        });
        ui.horizontal(|ui| {
            toggle_switch(ui, &mut settings.sync_zoom_pan, "Sync Zoom/Pan", theme);
        });
        ui.horizontal(|ui| {
            toggle_switch(
                ui,
                &mut settings.reset_zoom_pan_on_navigation,
                "Reset Zoom/Pan on Navigation",
                theme,
            );
        });
        ui.horizontal(|ui| {
            toggle_switch(ui, &mut settings.slider_preview, "Slider Preview", theme);
        });
    });

    #[cfg(feature = "official")]
    {
        ui.add_space(12.0);
        section(ui, "Theme", None, theme, |ui| {
            ui.label(
                egui::RichText::new("Accent Color")
                    .size(12.0)
                    .color(theme.muted),
            );
            ui.add_space(4.0);
            for preset in AccentPreset::ALL {
                let swatch = settings.color_of(preset);
                radio_row(
                    ui,
                    &mut settings.accent_preset,
                    preset,
                    preset.label(),
                    "",
                    Some(swatch),
                    theme,
                );
            }
            if settings.accent_preset == AccentPreset::Custom {
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.add_space(26.0);
                    ui.label(egui::RichText::new("Pick a color").size(12.0).color(theme.muted));
                    ui.color_edit_button_srgb(&mut settings.custom_accent);
                });
            }
        });
    }

    ui.add_space(10.0);
}

fn render_performance_tab(ui: &mut egui::Ui, settings: &mut AppSettings, theme: &UiTheme) {
    section(ui, "Graphics", None, theme, |ui| {
        ui.label(
            egui::RichText::new("GPU Memory Mode")
                .size(12.0)
                .color(theme.muted),
        );
        ui.add_space(4.0);
        radio_row(
            ui,
            &mut settings.gpu_memory_mode,
            GpuMemoryMode::Performance,
            "Performance",
            "Highest nav speed, largest GPU memory",
            None,
            theme,
        );
        radio_row(
            ui,
            &mut settings.gpu_memory_mode,
            GpuMemoryMode::Balanced,
            "Balanced",
            "Recommended for most users",
            None,
            theme,
        );
        radio_row(
            ui,
            &mut settings.gpu_memory_mode,
            GpuMemoryMode::LowMemory,
            "Low Memory",
            "Lowest GPU memory, slower navigation",
            None,
            theme,
        );
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("⚠ Restart required to apply")
                .size(11.0)
                .color(theme.muted),
        );
    });

    ui.add_space(12.0);

    section(ui, "Performance", Some("Double-click to reset"), theme, |ui| {
        let defaults = AppSettings::default();

        ui.horizontal(|ui| {
            ui.label("Cache Size");
            accent_slider(ui, &mut settings.cache_count, 1..=20, defaults.cache_count, theme, false);
        });
        ui.label(
            egui::RichText::new("Images prefetched in each direction. Higher = smoother keyboard nav, more GPU memory.")
                .size(11.0)
                .color(theme.muted),
        );
        ui.add_space(6.0);

        ui.horizontal(|ui| {
            ui.label("LRU Budget (MB)");
            accent_slider(ui, &mut settings.lru_budget_mb, 128..=4096, defaults.lru_budget_mb, theme, true);
        });
        ui.label(
            egui::RichText::new("GPU memory for caching slider-visited images. Higher = faster revisits, more VRAM.")
                .size(11.0)
                .color(theme.muted),
        );
        ui.add_space(6.0);

        ui.horizontal(|ui| {
            ui.label("Decode Threads");
            accent_slider(ui, &mut settings.decode_threads, 1..=16, defaults.decode_threads, theme, false);
        });
        ui.label(
            egui::RichText::new("Concurrent image decodes. Higher = faster cache fill, larger memory spikes.")
                .size(11.0)
                .color(theme.muted),
        );

        if settings.slider_preview {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("Slider Preview Budget (MB)");
                accent_slider(ui, &mut settings.preview_budget_mb, 0..=1024, defaults.preview_budget_mb, theme, true);
            });
            ui.label(
                egui::RichText::new("Default: 200MB (~420 thumbnails). Set to 0 for no limit.")
                    .size(11.0)
                    .color(theme.muted),
            );
        }
    });

    ui.add_space(10.0);
}
