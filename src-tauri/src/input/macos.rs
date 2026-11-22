//! macOS desktop pointer: a global NSEvent monitor for mouse events.
//! Global monitors only see events that go to other apps, so moves over our own (non click-through)
//! widget and character windows never arrive. Mouse-only global monitors need no permission.

use super::{DesktopPointer, MoveThrottle, EVENT};
use crate::{diag, main_thread};
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol, ProtocolObject};
use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplicationDidChangeScreenParametersNotification, NSEvent, NSEventMask, NSEventType, NSScreen, NSWindow};
use objc2_core_foundation::{CFArray, CFString};
use objc2_core_graphics::{kCGWindowLayer, CGWindowListCopyWindowInfo, CGWindowListOption};
use objc2_foundation::{NSArray, NSDictionary, NSNotification, NSNotificationCenter, NSNumber, NSPoint, NSString};
use std::cell::{Cell, RefCell};
use std::ptr::NonNull;
use std::sync::{Mutex, RwLock};
use std::time::Instant;
use tauri::{AppHandle, Emitter};

/// One NSScreen: its frame in points (Cocoa space: origin at the primary screen's bottom-left, y up)
/// and where its top-left corner sits in physical px.
#[derive(Clone, Copy, Debug, PartialEq)]
struct ScreenMap { min_x: f64, min_y: f64, max_x: f64, max_y: f64, origin_x: f64, origin_y: f64, scale: f64 }

/// Read on the main thread (at start and on screen changes); `to_physical` only reads this, from any thread.
static SCREENS: RwLock<Vec<ScreenMap>> = RwLock::new(Vec::new());
/// Kept so `stop()` (which takes no handle) can reach the main thread.
static APP: Mutex<Option<AppHandle>> = Mutex::new(None);

thread_local! {
    // Main thread only.
    static MONITOR: RefCell<Option<Retained<AnyObject>>> = const { RefCell::new(None) };
    static SCREEN_OBSERVER: RefCell<Option<Retained<ProtocolObject<dyn NSObjectProtocol>>>> = const { RefCell::new(None) };
}

/// `screens` are (x, y, width, height, backing scale) of each NSScreen frame, primary screen first.
/// A screen's top-left in physical px is its CGDisplayBounds origin (top-left space, points) times
/// its scale — the same formula tao uses for `Monitor::position()`. Computed from NSScreen directly
/// because `available_monitors()` can come back empty (seen on the dev Mac).
fn map_screens(screens: &[(f64, f64, f64, f64, f64)]) -> Vec<ScreenMap> {
    let Some(&(_, py, _, ph, _)) = screens.first() else { return Vec::new() };
    let primary_top = py + ph;
    screens.iter().map(|&(x, y, w, h, scale)| ScreenMap {
        min_x: x, min_y: y, max_x: x + w, max_y: y + h,
        origin_x: (x * scale).round(),
        origin_y: ((primary_top - (y + h)) * scale).round(),
        scale,
    }).collect()
}

/// `x = mon.x + (pt.x − screen.minX) × scale`, `y = mon.y + (screen.maxY − pt.y) × scale`, using the
/// screen under the point (x half-open on the right, y half-open at the bottom, since Cocoa's y grows
/// upward), else the nearest screen. Returns the point unchanged if no screens are known.
fn point_to_physical(screens: &[ScreenMap], x: f64, y: f64) -> (f64, f64) {
    let dist = |s: &ScreenMap| {
        let dx = (s.min_x - x).max(x - s.max_x).max(0.0);
        let dy = (s.min_y - y).max(y - s.max_y).max(0.0);
        dx * dx + dy * dy
    };
    let screen = screens.iter()
        .find(|s| x >= s.min_x && x < s.max_x && y > s.min_y && y <= s.max_y)
        .or_else(|| screens.iter().min_by(|a, b| dist(a).total_cmp(&dist(b))));
    match screen {
        Some(s) => (s.origin_x + (x - s.min_x) * s.scale, s.origin_y + (s.max_y - y) * s.scale),
        None => (x, y),
    }
}

/// Converts a Cocoa screen point (e.g. `NSEvent::mouseLocation()`) to physical px in Tauri's monitor space.
/// Safe to call from any thread; it only reads the screen cache.
pub(crate) fn to_physical(pt: NSPoint) -> (f64, f64) {
    point_to_physical(&SCREENS.read().unwrap_or_else(|e| e.into_inner()), pt.x, pt.y)
}

