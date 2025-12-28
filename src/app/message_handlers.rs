// Comprehensive message handler module that routes different message categories
// This significantly reduces the size of app.rs update() method

use std::path::PathBuf;
use std::sync::Arc;
use log::{info, warn, error, debug};
use iced_winit::runtime::Task;
use iced_wgpu::engine::CompressionStrategy;
use iced_core::Event;

use iced_runtime::clipboard;

use crate::app::{DataViewer, Message};
use crate::cache::img_cache::{CacheStrategy, CachedData, LoadOperation};
use crate::settings::UserSettings;
use crate::file_io;
use crate::loading_handler;
use crate::navigation_slider;
use crate::navigation_keyboard::{move_left_all, move_right_all};
use crate::menu::PaneLayout;
use crate::pane::{IMAGE_RENDER_TIMES, IMAGE_RENDER_FPS};
use crate::widgets::shader::{scene::Scene, cpu_scene::CpuScene};

#[allow(unused_imports)]
use std::time::Instant;

/// Main entry point for handling all messages
/// Routes messages to appropriate handler functions
pub fn handle_message(app: &mut DataViewer, message: Message) -> Task<Message> {
    match message {
        // Simple inline messages
        Message::Nothing => Task::none(),
        Message::Debug(s) => {
            app.title = s;
            Task::none()
        }
        Message::BackgroundColorChanged(color) => {
            app.background_color = color;
            Task::none()
        }
        Message::FontLoaded(_) => Task::none(),
        Message::TimerTick => {
            debug!("TimerTick received");
            Task::none()
        }
        Message::Quit => {
            std::process::exit(0);
        }
        Message::ReplayKeepAlive => {
            // This message is sent periodically during replay mode to keep the update loop active
            debug!("ReplayKeepAlive received - keeping replay update loop active");
            // Reset pending flag so a new keep-alive can be scheduled
            app.replay_keep_alive_pending = false;
            Task::none()
        }

        // UI state messages (About, Options, Logs)
        Message::ShowLogs | Message::OpenSettingsDir | Message::ExportDebugLogs |
        Message::ExportAllLogs | Message::ShowAbout | Message::HideAbout |
        Message::ShowOptions | Message::HideOptions | Message::OpenWebLink(_) => {
            handle_ui_messages(app, message)
        }

        // Settings messages
        Message::SaveSettings | Message::ClearSettingsStatus | Message::SettingsTabSelected(_) |
        Message::AdvancedSettingChanged(_, _) | Message::ResetAdvancedSettings => {
            handle_settings_messages(app, message)
        }

        // File operation messages
        Message::OpenFolder(_) | Message::OpenFile(_) | Message::FileDropped(_, _) |
        Message::Close | Message::FolderOpened(_, _) | Message::CopyFilename(_) | Message::CopyFilePath(_) => {
            handle_file_messages(app, message)
        }

        // Image loading messages
        Message::ImagesLoaded(_) | Message::SliderImageWidgetLoaded(_) | Message::SliderImageLoaded(_) => {
            handle_image_loading_messages(app, message)
        }

        // Slider and navigation messages
        Message::SliderChanged(_, _) | Message::SliderReleased(_, _) => {
            handle_slider_messages(app, message)
        }

        // Toggle and UI control messages
        Message::OnSplitResize(_) | Message::ResetSplit(_) | Message::ToggleSliderType(_) |
        Message::TogglePaneLayout(_) | Message::ToggleFooter(_) | Message::ToggleSyncedZoom(_) |
        Message::ToggleMouseWheelZoom(_) | Message::ToggleCopyButtons(_) | Message::ToggleMetadataDisplay(_) | Message::ToggleNearestNeighborFilter(_) |
        Message::ToggleFullScreen(_) | Message::ToggleFpsDisplay(_) | Message::ToggleSplitOrientation(_) |
        Message::CursorOnTop(_) | Message::CursorOnMenu(_) | Message::CursorOnFooter(_) |
        Message::PaneSelected(_, _) | Message::SetCacheStrategy(_) | Message::SetCompressionStrategy(_) |
        Message::WindowResized(_) => {
            handle_toggle_messages(app, message)
        }

        #[cfg(feature = "coco")]
        Message::ToggleCocoSimplification(_) => {
            handle_toggle_messages(app, message)
        }

        #[cfg(feature = "coco")]
        Message::SetCocoMaskRenderMode(_) => {
            handle_toggle_messages(app, message)
        }

        // Event messages (mouse, keyboard, file drops)
        Message::Event(event) => {
            handle_event_messages(app, event)
        }

        // Feature-specific messages
        #[cfg(feature = "selection")]
        Message::SelectionAction(msg) => {
            crate::widgets::selection_widget::handle_selection_message(
                msg,
                &app.panes,
                &mut app.selection_manager,
            )
        }

        #[cfg(feature = "coco")]
        Message::CocoAction(coco_msg) => {
            crate::coco::widget::handle_coco_message(
                coco_msg,
                &mut app.panes,
                &mut app.annotation_manager,
            )
        }
    }
}

