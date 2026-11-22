use serde::{Deserialize, Serialize};
use tauri::WebviewWindow;

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LayerKind { Background, Widget, Character }

#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WinWidgetHost { #[default] Bottom, DesktopChild }

pub struct AttachOptions {
    #[cfg_attr(not(windows), allow(dead_code))] // only the Windows layer code reads it
    pub win_widget_host: WinWidgetHost,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachReport { pub label: String, pub method: String, pub detail: String }

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
pub(crate) mod windows;

/// Attach a layer window to the OS desktop layer.
#[cfg(target_os = "macos")]
pub fn attach(window: &WebviewWindow, kind: LayerKind, _opts: &AttachOptions) -> Result<AttachReport, String> {
    macos::attach(window, kind)
}

#[cfg(windows)]
pub fn attach(window: &WebviewWindow, kind: LayerKind, opts: &AttachOptions) -> Result<AttachReport, String> {
    self::windows::attach(window, kind, opts)
}

/// True once Explorer's desktop windows can take a layer again (after an Explorer restart). Blocks up to ~1 s.
#[cfg(windows)]
pub fn desktop_ready() -> bool {
    self::windows::desktop_ready()
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn attach(window: &WebviewWindow, _kind: LayerKind, _opts: &AttachOptions) -> Result<AttachReport, String> {
    Ok(AttachReport { label: window.label().to_string(), method: "none".into(), detail: "not attached".into() })
}
