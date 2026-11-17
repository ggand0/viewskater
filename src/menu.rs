use eframe::egui;

use crate::app::DualPaneMode;
use crate::pane::Pane;
use crate::settings::{AppSettings, ImageSortKey, ImageSortOrder, SortDirection};
use crate::theme::UiTheme;

/// Toggle switch widget (iOS/Steam style).
/// Returns true if the value changed.
pub(crate) fn toggle_switch(ui: &mut egui::Ui, on: &mut bool, label: &str, theme: &UiTheme) -> bool {
    let desired_size = egui::vec2(32.0, 16.0);
    let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click());
    if response.clicked() {
        *on = !*on;
    }

    let how_on = ui.ctx().animate_bool_with_time(response.id, *on, 0.15);
    let radius = rect.height() / 2.0;
    let bg = if *on {
        theme.accent
    } else {
        theme.toggle_off
    };
    let knob_color = theme.toggle_knob;

    ui.painter().rect_filled(rect, radius, bg);

    let knob_x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), how_on);
    let knob_center = egui::pos2(knob_x, rect.center().y);
    ui.painter()
        .circle_filled(knob_center, radius - 2.0, knob_color);

    ui.label(label);

    response.clicked()
}

/// Disable egui's built-in button hover/active/open backgrounds inside
/// a menu popup (so `hover_row` can draw its own full-width highlight),
/// and return the full popup rect coordinates including margins.
fn setup_menu_hover(ui: &mut egui::Ui) -> (f32, f32) {
    let style = ui.style_mut();
    style.visuals.widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
    style.visuals.widgets.hovered.weak_bg_fill = egui::Color32::TRANSPARENT;
    style.visuals.widgets.hovered.bg_stroke = egui::Stroke::NONE;
    style.visuals.widgets.active.weak_bg_fill = egui::Color32::TRANSPARENT;
    style.visuals.widgets.active.bg_stroke = egui::Stroke::NONE;
    style.visuals.widgets.open.weak_bg_fill = egui::Color32::TRANSPARENT;
    style.visuals.widgets.open.bg_stroke = egui::Stroke::NONE;
    let margin = style.spacing.menu_margin;
    let ml = margin.left as f32;
    let mr = margin.right as f32;
    (
        ui.cursor().min.x - ml,
        ui.available_width() + ml + mr,
    )
}

/// Full-width menu row with hover background highlight.
///
/// Uses a Noop shape placeholder so the background is painted behind
/// content that's drawn afterwards. The highlight rect spans the full
/// popup width (including margins) so it reaches the popup border.
/// Hover is detected via raw pointer position to avoid stealing
/// interaction from child widgets (e.g. sub-menu buttons).
fn hover_row(
    ui: &mut egui::Ui,
    theme: &UiTheme,
    menu_left: f32,
    menu_width: f32,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let bg_idx = ui.painter().add(egui::Shape::Noop);
    let row = ui.scope(|ui| {
        add_contents(ui);
    });
    let highlight = egui::Rect::from_min_size(
        egui::pos2(menu_left, row.response.rect.min.y),
        egui::vec2(menu_width, row.response.rect.height()),
    );
    let hovered = ui
        .ctx()
        .input(|i| i.pointer.hover_pos().is_some_and(|p| highlight.contains(p)));
    if hovered {
        ui.painter().set(
            bg_idx,
            egui::Shape::rect_filled(highlight, 0.0, theme.menu_hover),
        );
    }
}

pub(crate) struct MenuBarState<'a> {
    pub settings: &'a mut AppSettings,
    pub current_sort: &'a mut ImageSortOrder,
    pub is_fullscreen: bool,
}

