//! Click-through for the one-big-window widget layout: the window ignores the mouse except over the
//! widgets. A 30Hz thread reads the global cursor and flips `set_ignore_cursor_events` only when the
//! answer changes. Widget rectangles (physical screen px) come from the page via `lab_set_hit_rects`.
#![cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]

use crate::diag;
use crate::geometry::{hit_any, Rect};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::WebviewWindow;

const POLL: Duration = Duration::from_millis(33);

static RECTS: Mutex<Option<HashMap<String, Vec<Rect>>>> = Mutex::new(None);

pub fn set_rects(label: &str, rects: Vec<Rect>) {
    RECTS.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(label.to_string(), rects);
}

fn rects_of(label: &str) -> Vec<Rect> {
    RECTS.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|m| m.get(label)).cloned().unwrap_or_default()
}

/// Remembers whether the window currently ignores the mouse and says when that has to change.
#[derive(Debug, PartialEq)]
pub struct IgnoreState { ignoring: Option<bool> }

impl IgnoreState {
    /// Nothing is known yet, so the first answer is always reported.
    pub fn new() -> Self { Self { ignoring: None } }
    /// `Some(new_ignore)` only when the window must switch; `None` when it is already right.
    pub fn update(&mut self, cursor_over_widget: bool) -> Option<bool> {
        let want = !cursor_over_widget;
        if self.ignoring == Some(want) { return None; }
        self.ignoring = Some(want);
        Some(want)
    }
}

/// macOS: `CGEventGetLocation` is in points with the origin at the top-left of the primary screen;
/// Cocoa (what `to_physical` takes) has the origin at its bottom-left.
pub fn cg_to_cocoa_y(cg_y: f64, primary_height: f64) -> f64 { primary_height - cg_y }

#[cfg(target_os = "macos")]
fn cursor_physical() -> Option<(i32, i32)> {
    use objc2_core_graphics::CGEvent;
    use objc2_foundation::NSPoint;
    let ev = CGEvent::new(None)?;
    let p = CGEvent::location(Some(&ev));
    let h = crate::input::macos::primary_height()?;
    let (x, y) = crate::input::macos::to_physical(NSPoint::new(p.x, cg_to_cocoa_y(p.y, h)));
    Some((x.round() as i32, y.round() as i32))
}

#[cfg(windows)]
fn cursor_physical() -> Option<(i32, i32)> {
    use ::windows::Win32::Foundation::POINT;
    use ::windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut p = POINT::default();
    // SAFETY: plain query into a local.
    unsafe { GetCursorPos(&mut p) }.ok()?;
    Some((p.x, p.y))
}

#[cfg(not(any(target_os = "macos", windows)))]
fn cursor_physical() -> Option<(i32, i32)> { None }

/// Starts the 30Hz watcher for `window`; it ends when the window is destroyed.
pub fn start(window: WebviewWindow) {
    #[cfg(target_os = "macos")]
    {
        // The screen cache `to_physical` reads must be filled (and kept current) from the main thread.
        use tauri::Manager;
        let app = window.app_handle().clone();
        let r = crate::main_thread::run_sync(&app, || {
            if let Some(mtm) = objc2::MainThreadMarker::new() { crate::input::macos::watch_screens(mtm); }
        });
        if let Err(e) = r { diag::log(format!("clickthrough: screen watch failed: {e}")); }
    }
    let label = window.label().to_string();
    let closed = Arc::new(AtomicBool::new(false));
    {
        let closed = closed.clone();
        window.on_window_event(move |e| {
            if matches!(e, tauri::WindowEvent::Destroyed) { closed.store(true, Ordering::SeqCst); }
        });
    }
    let spawned = std::thread::Builder::new().name(format!("clickthrough-{label}")).spawn(move || {
        let mut state = IgnoreState::new();
        diag::log(format!("clickthrough {label}: started"));
        while !closed.load(Ordering::SeqCst) {
            // No reading (e.g. before the first screen update): keep the window click-through.
            let over = cursor_physical().is_some_and(|(x, y)| hit_any(x, y, &rects_of(&label)));
            if let Some(ignore) = state.update(over) {
                diag::log(format!("clickthrough {label}: ignore_cursor_events={ignore}"));
                if window.set_ignore_cursor_events(ignore).is_err() && closed.load(Ordering::SeqCst) { break; }
            }
            std::thread::sleep(POLL);
        }
        diag::log(format!("clickthrough {label}: stopped"));
    });
    if let Err(e) = spawned { diag::log(format!("clickthrough: thread failed: {e}")); }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn first_answer_is_always_reported() {
        assert_eq!(IgnoreState::new().update(false), Some(true));
        assert_eq!(IgnoreState::new().update(true), Some(false));
    }
    #[test] fn reports_only_on_change() {
        let mut s = IgnoreState::new();
        assert_eq!(s.update(false), Some(true));
        assert_eq!(s.update(false), None);
        assert_eq!(s.update(true), Some(false));
        assert_eq!(s.update(true), None);
        assert_eq!(s.update(false), Some(true));
    }
    #[test] fn cg_y_flips_against_the_primary_height() {
        assert_eq!(cg_to_cocoa_y(0.0, 900.0), 900.0);
        assert_eq!(cg_to_cocoa_y(900.0, 900.0), 0.0);
        assert_eq!(cg_to_cocoa_y(100.0, 900.0), 800.0);
    }
    #[test] fn rects_are_kept_per_label() {
        set_rects("t-a", vec![Rect { x: 1, y: 2, w: 3, h: 4 }]);
        assert_eq!(rects_of("t-a"), vec![Rect { x: 1, y: 2, w: 3, h: 4 }]);
        assert!(rects_of("t-none").is_empty());
    }
}
