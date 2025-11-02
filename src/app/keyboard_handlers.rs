use log::debug;
use iced_core::keyboard::{self, Key, key::Named};
use iced_winit::runtime::Task;

use crate::app::{DataViewer, Message};
use crate::menu::PaneLayout;
use crate::file_io;
use crate::navigation_keyboard::{move_right_all, move_left_all};

// Helper function to check for the platform-appropriate modifier key
fn is_platform_modifier(modifiers: &keyboard::Modifiers) -> bool {
    #[cfg(target_os = "macos")]
    return modifiers.logo(); // Use Command key on macOS

    #[cfg(not(target_os = "macos"))]
    return modifiers.control(); // Use Control key on other platforms
}

impl DataViewer {
    pub(crate) fn handle_key_pressed_event(&mut self, key: &keyboard::Key, modifiers: keyboard::Modifiers) -> Vec<Task<Message>> {
        let mut tasks = Vec::new();

        match key.as_ref() {
            Key::Named(Named::Tab) => {
                debug!("Tab pressed");
                self.toggle_footer();
            }

            Key::Named(Named::Space) | Key::Character("b") => {
                debug!("Space pressed");
                self.toggle_slider_type();
            }

            Key::Character("h") | Key::Character("H") => {
                debug!("H key pressed");
                // Only toggle split orientation in dual pane mode
                if self.pane_layout == PaneLayout::DualPane {
                    self.toggle_split_orientation();
                }
            }

            Key::Character("1") => {
                debug!("Key1 pressed");
                if self.pane_layout == PaneLayout::DualPane && self.is_slider_dual {
                    self.panes[0].is_selected = !self.panes[0].is_selected;
                }

                // If shift+alt is pressed, load a file into pane0
                if modifiers.shift() && modifiers.alt() {
                    debug!("Key1 Shift+Alt pressed");
                    tasks.push(Task::perform(file_io::pick_file(), move |result| {
                        Message::FolderOpened(result, 0)
                    }));
                }

                // If alt is pressed, load a folder into pane0
                else if modifiers.alt() {
                    debug!("Key1 Alt pressed");
                    tasks.push(Task::perform(file_io::pick_folder(), move |result| {
                        Message::FolderOpened(result, 0)
                    }));
                }

                // If platform_modifier is pressed, switch to single pane layout
                else if is_platform_modifier(&modifiers) {
                    self.toggle_pane_layout(PaneLayout::SinglePane);
                }
            }
            Key::Character("2") => {
                debug!("Key2 pressed");
                if self.pane_layout == PaneLayout::DualPane {
                    if self.is_slider_dual {
                        self.panes[1].is_selected = !self.panes[1].is_selected;
                    }

                    // If shift+alt is pressed, load a file into pane1
                    if modifiers.shift() && modifiers.alt() {
                        debug!("Key2 Shift+Alt pressed");
                        tasks.push(Task::perform(file_io::pick_file(), move |result| {
                            Message::FolderOpened(result, 1)
                        }));
                    }

                    // If alt is pressed, load a folder into pane1
                    else if modifiers.alt() {
                        debug!("Key2 Alt pressed");
                        tasks.push(Task::perform(file_io::pick_folder(), move |result| {
                            Message::FolderOpened(result, 1)
                        }));
                    }
                }

                // If platform_modifier is pressed, switch to dual pane layout
                else if is_platform_modifier(&modifiers) {
                    debug!("Key2 Ctrl pressed");
                    self.toggle_pane_layout(PaneLayout::DualPane);
                }
            }

            Key::Character("c") |
            Key::Character("w") => {
                // Close the selected panes
                if is_platform_modifier(&modifiers) {
                    self.reset_state(-1);
                }
            }

            Key::Character("q") => {
                // Terminate the app
                if is_platform_modifier(&modifiers) {
                    std::process::exit(0);
                }
            }

            Key::Character("o") => {
                // If platform_modifier is pressed, open a file or folder
                if is_platform_modifier(&modifiers) {
                    let pane_index = if self.pane_layout == PaneLayout::SinglePane {
                        0 // Use first pane in single-pane mode
                    } else {
                        self.last_opened_pane as usize // Use last opened pane in dual-pane mode
                    };
                    debug!("o key pressed pane_index: {}", pane_index);

                    // If shift is pressed or we have uppercase O, open folder
                    if modifiers.shift() {
                        debug!("Opening folder with platform_modifier+shift+o");
                        tasks.push(Task::perform(file_io::pick_folder(), move |result| {
                            Message::FolderOpened(result, pane_index)
                        }));
                    } else {
                        // Otherwise open file
                        debug!("Opening file with platform_modifier+o");
                        tasks.push(Task::perform(file_io::pick_file(), move |result| {
                            Message::FolderOpened(result, pane_index)
                        }));
                    }
                }
            }

            Key::Named(Named::ArrowLeft) | Key::Character("a") => {
                // Check for first image navigation with platform modifier or Fn key
                if is_platform_modifier(&modifiers) {
                    debug!("Navigating to first image");

                    // Find which panes need to be updated
                    let mut operations = Vec::new();

                    for (idx, pane) in self.panes.iter_mut().enumerate() {
                        if pane.dir_loaded && (pane.is_selected || self.is_slider_dual) {
                            // Navigate to the first image (index 0)
                            if pane.img_cache.current_index > 0 {
                                let new_pos = 0;
                                pane.slider_value = new_pos as u16;
                                self.slider_value = new_pos as u16;

                                // Save the operation for later execution
                                operations.push((idx as isize, new_pos));
                            }
                        }
                    }

                    // Now execute all operations after the loop is complete
                    for (pane_idx, new_pos) in operations {
                        tasks.push(crate::navigation_slider::load_remaining_images(
                            &self.device,
                            &self.queue,
                            self.is_gpu_supported,
                            self.cache_strategy,
                            self.compression_strategy,
                            &mut self.panes,
                            &mut self.loading_status,
                            pane_idx,
                            new_pos,
                        ));
                    }

                    return tasks;
                }

                // Existing left-arrow logic
                if self.skate_right {
                    self.skate_right = false;

                    // Discard all queue items that are LoadNext or ShiftNext
                    self.loading_status.reset_load_next_queue_items();
                }

                if self.pane_layout == PaneLayout::DualPane && self.is_slider_dual && !self.panes.iter().any(|pane| pane.is_selected) {
                    debug!("No panes selected");
                }

                if self.skate_left {
                    // will be handled at the end of update() to run move_left_all
                } else if modifiers.shift() {
                    self.skate_left = true;
                } else {
                    self.skate_left = false;

                    debug!("move_left_all from handle_key_pressed_event()");
                    let task = move_left_all(
                        &self.device,
                        &self.queue,
                        self.cache_strategy,
                        self.compression_strategy,
                        &mut self.panes,
                        &mut self.loading_status,
                        &mut self.slider_value,
                        &self.pane_layout,
                        self.is_slider_dual,
                        self.last_opened_pane as usize);
                    tasks.push(task);
                }
            }
            Key::Named(Named::ArrowRight) | Key::Character("d") => {
                // Check for last image navigation with platform modifier or Fn key
                if is_platform_modifier(&modifiers) {
                    debug!("Navigating to last image");

                    // Find which panes need to be updated
                    let mut operations = Vec::new();

                    for (idx, pane) in self.panes.iter_mut().enumerate() {
                        if pane.dir_loaded && (pane.is_selected || self.is_slider_dual) {
                            // Get the last valid index
                            if let Some(last_index) = pane.img_cache.image_paths.len().checked_sub(1) {
                                if pane.img_cache.current_index < last_index {
                                    let new_pos = last_index;
                                    pane.slider_value = new_pos as u16;
                                    self.slider_value = new_pos as u16;

                                    // Save the operation for later execution
                                    operations.push((idx as isize, new_pos));
                                }
                            }
                        }
                    }

                    // Now execute all operations after the loop is complete
                    for (pane_idx, new_pos) in operations {
                        tasks.push(crate::navigation_slider::load_remaining_images(
                            &self.device,
                            &self.queue,
                            self.is_gpu_supported,
                            self.cache_strategy,
                            self.compression_strategy,
                            &mut self.panes,
                            &mut self.loading_status,
                            pane_idx,
                            new_pos,
                        ));
                    }

                    return tasks;
                }

                // Existing right-arrow logic
                debug!("Right key or 'D' key pressed!");
                if self.skate_left {
                    self.skate_left = false;

                    // Discard all queue items that are LoadPrevious or ShiftPrevious
                    self.loading_status.reset_load_previous_queue_items();
                }

                if self.pane_layout == PaneLayout::DualPane && self.is_slider_dual && !self.panes.iter().any(|pane| pane.is_selected) {
                    debug!("No panes selected");
                }

                if modifiers.shift() {
                    self.skate_right = true;
                } else {
                    self.skate_right = false;

                    let task = move_right_all(
                        &self.device,
                        &self.queue,
                        self.cache_strategy,
                        self.compression_strategy,
                        &mut self.panes,
                        &mut self.loading_status,
                        &mut self.slider_value,
                        &self.pane_layout,
                        self.is_slider_dual,
                        self.last_opened_pane as usize);
                    tasks.push(task);
                    debug!("handle_key_pressed_event() - tasks count: {}", tasks.len());
                }
            }

            Key::Named(Named::F3)  => {
                self.show_fps = !self.show_fps;
                debug!("Toggled debug FPS display: {}", self.show_fps);
            }

            Key::Named(Named::Super) => {
                #[cfg(target_os = "macos")] {
                    self.set_ctrl_pressed(true);
                }
            }
            Key::Named(Named::Control) => {
                #[cfg(not(target_os = "macos"))] {
                    self.set_ctrl_pressed(true);
                }
            }
            _ => {
                // Check if ML module wants to handle this key
                #[cfg(feature = "ml")]
                if let Some(task) = crate::ml_widget::handle_keyboard_event(
                    key,
                    modifiers,
                    &self.pane_layout,
                    self.last_opened_pane,
                ) {
                    tasks.push(task);
                }

                // Check if COCO module wants to handle this key
                #[cfg(feature = "coco")]
                if let Some(task) = crate::coco::widget::handle_keyboard_event(
                    key,
                    modifiers,
                    &self.pane_layout,
                    self.last_opened_pane,
                ) {
                    tasks.push(task);
                }
            }
        }

        tasks
    }

    pub(crate) fn handle_key_released_event(&mut self, key_code: &keyboard::Key, _modifiers: keyboard::Modifiers) -> Vec<Task<Message>> {
        #[allow(unused_mut)]
        let mut tasks = Vec::new();

        match key_code.as_ref() {
            Key::Named(Named::Tab) => {
                debug!("Tab released");
            }
            Key::Named(Named::Enter) | Key::Character("NumpadEnter")  => {
                debug!("Enter key released!");

            }
            Key::Named(Named::Escape) => {
                debug!("Escape key released!");

            }
            Key::Named(Named::ArrowLeft) | Key::Character("a") => {
                debug!("Left key or 'A' key released!");
                self.skate_left = false;
            }
            Key::Named(Named::ArrowRight) | Key::Character("d") => {
                debug!("Right key or 'D' key released!");
                self.skate_right = false;
            }
            Key::Named(Named::Super) => {
                #[cfg(target_os = "macos")] {
                    self.set_ctrl_pressed(false);
                }
            }
            Key::Named(Named::Control) => {
                #[cfg(not(target_os = "macos"))] {
                    self.set_ctrl_pressed(false);
                }
            }
            _ => {},
        }

        tasks
    }
}
