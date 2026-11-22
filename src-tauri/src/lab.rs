use crate::diag;
use crate::layers::{self, AttachOptions, LayerKind, WinWidgetHost};
use serde::Deserialize;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WidgetLayout { #[default] PerWindow, OneWindow }

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabSpec { pub kind: LayerKind, pub widgets: WidgetLayout, pub win_widget_host: WinWidgetHost, pub fps: u8 }

#[derive(Debug, PartialEq)]
pub struct LabPreset { pub fps: u8, pub widgets: WidgetLayout, pub win_widget_host: WinWidgetHost }

pub fn parse_lab_args(args: &[String]) -> Option<LabPreset> {
    if !args.iter().any(|a| a == "--lab-preset=baseline") { return None; }
    let mut p = LabPreset { fps: 60, widgets: WidgetLayout::PerWindow, win_widget_host: WinWidgetHost::Bottom };
    for a in args {
        if let Some(v) = a.strip_prefix("--fps=") {
            match v { "0" => p.fps = 0, "30" => p.fps = 30, "60" => p.fps = 60,
                _ => diag::log(format!("bad --fps value: {v}")) }
        } else if let Some(v) = a.strip_prefix("--widgets=") {
            match v { "per-window" => p.widgets = WidgetLayout::PerWindow, "one-window" => p.widgets = WidgetLayout::OneWindow,
                _ => diag::log(format!("bad --widgets value: {v}")) }
        } else if let Some(v) = a.strip_prefix("--win-widget-host=") {
            match v { "bottom" => p.win_widget_host = WinWidgetHost::Bottom, "desktop-child" => p.win_widget_host = WinWidgetHost::DesktopChild,
                _ => diag::log(format!("bad --win-widget-host value: {v}")) }
        }
    }
    Some(p)
}

/// A rectangle in physical pixels.
#[derive(Clone, Copy)]
struct Rect { x: i32, y: i32, w: u32, h: u32 }

fn open_layer(app: &AppHandle, label: &str, scene: &str, fps: u8, rect: Rect, kind: LayerKind, opts: &AttachOptions) -> Result<(), String> {
    let url = format!("layer.html#scene={scene}&fps={fps}");
    let win = WebviewWindowBuilder::new(app, label, WebviewUrl::App(url.into()))
        .transparent(true).decorations(false).shadow(false).resizable(false)
        .skip_taskbar(true).visible_on_all_workspaces(true).focused(false).visible(false)
        .build().map_err(|e| format!("{label}: {e}"))?;
    diag::log(format!("open {label}: rect x={} y={} w={} h={}", rect.x, rect.y, rect.w, rect.h));
    // Size first: on macOS a later resize keeps the bottom-left corner, which would move the top-left.
    win.set_size(PhysicalSize::new(rect.w, rect.h)).map_err(|e| e.to_string())?;
    win.set_position(PhysicalPosition::new(rect.x, rect.y)).map_err(|e| e.to_string())?;
    match layers::attach(&win, kind, opts) {
        Ok(report) => diag::record_attach(report),
        Err(e) => diag::log(format!("attach {label} failed: {e}")),
    }
    win.show().map_err(|e| e.to_string())
}

fn monitor_rect(m: &tauri::Monitor) -> Rect {
    Rect { x: m.position().x, y: m.position().y, w: m.size().width, h: m.size().height }
}

fn close_prefix(app: &AppHandle, prefix: &str) {
    for (label, win) in app.webview_windows() {
        if label.starts_with(prefix) { let _ = win.destroy(); }
    }
}

fn do_open(app: &AppHandle, spec: &LabSpec) -> Result<Vec<String>, String> {
    let opts = AttachOptions { win_widget_host: spec.win_widget_host };
    let primary = app.primary_monitor().map_err(|e| e.to_string())?
        .or(app.available_monitors().map_err(|e| e.to_string())?.into_iter().next())
        .ok_or("no monitor")?;
    diag::log(format!("primary {:?} {:?} scale {}", primary.position(), primary.size(), primary.scale_factor()));
    let mut made = Vec::new();
    match spec.kind {
        LayerKind::Background => {
            close_prefix(app, "bg-");
            let mut monitors = app.available_monitors().map_err(|e| e.to_string())?;
            if monitors.is_empty() {
                // Seen when the OS display list is unavailable; still give the primary monitor a background.
                diag::log("available_monitors() returned none; using the primary monitor only");
                monitors.push(primary.clone());
            }
            for (i, m) in monitors.iter().enumerate() {
                let label = format!("bg-{i}");
                open_layer(app, &label, "rain-window", spec.fps, monitor_rect(m), spec.kind, &opts)?;
                made.push(label);
            }
        }
        LayerKind::Widget => {
            close_prefix(app, "w-");
            match spec.widgets {
                WidgetLayout::OneWindow => {
                    open_layer(app, "w-all", "widgets-all", spec.fps, monitor_rect(&primary), spec.kind, &opts)?;
                    made.push("w-all".to_string());
                }
                WidgetLayout::PerWindow => {
                    let s = primary.scale_factor();
                    let r = monitor_rect(&primary);
                    let (w, h) = ((240.0 * s).round() as u32, (280.0 * s).round() as u32);
                    let (edge, gap) = ((40.0 * s).round() as i32, (24.0 * s).round() as i32);
                    for (i, name) in ["hourglass", "weather", "calendar", "note"].iter().enumerate() {
                        let label = format!("w-{name}");
                        let rect = Rect { x: r.x + r.w as i32 - edge - w as i32, y: r.y + edge + i as i32 * (h as i32 + gap), w, h };
                        open_layer(app, &label, name, spec.fps, rect, spec.kind, &opts)?;
                        made.push(label);
                    }
                }
            }
        }
        LayerKind::Character => {
            close_prefix(app, "ch");
            let s = primary.scale_factor();
            // work_area() excludes the menu bar / taskbar.
            let wa = primary.work_area();
            let area = Rect { x: wa.position.x, y: wa.position.y, w: wa.size.width, h: wa.size.height };
            let side = (96.0 * s).round() as u32;
            let rect = Rect { x: area.x, y: area.y + area.h as i32 - side as i32, w: side, h: side };
            open_layer(app, "ch", "walker", spec.fps, rect, spec.kind, &opts)?;
            made.push("ch".to_string());
        }
    }
    Ok(made)
}

#[tauri::command]
pub fn lab_open(app: AppHandle, spec: LabSpec) -> Result<Vec<String>, String> {
    do_open(&app, &spec)
}

#[tauri::command]
pub fn lab_close(app: AppHandle, prefix: String) -> Result<(), String> {
    close_prefix(&app, &prefix);
    Ok(())
}

pub fn open_preset(app: &AppHandle, p: &LabPreset) {
    for kind in [LayerKind::Background, LayerKind::Widget, LayerKind::Character] {
        let spec = LabSpec { kind, widgets: p.widgets, win_widget_host: p.win_widget_host, fps: p.fps };
        if let Err(e) = do_open(app, &spec) { diag::log(format!("preset open failed: {e}")); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(s: &[&str]) -> Vec<String> { s.iter().map(|x| x.to_string()).collect() }

    #[test] fn no_preset_means_no_auto_open() {
        assert_eq!(parse_lab_args(&args(&["app"])), None);
    }
    #[test] fn reads_all_preset_options() {
        assert_eq!(
            parse_lab_args(&args(&["app", "--lab-preset=baseline", "--fps=30", "--widgets=one-window", "--win-widget-host=desktop-child"])),
            Some(LabPreset { fps: 30, widgets: WidgetLayout::OneWindow, win_widget_host: WinWidgetHost::DesktopChild }));
    }
    #[test] fn bad_values_fall_back_to_defaults() {
        assert_eq!(
            parse_lab_args(&args(&["app", "--lab-preset=baseline", "--fps=45", "--widgets=grid"])),
            Some(LabPreset { fps: 60, widgets: WidgetLayout::PerWindow, win_widget_host: WinWidgetHost::Bottom }));
    }
}
