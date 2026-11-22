//! Windows layers.
//!
//! - Background, under the desktop icons. 23H2 and older: a child of the WorkerW right behind the
//!   window that holds SHELLDLL_DefView. 24H2 and newer ("raised desktop" with a layered icon view,
//!   Progman has WS_EX_NOREDIRECTIONBITMAP): a WS_EX_LAYERED child of Progman between
//!   SHELLDLL_DefView and Progman's WorkerW. The 24H2 order follows Lively Wallpaper
//!   (github.com/rocksdanister/lively, branch core-separation at 7ae2fe4, Core/WinDesktopCore.cs
//!   `TryAttachToDesktop` and `EnsureWorkerWZOrder`): WS_CHILD, WS_EX_LAYERED with alpha 255,
//!   SetParent(Progman), SetWindowPos(below DefView), then WorkerW to the bottom if it is not last.
//! - Widgets, above the icons and below normal windows. `Bottom`: a top-level tool window pinned to
//!   HWND_BOTTOM. `DesktopChild`: a child of the icon view's parent, above SHELLDLL_DefView.
//! - Character: a top-most tool window.
//!
//! tao rewrites the whole GWL_STYLE / GWL_EXSTYLE from its own flags whenever one of them changes
//! (`show()` does), which would drop the bits set here. A subclass edits WM_STYLECHANGING so the
//! bits stay, and pins the z-order of `Bottom` widgets in WM_WINDOWPOSCHANGING.
//!
//! Reparented windows are checked every 2 s (Explorer restarts, WorkerW recreated) and attached
//! again when their host changed. Everything goes to `diag::log`, since the user's diagnostics
//! paste is how a machine we cannot see gets debugged.

use super::{AttachOptions, AttachReport, LayerKind, WinWidgetHost};
use crate::desktop_tree::{self, DesktopHost, WinNode};
use crate::geometry::{self, Rect};
use crate::{diag, main_thread, watch};
use ::windows::core::{w, BOOL, PCWSTR};
use ::windows::Win32::Foundation::{COLORREF, ERROR_SUCCESS, HWND, LPARAM, LRESULT, RECT, WPARAM};
use ::windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ};
use ::windows::Win32::UI::HiDpi::{GetAwarenessFromDpiAwarenessContext, GetWindowDpiAwarenessContext};
use ::windows::Win32::UI::Shell::{DefSubclassProc, GetWindowSubclass, RemoveWindowSubclass, SetWindowSubclass};
use ::windows::Win32::UI::WindowsAndMessaging::*;
use std::cell::Cell;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Manager, WebviewWindow, WindowEvent};

/// Explorer's undocumented "spawn the WorkerW behind the icons" message.
const SPAWN_WORKERW: u32 = 0x052C;
const RETRY_DELAY: Duration = Duration::from_millis(500);
const KEEP_INTERVAL: Duration = Duration::from_secs(2);
/// A host problem that comes straight back after this many successful re-attaches is left alone.
const MAX_STRIKES: u32 = 3;
const SUBCLASS_ID: usize = 0x4C59_5231; // "LYR1"

// ---------- small Win32 helpers, shared with input/windows.rs and watch/windows.rs ----------

pub(crate) fn hwnd(h: isize) -> HWND { HWND(h as *mut core::ffi::c_void) }
pub(crate) fn raw(h: HWND) -> isize { h.0 as isize }

