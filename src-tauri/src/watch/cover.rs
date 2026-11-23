//! Window-coverage decision for the macOS watcher. Pure, so it compiles and is tested on every OS.
//! `NSWindow.occlusionState` alone says "visible" while a maximized window leaves the transparent menu
//! bar strip or its rounded corners open, so the watcher also checks the window bounds itself.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use crate::geometry::{self, Rect};

/// `NSMainMenuWindowLevel`: the menu bar and everything from it up never counts as a cover.
const MENU_BAR_LAYER: i32 = 24;
/// Translucent windows (blur, dimmed overlays) leave the desktop visible behind them.
const MIN_COVER_ALPHA: f64 = 0.99;

/// One entry of `CGWindowListCopyWindowInfo`; `bounds` is in top-left global points.
#[derive(Clone, Debug, PartialEq)]
pub struct CgWindow { pub layer: i32, pub alpha: f64, pub pid: i32, pub bounds: Rect, pub owner: String }

/// Our own layers sit below layer 0 and the desktop icons far below it, so "can cover" needs no z-order.
pub fn can_cover(w: &CgWindow, our_pid: i32) -> bool {
    (0..MENU_BAR_LAYER).contains(&w.layer) && w.alpha >= MIN_COVER_ALPHA && w.pid != our_pid
        && w.bounds.w > 0 && w.bounds.h > 0
}

/// True if the windows that can cover together contain all of `target`. An empty target is never covered
/// (an unplaced window must not be reported as hidden).
pub fn covered(target: Rect, windows: &[CgWindow], our_pid: i32) -> bool {
    if target.w <= 0 || target.h <= 0 { return false; }
    let rects: Vec<Rect> = windows.iter().filter(|w| can_cover(w, our_pid)).map(|w| w.bounds).collect();
    geometry::is_fully_covered(target, &rects)
}

/// "<owner> at x,y wxh, ... (area x,y wxh)" for the covering windows that overlap `target`, at most 8.
pub fn describe_covers(target: Rect, windows: &[CgWindow], our_pid: i32) -> String {
    let hits: Vec<String> = windows.iter()
        .filter(|w| can_cover(w, our_pid) && geometry::intersects(w.bounds, target))
        .map(|w| format!("{} at {},{} {}x{}", w.owner, w.bounds.x, w.bounds.y, w.bounds.w, w.bounds.h))
        .collect();
    let more = hits.len().saturating_sub(8);
    let mut s = hits.into_iter().take(8).collect::<Vec<_>>().join(", ");
    if more > 0 { s.push_str(&format!(", and {more} more")); }
    format!("{s} (area {},{} {}x{})", target.x, target.y, target.w, target.h)
}

/// A Cocoa rect (bottom-left origin, y up, on the primary screen's coordinate space) as a CG rect
/// (top-left origin, y down). Rounds inward, so a sub-point gap never makes the target look uncovered.
pub fn cocoa_to_cg(x: f64, y: f64, w: f64, h: f64, primary_h: f64) -> Rect {
    let cg_y = primary_h - (y + h);
    let (x0, y0) = (x.ceil() as i32, cg_y.ceil() as i32);
    let (x1, y1) = ((x + w).floor() as i32, (cg_y + h).floor() as i32);
    Rect { x: x0, y: y0, w: (x1 - x0).max(0), h: (y1 - y0).max(0) }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OURS: i32 = 100;
    fn r(x: i32, y: i32, w: i32, h: i32) -> Rect { Rect { x, y, w, h } }
    fn win(layer: i32, alpha: f64, pid: i32, bounds: Rect) -> CgWindow { CgWindow { layer, alpha, pid, bounds, owner: "App".into() } }
    fn normal(bounds: Rect) -> CgWindow { win(0, 1.0, 200, bounds) }
    const MAIN_VISIBLE: Rect = Rect { x: 0, y: 30, w: 2560, h: 1050 };

    #[test] fn one_window_over_the_visible_frame_covers() {
        assert!(covered(MAIN_VISIBLE, &[normal(r(0, 30, 2560, 1050))], OURS));
    }
    #[test] fn a_short_window_leaves_a_strip() {
        assert!(!covered(MAIN_VISIBLE, &[normal(r(0, 30, 2560, 1000))], OURS));
    }
    #[test] fn our_own_windows_never_cover() {
        assert!(!covered(MAIN_VISIBLE, &[win(0, 1.0, OURS, r(0, 30, 2560, 1050))], OURS));
    }
    #[test] fn the_menu_bar_and_desktop_icons_are_ignored() {
        assert!(!covered(MAIN_VISIBLE, &[win(24, 1.0, 200, r(0, 30, 2560, 1050))], OURS));
        assert!(!covered(MAIN_VISIBLE, &[win(-2147483603, 1.0, 200, r(0, 30, 2560, 1050))], OURS));
        assert!(covered(MAIN_VISIBLE, &[win(8, 1.0, 200, r(0, 30, 2560, 1050))], OURS), "layer 8 counts");
    }
    #[test] fn translucent_windows_are_ignored() {
        assert!(!covered(MAIN_VISIBLE, &[win(0, 0.5, 200, r(0, 30, 2560, 1050))], OURS));
    }
    #[test] fn two_halves_cover() {
        assert!(covered(MAIN_VISIBLE, &[normal(r(0, 30, 1280, 1050)), normal(r(1280, 30, 1280, 1050))], OURS));
    }
    #[test] fn an_empty_target_or_no_windows_is_not_covered() {
        assert!(!covered(r(0, 0, 0, 10), &[normal(r(0, 0, 100, 100))], OURS));
        assert!(!covered(MAIN_VISIBLE, &[], OURS));
    }
    #[test] fn cocoa_rects_become_top_left_rects() {
        assert_eq!(cocoa_to_cg(0.0, 0.0, 2560.0, 1050.0, 1080.0), r(0, 30, 2560, 1050));
        assert_eq!(cocoa_to_cg(-1920.0, 0.0, 1920.0, 1080.0, 1080.0), r(-1920, 0, 1920, 1080));
        assert_eq!(cocoa_to_cg(0.0, 1080.0, 1920.0, 1080.0, 1080.0), r(0, -1080, 1920, 1080));
    }
    #[test] fn fractional_edges_shrink_inward() {
        assert_eq!(cocoa_to_cg(0.4, 0.0, 99.2, 50.0, 100.0), r(1, 50, 98, 50));
        assert_eq!(cocoa_to_cg(0.0, 0.0, 0.5, 0.5, 100.0), r(0, 100, 0, 0));
    }
    #[test] fn describes_only_overlapping_covers() {
        let ws = [normal(r(0, 30, 1280, 1050)), normal(r(5000, 0, 10, 10)), win(24, 1.0, 1, r(0, 0, 2560, 30))];
        assert_eq!(describe_covers(MAIN_VISIBLE, &ws, OURS), "App at 0,30 1280x1050 (area 0,30 2560x1050)");
    }
}
