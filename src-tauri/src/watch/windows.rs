//! Windows visibility watch. Windows has no occlusion notification for other apps' windows, so
//! while any layer window is tracked a thread looks every 500 ms:
//! - full screen: the foreground window (not ours, not the shell) covers its monitor and has no
//!   caption, or SHQueryUserNotificationState reports a D3D full-screen app or presentation mode.
//!   Every layer on that monitor, the character included, gets `fullscreen`.
//! - covered: the visible windows above the layer (EnumWindows order, so windows behind a raised
//!   desktop after Win+D do not count) cover the background's monitor or the widget's own rect.
//!   Cloaked windows are skipped, or a window on another virtual desktop would count.
//!
//! Only changes are sent, except right after a window is tracked or attached again (see ANNOUNCE).

use super::{reason_change, Visibility, EVENT};
use crate::diag;
use crate::geometry::{self, Rect};
use crate::layers::windows::{class_name, ex_style, hwnd, is_ours, raw, style, to_rect, top_level_windows, window_rect};
use ::windows::Win32::Foundation::{HWND, POINT, RECT};
use ::windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS};
use ::windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MonitorFromWindow, HMONITOR, MONITORINFO, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY,
};
use ::windows::Win32::UI::Shell::{SHQueryUserNotificationState, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN};
use ::windows::Win32::UI::WindowsAndMessaging::{
    GetAncestor, GetForegroundWindow, IsIconic, IsWindowVisible, GA_ROOT, WS_CAPTION, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow, WindowEvent};

const TICK: Duration = Duration::from_millis(500);
/// A page registers its `visibility` listener only once it has loaded, which can be seconds after
/// the window is tracked (before `show()`) or attached again, and an event sent before that is lost.
/// For this long after either, the current state goes out on every tick, not only on changes.
const ANNOUNCE: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq)]
enum Role { Background, Widget, Character }

/// Window names from the common interface: `bg-<i>` and `ch`; every other layer is a widget.
fn role_of(label: &str) -> Role {
    if label.starts_with("bg-") { Role::Background } else if label == "ch" { Role::Character } else { Role::Widget }
}

/// Full screen wins over covered. The character floats above normal windows, so only full screen pauses it.
fn decide(role: Role, fullscreen_here: bool, covered: bool) -> &'static str {
    if fullscreen_here { "fullscreen" } else if role != Role::Character && covered { "occluded" } else { "visible" }
}

fn is_desktop_class(class: &str) -> bool { class == "Progman" || class == "WorkerW" }
fn is_taskbar_class(class: &str) -> bool { class == "Shell_TrayWnd" || class == "Shell_SecondaryTrayWnd" }

/// Whether a visible, non-minimized, uncloaked window can cover a layer. Tool windows and
/// click-through overlays (WS_EX_TRANSPARENT, e.g. game overlays spanning the screen) do not;
/// the taskbar does, so a maximized window plus the taskbar covers the whole monitor.
fn can_cover(ours: bool, class: &str, ex: u32) -> bool {
    if ours || is_desktop_class(class) { return false; }
    is_taskbar_class(class) || ex & (WS_EX_TOOLWINDOW.0 | WS_EX_TRANSPARENT.0) == 0
}