pub(crate) fn class_name(h: HWND) -> String {
    let mut buf = [0u16; 256];
    // SAFETY: plain query on any window handle; a stale handle just returns 0.
    let n = unsafe { GetClassNameW(h, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

pub(crate) fn to_rect(r: RECT) -> Rect {
    Rect { x: r.left, y: r.top, w: r.right - r.left, h: r.bottom - r.top }
}

pub(crate) fn window_rect(h: HWND) -> Option<Rect> {
    let mut r = RECT::default();
    // SAFETY: plain query; fails for a stale handle.
    unsafe { GetWindowRect(h, &mut r) }.ok()?;
    Some(to_rect(r))
}

/// True for windows of this process (layer windows, the lab window).
pub(crate) fn is_ours(h: HWND) -> bool {
    let mut pid = 0u32;
    // SAFETY: plain query.
    unsafe { GetWindowThreadProcessId(h, Some(&mut pid as *mut u32)) };
    pid == std::process::id()
}

pub(crate) fn ex_style(h: HWND) -> u32 {
    // SAFETY: plain query.
    unsafe { GetWindowLongW(h, GWL_EXSTYLE) as u32 }
}

pub(crate) fn style(h: HWND) -> u32 {
    // SAFETY: plain query.
    unsafe { GetWindowLongW(h, GWL_STYLE) as u32 }
}

fn is_window(h: isize) -> bool {
    // SAFETY: plain query.
    h != 0 && unsafe { IsWindow(Some(hwnd(h))) }.as_bool()
}

/// "10.0.26100 build 26100.2605 (24H2)": os_info's version plus the update revision and the
/// marketing version from the registry. Read once.
pub(crate) fn os_build() -> &'static str {
    static BUILD: OnceLock<String> = OnceLock::new();
    BUILD.get_or_init(|| {
        let key = w!("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion");
        let text = |name: PCWSTR| -> String {
            let mut buf = [0u16; 128];
            let mut len = (buf.len() * 2) as u32;
            // SAFETY: buffer and length describe the same array.
            let r = unsafe { RegGetValueW(HKEY_LOCAL_MACHINE, key, name, RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr().cast()), Some(&mut len as *mut u32)) };
            if r != ERROR_SUCCESS { return "?".into(); }
            let n = (len as usize / 2).saturating_sub(1).min(buf.len());
            String::from_utf16_lossy(&buf[..n])
        };
        let mut ubr = 0u32;
        let mut len = 4u32;
        // SAFETY: a DWORD value into a u32.
        let ubr_ok = unsafe { RegGetValueW(HKEY_LOCAL_MACHINE, key, w!("UBR"), RRF_RT_REG_DWORD, None, Some((&mut ubr as *mut u32).cast()), Some(&mut len as *mut u32)) } == ERROR_SUCCESS;
        format!("Windows {} build {}.{} ({})", os_info::get().version(), text(w!("CurrentBuild")),
            if ubr_ok { ubr.to_string() } else { "?".into() }, text(w!("DisplayVersion")))
    })
}

fn dpi_awareness(h: HWND) -> &'static str {
    // SAFETY: plain queries.
    match unsafe { GetAwarenessFromDpiAwarenessContext(GetWindowDpiAwarenessContext(h)) }.0 {
        0 => "unaware",
        1 => "system",
        2 => "per-monitor",
        _ => "invalid",
    }
}

/// Direct children of `parent`, front to back.
fn child_order(parent: HWND) -> Vec<isize> {
    let mut out = Vec::new();
    // SAFETY: plain queries; bounded so a window list that changes underneath cannot loop forever.
    let mut next = unsafe { GetWindow(parent, GW_CHILD) };
    while let Ok(h) = next {
        if h.is_invalid() || out.len() >= 256 { break; }
        out.push(raw(h));
        next = unsafe { GetWindow(h, GW_HWNDNEXT) };
    }
    out
}

fn describe_children(parent: isize) -> String {
    child_order(hwnd(parent)).iter()
        .map(|&c| format!("{} {c:#x}{}", class_name(hwnd(c)), if is_ours(hwnd(c)) { " (ours)" } else { "" }))
        .collect::<Vec<_>>()
        .join(", ")
}

// ---------- the desktop window tree ----------

struct Discovery { host: Option<DesktopHost>, nodes: Vec<WinNode>, progman: isize, defview: isize, sent: isize }

impl Discovery {
    fn summary(&self) -> String {
        let p = hwnd(self.progman);
        let ex = if self.progman != 0 { ex_style(p) } else { 0 };
        format!(
            "progman={:#x} exStyle={ex:#x} noRedirectionBitmap={} dpi={} 0x052C-result={}; tree=[{}]",
            self.progman, ex & WS_EX_NOREDIRECTIONBITMAP.0 != 0,
            if self.progman != 0 { dpi_awareness(p) } else { "-" }, self.sent, desktop_tree::describe(&self.nodes))
    }
}

unsafe extern "system" fn push_hwnd(h: HWND, l: LPARAM) -> BOOL {
    // SAFETY: `l` is the &mut Vec<isize> passed by the caller for the duration of the enumeration.
    unsafe { (*(l.0 as *mut Vec<isize>)).push(raw(h)) };
    BOOL(1)
}

/// All top-level windows, front to back.
pub(crate) fn top_level_windows() -> Vec<isize> {
    let mut top = Vec::new();
    // SAFETY: the callback only pushes into `top`, which outlives the call.
    let _ = unsafe { EnumWindows(Some(push_hwnd), LPARAM(&mut top as *mut Vec<isize> as isize)) };
    top
}

