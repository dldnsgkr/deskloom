//! macOS visibility watch: NSWindowDidChangeOcclusionStateNotification per layer window, plus our own
//! bounds check. A full-screen app's Space counts as occlusion for desktop-level windows, so it needs
//! no separate check. `occlusionState` alone is not enough: it stays Visible under a maximized window
//! (the transparent menu bar strip and the rounded corners show the desktop), so a background or widget
//! also counts as hidden when the windows that can cover it contain its whole area. Characters are
//! always on top of other windows and keep the occlusionState-only behaviour.

use super::cover::{self, CgWindow};
use super::{occlusion_change, Visibility, EVENT};
use crate::geometry::{self, Rect};
use crate::{diag, main_thread};
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol, ProtocolObject};
use objc2::MainThreadMarker;
use objc2_app_kit::{NSScreen, NSWindow, NSWindowDidChangeOcclusionStateNotification, NSWindowOcclusionState};
use objc2_core_foundation::{CFArray, CFString};
use objc2_core_graphics::{
    kCGWindowAlpha, kCGWindowBounds, kCGWindowLayer, kCGWindowOwnerName, kCGWindowOwnerPID,
    CGWindowListCopyWindowInfo, CGWindowListOption,
};
use objc2_foundation::{NSArray, NSDictionary, NSNotification, NSNotificationCenter, NSNumber, NSString};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow, WindowEvent};

/// The notification only fires on a change, so a window that is already hidden when it is shown
/// (screen locked, full-screen Space, covered) would never report. Check once this long after `show()`.
const INITIAL_CHECK: Duration = Duration::from_millis(500);

/// A window moving over or off a layer whose occlusionState stays Visible posts no notification, so the
/// bounds are checked on this tick as well. A visible -> hidden change is reported on the tick after it
/// first showed up, which also absorbs the 4-5 flaps within ~300 ms that a transition produces.
const TICK: Duration = Duration::from_millis(500);

/// How long after `track` the current state is re-sent on every tick (as in the Windows watcher).
const ANNOUNCE: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, PartialEq)]
enum Role { Background, Widget, Character }

fn role_of(label: &str) -> Role {
    if label.starts_with("bg-") { Role::Background } else if label == "ch" { Role::Character } else { Role::Widget }
}

struct Entry {
    id: u64,
    token: Retained<ProtocolObject<dyn NSObjectProtocol>>,
    last: Option<bool>,
    win: WebviewWindow,
    /// The current state is re-sent on each tick until then: a layer page registers its `visibility`
    /// listener after the first report, which would otherwise be lost.
    announce_until: Instant,
    /// A hidden verdict seen once; it is reported if the next tick still says hidden.
    pending_hidden: bool,
}

thread_local! {
    // Main thread only. Keyed by label; `id` tells a reopened window from the one it replaced.
    static OBSERVERS: RefCell<HashMap<String, Entry>> = RefCell::new(HashMap::new());
}
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
/// Number of tracked windows (kept in step with `OBSERVERS`), readable from the tick task.
static TRACKED: AtomicUsize = AtomicUsize::new(0);
static TICKING: AtomicBool = AtomicBool::new(false);

fn is_visible(win: &NSWindow) -> bool {
    win.occlusionState().contains(NSWindowOcclusionState::Visible)
}

fn cocoa_rect_to_cg(r: objc2_foundation::NSRect, primary_h: f64) -> Rect {
    cover::cocoa_to_cg(r.origin.x, r.origin.y, r.size.width, r.size.height, primary_h)
}

/// The area that must be covered for the layer to count as hidden, in CG coordinates: the layer's frame
/// cut to its screen's `visibleFrame`, so the menu bar and Dock count as covering for every layer (a
/// background's frame is its screen, so it needs exactly the `visibleFrame`; the one-window widget
/// layer spans the whole monitor, menu bar strip included, which no window can ever cover).
/// An empty area (a layer entirely inside the menu bar or off every screen) is never "covered"
/// (`cover::covered`), so such a layer keeps following occlusionState alone.
fn target_area(ns: &NSWindow, mtm: MainThreadMarker) -> Option<Rect> {
    let primary_h = NSScreen::screens(mtm).firstObject()?.frame().size.height;
    let frame = cocoa_rect_to_cg(ns.frame(), primary_h);
    Some(match ns.screen() {
        Some(s) => geometry::intersection(frame, cocoa_rect_to_cg(s.visibleFrame(), primary_h)),
        None => frame,
    })
}