/// Routes UI state messages (About, Options, Logs, etc.)
pub fn handle_ui_messages(app: &mut DataViewer, message: Message) -> Task<Message> {
    match message {
        Message::ShowLogs => {
            let app_name = "viewskater";
            let log_dir_path = crate::logging::get_log_directory(app_name);
            let _ = std::fs::create_dir_all(log_dir_path.clone());
            crate::logging::open_in_file_explorer(log_dir_path.to_string_lossy().as_ref());
            Task::none()
        }
        Message::OpenSettingsDir => {
            let settings_path = UserSettings::settings_path();
            if let Some(settings_dir) = settings_path.parent() {
                let _ = std::fs::create_dir_all(settings_dir);
                crate::logging::open_in_file_explorer(settings_dir.to_string_lossy().as_ref());
            }
            Task::none()
        }
        Message::ExportDebugLogs => {
            let app_name = "viewskater";
            if let Some(log_buffer) = crate::get_shared_log_buffer() {
                crate::logging::export_and_open_debug_logs(app_name, log_buffer);
            } else {
                warn!("Log buffer not available for export");
            }
            Task::none()
        }
        Message::ExportAllLogs => {
            handle_export_all_logs();
            Task::none()
        }
        Message::ShowAbout => {
            app.show_about = true;
            Task::perform(async {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }, |_| Message::Nothing)
        }
        Message::HideAbout => {
            app.show_about = false;
            Task::none()
        }
        Message::ShowOptions => {
            app.settings.show();
            Task::perform(async {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }, |_| Message::Nothing)
        }
        Message::HideOptions => {
            app.settings.hide();
            Task::none()
        }
        Message::OpenWebLink(url) => {
            if let Err(e) = webbrowser::open(&url) {
                warn!("Failed to open link: {}, error: {:?}", url, e);
            }
            Task::none()
        }
        _ => Task::none()
    }
}

/// Routes settings-related messages
pub fn handle_settings_messages(app: &mut DataViewer, message: Message) -> Task<Message> {
    match message {
        Message::SaveSettings => handle_save_settings(app),
        Message::ClearSettingsStatus => {
            app.settings.clear_save_status();
            Task::none()
        }
        Message::SettingsTabSelected(index) => {
            app.settings.set_active_tab(index);
            Task::none()
        }
        Message::AdvancedSettingChanged(field_name, value) => {
            app.settings.set_advanced_input(field_name, value);
            Task::none()
        }
        Message::ResetAdvancedSettings => {
            handle_reset_advanced_settings(app);
            Task::none()
        }
        _ => Task::none()
    }
}

/// Routes file operation messages
pub fn handle_file_messages(app: &mut DataViewer, message: Message) -> Task<Message> {
    match message {
        Message::OpenFolder(pane_index) => {
            Task::perform(file_io::pick_folder(), move |result| {
                Message::FolderOpened(result, pane_index)
            })
        }
        Message::OpenFile(pane_index) => {
            Task::perform(file_io::pick_file(), move |result| {
                Message::FolderOpened(result, pane_index)
            })
        }
        Message::FileDropped(pane_index, dropped_path) => {
            handle_file_dropped(app, pane_index, dropped_path)
        }
        Message::Close => {
            app.reset_state(-1);
            debug!("directory_path: {:?}", app.directory_path);
            debug!("self.current_image_index: {}", app.current_image_index);
            for pane in app.panes.iter_mut() {
                let img_cache = &mut pane.img_cache;
                debug!("img_cache.current_index: {}", img_cache.current_index);
                debug!("img_cache.image_paths.len(): {}", img_cache.image_paths.len());
            }
            Task::none()
        }
        Message::FolderOpened(result, pane_index) => {
            match result {
                Ok(dir) => {
                    debug!("Folder opened: {}", dir);
                    if pane_index > 0 && app.pane_layout == PaneLayout::SinglePane {
                        debug!("Ignoring request to open folder in pane {} while in single-pane mode", pane_index);
                        Task::none()
                    } else {
                        app.initialize_dir_path(&PathBuf::from(dir), pane_index)
                    }
                }
                Err(err) => {
                    debug!("Folder open failed: {:?}", err);
                    Task::none()
                }
            }
        }
        Message::CopyFilename(pane_index) => {
            let path = &app.panes[pane_index].img_cache.image_paths[app.panes[pane_index].img_cache.current_index];
            let filename_str = path.file_name().to_string();
            if let Some(filename) = file_io::get_filename(&filename_str) {
                debug!("Copying filename to clipboard: {}", filename);
                return clipboard::write(filename);
            }
            Task::none()
        }
        Message::CopyFilePath(pane_index) => {
            let path = &app.panes[pane_index].img_cache.image_paths[app.panes[pane_index].img_cache.current_index];
            let img_path = path.file_name().to_string();
            if let Some(dir_path) = app.panes[pane_index].directory_path.as_ref() {
                let full_path = format!("{}/{}", dir_path, img_path);
                debug!("Copying full path to clipboard: {}", full_path);
                return clipboard::write(full_path);
            }
            Task::none()
        }
        _ => Task::none()
    }
}