/// Height in points of the primary screen (the first one), if the cache has been filled.
pub(crate) fn primary_height() -> Option<f64> {
    SCREENS.read().unwrap_or_else(|e| e.into_inner()).first().map(|s| s.max_y - s.min_y)
}

fn refresh_screens(mtm: MainThreadMarker) {
    let raw: Vec<_> = NSScreen::screens(mtm).iter().map(|s| {
        let f = s.frame();
        (f.origin.x, f.origin.y, f.size.width, f.size.height, s.backingScaleFactor())
    }).collect();
    let mapped = map_screens(&raw);
    diag::log(format!("screens: {}", mapped.iter().map(|s| format!(
        "[pt x={} y={} w={} h={} scale={} -> px x={} y={}]",
        s.min_x, s.min_y, s.max_x - s.min_x, s.max_y - s.min_y, s.scale, s.origin_x, s.origin_y)).collect::<Vec<_>>().join(" ")));
    *SCREENS.write().unwrap_or_else(|e| e.into_inner()) = mapped;
}

/// Fills the screen cache and keeps it current. Idempotent; must run on the main thread.
/// Anything that calls `to_physical` without `start()` (Task 9) should call this first.
pub(crate) fn watch_screens(mtm: MainThreadMarker) {
    SCREEN_OBSERVER.with(|o| {
        if o.borrow().is_some() { return; }
        let block = RcBlock::new(|_: NonNull<NSNotification>| {
            // Posted on the main thread.
            if let Some(mtm) = MainThreadMarker::new() { refresh_screens(mtm); }
        });
        // SAFETY: valid name, no object filter, delivered on the posting (main) thread.
        let token = unsafe {
            NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
                Some(NSApplicationDidChangeScreenParametersNotification), None, None, &block)
        };
        *o.borrow_mut() = Some(token);
    });
    refresh_screens(mtm);
}

/// `kCGWindowLayer` of a window, or None if the window is gone.
fn window_layer(number: isize) -> Option<i64> {
    let list = CGWindowListCopyWindowInfo(CGWindowListOption::OptionIncludingWindow, u32::try_from(number).ok()?)?;
    // SAFETY: CFArray/NSArray and CFDictionary/NSDictionary and CFString/NSString are toll-free bridged;
    // the window list is an array of dictionaries keyed by strings.
    let arr: &NSArray<NSDictionary<NSString, AnyObject>> =
        unsafe { &*(&*list as *const CFArray as *const NSArray<NSDictionary<NSString, AnyObject>>) };
    let key: &NSString = unsafe { &*(kCGWindowLayer as *const CFString as *const NSString) };
    let layer = arr.firstObject()?.objectForKey(key)?.downcast::<NSNumber>().ok()?;
    Some(layer.integerValue() as i64)
}

fn handler(app: AppHandle) -> RcBlock<dyn Fn(NonNull<NSEvent>)> {
    let throttle = RefCell::new(MoveThrottle::default());
    // Window under the cursor and whether it sits below normal windows; looked up again only when it changes.
    let hit_window = Cell::new(-1isize);
    let over_desktop = Cell::new(false);
    let logged = Cell::new(0u8);
    RcBlock::new(move |ev: NonNull<NSEvent>| {
        // SAFETY: AppKit passes a valid event for the duration of the call.
        let ty = unsafe { ev.as_ref() }.r#type();
        let is_move = ty == NSEventType::MouseMoved || ty == NSEventType::LeftMouseDragged;
        if is_move && !throttle.borrow_mut().accept_move(Instant::now()) { return; }
        let Some(mtm) = MainThreadMarker::new() else { return };
        let pt = NSEvent::mouseLocation();
        let number = NSWindow::windowNumberAtPoint_belowWindowWithWindowNumber(pt, 0, mtm);
        if number != hit_window.get() {
            hit_window.set(number);
            // 0: nothing at all under the cursor, which is the bare desktop.
            over_desktop.set(number == 0 || window_layer(number).is_some_and(|layer| layer < 0));
        }
        let (x, y) = to_physical(pt);
        let p = DesktopPointer {
            x, y,
            down: ty == NSEventType::LeftMouseDown || ty == NSEventType::LeftMouseDragged,
            over_desktop: over_desktop.get(),
        };
        if logged.get() < 5 {
            logged.set(logged.get() + 1);
            diag::log(format!("desktop-pointer {p:?} (pt {:.1},{:.1} window {number})", pt.x, pt.y));
        }
        let _ = app.emit(EVENT, p);
    })
}

