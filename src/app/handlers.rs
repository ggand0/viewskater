use eframe::egui;

use crate::{menu::MenuAction, settings::ImageDiscoveryOptions};
use crate::pane::Pane;

use super::{App, DualPaneMode, SliderResult};

/// What one `step_navigation` call did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NavOutcome {
    /// At least one active pane moved to the next image.
    pub advanced: bool,
    /// A pane could move but its next texture was not in the sliding
    /// window yet, so nothing moved this frame. A stall.
    pub blocked: bool,
    /// No active pane can move further in this direction.
    pub at_end: bool,
}

impl App {
    pub(super) fn set_single_pane(&mut self) {
        if self.panes.len() >= 2 {
            self.panes.truncate(1);
        }
    }

    pub(super) fn set_dual_pane(&mut self, ctx: &egui::Context) {
        if self.panes.len() < 2 {
            let mut pane = Pane::new(
                ctx,
                self.settings.cache_count,
                self.settings.lru_budget_mb,
                self.settings.decode_threads,
                self.settings.mouse_wheel_zoom,
                self.settings.reset_zoom_pan_on_navigation,
                self.settings.preview_budget_mb,
            );
            if !self.panes[0].image_paths.is_empty() {
                if let Some(dir) = &self.panes[0].dir_path {
                    pane.open_path(
                        dir,
                        ctx,
                        self.current_discovery_options(),
                    );
                    pane.jump_to(self.panes[0].current_index, ctx);
                }
            }
            self.panes.push(pane);
        }
    }

    /// Independent dual pane: each pane has its own slider and the keys act
    /// only on the selected panes. Switching to single pane leaves
    /// `dual_pane_mode` as it is, so the pane count is part of the check.
    pub(super) fn is_independent(&self) -> bool {
        self.panes.len() >= 2 && self.dual_pane_mode == DualPaneMode::Independent
    }