/// Routes image loading messages
pub fn handle_image_loading_messages(app: &mut DataViewer, message: Message) -> Task<Message> {
    match message {
        Message::ImagesLoaded(result) => {
            debug!("ImagesLoaded");
            match result {
                Ok((image_data, metadata, operation)) => {
                    if let Some(op) = operation {
                        let cloned_op = op.clone();
                        match op {
                            LoadOperation::LoadNext((ref pane_indices, ref target_indices))
                            | LoadOperation::LoadPrevious((ref pane_indices, ref target_indices))
                            | LoadOperation::ShiftNext((ref pane_indices, ref target_indices))
                            | LoadOperation::ShiftPrevious((ref pane_indices, ref target_indices)) => {
                                let operation_type = cloned_op.operation_type();

                                loading_handler::handle_load_operation_all(
                                    &mut app.panes,
                                    &mut app.loading_status,
                                    pane_indices,
                                    target_indices,
                                    &image_data,
                                    &metadata,
                                    &cloned_op,
                                    operation_type,
                                );
                            }
                            LoadOperation::LoadPos((pane_index, target_indices_and_cache)) => {
                                loading_handler::handle_load_pos_operation(
                                    &mut app.panes,
                                    &mut app.loading_status,
                                    pane_index,
                                    &target_indices_and_cache,
                                    &image_data,
                                    &metadata,
                                );

                                // Signal replay controller that initial load is complete
                                if let Some(ref mut replay_controller) = app.replay_controller {
                                    if matches!(replay_controller.state, crate::replay::ReplayState::WaitingForReady { .. }) {
                                        debug!("LoadPos complete - signaling replay controller that app is ready to navigate");

                                        // Set image count for slider mode navigation
                                        if let Some(pane) = app.panes.get(pane_index) {
                                            replay_controller.set_image_count(pane.img_cache.image_paths.len());
                                        }

                                        // Reset FPS trackers right before navigation starts
                                        // This ensures no stale data from image loading contaminates metrics
                                        if let Ok(mut fps) = crate::CURRENT_FPS.lock() { *fps = 0.0; }
                                        if let Ok(mut fps) = IMAGE_RENDER_FPS.lock() { *fps = 0.0; }
                                        if let Ok(mut times) = crate::FRAME_TIMES.lock() { times.clear(); }
                                        if let Ok(mut times) = IMAGE_RENDER_TIMES.lock() { times.clear(); }
                                        iced_wgpu::reset_image_fps();

                                        replay_controller.on_ready_to_navigate();
                                    }
                                }
                            }
                        }
                    }
                }
                Err(err) => {
                    debug!("Image load failed: {:?}", err);
                }
            }
            Task::none()
        }
        Message::SliderImageWidgetLoaded(result) => {
            match result {
                Ok((pane_idx, pos, handle, dimensions, file_size)) => {
                    crate::track_async_delivery();

                    if let Some(pane) = app.panes.get_mut(pane_idx) {
                        pane.slider_image = Some(handle);
                        pane.slider_image_dimensions = Some(dimensions);
                        pane.slider_image_position = Some(pos);
                        // Update metadata for footer display during slider dragging
                        pane.current_image_metadata = Some(crate::cache::img_cache::ImageMetadata::new(
                            dimensions.0, dimensions.1, file_size
                        ));
                        // BUGFIX: Don't update current_index here! It causes desyncs when stale slider images
                        // load after slider release. The slider position is tracked in slider_image_position instead.
                        // pane.img_cache.current_index = pos;

                        debug!("Slider image loaded for pane {} at position {} with dimensions {:?}", pane_idx, pos, dimensions);
                    } else {
                        warn!("SliderImageWidgetLoaded: Invalid pane index {}", pane_idx);
                    }
                },
                Err((pane_idx, pos)) => {
                    warn!("SLIDER: Failed to load image widget for pane {} at position {}", pane_idx, pos);
                }
            }
            Task::none()
        }
        Message::SliderImageLoaded(result) => {
            match result {
                Ok((pos, cached_data)) => {
                    let pane = &mut app.panes[0];

                    if let CachedData::Cpu(bytes) = &cached_data {
                        debug!("SliderImageLoaded: loaded data: {:?}", bytes.len());

                        pane.current_image = CachedData::Cpu(bytes.clone());
                        pane.current_image_index = Some(pos);
                        pane.slider_scene = Some(Scene::CpuScene(CpuScene::new(
                            bytes.clone(), true)));

                        if let Some(device) = &pane.device {
                            if let Some(queue) = &pane.queue {
                                if let Some(scene) = &mut pane.slider_scene {
                                    scene.ensure_texture(device, queue, pane.pane_id);
                                }
                            }
                        }
                    }
                },
                Err(pos) => {
                    warn!("SLIDER: Failed to load image for position {}", pos);
                }
            }
            Task::none()
        }
        _ => Task::none()
    }
}

