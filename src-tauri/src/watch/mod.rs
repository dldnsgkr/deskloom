//! Visibility watch: tells each layer window when it is hidden or shown again, through the
//! `visibility` event, so its scene stops drawing while nobody can see it.
#![cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]

use serde::Serialize;

pub const EVENT: &str = "visibility";

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Visibility { pub label: String, pub visible: bool, pub reason: &'static str }

/// The event to send for an occlusion report, or None when `visible` equals the last sent state.
/// `last` starts as None (nothing sent yet), so the first report always goes out.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn occlusion_change(label: &str, last: &mut Option<bool>, visible: bool) -> Option<Visibility> {
    if *last == Some(visible) { return None; }
    *last = Some(visible);
    Some(Visibility { label: label.to_string(), visible, reason: if visible { "visible" } else { "occluded" } })
}

/// Like `occlusion_change`, for a watcher that also knows about full-screen apps. `reason` is
/// "visible", "occluded" or "fullscreen" and `visible` follows from it; None when unchanged.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn reason_change(label: &str, last: &mut Option<&'static str>, reason: &'static str) -> Option<Visibility> {
    if *last == Some(reason) { return None; }
    *last = Some(reason);
    Some(Visibility { label: label.to_string(), visible: reason == "visible", reason })
}

mod cover;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::track;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use self::windows::{reannounce, track};

/// Starts sending `visibility` for this window.
#[cfg(not(any(target_os = "macos", windows)))]
pub fn track(_window: &tauri::WebviewWindow) -> Result<(), String> { Ok(()) }

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn sends_only_changes() {
        let mut last = None;
        assert_eq!(occlusion_change("bg-0", &mut last, true),
            Some(Visibility { label: "bg-0".into(), visible: true, reason: "visible" }));
        assert_eq!(occlusion_change("bg-0", &mut last, true), None);
        assert_eq!(occlusion_change("bg-0", &mut last, false),
            Some(Visibility { label: "bg-0".into(), visible: false, reason: "occluded" }));
        assert_eq!(occlusion_change("bg-0", &mut last, false), None);
    }
    #[test] fn reason_change_sends_only_changes_and_derives_visible() {
        let mut last = None;
        assert_eq!(reason_change("w-note", &mut last, "visible"),
            Some(Visibility { label: "w-note".into(), visible: true, reason: "visible" }));
        assert_eq!(reason_change("w-note", &mut last, "visible"), None);
        assert_eq!(reason_change("w-note", &mut last, "fullscreen"),
            Some(Visibility { label: "w-note".into(), visible: false, reason: "fullscreen" }));
        assert_eq!(reason_change("w-note", &mut last, "occluded"),
            Some(Visibility { label: "w-note".into(), visible: false, reason: "occluded" }));
        assert_eq!(reason_change("w-note", &mut last, "occluded"), None);
    }
    #[test] fn serializes_as_the_common_interface() {
        let v = Visibility { label: "ch".into(), visible: false, reason: "occluded" };
        assert_eq!(serde_json::to_string(&v).unwrap(), r#"{"label":"ch","visible":false,"reason":"occluded"}"#);
    }
}
