//! Desktop pointer: streams the cursor to the layer windows as the `desktop-pointer` event,
//! so a click-through background can still react to the mouse.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))] // Task 8 uses these on Windows

use serde::Serialize;
use std::time::{Duration, Instant};

pub const EVENT: &str = "desktop-pointer";

/// Position in physical px, in the same space as Tauri's `Monitor::position()`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopPointer { pub x: f64, pub y: f64, pub down: bool, pub over_desktop: bool }

/// A move that comes within this long after the last sent move is dropped. Clicks are never dropped.
pub const MOVE_INTERVAL: Duration = Duration::from_millis(16);

#[derive(Default)]
pub struct MoveThrottle { last: Option<Instant> }

impl MoveThrottle {
    /// True if a move at `now` should be sent; remembers it as the last sent move.
    pub fn accept_move(&mut self, now: Instant) -> bool {
        if self.last.is_some_and(|t| now.saturating_duration_since(t) < MOVE_INTERVAL) { return false; }
        self.last = Some(now);
        true
    }
}

#[cfg(target_os = "macos")]
pub(crate) mod macos;
#[cfg(target_os = "macos")]
pub use macos::{start, stop};

/// Starts sending `desktop-pointer`. Task 8 fills this in for Windows.
#[cfg(not(target_os = "macos"))]
pub fn start(_app: &tauri::AppHandle) -> Result<(), String> { Ok(()) }

#[cfg(not(target_os = "macos"))]
pub fn stop() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn serializes_with_camel_case_fields() {
        let p = DesktopPointer { x: 1.5, y: 2.0, down: true, over_desktop: false };
        assert_eq!(serde_json::to_string(&p).unwrap(), r#"{"x":1.5,"y":2.0,"down":true,"overDesktop":false}"#);
    }
    #[test] fn drops_moves_within_16ms_of_the_last_sent_one() {
        let t0 = Instant::now();
        let mut th = MoveThrottle::default();
        assert!(th.accept_move(t0));
        assert!(!th.accept_move(t0 + Duration::from_millis(15)));
        assert!(th.accept_move(t0 + Duration::from_millis(16)));
        // Measured from the last *sent* move, not the last dropped one.
        assert!(!th.accept_move(t0 + Duration::from_millis(31)));
        assert!(th.accept_move(t0 + Duration::from_millis(32)));
    }
}