/// Top-level Progman and WorkerW windows (z = position among all top-level windows) and the direct
/// children of each (z = position among siblings). Other top-level windows are left out.
fn snapshot() -> Vec<WinNode> {
    let top = top_level_windows();
    let mut nodes: Vec<WinNode> = top.iter().enumerate()
        .filter_map(|(z, &h)| {
            let class = class_name(hwnd(h));
            (class == "Progman" || class == "WorkerW").then_some(WinNode { hwnd: h, class, parent: 0, z: z as u32 })
        })
        .collect();
    let parents: Vec<isize> = nodes.iter().map(|n| n.hwnd).collect();
    for p in parents {
        let mut all = Vec::new();
        // SAFETY: as above. EnumChildWindows walks all descendants; keep the direct children.
        let _ = unsafe { EnumChildWindows(Some(hwnd(p)), Some(push_hwnd), LPARAM(&mut all as *mut Vec<isize> as isize)) };
        let mut z = 0;
        for c in all {
            if raw(unsafe { GetAncestor(hwnd(c), GA_PARENT) }) != p { continue; }
            nodes.push(WinNode { hwnd: c, class: class_name(hwnd(c)), parent: p, z });
            z += 1;
        }
    }
    nodes
}

fn discover_once() -> Discovery {
    // SAFETY: plain calls; Progman may be missing while Explorer restarts.
    let progman = unsafe { FindWindowW(w!("Progman"), PCWSTR::null()) }.map(raw).unwrap_or(0);
    let mut sent = 0;
    if progman != 0 {
        let mut result = 0usize;
        // SAFETY: SMTO_NORMAL with a 1 s timeout, so a hung Explorer cannot block us for long.
        sent = unsafe {
            SendMessageTimeoutW(hwnd(progman), SPAWN_WORKERW, WPARAM(0xD), LPARAM(0x1), SMTO_NORMAL, 1000, Some(&mut result as *mut usize))
        }.0;
    }
    let nodes = snapshot();
    let host = desktop_tree::choose_desktop_host(&nodes);
    let defview = nodes.iter().find(|n| n.class == "SHELLDLL_DefView").map_or(0, |n| n.hwnd);
    Discovery { host, nodes, progman, defview, sent }
}

/// Steps 1–3: spawn the WorkerW, read the tree, and look again once after 0.5 s if no host is found.
fn discover() -> Discovery {
    let first = discover_once();
    if first.host.is_some() { return first; }
    diag::log(format!("desktop host not found, trying again in 0.5 s: {}", first.summary()));
    std::thread::sleep(RETRY_DELAY);
    discover_once()
}

pub fn desktop_ready() -> bool {
    discover_once().host.is_some()
}

// ---------- pinned styles ----------

/// Style bits a layer window keeps, and whether its z-order is pinned to HWND_BOTTOM.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Bits { set_style: u32, clear_style: u32, set_ex: u32, clear_ex: u32, pin_bottom: bool }

fn edit(value: u32, set: u32, clear: u32) -> u32 { (value | set) & !clear }

/// Lives as the subclass reference data; only touched on the window's own (main) thread.
struct Pin(Cell<Bits>);

unsafe extern "system" fn subclass_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM, id: usize, data: usize) -> LRESULT {
    // SAFETY: `data` is the Box<Pin> installed by `pin()`, freed only below on WM_NCDESTROY.
    let bits = unsafe { (*(data as *const Pin)).0.get() };
    match msg {
        WM_STYLECHANGING => {
            // SAFETY: for WM_STYLECHANGING lParam points to a STYLESTRUCT we may edit.
            let ss = unsafe { &mut *(lp.0 as *mut STYLESTRUCT) };
            if wp.0 as i32 == GWL_STYLE.0 {
                ss.styleNew = edit(ss.styleNew, bits.set_style, bits.clear_style);
            } else if wp.0 as i32 == GWL_EXSTYLE.0 {
                ss.styleNew = edit(ss.styleNew, bits.set_ex, bits.clear_ex);
            }
        }
        WM_WINDOWPOSCHANGING if bits.pin_bottom => {
            // SAFETY: for WM_WINDOWPOSCHANGING lParam points to a WINDOWPOS we may edit.
            let pos = unsafe { &mut *(lp.0 as *mut WINDOWPOS) };
            if !pos.flags.contains(SWP_NOZORDER) { pos.hwndInsertAfter = HWND_BOTTOM; }
        }
        WM_NCDESTROY => unsafe {
            // SAFETY: last message the window gets; remove ourselves, then free the data once.
            let _ = RemoveWindowSubclass(h, Some(subclass_proc), id);
            drop(Box::from_raw(data as *mut Pin));
        },
        _ => {}
    }
    // SAFETY: forwards to the next window procedure in the chain (tao's).
    unsafe { DefSubclassProc(h, msg, wp, lp) }
}