struct Entry { id: u64, label: String, hwnd: isize, role: Role, last: Option<&'static str>, announce_until: Instant }
struct State { entries: Vec<Entry>, app: Option<AppHandle>, running: bool, next_id: u64 }
static STATE: Mutex<State> = Mutex::new(State { entries: Vec::new(), app: None, running: false, next_id: 1 });

fn lock() -> MutexGuard<'static, State> { STATE.lock().unwrap_or_else(|e| e.into_inner()) }

/// Starts sending `visibility` for this window. Call before `show()`.
pub fn track(window: &WebviewWindow) -> Result<(), String> {
    let h = window.hwnd().map_err(|e| e.to_string())?.0 as isize;
    let label = window.label().to_string();
    let (id, start) = {
        let mut s = lock();
        let id = s.next_id;
        s.next_id += 1;
        s.entries.retain(|e| e.label != label);
        s.entries.push(Entry { id, label: label.clone(), hwnd: h, role: role_of(&label), last: None, announce_until: Instant::now() + ANNOUNCE });
        s.app = Some(window.app_handle().clone());
        (id, !std::mem::replace(&mut s.running, true))
    };
    window.on_window_event(move |e| {
        if let WindowEvent::Destroyed = e { lock().entries.retain(|x| x.id != id); }
    });
    if start {
        if let Err(e) = std::thread::Builder::new().name("visibility-watch".into()).spawn(run) {
            lock().running = false;
            return Err(format!("visibility watch thread failed to start: {e}"));
        }
        diag::log("visibility watch: started (500 ms poll)");
    }
    Ok(())
}

/// Sends the current state of `label` on every tick for a while again (after a re-attach).
pub fn reannounce(label: &str) {
    let until = Instant::now() + ANNOUNCE;
    for e in lock().entries.iter_mut().filter(|e| e.label == label) { e.announce_until = until; }
}

fn run() {
    let mut last_note = String::new();
    loop {
        std::thread::sleep(TICK);
        let (targets, app) = {
            let mut s = lock();
            if s.entries.is_empty() {
                s.running = false;
                drop(s);
                diag::log("visibility watch: stopped (no layer windows)");
                return;
            }
            (s.entries.iter().map(|e| (e.id, e.hwnd, e.role)).collect::<Vec<_>>(), s.app.clone())
        };
        let scene = Scene::capture();
        if scene.note != last_note {
            diag::log(format!("visibility watch: {}", scene.note));
            last_note = scene.note.clone();
        }
        let reasons: Vec<(u64, isize, Role, &'static str)> =
            targets.iter().map(|&(id, h, role)| (id, h, role, scene.reason(h, role))).collect();
        let now = Instant::now();
        let mut out = Vec::new();
        {
            let mut s = lock();
            for (id, h, role, reason) in reasons {
                let Some(e) = s.entries.iter_mut().find(|e| e.id == id) else { continue };
                if let Some(ev) = reason_change(&e.label, &mut e.last, reason) {
                    out.push((ev, Some((h, role))));
                } else if now < e.announce_until {
                    out.push((Visibility { label: e.label.clone(), visible: reason == "visible", reason }, None));
                }
            }
        }
        let Some(app) = app else { continue };
        for (ev, changed) in out {
            if let Some((h, role)) = changed {
                // Which windows count as covering, in case "occluded" fires wrongly on a machine we cannot see.
                let covers = if ev.reason == "occluded" { format!("; covered by: {}", scene.describe_covers(h, role)) } else { String::new() };
                diag::log(format!("visibility {} visible={} reason={}{covers}", ev.label, ev.visible, ev.reason));
            }
            let _ = app.emit(EVENT, ev);
        }
    }
}

/// The screen as seen on one tick.
struct Scene {
    /// Top-level windows, front to back.
    order: Vec<isize>,
    /// Windows that can cover a layer: (position in `order`, window, extended frame bounds).
    covers: Vec<(usize, isize, Rect)>,
    /// Monitor with a full-screen app, if any.
    fullscreen: Option<isize>,
    /// For the diagnostics, logged when it changes.
    note: String,
}

fn frame_bounds(h: HWND) -> Option<Rect> {
    let mut r = RECT::default();
    // SAFETY: a RECT-sized buffer for DWMWA_EXTENDED_FRAME_BOUNDS.
    let ok = unsafe { DwmGetWindowAttribute(h, DWMWA_EXTENDED_FRAME_BOUNDS, (&mut r as *mut RECT).cast(), size_of::<RECT>() as u32) }.is_ok();
    if ok { Some(to_rect(r)) } else { window_rect(h) }
}

fn is_cloaked(h: HWND) -> bool {
    let mut v = 0u32;
    // SAFETY: a u32 buffer for DWMWA_CLOAKED.
    unsafe { DwmGetWindowAttribute(h, DWMWA_CLOAKED, (&mut v as *mut u32).cast(), size_of::<u32>() as u32) }.is_ok() && v != 0
}

fn monitor_rect(m: HMONITOR) -> Option<Rect> {
    let mut mi = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
    // SAFETY: cbSize is set as the API requires.
    unsafe { GetMonitorInfoW(m, &mut mi) }.as_bool().then(|| to_rect(mi.rcMonitor))
}

/// Cheap checks first; class name and DWM queries only for visible windows of other processes.
fn cover_rect(h: HWND) -> Option<Rect> {
    // SAFETY: plain queries.
    if unsafe { !IsWindowVisible(h).as_bool() || IsIconic(h).as_bool() } { return None; }
    if !can_cover(is_ours(h), &class_name(h), ex_style(h)) || is_cloaked(h) { return None; }
    frame_bounds(h).filter(|r| r.w > 0 && r.h > 0)
}

fn fullscreen_monitor() -> (Option<isize>, String) {
    // SAFETY: plain queries.
    let fg = unsafe { GetForegroundWindow() };
    let quns = unsafe { SHQueryUserNotificationState() }.map_or(-1, |q| q.0);
    let shell_says = quns == QUNS_RUNNING_D3D_FULL_SCREEN.0 || quns == QUNS_PRESENTATION_MODE.0;
    let mon = if fg.is_invalid() {
        unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) }
    } else {
        unsafe { MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST) }
    };
    let class = if fg.is_invalid() { String::new() } else { class_name(fg) };
    let candidate = !fg.is_invalid() && !is_ours(fg) && !is_desktop_class(&class) && !is_taskbar_class(&class);
    let geometry = candidate && match (frame_bounds(fg), monitor_rect(mon)) {
        (Some(b), Some(m)) => geometry::is_fullscreen_window(b, m, style(fg) & WS_CAPTION.0 != 0),
        _ => false,
    };
    if geometry || shell_says {
        let m = mon.0 as isize;
        (Some(m), format!("full screen on monitor {m:#x}: foreground {class} {:#x} (covers monitor without caption={geometry}, QUNS={quns})", raw(fg)))
    } else {
        (None, format!("no full-screen app (QUNS={quns})"))
    }
}