/// Returns (MenuAction, menu_is_open) so fullscreen mode can keep the bar visible
/// while the user interacts with a dropdown.
pub(crate) fn show_menu_bar(
    ctx: &egui::Context,
    panes: &[Pane],
    dual_pane_mode: DualPaneMode,
    state: &mut MenuBarState<'_>,
    theme: &UiTheme,
    fps_text: Option<&str>,
) -> (MenuAction, bool) {
    let settings = &mut *state.settings;
    let current_sort = &mut *state.current_sort;
    let is_fullscreen = state.is_fullscreen;
    let mut action = MenuAction::None;
    let mut menu_is_open = false;
    let is_dual = panes.len() >= 2;
    let has_images = panes.first().is_some_and(|p| !p.image_paths.is_empty());

    egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
        let total_w = ui.available_width();
        // Ultra-narrow: hide everything
        if total_w < 25.0 {
            return;
        }
        let file_label = if total_w < 50.0 { "F" } else { "File" };
        let menu_unit = 50.0;
        let show_edit = total_w >= menu_unit * 2.0;
        let show_view = total_w >= menu_unit * 3.0;
        let show_help = total_w >= menu_unit * 4.0;

        let bar_id = egui::menu::bar(ui, |ui| {
            let bar_id = ui.id();
            ui.menu_button(file_label, |ui| {
                let (ml, mw) = setup_menu_hover(ui);
                hover_row(ui, theme, ml, mw, |ui| {
                    ui.menu_button("Open Folder  Ctrl+Shift+O", |ui| {
                        let (sl, sw) = setup_menu_hover(ui);
                        hover_row(ui, theme, sl, sw, |ui| {
                            if ui.button("Pane 1").clicked() {
                                action = MenuAction::OpenFolder(0);
                                ui.close_menu();
                            }
                        });
                        if is_dual {
                            hover_row(ui, theme, sl, sw, |ui| {
                                if ui.button("Pane 2").clicked() {
                                    action = MenuAction::OpenFolder(1);
                                    ui.close_menu();
                                }
                            });
                        }
                    });
                });
                hover_row(ui, theme, ml, mw, |ui| {
                    ui.menu_button("Open File  Ctrl+O", |ui| {
                        let (sl, sw) = setup_menu_hover(ui);
                        hover_row(ui, theme, sl, sw, |ui| {
                            if ui.button("Pane 1").clicked() {
                                action = MenuAction::OpenFile(0);
                                ui.close_menu();
                            }
                        });
                        if is_dual {
                            hover_row(ui, theme, sl, sw, |ui| {
                                if ui.button("Pane 2").clicked() {
                                    action = MenuAction::OpenFile(1);
                                    ui.close_menu();
                                }
                            });
                        }
                    });
                });
                ui.separator();
                hover_row(ui, theme, ml, mw, |ui| {
                    if ui
                        .add_enabled(has_images, egui::Button::new("Close  Ctrl+W"))
                        .clicked()
                    {
                        action = MenuAction::Close;
                        ui.close_menu();
                    }
                });
                hover_row(ui, theme, ml, mw, |ui| {
                    if ui.button("Quit  Ctrl+Q").clicked() {
                        action = MenuAction::Quit;
                        ui.close_menu();
                    }
                });
            });

            if show_edit {
                ui.menu_button("Edit", |ui| {
                    let (ml, mw) = setup_menu_hover(ui);
                    let trash_label = if cfg!(target_os = "macos") {
                        "Move to Trash  Del / Cmd+Backspace"
                    } else {
                        "Move to Trash  Del"
                    };
                    hover_row(ui, theme, ml, mw, |ui| {
                        if ui
                            .add_enabled(has_images, egui::Button::new(trash_label))
                            .clicked()
                        {
                            action = MenuAction::MoveToTrash;
                            ui.close_menu();
                        }
                    });
                    ui.separator();
                    hover_row(ui, theme, ml, mw, |ui| {
                        if ui.button("Preferences").clicked() {
                            action = MenuAction::ShowSettings;
                            ui.close_menu();
                        }
                    });
                });
            }

            if show_view {
                ui.menu_button("View", |ui| {
                let (ml, mw) = setup_menu_hover(ui);
                let pane_count = panes.len();
                let is_single = pane_count == 1;
                let is_synced = pane_count >= 2 && dual_pane_mode == DualPaneMode::Synced;
                let is_independent =
                    pane_count >= 2 && dual_pane_mode == DualPaneMode::Independent;
                hover_row(ui, theme, ml, mw, |ui| {
                    if ui.radio(is_single, "Single Pane  Ctrl+1").clicked() {
                        if !is_single {
                            action = MenuAction::SetSinglePane;
                        }
                        ui.close_menu();
                    }
                });
                hover_row(ui, theme, ml, mw, |ui| {
                    if ui.radio(is_synced, "Dual Pane (Synced)  Ctrl+2").clicked() {
                        if !is_synced {
                            action = MenuAction::SetDualPane;
                        }
                        ui.close_menu();
                    }
                });
                hover_row(ui, theme, ml, mw, |ui| {
                    if ui
                        .radio(is_independent, "Dual Pane (Independent)  Ctrl+3")
                        .clicked()
                    {
                        if !is_independent {
                            action = MenuAction::SetDualPaneIndependent;
                        }
                        ui.close_menu();
                    }
                });
                ui.separator();
                hover_row(ui, theme, ml, mw, |ui| {
                    ui.menu_button("Sort By", |ui| {
                        let (sl, sw) = setup_menu_hover(ui);
                        ui.style_mut().visuals.widgets.active.bg_fill = egui::Color32::TRANSPARENT;
                        for sort_key in ImageSortKey::ALL {
                            hover_row(ui, theme, sl, sw, |ui| {
                                ui.radio_value(&mut current_sort.key, sort_key, sort_key.label());
                            });
                        }
                        ui.separator();
                        for direction in SortDirection::ALL {
                            hover_row(ui, theme, sl, sw, |ui| {
                                ui.radio_value(
                                    &mut current_sort.direction,
                                    direction,
                                    direction.label(),
                                );
                            });
                        }
                        if *current_sort != settings.image_discovery_options.sort_order {
                            ui.separator();
                            hover_row(ui, theme, sl, sw, |ui| {
                                if ui.button("Reset to Default").clicked() {
                                    *current_sort = settings.image_discovery_options.sort_order;
                                }
                            });
                        }
                    });
                });
                ui.separator();
                hover_row(ui, theme, ml, mw, |ui| {
                    if ui.button("Rotate Left  Shift+R").clicked() {
                        action = MenuAction::RotateLeft;
                        ui.close_menu();
                    }
                });
                hover_row(ui, theme, ml, mw, |ui| {
                    if ui.button("Rotate Right  R").clicked() {
                        action = MenuAction::RotateRight;
                        ui.close_menu();
                    }
                });
                ui.separator();
                hover_row(ui, theme, ml, mw, |ui| {
                    if ui.button("Reset Zoom/Pan").clicked() {
                        action = MenuAction::ResetZoom;
                        ui.close_menu();
                    }
                });
                hover_row(ui, theme, ml, mw, |ui| {
                    let label = if is_fullscreen {
                        "Exit Fullscreen  F11"
                    } else {
                        "Fullscreen  F11"
                    };
                    if ui.button(label).clicked() {
                        action = MenuAction::ToggleFullscreen;
                        ui.close_menu();
                    }
                });
                ui.separator();
                hover_row(ui, theme, ml, mw, |ui| {
                    ui.horizontal(|ui| {
                        toggle_switch(ui, &mut settings.show_footer, "Footer  Tab", theme);
                    });
                });
                hover_row(ui, theme, ml, mw, |ui| {
                    ui.horizontal(|ui| {
                        toggle_switch(
                            ui,
                            &mut settings.show_metadata_panel,
                            "Metadata Panel  I",
                            theme,
                        );
                    });
                });
                hover_row(ui, theme, ml, mw, |ui| {
                    ui.horizontal(|ui| {
                        toggle_switch(ui, &mut settings.show_fps, "FPS Overlay", theme);
                    });
                });
                hover_row(ui, theme, ml, mw, |ui| {
                    ui.horizontal(|ui| {
                        toggle_switch(
                            ui,
                            &mut settings.show_cache_overlay,
                            "Cache Overlay",
                            theme,
                        );
                    });
                });
            });
            }

            if show_help {
                ui.menu_button("Help", |ui| {
                    let (ml, mw) = setup_menu_hover(ui);
                    hover_row(ui, theme, ml, mw, |ui| {
                        if ui.button("Show Logs").clicked() {
                            action = MenuAction::ShowLogs;
                            ui.close_menu();
                        }
                    });
                    hover_row(ui, theme, ml, mw, |ui| {
                        if ui.button("Export debug logs").clicked() {
                            action = MenuAction::ExportDebugLogs;
                            ui.close_menu();
                        }
                    });
                    ui.separator();
                    hover_row(ui, theme, ml, mw, |ui| {
                        if ui.button("About").clicked() {
                            action = MenuAction::ShowAbout;
                            ui.close_menu();
                        }
                    });
                });
            }

            // FPS display (right-aligned, only if space remains after menus)
            if let Some(fps) = fps_text {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let remaining = ui.available_width();
                    let font = egui::FontId::monospace(12.0);
                    let fps_color = egui::Color32::from_gray(220);
                    let fps_w = ui.fonts(|f| {
                        f.layout_no_wrap(fps.into(), font, fps_color).size().x
                    });
                    if remaining >= fps_w + 8.0 {
                        ui.label(
                            egui::RichText::new(fps)
                                .monospace()
                                .color(fps_color)
                                .size(12.0),
                        );
                    }
                });
            }
            bar_id
        }).inner;
        menu_is_open = egui::menu::BarState::load(ctx, bar_id).is_some();
    });

    (action, menu_is_open)
}