/// Routes slider and navigation messages
pub fn handle_slider_messages(app: &mut DataViewer, message: Message) -> Task<Message> {
    match message {
        Message::SliderChanged(pane_index, value) => {
            app.is_slider_moving = true;
            app.use_slider_image_for_render = true;
            app.last_slider_update = Instant::now();

            // Reset COCO zoom state when slider starts moving
            #[cfg(feature = "coco")]
            {
                if pane_index == -1 {
                    // Reset all panes
                    for pane in app.panes.iter_mut() {
                        pane.zoom_scale = 1.0;
                        pane.zoom_offset = iced_core::Vector::default();
                    }
                } else {
                    // Reset specific pane
                    if let Some(pane) = app.panes.get_mut(pane_index as usize) {
                        pane.zoom_scale = 1.0;
                        pane.zoom_offset = iced_core::Vector::default();
                    }
                }
            }

            let use_async = true;

            #[cfg(target_os = "linux")]
            let use_throttle = true;
            #[cfg(not(target_os = "linux"))]
            let use_throttle = false;

            if pane_index == -1 {
                app.prev_slider_value = app.slider_value;
                app.slider_value = value;

                if app.panes[0].slider_image.is_none() {
                    for pane in app.panes.iter_mut() {
                        pane.slider_scene = None;
                    }
                }
            } else {
                let pane_index_usize = pane_index as usize;

                if app.is_slider_dual && app.pane_layout == PaneLayout::DualPane {
                    for idx in 0..app.panes.len() {
                        if idx != pane_index_usize {
                            app.panes[idx].slider_image = None;
                            app.panes[idx].slider_image_position = None;
                        }
                    }
                }

                let pane = &mut app.panes[pane_index_usize];
                pane.prev_slider_value = pane.slider_value;
                pane.slider_value = value;

                if pane.slider_image.is_none() {
                    pane.slider_scene = None;
                }
            }

            navigation_slider::update_pos(
                &mut app.panes,
                pane_index,
                value as usize,
                use_async,
                use_throttle,
            )
        }
        Message::SliderReleased(pane_index, value) => {
            debug!("SLIDER_DEBUG: SliderReleased event received");
            app.is_slider_moving = false;

            let final_image_fps = iced_wgpu::get_image_fps();
            let upload_timestamps = iced_wgpu::get_image_upload_timestamps();

            if !upload_timestamps.is_empty() {
                if let Ok(mut render_times) = IMAGE_RENDER_TIMES.lock() {
                    *render_times = upload_timestamps.into_iter().collect();

                    if let Ok(mut fps) = IMAGE_RENDER_FPS.lock() {
                        *fps = final_image_fps as f32;
                        debug!("SLIDER_DEBUG: Synced image fps tracking, final FPS: {:.1}", final_image_fps);
                    }
                }
            }

            // Use the position of the currently displayed slider_image if available,
            // otherwise fall back to the slider value
            let pos = if pane_index >= 0 {
                app.panes.get(pane_index as usize)
                    .and_then(|pane| pane.slider_image_position)
                    .unwrap_or(value as usize)
            } else {
                // For pane_index == -1 (all panes), use slider_image_position from pane 0
                app.panes.first()
                    .and_then(|pane| pane.slider_image_position)
                    .unwrap_or(value as usize)
            };

            debug!("SliderReleased: Using position {} (slider_image_position) instead of slider value {}", pos, value);

            navigation_slider::load_remaining_images(
                &app.device,
                &app.queue,
                app.is_gpu_supported,
                app.cache_strategy,
                app.compression_strategy,
                &mut app.panes,
                &mut app.loading_status,
                pane_index,
                pos)
        }
        _ => Task::none()
    }
}

/// Routes toggle and UI control messages
pub fn handle_toggle_messages(app: &mut DataViewer, message: Message) -> Task<Message> {
    match message {
        Message::OnSplitResize(position) => {
            app.divider_position = Some(position);
            Task::none()
        }
        Message::ResetSplit(_position) => {
            app.divider_position = None;
            Task::none()
        }
        Message::ToggleSliderType(_bool) => {
            app.toggle_slider_type();
            Task::none()
        }
        Message::TogglePaneLayout(pane_layout) => {
            app.toggle_pane_layout(pane_layout);
            Task::none()
        }
        Message::ToggleFooter(_bool) => {
            app.toggle_footer();
            Task::none()
        }
        Message::ToggleSyncedZoom(enabled) => {
            app.synced_zoom = enabled;
            Task::none()
        }
        Message::ToggleMouseWheelZoom(enabled) => {
            app.mouse_wheel_zoom = enabled;
            for pane in app.panes.iter_mut() {
                pane.mouse_wheel_zoom = enabled;
            }
            Task::none()
        }
        Message::ToggleCopyButtons(enabled) => {
            app.show_copy_buttons = enabled;
            Task::none()
        }
        Message::ToggleMetadataDisplay(enabled) => {
            app.show_metadata = enabled;
            Task::none()
        }
        Message::ToggleNearestNeighborFilter(enabled) => {
            debug!("ToggleNearestNeighborFilter: setting to {}", enabled);
            app.nearest_neighbor_filter = enabled;

            // Force reload of current directories to apply the new filter immediately
            let mut tasks = Vec::new();
            for pane_index in 0..app.panes.len() {
                if let Some(dir_path) = app.panes[pane_index].directory_path.clone() {
                    debug!("Reloading directory for pane {}: {:?}", pane_index, dir_path);
                    tasks.push(app.initialize_dir_path(&PathBuf::from(dir_path), pane_index));
                }
            }

            Task::batch(tasks)
        }
        #[cfg(feature = "coco")]
        Message::ToggleCocoSimplification(enabled) => {
            app.coco_disable_simplification = enabled;
            Task::none()
        }
        #[cfg(feature = "coco")]
        Message::SetCocoMaskRenderMode(mode) => {
            app.coco_mask_render_mode = mode;
            Task::none()
        }
        Message::ToggleFullScreen(enabled) => {
            app.is_fullscreen = enabled;
            Task::none()
        }
        Message::ToggleFpsDisplay(value) => {
            app.show_fps = value;
            Task::none()
        }
        Message::ToggleSplitOrientation(_bool) => {
            app.toggle_split_orientation();
            Task::none()
        }
        Message::CursorOnTop(value) => {
            app.cursor_on_top = value;
            Task::none()
        }
        Message::CursorOnMenu(value) => {
            app.cursor_on_menu = value;
            Task::none()
        }
        Message::CursorOnFooter(value) => {
            app.cursor_on_footer = value;
            Task::none()
        }
        Message::PaneSelected(pane_index, is_selected) => {
            app.panes[pane_index].is_selected = is_selected;
            for (index, pane) in app.panes.iter_mut().enumerate() {
                debug!("pane_index: {}, is_selected: {}", index, pane.is_selected);
            }
            Task::none()
        }
        Message::SetCacheStrategy(strategy) => {
            app.update_cache_strategy(strategy);
            Task::none()
        }
        Message::SetCompressionStrategy(strategy) => {
            app.update_compression_strategy(strategy);
            Task::none()
        }
        Message::WindowResized(width) => {
            app.window_width = width;
            Task::none()
        }
        _ => Task::none()
    }
}

