//! Replay mode integration for DataViewer
//!
//! Handles replay controller updates and action processing for automated benchmarking.

use std::time::Duration;
use iced_winit::runtime::Task;
use log::{debug, info, warn};

use super::{DataViewer, Message};

impl DataViewer {
    /// Update replay mode logic and return any action that should be processed
    pub(crate) fn update_replay_mode(&mut self) -> Option<crate::replay::ReplayAction> {
        let replay_controller = self.replay_controller.as_mut()?;

        if !replay_controller.is_active() && !replay_controller.is_completed()
           && !replay_controller.config.test_directories.is_empty() {
            // Start replay if we have a controller but it's not active yet and not completed
            replay_controller.start();
            // Get the first directory for loading
            return replay_controller.get_current_directory().map(|dir| {
                crate::replay::ReplayAction::LoadDirectory(dir.clone())
            });
        }

        if !replay_controller.is_active() {
            return None;
        }

        debug!("App update called during active replay mode, state: {:?}", replay_controller.state);

        // Update metrics with current FPS and memory values
        let ui_fps = crate::CURRENT_FPS.lock().map(|fps| *fps).unwrap_or(0.0);
        let image_fps = if self.is_slider_moving {
            iced_wgpu::get_image_fps() as f32
        } else {
            crate::pane::IMAGE_RENDER_FPS.lock().map(|fps| *fps).unwrap_or(0.0)
        };
        let memory_mb = crate::CURRENT_MEMORY_USAGE.lock()
            .map(|mem| if *mem == u64::MAX { -1.0 } else { *mem as f64 / 1024.0 / 1024.0 })
            .unwrap_or(0.0);

        replay_controller.update_metrics(ui_fps, image_fps, memory_mb);

        // Extract state info before mutable operations (to satisfy borrow checker)
        let (state_type, start_time_elapsed) = match &replay_controller.state {
            crate::replay::ReplayState::NavigatingRight { start_time, .. } => ("right", Some(start_time.elapsed())),
            crate::replay::ReplayState::NavigatingLeft { start_time, .. } => ("left", Some(start_time.elapsed())),
            _ => ("other", None),
        };
        let duration_limit = replay_controller.config.duration_per_directory + Duration::from_secs(1);

        // Synchronize app navigation state with replay controller state
        match state_type {
            "right" => {
                // Check if we're at the end of images
                let at_end = self.panes.iter().any(|pane| {
                    pane.is_selected && pane.dir_loaded &&
                    pane.img_cache.current_index >= pane.img_cache.image_paths.len().saturating_sub(1)
                });

                // Notify replay controller about boundary state (affects metrics collection)
                replay_controller.set_at_boundary(at_end);

                if at_end && self.skate_right {
                    debug!("Reached end of images, stopping right navigation");
                    self.skate_right = false;
                } else if !at_end && !self.skate_right {
                    debug!("Syncing app state: setting skate_right = true");
                    self.skate_right = true;
                    self.skate_left = false;
                }

                // Force progress if stuck for too long
                if let Some(elapsed) = start_time_elapsed {
                    if elapsed > duration_limit {
                        warn!("Replay seems stuck in NavigatingRight state, forcing progress");
                        self.skate_right = false;
                    }
                }
            }
            "left" => {
                // Check if we're at the beginning of images
                let at_beginning = self.panes.iter().any(|pane| {
                    pane.is_selected && pane.dir_loaded && pane.img_cache.current_index == 0
                });

                // Notify replay controller about boundary state (affects metrics collection)
                replay_controller.set_at_boundary(at_beginning);

                if at_beginning && self.skate_left {
                    debug!("Reached beginning of images, stopping left navigation");
                    self.skate_left = false;
                } else if !at_beginning && !self.skate_left {
                    debug!("Syncing app state: setting skate_left = true");
                    self.skate_left = true;
                    self.skate_right = false;
                }

                // Force progress if stuck for too long
                if let Some(elapsed) = start_time_elapsed {
                    if elapsed > duration_limit {
                        warn!("Replay seems stuck in NavigatingLeft state, forcing progress");
                        self.skate_left = false;
                    }
                }
            }
            _ => {
                // For other states, ensure navigation flags are cleared
                if self.skate_right || self.skate_left {
                    debug!("Syncing app state: clearing navigation flags");
                    self.skate_right = false;
                    self.skate_left = false;
                }
            }
        }

        // Get action from replay controller
        let action = replay_controller.update();
        if let Some(ref a) = action {
            debug!("Replay controller returned action: {:?}", a);
        }

        // Schedule keep-alive task if replay is active and we don't already have one in flight
        // This prevents accumulating many delayed messages when update() is called rapidly
        if replay_controller.is_active() && !self.replay_keep_alive_pending {
            self.replay_keep_alive_task = Some(Task::perform(
                async { tokio::time::sleep(tokio::time::Duration::from_millis(50)).await; },
                |_| Message::ReplayKeepAlive
            ));
        }

        action
    }

