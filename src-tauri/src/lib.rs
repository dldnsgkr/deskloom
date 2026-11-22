mod diag;
mod lab;
mod layers;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![lab::lab_open, lab::lab_close, diag::diag_report])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // available_monitors() is empty during setup on macOS, so open the preset once the loop is ready.
            if let tauri::RunEvent::Ready = event {
                if let Some(preset) = lab::parse_lab_args(&std::env::args().collect::<Vec<_>>()) {
                    tauri::async_runtime::spawn(lab::open_preset(app.clone(), preset));
                }
            }
        });
}