/// Routes event messages (mouse wheel, keyboard, file drops)
pub fn handle_event_messages(app: &mut DataViewer, event: Event) -> Task<Message> {
    match event {
        Event::Mouse(iced_core::mouse::Event::WheelScrolled { delta }) => {
            if !app.ctrl_pressed && !app.mouse_wheel_zoom && !app.settings.is_visible() && !app.show_about {
                match delta {
                    iced_core::mouse::ScrollDelta::Lines { y, .. }
                    | iced_core::mouse::ScrollDelta::Pixels { y, .. } => {
                        if y > 0.0 {
                            // Clear slider state when using mouse wheel navigation
                            app.use_slider_image_for_render = false;
                            for pane in app.panes.iter_mut() {
                                pane.slider_image_position = None;
                            }

                            return move_left_all(
                                &app.device,
                                &app.queue,
                                app.cache_strategy,
                                app.compression_strategy,
                                &mut app.panes,
                                &mut app.loading_status,
                                &mut app.slider_value,
                                &app.pane_layout,
                                app.is_slider_dual,
                                app.last_opened_pane as usize);
                        } else if y < 0.0 {
                            // Clear slider state when using mouse wheel navigation
                            app.use_slider_image_for_render = false;
                            for pane in app.panes.iter_mut() {
                                pane.slider_image_position = None;
                            }

                            return move_right_all(
                                &app.device,
                                &app.queue,
                                app.cache_strategy,
                                app.compression_strategy,
                                &mut app.panes,
                                &mut app.loading_status,
                                &mut app.slider_value,
                                &app.pane_layout,
                                app.is_slider_dual,
                                app.last_opened_pane as usize
                            );
                        }
                    }
                };
            } else {
                // Mouse wheel with ctrl pressed or mouse_wheel_zoom enabled = zoom mode
                // Clear slider state to switch to ImageShader widget which handles zoom
                if app.use_slider_image_for_render {
                    app.use_slider_image_for_render = false;
                    for pane in app.panes.iter_mut() {
                        pane.slider_image_position = None;
                    }
                }
            }
            Task::none()
        }

        Event::Keyboard(iced_core::keyboard::Event::KeyPressed { key, modifiers, .. }) => {
            debug!("KeyPressed - Key pressed: {:?}, modifiers: {:?}", key, modifiers);
            debug!("modifiers.shift(): {}", modifiers.shift());
            let tasks = app.handle_key_pressed_event(&key, modifiers);

            if !tasks.is_empty() {
                return Task::batch(tasks);
            }
            Task::none()
        }

        Event::Keyboard(iced_core::keyboard::Event::KeyReleased { key, modifiers, .. }) => {
            let tasks = app.handle_key_released_event(&key, modifiers);
            if !tasks.is_empty() {
                return Task::batch(tasks);
            }
            Task::none()
        }

        #[cfg(any(target_os = "macos", target_os = "windows"))]
        Event::Window(iced_core::window::Event::FileDropped(dropped_paths, _position)) => {
            handle_window_file_drop(app, &dropped_paths[0])
        }

        #[cfg(target_os = "linux")]
        Event::Window(iced_core::window::Event::FileDropped(dropped_path, _)) => {
            handle_window_file_drop(app, &dropped_path[0])
        }

        _ => Task::none()
    }
}

// ============================================================================
// Helper functions
// ============================================================================

