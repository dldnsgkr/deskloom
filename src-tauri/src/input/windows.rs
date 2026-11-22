//! Windows desktop pointer: Raw Input (mouse, RIDEV_INPUTSINK) on a dedicated thread with a
//! message-only window. On each report the cursor position comes from GetCursorPos (physical px,
//! the same space as Tauri's `Monitor::position()`, since the process is per-monitor DPI aware).
//! A low-level mouse hook (WH_MOUSE_LL) is not used on purpose: when its thread is late, the whole
//! system's mouse is late with it. Raw Input is only a notification.
//!
//! Raw Input registration is per process and per device type: this replaces the registration tao
//! makes at startup for its device events (unused by the app), and `stop()` removes it.

use super::{DesktopPointer, MoveThrottle, EVENT};
use crate::diag;
use crate::layers::windows::{class_name, hwnd, is_background, is_ours, raw};
use ::windows::core::w;
use ::windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use ::windows::Win32::System::LibraryLoader::GetModuleHandleW;
use ::windows::Win32::UI::Input::{
    GetRawInputData, RegisterRawInputDevices, HRAWINPUT, RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER, RIDEV_INPUTSINK, RIDEV_REMOVE,
    RID_INPUT, RIM_TYPEMOUSE,
};
use ::windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetAncestor, GetCursorPos, GetDesktopWindow, GetMessageW,
    PostMessageW, PostQuitMessage, RegisterClassW, WindowFromPoint, GA_PARENT, HWND_MESSAGE, MSG, RI_MOUSE_LEFT_BUTTON_DOWN,
    RI_MOUSE_LEFT_BUTTON_UP, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_DESTROY, WM_INPUT, WNDCLASSW,
};
use std::cell::RefCell;
use std::sync::{mpsc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

/// HID usage page "generic desktop", usage "mouse".
const USAGE_PAGE_GENERIC: u16 = 1;
const USAGE_MOUSE: u16 = 2;

struct Running { hwnd: isize, thread: JoinHandle<()> }
static RUNNING: Mutex<Option<Running>> = Mutex::new(None);

/// What a window on the path from the hit window up to the desktop says about the cursor.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Hit {
    /// Our background under the icons.
    Background,
    /// Another window of ours (widget, character, lab): the background must not react under it.
    Ours,
    /// The desktop icon view or Explorer's desktop windows.
    Shell,
    Other,
}

fn is_desktop_shell_class(class: &str) -> bool {
    matches!(class, "SysListView32" | "SHELLDLL_DefView" | "WorkerW" | "Progman")
}

/// The first decisive window from the hit window upward decides; nothing decisive means another app.
fn verdict(path: impl IntoIterator<Item = Hit>) -> bool {
    for hit in path {
        match hit {
            Hit::Background | Hit::Shell => return true,
            Hit::Ours => return false,
            Hit::Other => {}
        }
    }
    false
}

/// Walks from the window under the cursor up to the desktop window.
fn over_desktop(hit: HWND) -> bool {
    // SAFETY: plain queries.
    let desktop = unsafe { GetDesktopWindow() };
    let mut path = Vec::new();
    let mut h = hit;
    while !h.is_invalid() && h != desktop && path.len() < 32 {
        let id = raw(h);
        path.push(if is_background(id) {
            Hit::Background
        } else if is_ours(h) {
            Hit::Ours
        } else if is_desktop_shell_class(&class_name(h)) {
            Hit::Shell
        } else {
            Hit::Other
        });
        if path.last() != Some(&Hit::Other) { break; }
        h = unsafe { GetAncestor(h, GA_PARENT) };
    }
    verdict(path)
}

/// Lives on the input thread only.
struct Ctx { app: AppHandle, throttle: MoveThrottle, down: bool, last_hit: isize, over: bool, logged: u8 }

thread_local! {
    static CTX: RefCell<Option<Ctx>> = const { RefCell::new(None) };
}

impl Ctx {
    fn on_mouse(&mut self, pressed: bool, released: bool) {
        if pressed { self.down = true; }
        if released { self.down = false; }
        // Moves (and wheel turns) within 16 ms of the last sent move are dropped; clicks never are.
        if !(pressed || released) && !self.throttle.accept_move(Instant::now()) { return; }
        let mut pt = POINT::default();
        // SAFETY: plain queries.
        if unsafe { GetCursorPos(&mut pt) }.is_err() { return; }
        let hit = unsafe { WindowFromPoint(pt) };
        if raw(hit) != self.last_hit {
            self.last_hit = raw(hit);
            self.over = over_desktop(hit);
        }
        let p = DesktopPointer { x: pt.x as f64, y: pt.y as f64, down: self.down, over_desktop: self.over };
        if self.logged < 5 {
            self.logged += 1;
            diag::log(format!("desktop-pointer {p:?} (window under cursor {:#x} {})", raw(hit), class_name(hit)));
        }
        let _ = self.app.emit(EVENT, p);
    }
}

fn on_input(handle: HRAWINPUT) {
    let mut data = RAWINPUT::default();
    let mut size = size_of::<RAWINPUT>() as u32;
    // SAFETY: `data` is a RAWINPUT-sized buffer and `size` says so; mouse reports fit in it.
    let n = unsafe {
        GetRawInputData(handle, RID_INPUT, Some((&mut data as *mut RAWINPUT).cast()), &mut size, size_of::<RAWINPUTHEADER>() as u32)
    };
    if n == 0 || n == u32::MAX || data.header.dwType != RIM_TYPEMOUSE.0 { return; }
    // SAFETY: dwType says the union holds a mouse report.
    let flags = u32::from(unsafe { data.data.mouse.Anonymous.Anonymous.usButtonFlags });
    let (pressed, released) = (flags & RI_MOUSE_LEFT_BUTTON_DOWN != 0, flags & RI_MOUSE_LEFT_BUTTON_UP != 0);
    CTX.with(|c| {
        if let Some(ctx) = c.borrow_mut().as_mut() { ctx.on_mouse(pressed, released); }
    });
}