/// The footer action clicked this frame, for the pane at the given index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FooterAction {
    RotateLeft(usize),
    RotateRight(usize),
    Trash(usize),
}

/// Paint the footer. Returns the action clicked this frame, if any.
pub(crate) fn show_footer(
    ctx: &egui::Context,
    panes: &[Pane],
    divider_fraction: f32,
    show_buttons: bool,
    theme: &UiTheme,
) -> Option<FooterAction> {
    let mut action = None;
    egui::TopBottomPanel::bottom("footer").show(ctx, |ui| {
        if panes.len() >= 2 {
            let available = ui.available_rect_before_wrap();
            let divider_w = 4.0;
            let left_w = (available.width() - divider_w) * divider_fraction;
            let right_x = available.min.x + left_w + divider_w;
            let right_w = available.width() - left_w - divider_w;

            let pad = 4.0;
            let left_rect = egui::Rect::from_min_size(
                available.min,
                egui::vec2(left_w - pad, available.height()),
            );
            let right_rect = egui::Rect::from_min_size(
                egui::pos2(right_x + pad, available.min.y),
                egui::vec2(right_w - pad, available.height()),
            );

            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(left_rect), |ui| {
                action = paint_pane_footer(ui, &panes[0], 0, show_buttons, theme);
            });
            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(right_rect), |ui| {
                if let Some(clicked) = paint_pane_footer(ui, &panes[1], 1, show_buttons, theme) {
                    action = Some(clicked);
                }
            });

            // Divider line matching the central panel
            let divider_center_x = available.min.x + left_w + divider_w / 2.0;
            ui.painter().vline(
                divider_center_x,
                available.y_range(),
                egui::Stroke::new(divider_w, egui::Color32::from_gray(60)),
            );
        } else {
            ui.horizontal(|ui| {
                if let Some(pane) = panes.first() {
                    action = paint_pane_footer(ui, pane, 0, show_buttons, theme);
                }
            });
        }
    });
    action
}

