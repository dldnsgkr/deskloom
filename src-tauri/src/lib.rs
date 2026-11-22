mod clickthrough;
mod desktop_tree;
mod geometry;
mod diag;
mod input;
mod lab;
mod layers;
mod main_thread;
mod watch;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    let mut preset = lab::parse_lab_args(&args);
    let lab_window = lab::wants_lab_window(&args);
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![lab::lab_open, lab::lab_close, lab::lab_open_special, lab::lab_set_hit_rects, diag::diag_report])
        .setup(move |app| {
            if lab_window { lab::open_lab_window(app.handle())?; } else { diag::log("--no-lab: no lab window"); }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the lab quits the app: the layer windows have no taskbar entry and nothing to close.
            if window.label() == "lab" && matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                diag::log("lab window closed; exiting");
                window.app_handle().exit(0);
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(move |app, event| {
            // available_monitors() is empty during setup on macOS, so open the preset once the loop is ready.
            if let tauri::RunEvent::Ready = event && let Some(preset) = preset.take() {
                tauri::async_runtime::spawn(lab::open_preset(app.clone(), preset));
            }
        });
}
