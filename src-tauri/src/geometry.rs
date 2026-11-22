#![allow(dead_code)]
//! Pure rectangle logic shared by the OS-specific layer code. Compiles on every OS.
//! Edges are computed in i64 so x + w cannot overflow i32.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    fn right(&self) -> i64 {
        self.x as i64 + self.w as i64
    }
    fn bottom(&self) -> i64 {
        self.y as i64 + self.h as i64
    }
}

/// True if the union of `covers` contains every point of `target`.
/// An empty target (w <= 0 or h <= 0) is vacuously covered.
pub fn is_fully_covered(target: Rect, covers: &[Rect]) -> bool {
    if target.w <= 0 || target.h <= 0 {
        return true;
    }
    let (tx0, ty0, tx1, ty1) = (target.x as i64, target.y as i64, target.right(), target.bottom());
    // Grid lines: target edges plus every cover edge that falls inside the target.
    let mut xs = vec![tx0, tx1];
    let mut ys = vec![ty0, ty1];
    for c in covers {
        for x in [c.x as i64, c.right()] {
            if x > tx0 && x < tx1 { xs.push(x); }
        }
        for y in [c.y as i64, c.bottom()] {
            if y > ty0 && y < ty1 { ys.push(y); }
        }
    }
    xs.sort_unstable();
    xs.dedup();
    ys.sort_unstable();
    ys.dedup();
    // Every cell must lie inside some cover. Cells are bounded by grid lines, so a cell is
    // either fully inside a cover or fully outside it; testing the cell's origin is enough.
    for wx in xs.windows(2) {
        for wy in ys.windows(2) {
            let (cx, cy) = (wx[0], wy[0]);
            let covered = covers.iter().any(|c| {
                c.w > 0 && c.h > 0 && cx >= c.x as i64 && cx < c.right() && cy >= c.y as i64 && cy < c.bottom()
            });
            if !covered {
                return false;
            }
        }
    }
    true
}

/// True if `window` contains all of `monitor`.
pub fn covers_monitor(window: Rect, monitor: Rect) -> bool {
    window.x <= monitor.x
        && window.y <= monitor.y
        && window.right() >= monitor.right()
        && window.bottom() >= monitor.bottom()
}

pub fn is_fullscreen_window(bounds: Rect, monitor: Rect, has_caption: bool) -> bool {
    covers_monitor(bounds, monitor) && !has_caption
}

/// Start inclusive, end exclusive.
pub fn hit_any(px: i32, py: i32, rects: &[Rect]) -> bool {
    let (px, py) = (px as i64, py as i64);
    rects.iter().any(|r| px >= r.x as i64 && px < r.right() && py >= r.y as i64 && py < r.bottom())
}

/// Converts a screen-space rect into the coordinate space of a parent whose origin is at `parent_origin` (screen coords).
pub fn to_parent_coords(screen: Rect, parent_origin: (i32, i32)) -> Rect {
    Rect { x: screen.x.saturating_sub(parent_origin.0), y: screen.y.saturating_sub(parent_origin.1), w: screen.w, h: screen.h }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: i32, y: i32, w: i32, h: i32) -> Rect { Rect { x, y, w, h } }

    #[test] fn covered_by_two_halves() { assert!(is_fully_covered(r(0,0,100,100), &[r(0,0,50,100), r(50,0,50,100)])); }
    #[test] fn ten_pixel_gap_is_not_covered() { assert!(!is_fully_covered(r(0,0,100,100), &[r(0,0,50,100), r(60,0,40,100)])); }
    #[test] fn nothing_covers_nothing() { assert!(!is_fully_covered(r(0,0,100,100), &[])); }
    #[test] fn larger_window_covers() { assert!(is_fully_covered(r(0,0,100,100), &[r(-10,-10,200,200)])); }
    #[test] fn three_odd_pieces_cover() {
        assert!(is_fully_covered(r(0,0,100,100), &[r(0,0,100,60), r(0,50,40,50), r(40,50,60,50)]));
    }

    #[test] fn borderless_fullscreen_counts() { assert!(is_fullscreen_window(r(0,0,1920,1080), r(0,0,1920,1080), false)); }
    #[test] fn maximized_above_taskbar_does_not_count() { assert!(!is_fullscreen_window(r(0,0,1920,1040), r(0,0,1920,1080), true)); }
    #[test] fn maximized_with_autohide_taskbar_does_not_count() { assert!(!is_fullscreen_window(r(0,0,1920,1080), r(0,0,1920,1080), true)); }
    #[test] fn fullscreen_on_second_monitor_only() {
        let w = r(1920,0,2560,1440);
        assert!(!covers_monitor(w, r(0,0,1920,1080)));
        assert!(covers_monitor(w, r(1920,0,2560,1440)));
    }

    #[test] fn hit_is_start_inclusive_end_exclusive() {
        let rs = [r(100,100,10,10)];
        assert!(hit_any(100,100,&rs)); assert!(!hit_any(110,100,&rs)); assert!(!hit_any(105,110,&rs));
    }

    #[test] fn left_monitor_with_negative_origin() {
        assert_eq!(to_parent_coords(r(-1920,0,1920,1080), (-1920,-200)), r(0,200,1920,1080));
    }
    #[test] fn primary_monitor_relative_to_virtual_origin() {
        assert_eq!(to_parent_coords(r(0,0,2560,1440), (-1920,-200)), r(1920,200,2560,1440));
    }
}