unsafe extern "system" fn wndproc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_INPUT => {
            on_input(HRAWINPUT(lp.0 as *mut core::ffi::c_void));
            // The docs ask for DefWindowProc after WM_INPUT so the system can clean up.
            unsafe { DefWindowProcW(h, msg, wp, lp) }
        }
        WM_CLOSE => {
            let _ = unsafe { DestroyWindow(h) };
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(h, msg, wp, lp) },
    }
}

fn device(flags: ::windows::Win32::UI::Input::RAWINPUTDEVICE_FLAGS, target: HWND) -> RAWINPUTDEVICE {
    RAWINPUTDEVICE { usUsagePage: USAGE_PAGE_GENERIC, usUsage: USAGE_MOUSE, dwFlags: flags, hwndTarget: target }
}

/// The input thread: window, registration, message loop, then cleanup.
fn pump(app: AppHandle, ready: mpsc::Sender<Result<isize, String>>) {
    // SAFETY: everything below runs on this thread, which owns the window it creates.
    unsafe {
        let instance: Option<HINSTANCE> = GetModuleHandleW(None).ok().map(Into::into);
        let class = w!("LayerDesktopPointer");
        let wc = WNDCLASSW { lpfnWndProc: Some(wndproc), hInstance: instance.unwrap_or_default(), lpszClassName: class, ..Default::default() };
        // 0 when the class already exists from an earlier start(); CreateWindowExW tells real failures.
        RegisterClassW(&wc);
        let h = match CreateWindowExW(WINDOW_EX_STYLE(0), class, w!(""), WINDOW_STYLE(0), 0, 0, 0, 0, Some(HWND_MESSAGE), None, instance, None) {
            Ok(h) => h,
            Err(e) => { let _ = ready.send(Err(format!("CreateWindowExW(message-only): {e}"))); return; }
        };
        if let Err(e) = RegisterRawInputDevices(&[device(RIDEV_INPUTSINK, h)], size_of::<RAWINPUTDEVICE>() as u32) {
            let _ = DestroyWindow(h);
            let _ = ready.send(Err(format!("RegisterRawInputDevices: {e}")));
            return;
        }
        CTX.with(|c| *c.borrow_mut() = Some(Ctx { app, throttle: MoveThrottle::default(), down: false, last_hit: -1, over: false, logged: 0 }));
        let _ = ready.send(Ok(raw(h)));
        let mut msg = MSG::default();
        // GetMessageW returns -1 on error, 0 on WM_QUIT.
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 { DispatchMessageW(&msg); }
        let _ = RegisterRawInputDevices(&[device(RIDEV_REMOVE, HWND::default())], size_of::<RAWINPUTDEVICE>() as u32);
        CTX.with(|c| *c.borrow_mut() = None);
    }
}

/// Starts sending `desktop-pointer`. Idempotent.
pub fn start(app: &AppHandle) -> Result<(), String> {
    let mut g = RUNNING.lock().unwrap_or_else(|e| e.into_inner());
    if g.is_some() { return Ok(()); }
    let (tx, rx) = mpsc::channel();
    let app2 = app.clone();
    let thread = std::thread::Builder::new().name("desktop-pointer".into()).spawn(move || pump(app2, tx))
        .map_err(|e| format!("desktop pointer thread: {e}"))?;
    match rx.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(h)) => {
            *g = Some(Running { hwnd: h, thread });
            diag::log(format!("desktop pointer: raw input registered (message-only window {h:#x}, RIDEV_INPUTSINK)"));
            Ok(())
        }
        Ok(Err(e)) => { let _ = thread.join(); Err(e) }
        Err(e) => Err(format!("desktop pointer thread did not start: {e}")),
    }
}

/// Stops sending `desktop-pointer`. Waits for the thread, so a following `start()` cannot have its
/// registration removed by the old thread's cleanup.
pub fn stop() {
    let Some(r) = RUNNING.lock().unwrap_or_else(|e| e.into_inner()).take() else { return };
    // SAFETY: posting to our own message-only window; it destroys itself and ends the loop.
    if unsafe { PostMessageW(Some(hwnd(r.hwnd)), WM_CLOSE, WPARAM(0), LPARAM(0)) }.is_ok() {
        let _ = r.thread.join();
    }
    diag::log("desktop pointer: raw input removed");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn first_decisive_window_wins() {
        assert!(verdict([Hit::Shell]));
        assert!(verdict([Hit::Other, Hit::Background]), "web view child of our background");
        assert!(!verdict([Hit::Other, Hit::Ours, Hit::Shell]), "our widget inside Progman");
        assert!(!verdict([Hit::Other, Hit::Other]), "another app");
        assert!(!verdict([]));
    }
    #[test] fn desktop_shell_classes() {
        for c in ["SysListView32", "SHELLDLL_DefView", "WorkerW", "Progman"] { assert!(is_desktop_shell_class(c), "{c}"); }
        assert!(!is_desktop_shell_class("Shell_TrayWnd"));
    }
}