impl Scene {
    fn capture() -> Scene {
        let order = top_level_windows();
        let covers = order.iter().enumerate().filter_map(|(z, &h)| cover_rect(hwnd(h)).map(|r| (z, h, r))).collect();
        let (fullscreen, note) = fullscreen_monitor();
        Scene { order, covers, fullscreen, note }
    }

    fn reason(&self, h: isize, role: Role) -> &'static str {
        // SAFETY: plain query; a closed window just yields a null handle.
        let mon = unsafe { MonitorFromWindow(hwnd(h), MONITOR_DEFAULTTONEAREST) };
        let fullscreen_here = self.fullscreen == Some(mon.0 as isize);
        let covered = role != Role::Character && {
            let (target, above) = self.target_and_above(h, role);
            let rects: Vec<Rect> = above.iter().map(|&(_, r)| r).collect();
            target.is_some_and(|t| geometry::is_fully_covered(t, &rects))
        };
        decide(role, fullscreen_here, covered)
    }

    /// The area a layer needs to be seen (its monitor for a background, else its own rect) and the
    /// windows above it that can cover it.
    fn target_and_above(&self, h: isize, role: Role) -> (Option<Rect>, Vec<(isize, Rect)>) {
        let w = hwnd(h);
        // SAFETY: plain queries; a closed window just yields null handles.
        let mon = unsafe { MonitorFromWindow(w, MONITOR_DEFAULTTONEAREST) };
        // Children of Progman / WorkerW sit where their top-level ancestor sits.
        let root = raw(unsafe { GetAncestor(w, GA_ROOT) });
        let rz = self.order.iter().position(|&x| x == root).unwrap_or(usize::MAX);
        let above = self.covers.iter().filter(|(z, _, _)| *z < rz).map(|&(_, c, r)| (c, r)).collect();
        let target = if role == Role::Background { monitor_rect(mon) } else { window_rect(w) };
        (target, above)
    }

    /// "Class 0x1234 at x,y wxh, …" for the covering windows that overlap the layer's area (at most 8).
    fn describe_covers(&self, h: isize, role: Role) -> String {
        let (target, above) = self.target_and_above(h, role);
        let Some(t) = target else { return "? (no target rect)".into() };
        let hits: Vec<String> = above.iter().filter(|&&(_, r)| geometry::intersects(r, t))
            .map(|&(c, r)| format!("{} {c:#x} at {},{} {}x{}", class_name(hwnd(c)), r.x, r.y, r.w, r.h))
            .collect();
        let more = hits.len().saturating_sub(8);
        let mut s = hits.into_iter().take(8).collect::<Vec<_>>().join(", ");
        if more > 0 { s.push_str(&format!(", and {more} more")); }
        format!("{s} (area {},{} {}x{})", t.x, t.y, t.w, t.h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn roles_follow_the_window_names() {
        assert_eq!(role_of("bg-0"), Role::Background);
        assert_eq!(role_of("bg-12"), Role::Background);
        assert_eq!(role_of("ch"), Role::Character);
        assert_eq!(role_of("w-note"), Role::Widget);
        assert_eq!(role_of("w-all"), Role::Widget);
    }
    #[test] fn fullscreen_beats_covered_and_the_character_ignores_covering() {
        assert_eq!(decide(Role::Background, true, true), "fullscreen");
        assert_eq!(decide(Role::Character, true, false), "fullscreen");
        assert_eq!(decide(Role::Background, false, true), "occluded");
        assert_eq!(decide(Role::Widget, false, true), "occluded");
        assert_eq!(decide(Role::Character, false, true), "visible");
        assert_eq!(decide(Role::Widget, false, false), "visible");
    }
    #[test] fn what_can_cover_a_layer() {
        assert!(can_cover(false, "Chrome_WidgetWin_1", 0));
        assert!(!can_cover(true, "Chrome_WidgetWin_1", 0), "our own windows");
        assert!(!can_cover(false, "Progman", 0));
        assert!(!can_cover(false, "WorkerW", 0));
        assert!(!can_cover(false, "SomeTool", WS_EX_TOOLWINDOW.0));
        assert!(!can_cover(false, "Overlay", WS_EX_TRANSPARENT.0));
        assert!(can_cover(false, "Shell_TrayWnd", WS_EX_TOOLWINDOW.0), "the taskbar counts");
    }
}