/// Width reserved for the rotate-left, rotate-right and trash buttons,
/// including the gaps before the counter and between buttons.
const FOOTER_BUTTONS_W: f32 = 88.0;

/// Paint one pane's footer. Returns the action clicked in this pane, if any.
fn paint_pane_footer(
    ui: &mut egui::Ui,
    pane: &Pane,
    pane_idx: usize,
    show_buttons: bool,
    theme: &UiTheme,
) -> Option<FooterAction> {
    let mut action = None;
    ui.horizontal(|ui| {
        let Some(path) = pane.image_paths.get(pane.current_index) else {
            return;
        };

        let font = egui::FontId::monospace(13.0);
        let bright = egui::Color32::from_gray(200);
        let dim = egui::Color32::from_gray(160);
        let sep_w = 20.0; // approximate separator + spacing width

        // Prepare text elements
        let index_text = format!("{} / {}", pane.current_index + 1, pane.image_paths.len());
        let filename = path.as_os_str().to_string_lossy();
        // Only show file names relative to panes' top level dir
        let filename = match &pane.dir_path {
            Some(dirpath) => {
                match dirpath.to_str() {
                    Some(dirpath) => {
                        filename
                            .strip_prefix(dirpath).unwrap_or(&filename)
                            // Also remove potentially left-over leading /
                            .trim_start_matches('/')
                            .to_string()
                    }
                    None => filename.to_string()
                }
            }
            None => filename.to_string(),
        };
        let resolution = pane
            .current_texture
            .as_ref()
            .map(|tex| format!("{}x{}", tex.size()[0], tex.size()[1]));
        let file_size = pane
            .current_record
            .as_ref()
            .and_then(|record| record.file_size)
            .map(format_file_size);

        // Measure all widths upfront to decide what fits
        let total = ui.available_width();
        let margin = 16.0;
        let measure = |ui: &egui::Ui, text: &str| -> f32 {
            ui.fonts(|f| f.layout_no_wrap(text.into(), font.clone(), bright).size().x)
        };

        let index_w = measure(ui, &index_text);
        let short_index = format!("{}", pane.current_index + 1);
        let short_index_w = measure(ui, &short_index);
        let filename_w = measure(ui, &filename);
        let res_w = resolution.as_ref().map_or(0.0, |r| measure(ui, r) + sep_w);
        let size_w = file_size.as_ref().map_or(0.0, |s| measure(ui, s) + sep_w);
        let button_w = if show_buttons { FOOTER_BUTTONS_W } else { 0.0 };

        let remaining = total - index_w - button_w - margin;
        let show_filename = remaining >= filename_w;
        let show_res = show_filename && remaining >= filename_w + res_w;
        let show_size = show_res && remaining >= filename_w + res_w + size_w;

        // Render visible elements (priority: index > filename > resolution > file size)
        if show_filename {
            ui.label(egui::RichText::new(&filename).monospace().color(bright).size(13.0));
        }
        if show_res {
            ui.separator();
            ui.label(
                egui::RichText::new(resolution.as_deref().unwrap_or(""))
                    .monospace()
                    .color(dim)
                    .size(13.0),
            );
        }
        if show_size {
            ui.separator();
            ui.label(
                egui::RichText::new(file_size.as_deref().unwrap_or(""))
                    .monospace()
                    .color(dim)
                    .size(13.0),
            );
        }

        // Right end: the counter stays in the corner, then the trash and
        // rotate buttons sit to its left (the iced footer put the copy
        // buttons there too). The counter is shortened before the buttons
        // are dropped.
        if !pane.image_paths.is_empty() {
            let used = ui.min_rect().width();
            let space = total - used - margin;

            ui.with_layout(
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    let counter_w = if space >= index_w + button_w {
                        ui.label(
                            egui::RichText::new(&index_text).monospace().color(bright).size(13.0),
                        );
                        index_w
                    } else if space >= short_index_w + button_w {
                        ui.label(
                            egui::RichText::new(&short_index).monospace().color(bright).size(13.0),
                        );
                        short_index_w
                    } else if space >= index_w {
                        ui.label(
                            egui::RichText::new(&index_text).monospace().color(bright).size(13.0),
                        );
                        return;
                    } else if space >= short_index_w {
                        ui.label(
                            egui::RichText::new(&short_index).monospace().color(bright).size(13.0),
                        );
                        return;
                    } else {
                        return;
                    };
                    if show_buttons && space - counter_w >= FOOTER_BUTTONS_W {
                        ui.add_space(4.0);
                        if trash_button(ui, theme).clicked() {
                            action = Some(FooterAction::Trash(pane_idx));
                        }
                        if rotate_button(ui, theme, true).clicked() {
                            action = Some(FooterAction::RotateRight(pane_idx));
                        }
                        if rotate_button(ui, theme, false).clicked() {
                            action = Some(FooterAction::RotateLeft(pane_idx));
                        }
                    }
                },
            );
        }
    });
    action
}

