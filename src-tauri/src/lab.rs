use crate::layers::{self, AttachOptions, LayerKind, WinWidgetHost};
use crate::{clickthrough, diag, input, watch};
use serde::Deserialize;
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder};
#[cfg(not(target_os = "macos"))]
use tauri::{PhysicalPosition, PhysicalSize};

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

/// `--no-lab` (only together with `--lab-preset=baseline`): the lab window is not created, so a
/// measurement counts only the layers and not the lab's web view. Without a preset the app would
/// have no window at all and nothing to close, so the flag is ignored there.
pub fn wants_lab_window(args: &[String]) -> bool {
    if !args.iter().any(|a| a == "--no-lab") { return true; }
    let preset = args.iter().any(|a| a == "--lab-preset=baseline");
    if !preset { diag::log("--no-lab ignored: it only applies together with --lab-preset=baseline"); }
    !preset
}

/// Builds the lab window from its entry in tauri.conf.json, which has `"create": false` so that
/// `--no-lab` runs never have it.
pub fn open_lab_window(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let config = app.config().app.windows.iter().find(|w| w.label == "lab").cloned()
        .ok_or("tauri.conf.json has no 'lab' window")?;
    WebviewWindowBuilder::from_config(app, &config)?.build()?;
    Ok(())
}

/// Widget slot size and spacing, in logical px. `src/lab/scenes/widgets-all.ts` repeats these and
/// `widget_slots`; the checklist compares the two layouts, so keep them identical.
const WIDGET_W: f64 = 240.0;
const WIDGET_H: f64 = 280.0;
const WIDGET_EDGE: f64 = 40.0;
const WIDGET_GAP: f64 = 24.0;

/// Top-left of each of `n` widget slots on a `w` x `h` monitor (logical px, relative to the monitor).
/// Columns fill top to bottom from the top-right corner and wrap to a new column on the left, so a
/// short screen never pushes a slot below its bottom edge.
fn widget_slots(w: f64, h: f64, n: usize) -> Vec<(f64, f64)> {
    let rows = (((h - 2.0 * WIDGET_EDGE + WIDGET_GAP) / (WIDGET_H + WIDGET_GAP)).floor() as usize).max(1);
    (0..n).map(|i| {
        let (col, row) = ((i / rows) as f64, (i % rows) as f64);
        (w - WIDGET_EDGE - WIDGET_W - col * (WIDGET_W + WIDGET_GAP), WIDGET_EDGE + row * (WIDGET_H + WIDGET_GAP))
    }).collect()
}

/// A rectangle in physical pixels on a monitor with scale factor `scale`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Rect { x: i32, y: i32, w: u32, h: u32, scale: f64 }

impl Rect {
    fn screen(self) -> crate::geometry::Rect {
        crate::geometry::Rect { x: self.x, y: self.y, w: self.w as i32, h: self.h as i32 }
    }
}

/// `r` in logical units (macOS points): x, y, width, height. A monitor's physical position is its
/// origin in points times that monitor's own scale, so dividing by the target monitor's scale gives
/// the points back. tao would divide by the window's current scale instead, which is wrong while the
/// new window still sits on a monitor with another scale (Retina MacBook plus a 1x display).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn logical_rect(r: Rect) -> (f64, f64, f64, f64) {
    let s = if r.scale > 0.0 { r.scale } else { 1.0 };
    (r.x as f64 / s, r.y as f64 / s, r.w as f64 / s, r.h as f64 / s)
}

/// Moves a new, hidden layer window to `rect`.
#[cfg(target_os = "macos")]
fn place(win: &tauri::WebviewWindow, rect: Rect) -> Result<(), String> {
    let (x, y, w, h) = logical_rect(rect);
    // Size first: on macOS a later resize keeps the bottom-left corner, which would move the top-left.
    win.set_size(LogicalSize::new(w, h)).map_err(|e| e.to_string())?;
    win.set_position(LogicalPosition::new(x, y)).map_err(|e| e.to_string())
}