    /// Process a replay action and return the appropriate task
    pub(crate) fn process_replay_action(&mut self, action: crate::replay::ReplayAction) -> Option<Task<Message>> {
        match action {
            crate::replay::ReplayAction::LoadDirectory(path) => {
                info!("Loading directory for replay: {}", path.display());
                self.reset_state(-1);

                // Reset FPS counters and timing history for fresh measurements
                if let Ok(mut fps) = crate::CURRENT_FPS.lock() { *fps = 0.0; }
                if let Ok(mut fps) = crate::pane::IMAGE_RENDER_FPS.lock() { *fps = 0.0; }
                if let Ok(mut times) = crate::FRAME_TIMES.lock() { times.clear(); }
                if let Ok(mut times) = crate::pane::IMAGE_RENDER_TIMES.lock() { times.clear(); }
                iced_wgpu::reset_image_fps();

                // Initialize directory and get the image loading task
                let load_task = self.initialize_dir_path(&path, 0);

                // Notify replay controller that directory loading started
                // on_ready_to_navigate() will be called when ImagesLoaded (LoadPos) completes
                if let Some(ref mut replay_controller) = self.replay_controller {
                    if let Some(directory_index) = replay_controller.config.test_directories.iter().position(|p| p == &path) {
                        replay_controller.on_directory_loaded(directory_index);
                    }
                }

                // Return the load task so images actually get loaded
                Some(load_task)
            }
            crate::replay::ReplayAction::RestartIteration(path) => {
                // Restart iteration by fully reloading the first directory
                // This ensures pane state is properly reset to the beginning
                info!("Restarting iteration - loading directory: {}", path.display());
                self.reset_state(-1);

                // Reset FPS counters and timing history for fresh measurements
                if let Ok(mut fps) = crate::CURRENT_FPS.lock() { *fps = 0.0; }
                if let Ok(mut fps) = crate::pane::IMAGE_RENDER_FPS.lock() { *fps = 0.0; }
                if let Ok(mut times) = crate::FRAME_TIMES.lock() { times.clear(); }
                if let Ok(mut times) = crate::pane::IMAGE_RENDER_TIMES.lock() { times.clear(); }
                iced_wgpu::reset_image_fps();

                // Initialize directory and get the image loading task
                let load_task = self.initialize_dir_path(&path, 0);

                // Notify replay controller that directory loading started
                if let Some(ref mut replay_controller) = self.replay_controller {
                    if let Some(directory_index) = replay_controller.config.test_directories.iter().position(|p| p == &path) {
                        replay_controller.on_directory_loaded(directory_index);
                    }
                }

                Some(load_task)
            }
            crate::replay::ReplayAction::NavigateRight => {
                self.skate_right = true;
                if let Some(ref mut replay_controller) = self.replay_controller {
                    replay_controller.on_navigation_performed();
                }
                None
            }
            crate::replay::ReplayAction::NavigateLeft => {
                self.skate_left = true;
                if let Some(ref mut replay_controller) = self.replay_controller {
                    replay_controller.on_navigation_performed();
                }
                None
            }
            crate::replay::ReplayAction::StartNavigatingLeft => {
                // Reset FPS trackers before starting left navigation
                if let Ok(mut fps) = crate::CURRENT_FPS.lock() { *fps = 0.0; }
                if let Ok(mut fps) = crate::pane::IMAGE_RENDER_FPS.lock() { *fps = 0.0; }
                if let Ok(mut times) = crate::FRAME_TIMES.lock() { times.clear(); }
                if let Ok(mut times) = crate::pane::IMAGE_RENDER_TIMES.lock() { times.clear(); }
                iced_wgpu::reset_image_fps();

                self.skate_right = false;
                self.skate_left = true;
                if let Some(ref mut replay_controller) = self.replay_controller {
                    replay_controller.on_navigation_performed();
                }
                None
            }
            crate::replay::ReplayAction::Finish => {
                info!("Replay mode finished");
                if let Some(ref controller) = self.replay_controller {
                    if controller.config.auto_exit {
                        info!("Auto-exit enabled, exiting application");
                        std::process::exit(0);
                    }
                }
                None
            }
        }
    }
}