/// Installs or updates the subclass and applies `bits` now. Main thread only.
unsafe fn pin(h: HWND, bits: Bits) -> Result<(), String> {
    let mut data = 0usize;
    // SAFETY: called on the thread that owns `h`.
    unsafe {
        if GetWindowSubclass(h, Some(subclass_proc), SUBCLASS_ID, Some(&mut data as *mut usize)).as_bool() && data != 0 {
            (*(data as *const Pin)).0.set(bits);
        } else {
            let p = Box::into_raw(Box::new(Pin(Cell::new(bits))));
            if !SetWindowSubclass(h, Some(subclass_proc), SUBCLASS_ID, p as usize).as_bool() {
                drop(Box::from_raw(p));
                return Err("SetWindowSubclass failed".into());
            }
        }
        SetWindowLongW(h, GWL_STYLE, edit(style(h), bits.set_style, bits.clear_style) as i32);
        SetWindowLongW(h, GWL_EXSTYLE, edit(ex_style(h), bits.set_ex, bits.clear_ex) as i32);
        let _ = SetWindowPos(h, None, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED);
    }
    Ok(())
}

// ---------- placement (main thread) ----------

#[derive(Clone, Copy, Debug, PartialEq)]
enum Role {
    Background,
    DesktopChild,
    Bottom,
    Character,
    /// A background whose desktop attach failed: kept under normal windows and click-through.
    FallbackBackground,
}

fn method(role: Role, host: Option<DesktopHost>) -> &'static str {
    match (role, host) {
        (Role::Background, Some(DesktopHost::Legacy { .. })) => "win-legacy-workerw",
        (Role::Background, Some(DesktopHost::Progman { .. })) => "win-progman",
        (Role::DesktopChild, _) => "win-widget-desktop-child",
        (Role::Bottom, _) => "win-widget-bottom",
        (Role::Character, _) => "win-topmost",
        (Role::Background | Role::FallbackBackground, _) => "win-fallback-bottom",
    }
}

/// The window a desktop layer becomes a child of.
fn parent_of(role: Role, host: DesktopHost) -> isize {
    match (role, host) {
        (_, DesktopHost::Progman { progman, .. }) => progman,
        (Role::Background, DesktopHost::Legacy { workerw, .. }) => workerw,
        (_, DesktopHost::Legacy { defview_parent, .. }) => defview_parent,
    }
}

fn desktop_bits(role: Role, host: DesktopHost) -> Bits {
    // 24H2: children of the NOREDIRECTIONBITMAP Progman must be layered to be drawn (Microsoft's
    // note quoted in Lively's SetupDesktopLayer). The background also lets clicks through, in case
    // the layered icon view passes clicks on its transparent parts down to it. It never needs focus.
    let set_ex = match (role, host) {
        (Role::Background, DesktopHost::Progman { .. }) => WS_EX_LAYERED.0 | WS_EX_TRANSPARENT.0 | WS_EX_NOACTIVATE.0,
        (Role::Background, _) => WS_EX_NOACTIVATE.0,
        (_, DesktopHost::Progman { .. }) => WS_EX_LAYERED.0,
        _ => 0,
    };
    // SetParent does not change WS_CHILD/WS_POPUP; the docs say to set WS_CHILD first.
    Bits { set_style: WS_CHILD.0, clear_style: WS_POPUP.0, set_ex, clear_ex: WS_EX_APPWINDOW.0, pin_bottom: false }
}

/// Moves `h` to `screen` (screen px) expressed in `parent`'s coordinates; `z` None keeps the z-order.
unsafe fn move_into(h: HWND, parent: HWND, screen: Rect, z: Option<HWND>) -> Result<Rect, String> {
    let origin = window_rect(parent).ok_or("GetWindowRect(parent) failed")?;
    let r = geometry::to_parent_coords(screen, (origin.x, origin.y));
    let flags = if z.is_some() { SWP_NOACTIVATE } else { SWP_NOACTIVATE | SWP_NOZORDER };
    // SAFETY: main thread, `h` is ours.
    unsafe { SetWindowPos(h, z, r.x, r.y, r.w, r.h, flags) }.map_err(|e| format!("SetWindowPos(position): {e}"))?;
    Ok(r)
}

