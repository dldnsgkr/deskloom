//! Visibility watch: tells each layer window when it is hidden or shown again, through the
//! `visibility` event, so its scene stops drawing while nobody can see it.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))] // Task 8 uses these on Windows

use serde::Serialize;

pub const EVENT: &str = "visibility";

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Visibility { pub label: String, pub visible: bool, pub reason: &'static str }

/// The event to send for an occlusion report, or None when `visible` equals the last sent state.
/// `last` starts as None (nothing sent yet), so the first report always goes out.
pub fn occlusion_change(label: &str, last: &mut Option<bool>, visible: bool) -> Option<Visibility> {
    if *last == Some(visible) { return None; }
    *last = Some(visible);
    Some(Visibility { label: label.to_string(), visible, reason: if visible { "visible" } else { "occluded" } })
}

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::track;

/// Starts sending `visibility` for this window. Task 8 fills this in for Windows.
#[cfg(not(target_os = "macos"))]
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
    #[test] fn serializes_as_the_common_interface() {
        let v = Visibility { label: "ch".into(), visible: false, reason: "occluded" };
        assert_eq!(serde_json::to_string(&v).unwrap(), r#"{"label":"ch","visible":false,"reason":"occluded"}"#);
    }
}