/// Starts sending `desktop-pointer`. Idempotent.
pub fn start(app: &AppHandle) -> Result<(), String> {
    *APP.lock().unwrap_or_else(|e| e.into_inner()) = Some(app.clone());
    let app2 = app.clone();
    main_thread::run_sync(app, move || -> Result<(), String> {
        let mtm = MainThreadMarker::new().ok_or_else(|| "not on the main thread".to_string())?;
        watch_screens(mtm);
        MONITOR.with(|m| {
            if m.borrow().is_some() { return Ok(()); }
            // LeftMouseUp too, so `down` goes back to false on release without waiting for the next move.
            let mask = NSEventMask::MouseMoved | NSEventMask::LeftMouseDown | NSEventMask::LeftMouseDragged | NSEventMask::LeftMouseUp;
            let token = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(mask, &handler(app2))
                .ok_or_else(|| "addGlobalMonitorForEvents returned nil".to_string())?;
            *m.borrow_mut() = Some(token);
            diag::log("desktop pointer: global mouse monitor installed");
            Ok(())
        })
    })?
}

/// Stops sending `desktop-pointer`.
pub fn stop() {
    let Some(app) = APP.lock().unwrap_or_else(|e| e.into_inner()).clone() else { return };
    let _ = app.run_on_main_thread(|| {
        MONITOR.with(|m| {
            if let Some(token) = m.borrow_mut().take() {
                // SAFETY: the token came from addGlobalMonitorForEvents and is removed once.
                unsafe { NSEvent::removeMonitor(&token) };
                diag::log("desktop pointer: global mouse monitor removed");
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    // Primary 1440x900 @2x at the origin; a 1920x1080 @1x screen to its right, bottoms aligned.
    fn two_screens() -> Vec<ScreenMap> {
        map_screens(&[(0.0, 0.0, 1440.0, 900.0, 2.0), (1440.0, 0.0, 1920.0, 1080.0, 1.0)])
    }

    #[test] fn top_left_corner_is_the_cg_display_origin_times_scale() {
        let s = two_screens();
        assert_eq!((s[0].origin_x, s[0].origin_y), (0.0, 0.0));
        // The taller screen reaches 180pt above the primary's top, so its top-left y is -180.
        assert_eq!((s[1].origin_x, s[1].origin_y), (1440.0, -180.0));
    }
    #[test] fn primary_points_flip_and_scale() {
        let s = two_screens();
        assert_eq!(point_to_physical(&s, 0.0, 900.0), (0.0, 0.0));
        assert_eq!(point_to_physical(&s, 100.0, 800.0), (200.0, 200.0));
        assert_eq!(point_to_physical(&s, 1439.0, 1.0), (2878.0, 1798.0));
    }
    #[test] fn secondary_screen_uses_its_own_origin_and_scale() {
        let s = two_screens();
        assert_eq!(point_to_physical(&s, 1440.0, 1080.0), (1440.0, -180.0));
        assert_eq!(point_to_physical(&s, 1540.0, 80.0), (1540.0, 820.0));
    }
    #[test] fn points_off_every_screen_use_the_nearest() {
        let s = two_screens();
        // Above the primary's top edge (where only the secondary extends): nearest is still the primary.
        assert_eq!(point_to_physical(&s, 100.0, 950.0), (200.0, -100.0));
    }
    #[test] fn screen_below_the_primary() {
        let s = map_screens(&[(0.0, 0.0, 1440.0, 900.0, 2.0), (0.0, -1080.0, 1920.0, 1080.0, 1.0)]);
        assert_eq!((s[1].origin_x, s[1].origin_y), (0.0, 900.0));
        assert_eq!(point_to_physical(&s, 10.0, -10.0), (10.0, 910.0));
    }
    #[test] fn no_screens_returns_the_point() {
        assert_eq!(point_to_physical(&[], 5.0, 6.0), (5.0, 6.0));
    }
}
