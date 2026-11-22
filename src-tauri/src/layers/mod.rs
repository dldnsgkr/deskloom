use serde::{Deserialize, Serialize};
use tauri::WebviewWindow;

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LayerKind { Background, Widget, Character }

#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WinWidgetHost { #[default] Bottom, DesktopChild }

pub struct AttachOptions { pub win_widget_host: WinWidgetHost }

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachReport { pub label: String, pub method: String, pub detail: String }

/// Attach a layer window to the OS desktop layer. Tasks 7 (macOS) and 8 (Windows) fill this in.
pub fn attach(window: &WebviewWindow, _kind: LayerKind, _opts: &AttachOptions) -> Result<AttachReport, String> {
    Ok(AttachReport { label: window.label().to_string(), method: "none".into(), detail: "not attached".into() })
}