/// Lively's EnsureWorkerWZOrder: WorkerW draws the wallpaper, so it must be Progman's last child.
unsafe fn ensure_workerw_last(progman: isize, workerw: isize) -> String {
    if child_order(hwnd(progman)).last() == Some(&workerw) { return "WorkerW already last".into(); }
    // SAFETY: Explorer's window; SetWindowPos across processes is allowed at the same integrity level.
    match unsafe { SetWindowPos(hwnd(workerw), Some(HWND_BOTTOM), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE) } {
        Ok(()) => "moved WorkerW to the bottom".into(),
        Err(e) => format!("moving WorkerW to the bottom failed: {e}"),
    }
}

/// Steps 1.4–1.5 and the desktop-child widget: reparent, order, position. Main thread only.
unsafe fn place_in_desktop(h: isize, role: Role, host: DesktopHost, screen: Rect) -> Result<String, String> {
    if !is_window(h) { return Err("window is gone".into()); }
    let (w, parent) = (hwnd(h), parent_of(role, host));
    let bits = desktop_bits(role, host);
    let mut notes = Vec::new();
    // SAFETY: main thread; `w` is our window, the others belong to Explorer.
    unsafe {
        pin(w, bits)?;
        if bits.set_ex & WS_EX_LAYERED.0 != 0 {
            SetLayeredWindowAttributes(w, COLORREF(0), 255, LWA_ALPHA).map_err(|e| format!("SetLayeredWindowAttributes: {e}"))?;
        }
        SetParent(w, Some(hwnd(parent))).map_err(|e| format!("SetParent({parent:#x}): {e}"))?;
        let r = match (role, host) {
            (Role::Background, DesktopHost::Progman { progman, defview, workerw }) => {
                if let Err(e) = SetWindowPos(w, Some(hwnd(defview)), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE) {
                    notes.push(format!("SetWindowPos(below DefView) failed: {e}"));
                }
                notes.push(ensure_workerw_last(progman, workerw));
                move_into(w, hwnd(parent), screen, None)?
            }
            (Role::DesktopChild, _) => move_into(w, hwnd(parent), screen, Some(HWND_TOP))?,
            _ => move_into(w, hwnd(parent), screen, None)?,
        };
        Ok(format!(
            "parent={parent:#x} ({}) at x={} y={} w={} h={} in parent coords; style={:#x} exStyle={:#x}; {}children of parent: [{}]",
            class_name(hwnd(parent)), r.x, r.y, r.w, r.h, style(w), ex_style(w),
            notes.iter().map(|n| format!("{n}; ")).collect::<String>(), describe_children(parent)))
    }
}

/// Bottom widget, character, or a background that could not get under the icons. Main thread only.
/// `screen` is where the window goes if a failed desktop attach left it inside a desktop window.
unsafe fn place_top_level(h: isize, role: Role, screen: Rect) -> Result<String, String> {
    if !is_window(h) { return Err("window is gone".into()); }
    let w = hwnd(h);
    let top = Bits { clear_style: WS_CHILD.0, clear_ex: WS_EX_APPWINDOW.0, ..Bits::default() };
    let (bits, z) = match role {
        Role::Bottom => (Bits { set_ex: WS_EX_TOOLWINDOW.0, pin_bottom: true, ..top }, HWND_BOTTOM),
        Role::Character => (Bits { set_ex: WS_EX_TOOLWINDOW.0 | WS_EX_TOPMOST.0, ..top }, HWND_TOPMOST),
        // Out of the way: under normal windows, click-through, never focused. It still covers the icons.
        _ => (Bits {
            set_ex: WS_EX_TOOLWINDOW.0 | WS_EX_LAYERED.0 | WS_EX_TRANSPARENT.0 | WS_EX_NOACTIVATE.0, pin_bottom: true, ..top
        }, HWND_BOTTOM),
    };
    // SAFETY: main thread, `w` is ours.
    unsafe {
        pin(w, bits)?;
        if bits.set_ex & WS_EX_LAYERED.0 != 0 {
            SetLayeredWindowAttributes(w, COLORREF(0), 255, LWA_ALPHA).map_err(|e| format!("SetLayeredWindowAttributes: {e}"))?;
        }
        let mut note = String::new();
        if GetAncestor(w, GA_PARENT) != GetDesktopWindow() {
            note = match SetParent(w, None) {
                Ok(_) => "taken back out of its desktop parent; ".to_string(),
                Err(e) => format!("SetParent(none) failed: {e}; "),
            };
            SetWindowPos(w, Some(z), screen.x, screen.y, screen.w, screen.h, SWP_NOACTIVATE).map_err(|e| format!("SetWindowPos: {e}"))?;
        } else {
            SetWindowPos(w, Some(z), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE).map_err(|e| format!("SetWindowPos(z): {e}"))?;
        }
        Ok(format!("{note}style={:#x} exStyle={:#x}", style(w), ex_style(w)))
    }
}