fn handle_window_file_drop(app: &mut DataViewer, path: &std::path::Path) -> Task<Message> {
    if app.pane_layout != PaneLayout::SinglePane {
        return Task::none();
    }

    // Check if it's a JSON file that might be COCO format
    #[cfg(feature = "coco")]
    if path.extension().and_then(|s| s.to_str()) == Some("json") {
        debug!("JSON file detected in window event, checking if it's COCO format: {}", path.display());
        match std::fs::read_to_string(path) {
            Ok(content) => {
                if crate::coco::parser::CocoDataset::is_coco_format(&content) {
                    info!("✓ Detected COCO JSON file: {}", path.display());
                    return Task::done(Message::CocoAction(
                        crate::coco::widget::CocoMessage::LoadCocoFile(path.to_path_buf())
                    ));
                } else {
                    debug!("JSON file is not COCO format, treating as regular file");
                }
            }
            Err(e) => {
                warn!("Failed to read JSON file: {}", e);
            }
        }
    }

    app.reset_state(-1);
    debug!("File dropped: {:?}", path);
    app.initialize_dir_path(&path.to_path_buf(), 0)
}

fn handle_file_dropped(app: &mut DataViewer, pane_index: isize, dropped_path: String) -> Task<Message> {
    let path = PathBuf::from(&dropped_path);

    #[cfg(feature = "coco")]
    debug!("COCO FEATURE IS ENABLED");
    #[cfg(not(feature = "coco"))]
    debug!("COCO FEATURE IS DISABLED");

    #[cfg(feature = "coco")]
    if path.extension().and_then(|s| s.to_str()) == Some("json") {
        debug!("JSON file detected, checking if it's COCO format: {}", path.display());
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                if crate::coco::parser::CocoDataset::is_coco_format(&content) {
                    info!("✓ Detected COCO JSON file: {}", path.display());
                    return Task::none();
                } else {
                    debug!("JSON file is not COCO format, treating as regular file");
                }
            }
            Err(e) => {
                warn!("Failed to read JSON file: {}", e);
            }
        }
    }

    debug!("Message::FileDropped - Resetting state");
    app.reset_state(pane_index);

    debug!("File dropped: {:?}, pane_index: {}", dropped_path, pane_index);
    debug!("self.dir_loaded, pane_index, last_opened_pane: {:?}, {}, {}",
        app.panes[pane_index as usize].dir_loaded, pane_index, app.last_opened_pane);
    app.initialize_dir_path(&path, pane_index as usize)
}