/// Moves a new, hidden layer window to `rect`.
#[cfg(not(target_os = "macos"))]
fn place(win: &tauri::WebviewWindow, rect: Rect) -> Result<(), String> {
    // Position first: arriving on a monitor with another DPI makes tao rescale the window to keep its
    // logical size (WM_DPICHANGED); a size set after the move stays. attach() gets the rect as well.
    win.set_position(PhysicalPosition::new(rect.x, rect.y)).map_err(|e| e.to_string())?;
    win.set_size(PhysicalSize::new(rect.w, rect.h)).map_err(|e| e.to_string())
}

/// Windows: a window attached under Explorer's desktop windows is a child of them and is destroyed
/// with them when Explorer restarts. Such windows are reopened unless the lab closed them; the flag
/// per open window tells the two apart (set by `close_prefix`).
#[cfg(windows)]
static REOPEN: std::sync::Mutex<Vec<(String, std::sync::Arc<std::sync::atomic::AtomicBool>)>> = std::sync::Mutex::new(Vec::new());

#[cfg(windows)]
fn mark_closing(prefix: &str) {
    let mut v = REOPEN.lock().unwrap_or_else(|e| e.into_inner());
    v.retain(|(label, closing)| {
        if !label.starts_with(prefix) { return true; }
        closing.store(true, std::sync::atomic::Ordering::SeqCst);
        false
    });
}