    pub(super) fn open_folder_dialog(&mut self, pane_idx: usize, ctx: &egui::Context) {
        let current_discovery_options = self.current_discovery_options();
        if let Some(pane) = self.panes.get_mut(pane_idx) {
            if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                pane.open_path(
                    &dir,
                    ctx,
                    current_discovery_options,
                );
            }
        }
    }

    pub(super) fn open_file_dialog(&mut self, pane_idx: usize, ctx: &egui::Context) {
        let current_discovery_options = self.current_discovery_options();
        if let Some(pane) = self.panes.get_mut(pane_idx) {
            if let Some(file) = rfd::FileDialog::new()
                .add_filter("Images", &crate::file_io::supported_extensions())
                .pick_file()
            {
                pane.open_path(
                    &file,
                    ctx,
                    current_discovery_options,
                );
            }
        }
    }

    pub(super) fn close_images(&mut self) {
        for pane in &mut self.panes {
            pane.close();
        }
        // Return freed heap pages to the OS. Without this, glibc keeps the
        // arena expanded and RSS stays inflated after dropping large caches.
        #[cfg(target_os = "linux")]
        unsafe {
            libc::malloc_trim(0);
        }
    }

    /// 旋转当前激活的图片；独立双栏时只旋转选中的栏。
    fn rotate_panes(&mut self, clockwise: bool) {
        let use_selection = self.is_independent();
        for pane in &mut self.panes {
            if !use_selection || pane.selected {
                pane.rotate(clockwise);
            }
        }
    }

    /// 底部按钮栏的旋转只作用于按钮所在的那一栏。
    pub(super) fn rotate_pane(&mut self, pane_idx: usize, clockwise: bool) {
        if let Some(pane) = self.panes.get_mut(pane_idx) {
            pane.rotate(clockwise);
        }
    }

    pub(super) fn handle_menu_action(&mut self, action: MenuAction, ctx: &egui::Context) {
        match action {
            MenuAction::None => {}
            MenuAction::OpenFolder(idx) => self.open_folder_dialog(idx, ctx),
            MenuAction::OpenFile(idx) => self.open_file_dialog(idx, ctx),
            MenuAction::Close => self.close_images(),
            MenuAction::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            MenuAction::SetSinglePane => self.set_single_pane(),
            MenuAction::SetDualPane => {
                self.set_dual_pane(ctx);
                self.dual_pane_mode = DualPaneMode::Synced;
            }
            MenuAction::SetDualPaneIndependent => {
                self.set_dual_pane(ctx);
                self.dual_pane_mode = DualPaneMode::Independent;
            }
            MenuAction::RotateLeft => self.rotate_panes(false),
            MenuAction::RotateRight => self.rotate_panes(true),
            MenuAction::ResetZoom => {
                for pane in &mut self.panes {
                    pane.zoom = 1.0;
                    pane.pan = egui::Vec2::ZERO;
                }
            }
            MenuAction::ToggleFullscreen => self.toggle_fullscreen(ctx),
            MenuAction::ShowAbout => self.show_about = true,
            MenuAction::ShowSettings => self.show_settings = true,
            MenuAction::ShowLogs => {
                let log_dir = crate::file_io::get_log_directory();
                let _ = std::fs::create_dir_all(&log_dir);
                crate::file_io::open_in_file_explorer(&log_dir.to_string_lossy());
            }
            MenuAction::ExportDebugLogs => {
                crate::file_io::export_and_open_debug_logs(&self.log_buffer);
            }
            MenuAction::MoveToTrash => self.trash_current_images(ctx),
        }
    }

    /// Delete (any platform) or Cmd+Backspace (macOS, the Finder shortcut)
    /// was pressed this frame. Key repeats are ignored on purpose: holding
    /// the key must not trash a run of files.
    fn trash_key_pressed(ctx: &egui::Context) -> bool {
        ctx.input(|i| {
            i.events.iter().any(|e| match e {
                egui::Event::Key {
                    key,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } => {
                    *key == egui::Key::Delete
                        || (cfg!(target_os = "macos")
                            && *key == egui::Key::Backspace
                            && modifiers.command)
                }
                _ => false,
            })
        })
    }

    /// Apply slider result to all panes (synced mode).
    /// Returns true if any pane put a new image on screen this frame.
    pub(super) fn apply_slider_result_all(&mut self, result: SliderResult, ctx: &egui::Context) -> bool {
        let mut shown = false;
        if let Some(idx) = result.target {
            for pane in &mut self.panes {
                if pane.apply_slider_target(idx, ctx) {
                    self.perf.record_image_load();
                    shown = true;
                }
            }
            ctx.request_repaint();
        }

        if result.released {
            for pane in &mut self.panes {
                pane.apply_slider_release(ctx);
            }
        }
        shown
    }

    /// Apply slider result to a single pane (independent mode).
    pub(super) fn apply_slider_result_one(
        &mut self,
        pane_idx: usize,
        result: SliderResult,
        ctx: &egui::Context,
    ) {
        if let Some(idx) = result.target {
            if let Some(pane) = self.panes.get_mut(pane_idx) {
                if pane.apply_slider_target(idx, ctx) {
                    self.perf.record_image_load();
                }
            }
            ctx.request_repaint();
        }

        if result.released {
            if let Some(pane) = self.panes.get_mut(pane_idx) {
                pane.apply_slider_release(ctx);
            }
        }
    }

    pub(super) fn apply_settings_to_caches(&mut self) {
        for pane in &mut self.panes {
            if let Some(cache) = &mut pane.cache {
                cache.set_cache_count(
                    self.settings.cache_count,
                    pane.current_index,
                    &pane.image_paths,
                );
            }
            pane.decode_cache.set_budget_mb(self.settings.lru_budget_mb);
            if let Some(cache) = &mut pane.cache {
                cache.set_decode_threads(self.settings.decode_threads);
            }
            pane.cache_count = self.settings.cache_count;
            pane.lru_budget_mb = self.settings.lru_budget_mb;
            pane.decode_threads = self.settings.decode_threads;
            pane.mouse_wheel_zoom = self.settings.mouse_wheel_zoom;
            pane.reset_zoom_pan_on_navigation = self.settings.reset_zoom_pan_on_navigation;
            pane.preview_budget_mb = self.settings.preview_budget_mb;
            if let Some(tc) = &mut pane.thumbnail_cache {
                tc.set_budget_mb(self.settings.preview_budget_mb);
            }
        }
    }

    pub(super) fn reload_sorted_panes(&mut self, ctx: &egui::Context) {
        let current_discovery_options = self.current_discovery_options();
        for pane in &mut self.panes {
            // store active image path, so that we can jump back to it after reloading
            let active_img = pane.image_paths.get(pane.current_index).cloned();
            let Some(path) = pane.dir_path.clone() else {
                continue;
            };
            let zoom = pane.zoom;
            let pan = pane.pan;
            pane.open_path(
                &path,
                ctx,
                current_discovery_options,
            );
            pane.zoom = zoom;
            pane.pan = pan;
            // if an imae was active before AND the image is still in our file list, jump to it
            // a previously active image might not be in the list anymore if
            //  a) it doesnt match our file discovery options (recursive, hidden)
            //  b) or, it doesnt exist on disk anymore
            if let Some(active_img) = active_img {
                if let Some(new_idx) = pane.image_paths.iter().position(|item| *item == *active_img) {
                    pane.jump_to(new_idx, ctx);
                }
            }
        }
    }

    pub(super) fn toggle_fullscreen(&mut self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(
            !ctx.input(|i| i.viewport().fullscreen.unwrap_or(false)))
        );
    }

    pub(super) fn handle_keyboard(&mut self, ctx: &egui::Context) {
        // The metadata panel's filter box has focus: the keys are text
        // now, not shortcuts. A and D would move six images while someone
        // types "camera" otherwise. The panel reports this itself.
        // `ctx.wants_keyboard_input()` is true for any focused widget, and
        // Tab, the footer toggle, also moves egui's focus onto a button,
        // which would leave every shortcut dead after one Tab press.
        if self.metadata_panel.filter_has_focus {
            return;
        }

        let (home, end, shift, nav_right_pressed, nav_left_pressed,
             nav_right_held, nav_left_held, set_single, set_dual,
             set_independent, select_pane1, select_pane2,
             toggle_footer, toggle_metadata_panel, open_folder, open_file, close, quit,
             toggle_fullscreen, escape, scroll_delta, command_held) =
            ctx.input(|i| {
                (
                    i.key_pressed(egui::Key::Home),
                    i.key_pressed(egui::Key::End),
                    i.modifiers.shift,
                    i.key_pressed(egui::Key::ArrowRight) || i.key_pressed(egui::Key::D),
                    i.key_pressed(egui::Key::ArrowLeft) || i.key_pressed(egui::Key::A),
                    i.key_down(egui::Key::ArrowRight) || i.key_down(egui::Key::D),
                    i.key_down(egui::Key::ArrowLeft) || i.key_down(egui::Key::A),
                    i.key_pressed(egui::Key::Num1) && i.modifiers.command,
                    i.key_pressed(egui::Key::Num2) && i.modifiers.command,
                    i.key_pressed(egui::Key::Num3) && i.modifiers.command,
                    i.key_pressed(egui::Key::Num1) && !i.modifiers.command,
                    i.key_pressed(egui::Key::Num2) && !i.modifiers.command,
                    i.key_pressed(egui::Key::Tab),
                    i.key_pressed(egui::Key::I),
                    i.key_pressed(egui::Key::O) && i.modifiers.command && i.modifiers.shift,
                    i.key_pressed(egui::Key::O) && i.modifiers.command && !i.modifiers.shift,
                    i.key_pressed(egui::Key::W) && i.modifiers.command,
                    i.key_pressed(egui::Key::Q) && i.modifiers.command,
                    i.key_pressed(egui::Key::F11),
                    i.key_pressed(egui::Key::Escape),
                    i.raw_scroll_delta.y,
                    i.modifiers.command,
                )
            });

        if quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if toggle_fullscreen {
            self.toggle_fullscreen(ctx);
            return;
        }
        if escape && ctx.input(|i| i.viewport().fullscreen.unwrap_or(false)) {
            self.toggle_fullscreen(ctx);
            return;
        }
        if open_folder {
            self.open_folder_dialog(0, ctx);
            return;
        }
        if open_file {
            self.open_file_dialog(0, ctx);
            return;
        }
        if close {
            self.close_images();
            return;
        }
        if toggle_footer {
            self.settings.show_footer = !self.settings.show_footer;
            self.settings.save();
            return;
        }
        if toggle_metadata_panel {
            self.settings.show_metadata_panel = !self.settings.show_metadata_panel;
            self.settings.save();
            return;
        }

        // Pane selection toggle (bare 1/2 keys, only in independent dual-pane mode)
        if self.is_independent() {
            if select_pane1 {
                self.panes[0].selected = !self.panes[0].selected;
                return;
            }
            if select_pane2 {
                self.panes[1].selected = !self.panes[1].selected;
                return;
            }
        }

        // Skate mode (Shift held): advance every frame while key is down
        // Normal mode: advance once per key press/repeat event (~30hz)
        let nav_right = if shift { nav_right_held } else { nav_right_pressed };
        let nav_left = if shift { nav_left_held } else { nav_left_pressed };

        if set_single && self.panes.len() >= 2 {
            self.set_single_pane();
            return;
        }
        if set_dual {
            self.set_dual_pane(ctx);
            self.dual_pane_mode = DualPaneMode::Synced;
            return;
        }
        if set_independent {
            self.set_dual_pane(ctx);
            self.dual_pane_mode = DualPaneMode::Independent;
            return;
        }

        // In independent dual pane only the selected panes navigate. In
        // synced dual pane and in single pane every pane does.
        let use_selection = self.is_independent();
        let is_active = |p: &Pane| !use_selection || p.selected;

        if self.show_settings || self.show_about || self.pending_permanent_delete.is_some() {
            return;
        }

        // R 顺时针旋转，Shift+R 逆时针旋转。
        let rotate = ctx.input(|i| {
            (i.key_pressed(egui::Key::R) && !i.modifiers.command).then_some(!i.modifiers.shift)
        });
        if let Some(clockwise) = rotate {
            self.rotate_panes(clockwise);
            return;
        }

        if Self::trash_key_pressed(ctx) {
            self.trash_current_images(ctx);
            return;
        }

        if home {
            for pane in &mut self.panes {
                if is_active(pane) {
                    pane.jump_to(0, ctx);
                }
            }
            self.perf.record_image_load();
        } else if end {
            for pane in &mut self.panes {
                if is_active(pane) {
                    let last = pane.image_paths.len().saturating_sub(1);
                    pane.jump_to(last, ctx);
                }
            }
            self.perf.record_image_load();
        } else if nav_right {
            self.step_navigation(1, ctx);
        } else if nav_left {
            self.step_navigation(-1, ctx);
        }

        if !self.settings.mouse_wheel_zoom && !command_held && scroll_delta != 0.0 {
            let dir: isize = if scroll_delta > 0.0 { -1 } else { 1 };
            let all_ready = self.panes.iter().all(|p| {
                !is_active(p) || p.image_paths.is_empty() || p.is_next_cached(dir)
            });
            if all_ready {
                let any_advanced = self.panes.iter_mut().fold(false, |acc, p| {
                    if is_active(p) { p.navigate(dir, ctx) || acc } else { acc }
                });
                if any_advanced {
                    self.perf.record_image_load();
                }
            }
            ctx.request_repaint();
        }
    }

    /// One keyboard navigation step in direction `dir` (+1 right, -1 left)
    /// for every active pane, the way a held key does it: move only when
    /// every active pane already has the next texture in its sliding
    /// window, otherwise draw another frame and try again. `--bench-nav`
    /// calls this directly in place of the key state, so the benchmark
    /// runs the same code as a person holding the key.
    pub(crate) fn step_navigation(&mut self, dir: isize, ctx: &egui::Context) -> NavOutcome {
        let use_selection = self.is_independent();
        let is_active = |p: &Pane| !use_selection || p.selected;

        let any_can = self.panes.iter().any(|p| {
            is_active(p)
                && if dir > 0 { p.can_navigate_forward() } else { p.can_navigate_backward() }
        });
        if !any_can {
            return NavOutcome { advanced: false, blocked: false, at_end: true };
        }

        let all_ready = self.panes.iter().all(|p| {
            !is_active(p) || p.image_paths.is_empty() || p.is_next_cached(dir)
        });
        let mut advanced = false;
        if all_ready {
            advanced = self.panes.iter_mut().fold(false, |acc, p| {
                if is_active(p) { p.navigate(dir, ctx) || acc } else { acc }
            });
            if advanced {
                self.perf.record_image_load();
            }
        }
        ctx.request_repaint();
        NavOutcome { advanced, blocked: !all_ready, at_end: false }
    }

    /// Drain any paths forwarded from the platform layer (e.g. macOS Finder
    /// "Open With"). Each path goes through the same entrypoint as CLI args
    /// and drag-and-drop, so it loads the image and its sibling directory.
    pub(super) fn handle_external_open_requests(&mut self, ctx: &egui::Context) {
        let current_discovery_options = self.current_discovery_options();
        while let Ok(path) = self.file_receiver.try_recv() {
            log::info!("External open request: {}", path.display());
            self.panes[0].open_path(
                &path,
                ctx,
                current_discovery_options,
            );
            if self.panes[0].current_texture.is_some() {
                self.perf.record_image_load();
            }
            ctx.request_repaint();
        }
    }

    pub(super) fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped: Vec<egui::DroppedFile> = ctx.input(|i| i.raw.dropped_files.clone());
        if let Some(file) = dropped.first() {
            if let Some(path) = &file.path {
                let current_discovery_options = self.current_discovery_options();
                if self.panes.len() >= 2 {
                    let hover = ctx.input(|i| i.pointer.hover_pos());
                    let latest = ctx.input(|i| i.pointer.latest_pos());
                    let screen = ctx.screen_rect();
                    let divider_x =
                        screen.min.x + screen.width() * self.divider_fraction;
                    log::debug!(
                        "DnD drop: hover_pos={:?}, latest_pos={:?}, screen={:?}, divider_x={}, fraction={}",
                        hover, latest, screen, divider_x, self.divider_fraction
                    );
                    let target = hover
                        .or(latest)
                        .map(|pos| {
                            log::debug!("DnD using pos x={}, divider_x={} → pane {}", pos.x, divider_x, if pos.x < divider_x { 0 } else { 1 });
                            if pos.x < divider_x { 0 } else { 1 }
                        })
                        .unwrap_or(0);
                    self.panes[target].open_path(
                        path,
                        ctx,
                        current_discovery_options,
                    );
                } else {
                    self.panes[0].open_path(
                        path,
                        ctx,
                        current_discovery_options,
                    );
                }
            }
        }
    }

    pub(super) fn current_discovery_options(&self) -> ImageDiscoveryOptions {
        let mut opts = self.settings.image_discovery_options;
        opts.sort_order = self.current_sort;
        opts
    }
}