fn handle_save_settings(app: &mut DataViewer) -> Task<Message> {
    let parse_value = |key: &str, _default: u64| -> Result<u64, String> {
        app.settings.advanced_input
            .get(key)
            .ok_or_else(|| format!("Missing value for {}", key))?
            .parse::<u64>()
            .map_err(|_| format!("Invalid number for {}", key))
    };

    let cache_size = match parse_value("cache_size", 5) {
        Ok(v) if v > 0 && v <= 100 => v as usize,
        Ok(_) => {
            app.settings.set_save_status(Some("Error: Cache size must be between 1 and 100".to_string()));
            return Task::perform(async {
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            }, |_| Message::ClearSettingsStatus);
        }
        Err(e) => {
            app.settings.set_save_status(Some(format!("Error parsing cache_size: {}", e)));
            return Task::perform(async {
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            }, |_| Message::ClearSettingsStatus);
        }
    };

    let max_loading_queue_size = match parse_value("max_loading_queue_size", 3) {
        Ok(v) if v > 0 && v <= 50 => v as usize,
        Ok(_) => {
            app.settings.set_save_status(Some("Error: Max loading queue size must be between 1 and 50".to_string()));
            return Task::perform(async {
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            }, |_| Message::ClearSettingsStatus);
        }
        Err(e) => {
            app.settings.set_save_status(Some(format!("Error parsing max_loading_queue_size: {}", e)));
            return Task::perform(async {
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            }, |_| Message::ClearSettingsStatus);
        }
    };

    let max_being_loaded_queue_size = match parse_value("max_being_loaded_queue_size", 3) {
        Ok(v) if v > 0 && v <= 50 => v as usize,
        Ok(_) => {
            app.settings.set_save_status(Some("Error: Max being loaded queue size must be between 1 and 50".to_string()));
            return Task::perform(async {
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            }, |_| Message::ClearSettingsStatus);
        }
        Err(e) => {
            app.settings.set_save_status(Some(format!("Error parsing max_being_loaded_queue_size: {}", e)));
            return Task::perform(async {
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            }, |_| Message::ClearSettingsStatus);
        }
    };

    let window_width = match parse_value("window_width", 1200) {
        Ok(v) if (400..=10000).contains(&v) => v as u32,
        Ok(_) => {
            app.settings.set_save_status(Some("Error: Window width must be between 400 and 10000".to_string()));
            return Task::perform(async {
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            }, |_| Message::ClearSettingsStatus);
        }
        Err(e) => {
            app.settings.set_save_status(Some(format!("Error parsing window_width: {}", e)));
            return Task::perform(async {
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            }, |_| Message::ClearSettingsStatus);
        }
    };

    let window_height = match parse_value("window_height", 800) {
        Ok(v) if (300..=10000).contains(&v) => v as u32,
        Ok(_) => {
            app.settings.set_save_status(Some("Error: Window height must be between 300 and 10000".to_string()));
            return Task::perform(async {
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            }, |_| Message::ClearSettingsStatus);
        }
        Err(e) => {
            app.settings.set_save_status(Some(format!("Error parsing window_height: {}", e)));
            return Task::perform(async {
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            }, |_| Message::ClearSettingsStatus);
        }
    };

    let atlas_size = match parse_value("atlas_size", 2048) {
        Ok(v) if (256..=8192).contains(&v) && v.is_power_of_two() => v as u32,
        Ok(_) => {
            app.settings.set_save_status(Some("Error: Atlas size must be a power of 2 between 256 and 8192".to_string()));
            return Task::perform(async {
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            }, |_| Message::ClearSettingsStatus);
        }
        Err(e) => {
            app.settings.set_save_status(Some(format!("Error parsing atlas_size: {}", e)));
            return Task::perform(async {
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            }, |_| Message::ClearSettingsStatus);
        }
    };

    let double_click_threshold_ms = match parse_value("double_click_threshold_ms", 250) {
        Ok(v) if (50..=1000).contains(&v) => v as u16,
        Ok(_) => {
            app.settings.set_save_status(Some("Error: Double-click threshold must be between 50 and 1000 ms".to_string()));
            return Task::perform(async {
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            }, |_| Message::ClearSettingsStatus);
        }
        Err(e) => {
            app.settings.set_save_status(Some(format!("Error parsing double_click_threshold_ms: {}", e)));
            return Task::perform(async {
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            }, |_| Message::ClearSettingsStatus);
        }
    };

    let archive_cache_size = match parse_value("archive_cache_size", 200) {
        Ok(v) if (10..=10000).contains(&v) => v,
        Ok(_) => {
            app.settings.set_save_status(Some("Error: Archive cache size must be between 10 and 10000 MB".to_string()));
            return Task::perform(async {
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            }, |_| Message::ClearSettingsStatus);
        }
        Err(e) => {
            app.settings.set_save_status(Some(format!("Error parsing archive_cache_size: {}", e)));
            return Task::perform(async {
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            }, |_| Message::ClearSettingsStatus);
        }
    };

    let archive_warning_threshold_mb = match parse_value("archive_warning_threshold_mb", 500) {
        Ok(v) if (10..=10000).contains(&v) => v,
        Ok(_) => {
            app.settings.set_save_status(Some("Error: Archive warning threshold must be between 10 and 10000 MB".to_string()));
            return Task::perform(async {
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            }, |_| Message::ClearSettingsStatus);
        }
        Err(e) => {
            app.settings.set_save_status(Some(format!("Error parsing archive_warning_threshold_mb: {}", e)));
            return Task::perform(async {
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            }, |_| Message::ClearSettingsStatus);
        }
    };

    let settings = UserSettings {
        show_fps: app.show_fps,
        show_footer: app.show_footer,
        is_horizontal_split: app.is_horizontal_split,
        synced_zoom: app.synced_zoom,
        mouse_wheel_zoom: app.mouse_wheel_zoom,
        show_copy_buttons: app.show_copy_buttons,
        show_metadata: app.show_metadata,
        nearest_neighbor_filter: app.nearest_neighbor_filter,
        cache_strategy: match app.cache_strategy {
            CacheStrategy::Cpu => "cpu".to_string(),
            CacheStrategy::Gpu => "gpu".to_string(),
        },
        compression_strategy: match app.compression_strategy {
            CompressionStrategy::None => "none".to_string(),
            CompressionStrategy::Bc1 => "bc1".to_string(),
        },
        is_slider_dual: app.is_slider_dual,
        cache_size,
        max_loading_queue_size,
        max_being_loaded_queue_size,
        window_width,
        window_height,
        atlas_size,
        double_click_threshold_ms,
        archive_cache_size,
        archive_warning_threshold_mb,
        #[cfg(feature = "coco")]
        coco_disable_simplification: app.coco_disable_simplification,
        #[cfg(not(feature = "coco"))]
        coco_disable_simplification: false,
        #[cfg(feature = "coco")]
        coco_mask_render_mode: app.coco_mask_render_mode,
        #[cfg(not(feature = "coco"))]
        coco_mask_render_mode: crate::settings::CocoMaskRenderMode::default(),
        use_binary_size: app.use_binary_size,
    };

    let old_settings = UserSettings::load(None);
    let window_settings_changed = window_width != old_settings.window_width ||
                                  window_height != old_settings.window_height ||
                                  atlas_size != old_settings.atlas_size;

    match settings.save() {
        Ok(_) => {
            info!("Settings saved successfully");

            app.archive_cache_size = archive_cache_size * 1_048_576;
            app.archive_warning_threshold_mb = archive_warning_threshold_mb;
            info!("Archive settings applied immediately: cache_size={}MB, warning_threshold={}MB",
                archive_cache_size, archive_warning_threshold_mb);

            if cache_size != app.cache_size {
                info!("Cache size changed from {} to {}, reloading all panes", app.cache_size, cache_size);
                app.cache_size = cache_size;

                let pane_file_lengths: Vec<usize> = app.panes.iter()
                    .map(|p| p.img_cache.num_files)
                    .collect();

                let cache_size = app.cache_size;
                let archive_cache_size = app.archive_cache_size;
                let archive_warning_threshold_mb = app.archive_warning_threshold_mb;

                for (i, pane) in app.panes.iter_mut().enumerate() {
                    if let Some(dir_path) = &pane.directory_path.clone() {
                        if pane.dir_loaded {
                            let path = PathBuf::from(dir_path);

                            let _ = pane.initialize_dir_path(
                                &Arc::clone(&app.device),
                                &Arc::clone(&app.queue),
                                app.is_gpu_supported,
                                app.cache_strategy,
                                app.compression_strategy,
                                &app.pane_layout,
                                &pane_file_lengths,
                                i,
                                &path,
                                app.is_slider_dual,
                                &mut app.slider_value,
                                cache_size,
                                archive_cache_size,
                                archive_warning_threshold_mb,
                            );
                        }
                    }
                }
            }

            if max_loading_queue_size != app.max_loading_queue_size || max_being_loaded_queue_size != app.max_being_loaded_queue_size {
                info!("Queue size settings changed: max_loading_queue_size={}, max_being_loaded_queue_size={}", max_loading_queue_size, max_being_loaded_queue_size);
                app.max_loading_queue_size = max_loading_queue_size;
                app.max_being_loaded_queue_size = max_being_loaded_queue_size;

                for pane in app.panes.iter_mut() {
                    pane.max_loading_queue_size = max_loading_queue_size;
                    pane.max_being_loaded_queue_size = max_being_loaded_queue_size;
                }
            }

            if double_click_threshold_ms != app.double_click_threshold_ms {
                info!("Double-click threshold changed from {} to {} ms", app.double_click_threshold_ms, double_click_threshold_ms);
                app.double_click_threshold_ms = double_click_threshold_ms;
            }

            app.settings.set_save_status(Some(if window_settings_changed {
                "Settings saved! Window settings require restart, other changes applied immediately.".to_string()
            } else {
                "Settings saved! All changes applied immediately.".to_string()
            }));

            Task::perform(async {
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            }, |_| Message::ClearSettingsStatus)
        }
        Err(e) => {
            error!("Failed to save settings: {}", e);
            app.settings.set_save_status(Some(format!("Error: {}", e)));

            Task::perform(async {
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            }, |_| Message::ClearSettingsStatus)
        }
    }
}

