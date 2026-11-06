//! The dictation pill: a small always-on-top, click-through, non-activating
//! window that shows recording / processing / result state.
//!
//! `Overlay::set` is called from the controller thread. Every window or panel
//! operation is dispatched with `run_on_main_thread` (AppKit and Win32 window
//! calls must run on the UI thread); emitting events is thread-safe.
use crate::state::AppState;
use dictation_core::controller::{Overlay, OverlayState};
use dictation_core::settings::OverlayPosition;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition};

/// Window label of the overlay in tauri.conf.json.
pub const OVERLAY_LABEL: &str = "overlay";
/// Logical size from tauri.conf.json.
const WIDTH: f64 = 120.0;
const HEIGHT: f64 = 36.0;
/// Logical distance from the screen edge.
const MARGIN: f64 = 24.0;
/// Matches the CSS fade-out so the pill fades before the window goes away.
const FADE_OUT: Duration = Duration::from_millis(150);

/// Top-left physical position of the pill inside `area` = (x, y, width, height)
/// in physical pixels, for a monitor with the given scale factor.
pub fn compute_position(
    area: (i32, i32, u32, u32),
    scale: f64,
    pos: &OverlayPosition,
) -> (i32, i32) {
    let (ax, ay, aw, ah) = area;
    let w = (WIDTH * scale).round() as i32;
    let h = (HEIGHT * scale).round() as i32;
    let m = (MARGIN * scale).round() as i32;
    let (aw, ah) = (aw as i32, ah as i32);
    let x = match pos {
        OverlayPosition::BottomCenter | OverlayPosition::TopCenter => ax + (aw - w) / 2,
        OverlayPosition::BottomLeft => ax + m,
        OverlayPosition::BottomRight => ax + aw - w - m,
    };
    let y = match pos {
        OverlayPosition::TopCenter => ay + m,
        _ => ay + ah - h - m,
    };
    (x, y)
}

pub struct TauriOverlay {
    app: AppHandle,
    state: Arc<AppState>,
    /// Bumped on every `set`; a pending hide only fires if it still matches.
    generation: Arc<AtomicU64>,
    visible: Arc<AtomicBool>,
}

impl TauriOverlay {
    pub fn new(app: AppHandle, state: Arc<AppState>) -> Self {
        TauriOverlay {
            app,
            state,
            generation: Arc::new(AtomicU64::new(0)),
            visible: Arc::new(AtomicBool::new(false)),
        }
    }

    fn show(&self, reposition: bool) {
        let app = self.app.clone();
        let position = self.state.settings_snapshot().overlay_position;
        let visible = self.visible.clone();
        let res = self.app.run_on_main_thread(move || {
            if reposition || !visible.load(Ordering::SeqCst) {
                place(&app, &position);
            }
            platform_show(&app);
            visible.store(true, Ordering::SeqCst);
        });
        if let Err(e) = res {
            log::warn!("overlay show failed: {e}");
        }
    }

    fn hide_now(app: &AppHandle, visible: &AtomicBool) {
        visible.store(false, Ordering::SeqCst);
        let app2 = app.clone();
        if let Err(e) = app.run_on_main_thread(move || platform_hide(&app2)) {
            log::warn!("overlay hide failed: {e}");
        }
    }

    /// Hide after `delay`, unless a newer state arrives first.
    fn hide_after(&self, delay: Duration, generation: u64) {
        let app = self.app.clone();
        let gen_counter = self.generation.clone();
        let visible = self.visible.clone();
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            if gen_counter.load(Ordering::SeqCst) != generation {
                return;
            }
            // Let the webview fade out and stop its animation loop first.
            let _ = app.emit_to(OVERLAY_LABEL, "overlay-state", OverlayState::Hidden);
            std::thread::sleep(FADE_OUT);
            if gen_counter.load(Ordering::SeqCst) == generation {
                Self::hide_now(&app, &visible);
            }
        });
    }
}

impl Overlay for TauriOverlay {
    fn set(&self, s: OverlayState) {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let delay = match &s {
            OverlayState::Hidden => {
                let _ = self
                    .app
                    .emit_to(OVERLAY_LABEL, "overlay-state", OverlayState::Hidden);
                Self::hide_now(&self.app, &self.visible);
                return;
            }
            OverlayState::Recording => {
                self.show(true);
                None
            }
            OverlayState::Processing => {
                self.show(false);
                None
            }
            OverlayState::Done => {
                self.show(false);
                Some(Duration::from_millis(700))
            }
            OverlayState::NoSpeech
            | OverlayState::Copied
            | OverlayState::MicError(_)
            | OverlayState::NoModel
            | OverlayState::Error(_) => {
                self.show(false);
                Some(Duration::from_millis(1800))
            }
        };
        let _ = self.app.emit_to(OVERLAY_LABEL, "overlay-state", &s);
        if let Some(d) = delay {
            self.hide_after(d, generation);
        }
    }
}

