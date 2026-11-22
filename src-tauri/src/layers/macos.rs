//! macOS layers: window levels relative to the desktop picture and the Finder desktop icons.

use super::{AttachReport, LayerKind};
use crate::main_thread;
use objc2_app_kit::{NSFloatingWindowLevel, NSWindow, NSWindowCollectionBehavior};
use objc2_core_graphics::{CGWindowLevelForKey, CGWindowLevelKey};
use tauri::{Manager, WebviewWindow};

/// Level, whether clicks pass through, and the method name recorded in the diagnostics.
fn placement(kind: LayerKind) -> (isize, bool, &'static str) {
    match kind {
        LayerKind::Background => (CGWindowLevelForKey(CGWindowLevelKey::DesktopWindowLevelKey) as isize, true, "mac-desktop"),
        LayerKind::Widget => (CGWindowLevelForKey(CGWindowLevelKey::DesktopIconWindowLevelKey) as isize + 1, false, "mac-desktop-icon+1"),
        LayerKind::Character => (NSFloatingWindowLevel, false, "mac-floating"),
    }
}

pub fn attach(window: &WebviewWindow, kind: LayerKind) -> Result<AttachReport, String> {
    let label = window.label().to_string();
    let (level, ignores, method) = placement(kind);
    let w = window.clone();
    let applied = main_thread::run_sync(window.app_handle(), move || -> Result<(isize, usize), String> {
        let ptr = w.ns_window().map_err(|e| e.to_string())?;
        // SAFETY: tauri hands out the window's live NSWindow pointer and we are on the main thread.
        let ns: &NSWindow = unsafe { &*(ptr as *const NSWindow) };
        ns.setLevel(level);
        ns.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );
        ns.setIgnoresMouseEvents(ignores);
        Ok((ns.level(), ns.collectionBehavior().0))
    })??;
    Ok(AttachReport {
        label,
        method: method.into(),
        detail: format!("level={} collectionBehavior={:#x} ignoresMouseEvents={ignores}", applied.0, applied.1),
    })
}
