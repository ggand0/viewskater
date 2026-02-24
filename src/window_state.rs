use iced_winit::winit;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::monitor::MonitorHandle;
use log::error;

use crate::settings::WindowState;
use crate::app::DataViewer;

/// Minimum visible pixels required to consider a window on-screen.
const VISIBLE_SIZE: i32 = 30;

/// Checks whether a window is visible on its monitor.
/// Returns (is_visible, corrected_position). If not visible,
/// corrected_position is snapped to the monitor's origin.
pub fn get_window_visible(
    current_position: PhysicalPosition<i32>,
    current_size: PhysicalSize<u32>,
    monitor: Option<MonitorHandle>,
) -> (bool, PhysicalPosition<i32>) {
    let mut cx = current_position.x;
    let mut cy = current_position.y;
    let mut visible = true;

    if let Some(mh) = monitor {
        let mut plus_area = mh.position();
        let mut minus_area = mh.position();
        plus_area.x += mh.size().width as i32 - VISIBLE_SIZE;
        plus_area.y += mh.size().height as i32 - VISIBLE_SIZE;
        minus_area.x -= current_size.width as i32 - VISIBLE_SIZE;
        minus_area.y -= current_size.height as i32 - VISIBLE_SIZE;
        if cx >= plus_area.x || cy >= plus_area.y || cx <= minus_area.x || cy <= minus_area.y {
            visible = false;
            cx = mh.position().x;
            cy = mh.position().y;
        }
    }

    (visible, PhysicalPosition::new(cx, cy))
}

/// Saves the current window state from the iced application state to disk.
pub fn save_window_state_to_disk(app: &DataViewer) {
    let mut settings = crate::settings::UserSettings::load(None);
    let mut pos = app.last_windowed_position;
    let tuple = get_window_visible(pos, app.window_size, app.last_monitor.clone());
    if !tuple.0 {
        pos = tuple.1;
    }
    settings.window_position_x = pos.x;
    settings.window_position_y = pos.y;
    if app.window_state == WindowState::Window {
        settings.window_width = app.window_size.width;
        settings.window_height = app.window_size.height;
    }
    settings.window_state = app.window_state;
    if let Err(e) = settings.save() {
        error!("Failed to save window state: {e}");
    }
}

/// macOS: zoom to maximize if needed, and register a termination observer
/// to persist window state on Cmd+Q.
///
/// Winit creates a native Quit menu item with Cmd+Q → `[NSApp terminate:]`,
/// which bypasses winit's event loop (CloseRequested never fires, keyboard
/// handler never sees Cmd+Q). The observer reads the NSWindow frame directly
/// at termination time — authoritative and animation-free.
#[cfg(target_os = "macos")]
pub fn setup_macos_window(window: &winit::window::Window) {
    use objc2_app_kit::{NSView, NSScreen};
    use objc2_foundation::{MainThreadMarker, NSNotificationCenter, NSNotification};
    use block2::RcBlock;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Ok(handle) = window.window_handle() else { return };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else { return };

    let ns_view = appkit.ns_view.as_ptr() as *mut objc2::runtime::AnyObject;
    let ns_view: &NSView = unsafe { &*(ns_view as *const NSView) };
    let Some(ns_window) = ns_view.window() else { return };

    // Zoom to maximize if needed. zoom() saves the current (unzoomed) frame
    // to _savedFrame, so double-click title bar unzoom works correctly.
    if crate::config::CONFIG.window_state == WindowState::Maximized {
        ns_window.zoom(None);
    }

    // Register NSApplicationWillTerminateNotification observer.
    // This fires on Cmd+Q before exit(), letting us save state.
    let ns_win = ns_window.clone();
    let block = RcBlock::new(move |_: std::ptr::NonNull<NSNotification>| {
        let mut settings = crate::settings::UserSettings::load(None);

        if ns_win.isZoomed() {
            // Only save the state flag. The windowed position/size in settings
            // is correct from the last Focused(false) save or from CONFIG.
            settings.window_state = WindowState::Maximized;
        } else {
            let frame = ns_win.frame();
            let scale = ns_win.backingScaleFactor();

            // Convert position: macOS bottom-left origin (points)
            // → top-left origin (physical pixels)
            let screen_height = MainThreadMarker::new()
                .and_then(|mtm| NSScreen::mainScreen(mtm))
                .map(|s| s.frame().size.height)
                .unwrap_or(0.0);
            let x = (frame.origin.x * scale) as i32;
            let y = ((screen_height - frame.origin.y - frame.size.height) * scale) as i32;

            settings.window_position_x = x;
            settings.window_position_y = y;

            // Inner size (content area) in physical pixels
            if let Some(content_view) = ns_win.contentView() {
                let content = content_view.frame();
                settings.window_width = (content.size.width * scale) as u32;
                settings.window_height = (content.size.height * scale) as u32;
            }

            settings.window_state = WindowState::Window;
        }

        if let Err(e) = settings.save() {
            log::error!("Failed to save window state on termination: {e}");
        }
    });

    let center = unsafe { NSNotificationCenter::defaultCenter() };
    let observer = unsafe {
        center.addObserverForName_object_queue_usingBlock(
            Some(objc2_app_kit::NSApplicationWillTerminateNotification),
            None,
            None,
            &block,
        )
    };
    // Keep the observer alive for the entire process lifetime
    std::mem::forget(observer);
}