/// The footer's Move to Trash button: a wastebasket glyph from egui's
/// bundled emoji font, dim at rest and accent on hover like every other
/// control.
pub(crate) fn trash_button(ui: &mut egui::Ui, theme: &UiTheme) -> egui::Response {
    let shortcut = if cfg!(target_os = "macos") { "Del / Cmd+Backspace" } else { "Del" };
    footer_icon_button(
        ui,
        theme,
        "🗑",
        egui::FontId::proportional(15.0),
        egui::vec2(20.0, 18.0),
        0.0,
        format!("Move to Trash ({shortcut})"),
    )
}

/// A rotate button; `clockwise` picks the glyph and its shortcut hint.
fn rotate_button(ui: &mut egui::Ui, theme: &UiTheme, clockwise: bool) -> egui::Response {
    // The open-circle arrows open at the top. Rotating them 180 degrees puts
    // the opening at the bottom without changing their direction.
    let (glyph, shortcut) = if clockwise {
        ("↻", "R")
    } else {
        ("↺", "Shift+R")
    };
    footer_icon_button(
        ui,
        theme,
        glyph,
        egui::FontId::proportional(15.0),
        egui::vec2(20.0, 18.0),
        std::f32::consts::PI,
        format!(
            "Rotate {} ({shortcut})",
            if clockwise { "right" } else { "left" }
        ),
    )
}