/// On-screen windows with bounds, owner and layer. Needs no Screen Recording permission as long as
/// `kCGWindowName` is not read. Windows that can never cover are dropped before their bounds are parsed.
fn fetch_windows(our_pid: i32) -> Vec<CgWindow> {
    let opts = CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements;
    let Some(list) = CGWindowListCopyWindowInfo(opts, 0) else { return Vec::new() };
    // SAFETY: CFArray/NSArray, CFDictionary/NSDictionary and CFString/NSString are toll-free bridged;
    // the window list is an array of dictionaries keyed by strings.
    let arr: &NSArray<NSDictionary<NSString, AnyObject>> =
        unsafe { &*(&*list as *const CFArray as *const NSArray<NSDictionary<NSString, AnyObject>>) };
    // SAFETY: the kCGWindow* keys are immortal CFStrings, bridged as above.
    let key = |k: &'static CFString| -> &NSString { unsafe { &*(k as *const CFString as *const NSString) } };
    let (k_layer, k_alpha, k_pid, k_bounds, k_owner) =
        (key(unsafe { kCGWindowLayer }), key(unsafe { kCGWindowAlpha }), key(unsafe { kCGWindowOwnerPID }),
         key(unsafe { kCGWindowBounds }), key(unsafe { kCGWindowOwnerName }));
    let num = |d: &NSDictionary<NSString, AnyObject>, k: &NSString| d.objectForKey(k)?.downcast::<NSNumber>().ok();
    let (kx, ky, kw, kh) = (NSString::from_str("X"), NSString::from_str("Y"), NSString::from_str("Width"), NSString::from_str("Height"));
    let mut out = Vec::new();
    for d in arr.iter() {
        let (Some(layer), Some(alpha), Some(pid)) = (num(&d, k_layer), num(&d, k_alpha), num(&d, k_pid)) else { continue };
        let mut w = CgWindow {
            layer: layer.intValue(), alpha: alpha.doubleValue(), pid: pid.intValue(),
            bounds: Rect { x: 0, y: 0, w: 1, h: 1 }, owner: String::new(),
        };
        if !cover::can_cover(&w, our_pid) { continue; }
        let Some(b) = d.objectForKey(k_bounds).and_then(|b| b.downcast::<NSDictionary>().ok()) else { continue };
        // SAFETY: kCGWindowBounds is a dictionary keyed by strings; only the type parameter changes.
        let b: &NSDictionary<NSString, AnyObject> = unsafe { &*(&*b as *const NSDictionary as *const NSDictionary<NSString, AnyObject>) };
        let (Some(x), Some(y), Some(bw), Some(bh)) = (num(b, &kx), num(b, &ky), num(b, &kw), num(b, &kh)) else { continue };
        w.bounds = Rect { x: x.doubleValue().round() as i32, y: y.doubleValue().round() as i32,
                          w: bw.doubleValue().round() as i32, h: bh.doubleValue().round() as i32 };
        w.owner = d.objectForKey(k_owner).and_then(|o| o.downcast::<NSString>().ok()).map(|o| o.to_string()).unwrap_or_default();
        out.push(w);
    }
    out
}

/// Recomputes the state of one tracked layer (`only`) or of all of them, and sends `visibility` for
/// what changed. `tick` marks the periodic call, which is the one that confirms a hidden verdict.
/// Main thread only.
fn evaluate(app: &AppHandle, only: Option<(&str, u64)>, tick: bool) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let items: Vec<(String, u64, WebviewWindow)> = OBSERVERS.with(|o| o.borrow().iter()
        .filter(|(l, e)| only.is_none_or(|(ol, oid)| ol == l.as_str() && oid == e.id))
        .map(|(l, e)| (l.clone(), e.id, e.win.clone())).collect());
    let our_pid = std::process::id() as i32;
    // One CG call per check, and none when no layer needs it.
    let mut windows: Option<Vec<CgWindow>> = None;
    for (label, id, win) in items {
        let Ok(ptr) = win.ns_window() else { continue };
        // SAFETY: tauri hands out the live NSWindow pointer; the window is tracked, so still open, and we are on the main thread.
        let ns: &NSWindow = unsafe { &*(ptr as *const NSWindow) };
        let role = role_of(&label);
        let occ = is_visible(ns);
        let mut why = None;
        let mut visible = occ;
        if let (true, Some(target)) = (occ && role != Role::Character, target_area(ns, mtm)) {
            let ws = windows.get_or_insert_with(|| fetch_windows(our_pid));
            if cover::covered(target, ws, our_pid) {
                visible = false;
                why = Some(cover::describe_covers(target, ws, our_pid));
            }
        }
        let (ev, resend) = OBSERVERS.with(|o| match o.borrow_mut().get_mut(&label) {
            Some(e) if e.id == id => {
                let ev = if visible { e.pending_hidden = false; occlusion_change(&label, &mut e.last, true) }
                // First report goes out at once; a visible -> hidden flip waits for one more tick.
                else if e.last == Some(true) && !(tick && e.pending_hidden) { e.pending_hidden = true; None }
                else { e.pending_hidden = false; occlusion_change(&label, &mut e.last, false) };
                let resend = if ev.is_none() && tick && Instant::now() < e.announce_until { e.last } else { None };
                (ev, resend)
            }
            _ => (None, None),
        });
        if let Some(ev) = ev {
            let covers = why.map(|w| format!("; covered by: {w}")).unwrap_or_default();
            diag::log(format!("visibility {} visible={} reason={}{covers}", ev.label, ev.visible, ev.reason));
            let _ = app.emit(EVENT, ev);
        } else if let Some(v) = resend {
            // Same state again, without a log line.
            let _ = app.emit(EVENT, Visibility { label: label.clone(), visible: v, reason: if v { "visible" } else { "occluded" } });
        }
    }
}