#[cfg(windows)]
fn reopen_when_host_dies(win: &tauri::WebviewWindow, label: &str, scene: &str, fps: u8, rect: Rect, kind: LayerKind, host: WinWidgetHost) {
    use std::sync::{atomic::{AtomicBool, Ordering}, Arc};
    let closing = Arc::new(AtomicBool::new(false));
    {
        let mut v = REOPEN.lock().unwrap_or_else(|e| e.into_inner());
        v.retain(|(l, _)| l != label);
        v.push((label.to_string(), closing.clone()));
    }
    let (app, label, scene) = (win.app_handle().clone(), label.to_string(), scene.to_string());
    win.on_window_event(move |e| {
        if !matches!(e, tauri::WindowEvent::Destroyed) || closing.load(Ordering::SeqCst) { return; }
        diag::log(format!("{label} was destroyed without being closed (Explorer restarted?); reopening once the desktop is back"));
        let (app, label, scene, closing) = (app.clone(), label.clone(), scene.clone(), closing.clone());
        tauri::async_runtime::spawn(async move {
            // Bounded by time: each desktop_ready() call can itself block for up to ~1 s.
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
            let mut ready = false;
            while std::time::Instant::now() < deadline {
                if closing.load(Ordering::SeqCst) { return; }
                if tauri::async_runtime::spawn_blocking(layers::desktop_ready).await.unwrap_or(false) { ready = true; break; }
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
            if !ready { diag::log(format!("{label}: the desktop did not come back within a minute; not reopening")); return; }
            for _ in 0..100 {
                if app.get_webview_window(&label).is_none() { break; }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            if closing.load(Ordering::SeqCst) { return; }
            match open_layer(&app, &label, &scene, fps, rect, kind, &AttachOptions { win_widget_host: host }) {
                Ok(()) => diag::log(format!("{label} reopened")),
                Err(e) => diag::log(format!("{label} reopen failed: {e}")),
            }
        });
    });
}

fn open_layer(app: &AppHandle, label: &str, scene: &str, fps: u8, rect: Rect, kind: LayerKind, opts: &AttachOptions) -> Result<(), String> {
    let url = format!("layer.html#scene={scene}&fps={fps}");
    let win = WebviewWindowBuilder::new(app, label, WebviewUrl::App(url.into()))
        .transparent(true).decorations(false).shadow(false).resizable(false)
        .skip_taskbar(true).visible_on_all_workspaces(true).focused(false).visible(false)
        .build().map_err(|e| format!("{label}: {e}"))?;
    diag::log(format!("open {label}: rect x={} y={} w={} h={} (monitor scale {})", rect.x, rect.y, rect.w, rect.h, rect.scale));
    place(&win, rect)?;
    match layers::attach(&win, kind, rect.screen(), opts) {
        Ok(report) => {
            diag::log(format!("attach {label}: {} {}", report.method, report.detail));
            #[cfg(windows)]
            if matches!(report.method.as_str(), "win-legacy-workerw" | "win-progman" | "win-widget-desktop-child") {
                reopen_when_host_dies(&win, label, scene, fps, rect, kind, opts.win_widget_host);
            }
            diag::record_attach(report);
        }
        Err(e) => diag::log(format!("attach {label} failed: {e}")),
    }
    // Before show(), so the first occlusion report (hidden -> visible) is not missed.
    if let Err(e) = watch::track(&win) { diag::log(format!("watch {label} failed: {e}")); }
    if label == "w-all" { clickthrough::start(win.clone()); }
    win.show().map_err(|e| e.to_string())
}

fn monitor_rect(m: &tauri::Monitor) -> Rect {
    Rect { x: m.position().x, y: m.position().y, w: m.size().width, h: m.size().height, scale: m.scale_factor() }
}

/// Destroys every window whose label starts with `prefix` and waits until Tauri has
/// released the labels (removal is asynchronous), so the same labels can be reopened.
async fn close_prefix(app: &AppHandle, prefix: &str) -> Result<(), String> {
    #[cfg(windows)]
    mark_closing(prefix);
    let labels: Vec<String> = app.webview_windows().into_keys().filter(|l| l.starts_with(prefix)).collect();
    for label in &labels {
        if let Some(win) = app.get_webview_window(label) { let _ = win.hide(); let _ = win.destroy(); }
    }
    for _ in 0..100 {
        if labels.iter().all(|l| app.get_webview_window(l).is_none()) { return Ok(()); }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    Err(format!("timed out waiting for windows '{prefix}*' to close"))
}

async fn do_open(app: &AppHandle, spec: &LabSpec) -> Result<Vec<String>, String> {
    let opts = AttachOptions { win_widget_host: spec.win_widget_host };
    let primary = app.primary_monitor().map_err(|e| e.to_string())?
        .or(app.available_monitors().map_err(|e| e.to_string())?.into_iter().next())
        .ok_or("no monitor")?;
    diag::log(format!("primary {:?} {:?} scale {}", primary.position(), primary.size(), primary.scale_factor()));
    let mut made = Vec::new();
    match spec.kind {
        LayerKind::Background => {
            close_prefix(app, "bg-").await?;
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
            if let Err(e) = input::start(app) { diag::log(format!("desktop pointer start failed: {e}")); }
        }
        LayerKind::Widget => {
            close_prefix(app, "w-").await?;
            match spec.widgets {
                WidgetLayout::OneWindow => {
                    open_layer(app, "w-all", "widgets-all", spec.fps, monitor_rect(&primary), spec.kind, &opts)?;
                    made.push("w-all".to_string());
                }
                WidgetLayout::PerWindow => {
                    let s = primary.scale_factor();
                    let r = monitor_rect(&primary);
                    let (w, h) = ((WIDGET_W * s).round() as u32, (WIDGET_H * s).round() as u32);
                    let slots = widget_slots(r.w as f64 / s, r.h as f64 / s, 4);
                    for (i, name) in ["hourglass", "weather", "calendar", "note"].iter().enumerate() {
                        let label = format!("w-{name}");
                        let (lx, ly) = slots[i];
                        let rect = Rect { x: r.x + (lx * s).round() as i32, y: r.y + (ly * s).round() as i32, w, h, scale: s };
                        open_layer(app, &label, name, spec.fps, rect, spec.kind, &opts)?;
                        made.push(label);
                    }
                }
            }
        }
        LayerKind::Character => {
            close_prefix(app, "ch").await?;
            let s = primary.scale_factor();
            // work_area() excludes the menu bar / taskbar.
            let wa = primary.work_area();
            let area = Rect { x: wa.position.x, y: wa.position.y, w: wa.size.width, h: wa.size.height, scale: s };
            let side = (96.0 * s).round() as u32;
            let rect = Rect { x: area.x, y: area.y + area.h as i32 - side as i32, w: side, h: side, scale: s };
            open_layer(app, "ch", "walker", spec.fps, rect, spec.kind, &opts)?;
            made.push("ch".to_string());
        }
    }
    Ok(made)
}

#[tauri::command]
pub async fn lab_open(app: AppHandle, spec: LabSpec) -> Result<Vec<String>, String> {
    do_open(&app, &spec).await
}

/// Which panel of the quality mockup a `mockup-*` name shows, e.g. "mockup-a" -> "a".
fn mockup_panel(name: &str) -> Option<&str> {
    match name.strip_prefix("mockup-")? { p @ ("a" | "b" | "c") => Some(p), _ => None }
}

/// Opens the mockup in a plain borderless window (not attached to the desktop). If it is already
/// open, the page is reloaded with the other panel (a hash-only change would not rerun its script).
fn open_mockup(app: &AppHandle, panel: &str) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("mockup") {
        win.eval(format!("location.hash = '#{panel}'; location.reload();")).map_err(|e| e.to_string())?;
        return win.set_focus().map_err(|e| e.to_string());
    }
    let url = format!("lab/widget-quality.html#{panel}");
    let win = WebviewWindowBuilder::new(app, "mockup", WebviewUrl::App(url.into()))
        .decorations(false).resizable(false).visible(false).build().map_err(|e| format!("mockup: {e}"))?;
    // Size first, then position (macOS keeps the bottom-left corner on a resize).
    win.set_size(LogicalSize::new(1200.0, 560.0)).map_err(|e| e.to_string())?;
    win.set_position(LogicalPosition::new(80.0, 80.0)).map_err(|e| e.to_string())?;
    diag::log(format!("open mockup: panel {panel}"));
    win.show().map_err(|e| e.to_string())?;
    win.set_focus().map_err(|e| e.to_string())
}

/// The glass experiment: a still wallpaper on every monitor and one widget-layer window with two cards.
async fn open_glass(app: &AppHandle, host: WinWidgetHost) -> Result<(), String> {
    let opts = AttachOptions { win_widget_host: host };
    let primary = app.primary_monitor().map_err(|e| e.to_string())?
        .or(app.available_monitors().map_err(|e| e.to_string())?.into_iter().next())
        .ok_or("no monitor")?;
    close_prefix(app, "bg-").await?;
    let mut monitors = app.available_monitors().map_err(|e| e.to_string())?;
    if monitors.is_empty() { monitors.push(primary.clone()); }
    for (i, m) in monitors.iter().enumerate() {
        open_layer(app, &format!("bg-{i}"), "still", 0, monitor_rect(m), LayerKind::Background, &opts)?;
    }
    if !app.webview_windows().keys().any(|l| l.starts_with("bg-")) { input::stop(); }
    close_prefix(app, "w-").await?;
    let s = primary.scale_factor();
    let r = monitor_rect(&primary);
    let (w, h) = ((520.0 * s).round() as u32, (260.0 * s).round() as u32);
    let rect = Rect { x: r.x + ((r.w as i32 - w as i32) / 2), y: r.y + ((r.h as i32 - h as i32) / 2), w, h, scale: s };
    open_layer(app, "w-glass", "glass-test", 60, rect, LayerKind::Widget, &opts)
}

/// Lab experiments that are not a plain layer: "mockup-a|b|c" and "glass".
/// `win_widget_host` only matters on Windows (which host the widget-layer window attaches to).
#[tauri::command]
pub async fn lab_open_special(app: AppHandle, name: String, win_widget_host: Option<WinWidgetHost>) -> Result<(), String> {
    if let Some(panel) = mockup_panel(&name) { return open_mockup(&app, panel); }
    if name == "glass" { return open_glass(&app, win_widget_host.unwrap_or(WinWidgetHost::Bottom)).await; }
    Err(format!("unknown special: {name}"))
}

#[tauri::command]
pub async fn lab_close(app: AppHandle, prefix: String) -> Result<(), String> {
    let result = close_prefix(&app, &prefix).await;
    // The desktop pointer only feeds the background; stop it once no background is left.
    if !app.webview_windows().keys().any(|l| l.starts_with("bg-")) { input::stop(); }
    result
}

/// Widget rectangles of a click-through window, in physical screen px.
#[tauri::command]
pub fn lab_set_hit_rects(_app: AppHandle, label: String, rects: Vec<crate::geometry::Rect>) {
    diag::log(format!("hit rects {label}: {}", rects.len()));
    clickthrough::set_rects(&label, rects);
}

pub async fn open_preset(app: AppHandle, p: LabPreset) {
    for kind in [LayerKind::Background, LayerKind::Widget, LayerKind::Character] {
        let spec = LabSpec { kind, widgets: p.widgets, win_widget_host: p.win_widget_host, fps: p.fps };
        if let Err(e) = do_open(&app, &spec).await { diag::log(format!("preset open failed: {e}")); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(s: &[&str]) -> Vec<String> { s.iter().map(|x| x.to_string()).collect() }

    #[test] fn mockup_names_pick_a_panel() {
        assert_eq!(mockup_panel("mockup-a"), Some("a"));
        assert_eq!(mockup_panel("mockup-c"), Some("c"));
        assert_eq!(mockup_panel("mockup-d"), None);
        assert_eq!(mockup_panel("glass"), None);
    }
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
    #[test] fn no_lab_drops_the_lab_window_only_with_a_preset() {
        assert!(wants_lab_window(&args(&["app"])));
        assert!(wants_lab_window(&args(&["app", "--lab-preset=baseline", "--fps=0"])));
        assert!(!wants_lab_window(&args(&["app", "--lab-preset=baseline", "--fps=0", "--no-lab"])));
        assert!(!wants_lab_window(&args(&["app", "--no-lab", "--lab-preset=baseline"])));
        assert!(wants_lab_window(&args(&["app", "--no-lab"])), "no preset: the lab is the only window");
    }
    #[test] fn no_lab_leaves_the_preset_unchanged() {
        assert_eq!(
            parse_lab_args(&args(&["app", "--lab-preset=baseline", "--fps=30", "--no-lab", "--widgets=one-window"])),
            Some(LabPreset { fps: 30, widgets: WidgetLayout::OneWindow, win_widget_host: WinWidgetHost::Bottom }));
    }
    #[test] fn widget_slots_wrap_to_a_new_column_on_short_screens() {
        assert_eq!(widget_slots(2560.0, 1080.0, 4), [(2280.0, 40.0), (2280.0, 344.0), (2280.0, 648.0), (2016.0, 40.0)]);
        assert_eq!(widget_slots(1512.0, 982.0, 4), [(1232.0, 40.0), (1232.0, 344.0), (1232.0, 648.0), (968.0, 40.0)]);
        assert_eq!(widget_slots(2560.0, 1440.0, 4), [(2280.0, 40.0), (2280.0, 344.0), (2280.0, 648.0), (2280.0, 952.0)]);
        assert_eq!(widget_slots(800.0, 100.0, 2), [(520.0, 40.0), (256.0, 40.0)], "at least one row");
    }
    fn r(x: i32, y: i32, w: u32, h: u32, scale: f64) -> Rect { Rect { x, y, w, h, scale } }
    #[test] fn logical_rect_divides_by_the_target_monitors_scale() {
        // Retina MacBook (2x) at the origin, 1x display to its right at x = 1512 pt: tauri reports
        // the 1x display at physical (1512, 0) and the MacBook at 3024x1964.
        assert_eq!(logical_rect(r(1512, 0, 1920, 1080, 1.0)), (1512.0, 0.0, 1920.0, 1080.0));
        assert_eq!(logical_rect(r(0, 0, 3024, 1964, 2.0)), (0.0, 0.0, 1512.0, 982.0));
        // 2x display left of the primary at -1920 pt: physical x is -3840.
        assert_eq!(logical_rect(r(-3840, -200, 3840, 2160, 2.0)), (-1920.0, -100.0, 1920.0, 1080.0));
        // A widget on the Retina screen: 240x280 pt at 2x.
        assert_eq!(logical_rect(r(2464, 80, 480, 560, 2.0)), (1232.0, 40.0, 240.0, 280.0));
        // A bad scale never divides by zero.
        assert_eq!(logical_rect(r(10, 20, 30, 40, 0.0)), (10.0, 20.0, 30.0, 40.0));
    }
    #[test] fn screen_rect_keeps_physical_pixels() {
        assert_eq!(r(-1920, 0, 1920, 1080, 1.5).screen(), crate::geometry::Rect { x: -1920, y: 0, w: 1920, h: 1080 });
    }
}