// ---------- attach ----------

/// A reparented window the keeper watches.
#[derive(Clone)]
struct Kept {
    label: String, hwnd: isize, role: Role, screen: Rect, host: DesktopHost, defview: isize,
    app: AppHandle,
    /// Keeper bookkeeping, see `next_step`.
    strikes: u32, last_problem: String, failures: u32,
}

struct Keeper { kept: Vec<Kept>, running: bool }
static KEEPER: Mutex<Keeper> = Mutex::new(Keeper { kept: Vec::new(), running: false });

/// True if `h` is one of our background windows under the icons (for the desktop pointer).
pub(crate) fn is_background(h: isize) -> bool {
    KEEPER.lock().unwrap_or_else(|e| e.into_inner()).kept.iter().any(|k| k.hwnd == h && k.role == Role::Background)
}

/// Discover, then reparent on the main thread. Runs on the calling (non-main) thread.
fn attach_to_desktop(app: &AppHandle, label: &str, h: isize, role: Role, screen: Rect) -> Result<(AttachReport, DesktopHost, isize), String> {
    let d = discover();
    let Some(host) = d.host else {
        return Err(format!("no desktop host after retry; {}; {}", os_build(), d.summary()));
    };
    let placed = main_thread::run_sync(app, move || unsafe { place_in_desktop(h, role, host, screen) })?
        .map_err(|e| format!("{e}; host={host:?}; {}; {}", os_build(), d.summary()))?;
    let detail = format!(
        "{}; host={host:?}; our dpi={}; {}; {placed}",
        os_build(), dpi_awareness(hwnd(h)), d.summary());
    Ok((AttachReport { label: label.to_string(), method: method(role, Some(host)).into(), detail }, host, d.defview))
}

pub fn attach(window: &WebviewWindow, kind: LayerKind, opts: &AttachOptions) -> Result<AttachReport, String> {
    let label = window.label().to_string();
    let h = window.hwnd().map_err(|e| e.to_string())?.0 as isize;
    let app = window.app_handle().clone();
    // On the main thread: the lab's set_size/set_position were queued there just before, and run first.
    let screen = main_thread::run_sync(&app, move || window_rect(hwnd(h)))?.ok_or("GetWindowRect failed")?;
    let role = match (kind, opts.win_widget_host) {
        (LayerKind::Background, _) => Role::Background,
        (LayerKind::Widget, WinWidgetHost::Bottom) => Role::Bottom,
        (LayerKind::Widget, WinWidgetHost::DesktopChild) => Role::DesktopChild,
        (LayerKind::Character, _) => Role::Character,
    };
    diag::log(format!("attach {label}: hwnd={h:#x} role={role:?} screen={screen:?}; {}", os_build()));
    let top_level = |role: Role| -> Result<String, String> {
        main_thread::run_sync(&app, move || unsafe { place_top_level(h, role, screen) })?
    };
    match role {
        Role::Background | Role::DesktopChild => match attach_to_desktop(&app, &label, h, role, screen) {
            Ok((report, host, defview)) => {
                keep(window, Kept {
                    label: label.clone(), hwnd: h, role, screen, host, defview, app: app.clone(),
                    strikes: 0, last_problem: String::new(), failures: 0,
                });
                Ok(report)
            }
            Err(e) if role == Role::DesktopChild => {
                diag::log(format!("attach {label}: desktop-child failed, falling back to bottom: {e}"));
                let placed = top_level(Role::Bottom)?;
                Ok(AttachReport {
                    label, method: method(Role::Bottom, None).into(),
                    detail: format!("desktop-child failed, fell back to bottom ({placed}): {e}"),
                })
            }
            Err(e) => {
                let fallback = top_level(Role::FallbackBackground).unwrap_or_else(|e| format!("failed: {e}"));
                Err(format!("{e}; fallback ({}): bottom, click-through, {fallback}", method(Role::FallbackBackground, None)))
            }
        },
        Role::Bottom | Role::Character | Role::FallbackBackground => {
            let placed = top_level(role)?;
            Ok(AttachReport { label, method: method(role, None).into(), detail: format!("{}; {placed}", os_build()) })
        }
    }
}