fn handle_reset_advanced_settings(app: &mut DataViewer) {
    use crate::config;

    app.show_fps = false;
    app.show_footer = true;
    app.is_horizontal_split = false;
    app.synced_zoom = true;
    app.mouse_wheel_zoom = false;
    app.cache_strategy = CacheStrategy::Gpu;
    app.compression_strategy = CompressionStrategy::None;
    app.is_slider_dual = false;

    app.settings.advanced_input.insert("cache_size".to_string(), config::DEFAULT_CACHE_SIZE.to_string());
    app.settings.advanced_input.insert("max_loading_queue_size".to_string(), config::DEFAULT_MAX_LOADING_QUEUE_SIZE.to_string());
    app.settings.advanced_input.insert("max_being_loaded_queue_size".to_string(), config::DEFAULT_MAX_BEING_LOADED_QUEUE_SIZE.to_string());
    app.settings.advanced_input.insert("window_width".to_string(), config::DEFAULT_WINDOW_WIDTH.to_string());
    app.settings.advanced_input.insert("window_height".to_string(), config::DEFAULT_WINDOW_HEIGHT.to_string());
    app.settings.advanced_input.insert("atlas_size".to_string(), config::DEFAULT_ATLAS_SIZE.to_string());
    app.settings.advanced_input.insert("double_click_threshold_ms".to_string(), config::DEFAULT_DOUBLE_CLICK_THRESHOLD_MS.to_string());
    app.settings.advanced_input.insert("archive_cache_size".to_string(), config::DEFAULT_ARCHIVE_CACHE_SIZE.to_string());
    app.settings.advanced_input.insert("archive_warning_threshold_mb".to_string(), config::DEFAULT_ARCHIVE_WARNING_THRESHOLD_MB.to_string());
}

fn handle_export_all_logs() {
    println!("DEBUG: ExportAllLogs message received");
    let app_name = "viewskater";
    if let Some(log_buffer) = crate::get_shared_log_buffer() {
        println!("DEBUG: Got log buffer, starting export...");
        if let Some(stdout_buffer) = crate::get_shared_stdout_buffer() {
            println!("DEBUG: Got stdout buffer, calling export_and_open_all_logs...");
            crate::logging::export_and_open_all_logs(app_name, log_buffer, stdout_buffer);
            println!("DEBUG: export_and_open_all_logs completed");
        } else {
            println!("DEBUG: Stdout buffer not available, exporting debug logs only");
            match crate::logging::export_debug_logs(app_name, log_buffer) {
                Ok(debug_log_path) => {
                    println!("DEBUG: Export successful to: {}", debug_log_path.display());
                    info!("Debug logs successfully exported to: {}", debug_log_path.display());
                }
                Err(e) => {
                    println!("DEBUG: Export failed: {}", e);
                    error!("Failed to export debug logs: {}", e);
                    eprintln!("Failed to export debug logs: {}", e);
                }
            }
        }
        println!("DEBUG: Export operation completed");
    } else {
        println!("DEBUG: Log buffer not available");
        warn!("Log buffer not available for export");
    }
    println!("DEBUG: ExportAllLogs handler finished");
}
