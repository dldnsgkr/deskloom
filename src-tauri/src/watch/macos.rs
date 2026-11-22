//! macOS visibility watch: NSWindowDidChangeOcclusionStateNotification per layer window.
//! A full-screen app's Space counts as occlusion for desktop-level windows, so it needs no separate check.

use super::{occlusion_change, EVENT};
use crate::{diag, main_thread};
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol, ProtocolObject};
use objc2_app_kit::{NSWindow, NSWindowDidChangeOcclusionStateNotification, NSWindowOcclusionState};
use objc2_foundation::{NSNotification, NSNotificationCenter};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow, WindowEvent};

/// The notification only fires on a change, so a window that is already hidden when it is shown
/// (screen locked, full-screen Space, covered) would never report. Check once this long after `show()`.
const INITIAL_CHECK: Duration = Duration::from_millis(500);

struct Entry { id: u64, token: Retained<ProtocolObject<dyn NSObjectProtocol>>, last: Option<bool> }

thread_local! {
    // Main thread only. Keyed by label; `id` tells a reopened window from the one it replaced.
    static OBSERVERS: RefCell<HashMap<String, Entry>> = RefCell::new(HashMap::new());
}
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn is_visible(win: &NSWindow) -> bool {
    win.occlusionState().contains(NSWindowOcclusionState::Visible)
}

/// Sends `visibility` if the state changed. Main thread only.
fn report(app: &AppHandle, label: &str, id: u64, visible: bool) {
    let ev = OBSERVERS.with(|o| match o.borrow_mut().get_mut(label) {
        Some(e) if e.id == id => occlusion_change(label, &mut e.last, visible),
        _ => None,
    });
    if let Some(ev) = ev {
        diag::log(format!("visibility {} visible={} reason={}", ev.label, ev.visible, ev.reason));
        let _ = app.emit(EVENT, ev);
    }
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
            if let Some(win) = obj.downcast_ref::<NSWindow>() { report(&app, &lbl, id, is_visible(win)); }
        });
        let obj: &AnyObject = ns;
        // SAFETY: valid name and object; delivered on the posting (main) thread.
        let token = unsafe {
            NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
                Some(NSWindowDidChangeOcclusionStateNotification), Some(obj), None, &block)
        };
        let old = OBSERVERS.with(|o| o.borrow_mut().insert(label, Entry { id, token, last: None }));
        if let Some(old) = old { remove_observer(&old.token); }
        Ok(())
    })??;

    let (w, app, label) = (window.clone(), window.app_handle().clone(), window.label().to_string());
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(INITIAL_CHECK).await;
        let app2 = app.clone();
        let _ = app.run_on_main_thread(move || {
            // Skip if the window was closed or replaced meanwhile.
            if !OBSERVERS.with(|o| o.borrow().get(&label).is_some_and(|e| e.id == id)) { return; }
            let Ok(ptr) = w.ns_window() else { return };
            // SAFETY: as above; the window is still tracked, so still open.
            let visible = is_visible(unsafe { &*(ptr as *const NSWindow) });
            report(&app2, &label, id, visible);
        });
    });

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
            if o.get(&label).is_some_and(|e| e.id == id) { o.remove(&label) } else { None }
        });
        if let Some(e) = entry { remove_observer(&e.token); }
    });
}