// ---------- step 1.6: keep reparented windows attached ----------

fn keep(window: &WebviewWindow, k: Kept) {
    let h = k.hwnd;
    let start = {
        let mut g = KEEPER.lock().unwrap_or_else(|e| e.into_inner());
        g.kept.retain(|x| x.hwnd != h);
        g.kept.push(k);
        !std::mem::replace(&mut g.running, true)
    };
    window.on_window_event(move |e| {
        if let WindowEvent::Destroyed = e { forget(h); }
    });
    if start {
        let spawned = std::thread::Builder::new().name("layer-keeper".into()).spawn(keeper_loop);
        if let Err(e) = spawned {
            KEEPER.lock().unwrap_or_else(|e| e.into_inner()).running = false;
            diag::log(format!("layer keeper thread failed to start: {e}"));
        }
    }
}

fn forget(h: isize) {
    KEEPER.lock().unwrap_or_else(|e| e.into_inner()).kept.retain(|k| k.hwnd != h);
}

/// Runs while any reparented window exists; exits when the last one is gone.
fn keeper_loop() {
    loop {
        std::thread::sleep(KEEP_INTERVAL);
        let kept = {
            let mut g = KEEPER.lock().unwrap_or_else(|e| e.into_inner());
            if g.kept.is_empty() { g.running = false; return; }
            g.kept.clone()
        };
        for k in kept { check(k); }
    }
}

/// What is wrong with a kept window's placement, if anything.
fn problem(k: &Kept) -> Option<String> {
    let parent = parent_of(k.role, k.host);
    if !is_window(parent) { return Some(format!("host {parent:#x} is gone")); }
    // SAFETY: plain query.
    let actual = unsafe { GetParent(hwnd(k.hwnd)) }.map(raw).unwrap_or(0);
    if actual != parent { return Some(format!("parent is {actual:#x}, expected {parent:#x}")); }
    let order = child_order(hwnd(parent));
    let pos = |x: isize| order.iter().position(|&c| c == x);
    match (k.role, k.host) {
        (Role::Background, DesktopHost::Progman { defview, workerw, .. }) => {
            if !is_window(workerw) { return Some(format!("WorkerW {workerw:#x} is gone")); }
            if order.last() != Some(&workerw) { return Some("WorkerW is no longer Progman's last child".into()); }
            if pos(defview) > pos(k.hwnd) { return Some("the window is above SHELLDLL_DefView".into()); }
        }
        (Role::DesktopChild, _) if k.defview != 0 && pos(k.hwnd) > pos(k.defview) => {
            return Some("the widget is below SHELLDLL_DefView".into());
        }
        _ => {}
    }
    None
}

fn update(h: isize, f: impl FnOnce(&mut Kept)) {
    if let Some(k) = KEEPER.lock().unwrap_or_else(|e| e.into_inner()).kept.iter_mut().find(|k| k.hwnd == h) { f(k); }
}

/// What the keeper does about a problem it found.
#[derive(Debug, PartialEq)]
enum Next { Attach, GiveUp, Quiet }

/// `strikes` counts successful re-attaches after which the same problem came straight back. Failed
/// attempts (Explorer still restarting) never count, so those are retried for as long as it takes.
fn next_step(why: &str, last_problem: &str, last_failed: bool, strikes: u32) -> (u32, Next) {
    let strikes = if why == last_problem && !last_failed { strikes + 1 } else { 0 };
    let next = match strikes.cmp(&MAX_STRIKES) {
        std::cmp::Ordering::Less => Next::Attach,
        std::cmp::Ordering::Equal => Next::GiveUp,
        std::cmp::Ordering::Greater => Next::Quiet,
    };
    (strikes, next)
}