fn footer_icon_button(
    ui: &mut egui::Ui,
    theme: &UiTheme,
    glyph: &str,
    font: egui::FontId,
    size: egui::Vec2,
    angle: f32,
    tooltip: String,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let color = if response.hovered() {
        theme.accent
    } else {
        egui::Color32::from_gray(160)
    };
    if angle == 0.0 {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            glyph,
            font,
            color,
        );
    } else {
        let galley = ui.fonts(|f| f.layout_no_wrap(glyph.to_owned(), font, color));
        let pivot = rect.center() + galley.size() * 0.5;
        ui.painter().add(egui::Shape::Text(egui::epaint::TextShape {
            pos: pivot,
            galley,
            underline: egui::Stroke::NONE,
            fallback_color: color,
            override_text_color: Some(color),
            opacity_factor: 1.0,
            angle,
        }));
    }
    response.on_hover_text(tooltip)
}

pub(crate) fn format_file_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

#[derive(PartialEq)]
pub(crate) enum MenuAction {
    None,
    OpenFolder(usize),
    OpenFile(usize),
    Close,
    Quit,
    SetSinglePane,
    SetDualPane,
    SetDualPaneIndependent,
    RotateLeft,
    RotateRight,
    ResetZoom,
    ToggleFullscreen,
    ShowAbout,
    ShowSettings,
    ShowLogs,
    ExportDebugLogs,
    MoveToTrash,
}