/// Runs `evaluate` for every layer each `TICK` while any is tracked.
fn ensure_ticking(app: &AppHandle) {
    if TICKING.swap(true, Ordering::AcqRel) { return; }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(TICK).await;
            if TRACKED.load(Ordering::Acquire) == 0 {
                TICKING.store(false, Ordering::Release);
                // A window tracked between the check and the store saw TICKING still set and started nothing.
                if TRACKED.load(Ordering::Acquire) == 0 || TICKING.swap(true, Ordering::AcqRel) { break; }
                continue;
            }
            let a = app.clone();
            let _ = app.run_on_main_thread(move || evaluate(&a, None, true));
        }
    });
}

fn remove_observer(token: &ProtocolObject<dyn NSObjectProtocol>) {
    // SAFETY: the token came from addObserverForName and is removed once (it leaves the map here).
    unsafe { NSNotificationCenter::defaultCenter().removeObserver(token.as_ref()) };
}

/// Starts sending `visibility` for this window. Call before `show()`.
pub fn track(window: &WebviewWindow) -> Result<(), String> {
    let label = window.label().to_string();
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let (w, app, lbl) = (window.clone(), window.app_handle().clone(), label.clone());
    main_thread::run_sync(window.app_handle(), move || -> Result<(), String> {
        let ptr = w.ns_window().map_err(|e| e.to_string())?;
        // SAFETY: tauri hands out the window's live NSWindow pointer and we are on the main thread.
        let ns: &NSWindow = unsafe { &*(ptr as *const NSWindow) };
        let block = RcBlock::new(move |note: NonNull<NSNotification>| {
            // SAFETY: valid for the duration of the call; the object is the observed NSWindow.
            let Some(obj) = unsafe { note.as_ref() }.object() else { return };
            if obj.downcast_ref::<NSWindow>().is_some() { evaluate(&app, Some((&lbl, id)), false); }
        });
        let obj: &AnyObject = ns;
        // SAFETY: valid name and object; delivered on the posting (main) thread.
        let token = unsafe {
            NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
                Some(NSWindowDidChangeOcclusionStateNotification), Some(obj), None, &block)
        };
        let old = OBSERVERS.with(|o| {
            let mut o = o.borrow_mut();
            let old = o.insert(label, Entry { id, token, last: None, win: w.clone(), announce_until: Instant::now() + ANNOUNCE, pending_hidden: false });
            TRACKED.store(o.len(), Ordering::Release);
            old
        });
        if let Some(old) = old { remove_observer(&old.token); }
        Ok(())
    })??;

    let (app, label) = (window.app_handle().clone(), window.label().to_string());
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(INITIAL_CHECK).await;
        let app2 = app.clone();
        // `evaluate` skips a window that was closed or replaced meanwhile (id no longer matches).
        let _ = app.run_on_main_thread(move || evaluate(&app2, Some((&label, id)), false));
    });
    ensure_ticking(window.app_handle());

    let (app, label) = (window.app_handle().clone(), window.label().to_string());
    window.on_window_event(move |e| {
        if let WindowEvent::Destroyed = e { untrack(&app, label.clone(), id); }
    });
    Ok(())
}

/// Removes the observer, unless a newer window with the same label has replaced it.
fn untrack(app: &AppHandle, label: String, id: u64) {
    let _ = app.run_on_main_thread(move || {
        let entry = OBSERVERS.with(|o| {
            let mut o = o.borrow_mut();
            let removed = if o.get(&label).is_some_and(|e| e.id == id) { o.remove(&label) } else { None };
            TRACKED.store(o.len(), Ordering::Release);
            removed
        });
        if let Some(e) = entry { remove_observer(&e.token); }
    });
}