fn check(k: Kept) {
    if !is_window(k.hwnd) {
        forget(k.hwnd);
        diag::log(format!("keeper {}: window {:#x} is gone (destroyed with its host?)", k.label, k.hwnd));
        return;
    }
    let Some(why) = problem(&k) else {
        if k.strikes > 0 || k.failures > 0 || !k.last_problem.is_empty() {
            update(k.hwnd, |x| { x.strikes = 0; x.failures = 0; x.last_problem.clear(); });
        }
        return;
    };
    let (strikes, next) = next_step(&why, &k.last_problem, k.failures > 0, k.strikes);
    update(k.hwnd, |x| { x.strikes = strikes; x.last_problem = why.clone(); });
    match next {
        Next::Quiet => return,
        Next::GiveUp => {
            diag::log(format!("keeper {}: '{why}' keeps coming back after attaching again; leaving it until it changes", k.label));
            return;
        }
        Next::Attach => {}
    }
    if k.failures == 0 { diag::log(format!("keeper {}: {why}; attaching again ({})", k.label, os_build())); }
    match attach_to_desktop(&k.app, &k.label, k.hwnd, k.role, k.screen) {
        Ok((report, host, defview)) => {
            diag::log(format!("re-attach {}: {} {}", k.label, report.method, report.detail));
            diag::record_attach(report);
            update(k.hwnd, |x| { x.host = host; x.defview = defview; x.failures = 0; });
            watch::reannounce(&k.label);
        }
        Err(e) => {
            let failures = k.failures + 1;
            update(k.hwnd, |x| x.failures = failures);
            // 1st, 2nd, 4th, 8th… failure, so a long Explorer outage does not flood the 200-line log.
            if failures.is_power_of_two() {
                diag::log(format!("re-attach {} failed ({failures} in a row), retrying every 2 s: {e}", k.label));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEGACY: DesktopHost = DesktopHost::Legacy { workerw: 12, defview_parent: 10 };
    const PROGMAN: DesktopHost = DesktopHost::Progman { progman: 1, defview: 11, workerw: 12 };

    #[test] fn method_names_match_the_interface() {
        assert_eq!(method(Role::Background, Some(LEGACY)), "win-legacy-workerw");
        assert_eq!(method(Role::Background, Some(PROGMAN)), "win-progman");
        assert_eq!(method(Role::DesktopChild, Some(PROGMAN)), "win-widget-desktop-child");
        assert_eq!(method(Role::Bottom, None), "win-widget-bottom");
        assert_eq!(method(Role::Character, None), "win-topmost");
    }
    #[test] fn parents() {
        assert_eq!(parent_of(Role::Background, LEGACY), 12);
        assert_eq!(parent_of(Role::DesktopChild, LEGACY), 10);
        assert_eq!(parent_of(Role::Background, PROGMAN), 1);
        assert_eq!(parent_of(Role::DesktopChild, PROGMAN), 1);
    }
    #[test] fn children_of_progman_are_layered_and_only_the_background_lets_clicks_through() {
        let bg = desktop_bits(Role::Background, PROGMAN);
        assert_eq!(bg.set_ex, WS_EX_LAYERED.0 | WS_EX_TRANSPARENT.0 | WS_EX_NOACTIVATE.0);
        assert_eq!(desktop_bits(Role::DesktopChild, PROGMAN).set_ex, WS_EX_LAYERED.0);
        assert_eq!(desktop_bits(Role::Background, LEGACY).set_ex, WS_EX_NOACTIVATE.0);
        assert_eq!(desktop_bits(Role::DesktopChild, LEGACY).set_ex, 0);
        assert_eq!((bg.set_style, bg.clear_style), (WS_CHILD.0, WS_POPUP.0));
    }
    #[test] fn keeper_gives_up_only_on_a_problem_that_returns_after_successful_attaches() {
        // First sighting, then the same problem after each successful re-attach.
        assert_eq!(next_step("p", "", false, 0), (0, Next::Attach));
        assert_eq!(next_step("p", "p", false, 0), (1, Next::Attach));
        assert_eq!(next_step("p", "p", false, 1), (2, Next::Attach));
        assert_eq!(next_step("p", "p", false, 2), (3, Next::GiveUp));
        assert_eq!(next_step("p", "p", false, 3), (4, Next::Quiet));
        // Failed attempts (host still missing) keep retrying.
        assert_eq!(next_step("p", "p", true, 2), (0, Next::Attach));
        // A different problem starts over.
        assert_eq!(next_step("q", "p", false, 5), (0, Next::Attach));
    }
    #[test] fn edit_sets_then_clears() {
        assert_eq!(edit(0b1010, 0b0101, 0b1000), 0b0111);
    }
}
