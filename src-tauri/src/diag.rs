use crate::layers::AttachReport;
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::Mutex;
use tauri::AppHandle;

const MAX_LOG: usize = 200;
static LOG: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());
static ATTACH: Mutex<Vec<AttachReport>> = Mutex::new(Vec::new());

pub fn log(msg: impl Into<String>) {
    let msg = msg.into();
    eprintln!("[diag] {msg}");
    let mut buf = LOG.lock().unwrap_or_else(|e| e.into_inner());
    if buf.len() >= MAX_LOG { buf.pop_front(); }
    buf.push_back(msg);
}

pub fn record_attach(report: AttachReport) {
    let mut v = ATTACH.lock().unwrap_or_else(|e| e.into_inner());
    v.retain(|r| r.label != report.label);
    v.push(report);
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorInfo { name: String, x: i32, y: i32, width: u32, height: u32, scale: f64 }

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagReport {
    app_version: String,
    os: String,
    os_version: String,
    arch: String,
    webview_version: String,
    monitors: Vec<MonitorInfo>,
    attach: Vec<AttachReport>,
    log: Vec<String>,
}

#[tauri::command]
pub fn diag_report(app: AppHandle) -> DiagReport {
    let monitors = app.available_monitors().unwrap_or_default().iter().map(|m| MonitorInfo {
        name: m.name().cloned().unwrap_or_default(),
        x: m.position().x, y: m.position().y,
        width: m.size().width, height: m.size().height,
        scale: m.scale_factor(),
    }).collect();
    DiagReport {
        app_version: app.package_info().version.to_string(),
        os: std::env::consts::OS.into(),
        os_version: os_info::get().to_string(),
        arch: std::env::consts::ARCH.into(),
        webview_version: tauri::webview_version().unwrap_or_else(|e| e.to_string()),
        monitors,
        attach: ATTACH.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        log: LOG.lock().unwrap_or_else(|e| e.into_inner()).iter().cloned().collect(),
    }
}