/// Move the overlay to the monitor that contains the cursor (primary monitor
/// as fallback). Must run on the main thread.
fn place(app: &AppHandle, pos: &OverlayPosition) {
    let Some(window) = app.get_webview_window(OVERLAY_LABEL) else {
        return;
    };
    let cursor = app.cursor_position().ok();
    let monitors = app.available_monitors().unwrap_or_default();
    let under_cursor = cursor.and_then(|c| {
        monitors.iter().find(|m| {
            let p = m.position();
            let s = m.size();
            c.x >= p.x as f64
                && c.y >= p.y as f64
                && c.x < p.x as f64 + s.width as f64
                && c.y < p.y as f64 + s.height as f64
        })
    });
    let primary = app.primary_monitor().ok().flatten();
    let Some(monitor) = under_cursor.or(primary.as_ref()).or(monitors.first()) else {
        return;
    };
    // The work area excludes the Dock / taskbar / menu bar.
    let wa = monitor.work_area();
    let area = (wa.position.x, wa.position.y, wa.size.width, wa.size.height);
    let (x, y) = compute_position(area, monitor.scale_factor(), pos);
    if let Err(e) = window.set_position(PhysicalPosition::new(x, y)) {
        log::warn!("overlay set_position failed: {e}");
    }
}

// ---- platform glue -------------------------------------------------------

#[cfg(target_os = "macos")]
mod mac {
    use tauri_nspanel::tauri_panel;
    tauri_panel! {
        panel!(OverlayPanel {
            config: {
                can_become_key_window: false,
                can_become_main_window: false,
                is_floating_panel: true,
                hides_on_deactivate: false
            }
        })
    }
    pub use self::OverlayPanel as Panel;
}

/// One-time setup; call from `setup`, which runs on the main thread.
pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let Some(window) = app.get_webview_window(OVERLAY_LABEL) else {
        log::warn!("overlay window missing");
        return Ok(());
    };
    window.set_ignore_cursor_events(true)?;

    #[cfg(target_os = "macos")]
    {
        use tauri_nspanel::objc2_app_kit::NSWindowStyleMask;
        use tauri_nspanel::{CollectionBehavior, Panel, PanelLevel, WebviewWindowExt};
        let panel = window.to_panel::<mac::Panel>()?;
        if let Err(e) = panel.add_style_mask(NSWindowStyleMask::NonactivatingPanel) {
            log::warn!("overlay: {e}");
        }
        panel.set_level(PanelLevel::Floating.value());
        panel.set_collection_behavior(
            CollectionBehavior::new()
                .can_join_all_spaces()
                .stationary()
                .full_screen_auxiliary()
                .value(),
        );
        panel.set_ignores_mouse_events(true);
        panel.hide();
    }

    #[cfg(target_os = "windows")]
    {
        use windows::Win32::UI::WindowsAndMessaging::{
            GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
        };
        let hwnd = win_hwnd(&window)?;
        unsafe {
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            let add = (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0) as isize;
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | add);
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn win_hwnd(window: &tauri::WebviewWindow) -> tauri::Result<windows::Win32::Foundation::HWND> {
    // Rebuild the handle so a version mismatch between tauri's `windows`
    // dependency and ours cannot break the build.
    let raw = window.hwnd()?.0 as isize;
    Ok(windows::Win32::Foundation::HWND(
        raw as *mut std::ffi::c_void,
    ))
}

#[cfg(target_os = "macos")]
fn platform_show(app: &AppHandle) {
    use tauri_nspanel::{ManagerExt, Panel};
    // orderFrontRegardless: shows without activating the app or taking focus.
    match app.get_webview_panel(OVERLAY_LABEL) {
        Ok(p) => p.show(),
        Err(e) => log::warn!("overlay panel missing: {e:?}"),
    }
}

#[cfg(target_os = "macos")]
fn platform_hide(app: &AppHandle) {
    use tauri_nspanel::{ManagerExt, Panel};
    if let Ok(p) = app.get_webview_panel(OVERLAY_LABEL) {
        p.hide();
    }
}

#[cfg(target_os = "windows")]
fn platform_show(app: &AppHandle) {
    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_SHOWNOACTIVATE};
    if let Some(w) = app.get_webview_window(OVERLAY_LABEL) {
        if let Ok(hwnd) = win_hwnd(&w) {
            // Tauri's show() uses SW_SHOW, which activates the window.
            let _ = unsafe { ShowWindow(hwnd, SW_SHOWNOACTIVATE) };
        }
    }
}

#[cfg(target_os = "windows")]
fn platform_hide(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(OVERLAY_LABEL) {
        let _ = w.hide();
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn platform_show(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(OVERLAY_LABEL) {
        let _ = w.show();
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn platform_hide(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(OVERLAY_LABEL) {
        let _ = w.hide();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: (i32, i32, u32, u32) = (100, 50, 1920, 1000);

    #[test]
    fn bottom_center_1x() {
        assert_eq!(
            compute_position(AREA, 1.0, &OverlayPosition::BottomCenter),
            (100 + 900, 50 + 1000 - 36 - 24)
        );
    }

    #[test]
    fn top_center_2x_scales_margin_and_size() {
        let area = (0, 0, 2880, 1800);
        assert_eq!(
            compute_position(area, 2.0, &OverlayPosition::TopCenter),
            ((2880 - 240) / 2, 48)
        );
    }

    #[test]
    fn corners_keep_margin() {
        assert_eq!(
            compute_position(AREA, 1.0, &OverlayPosition::BottomLeft),
            (124, 50 + 1000 - 36 - 24)
        );
        assert_eq!(
            compute_position(AREA, 1.0, &OverlayPosition::BottomRight),
            (100 + 1920 - 120 - 24, 50 + 1000 - 36 - 24)
        );
    }
}
