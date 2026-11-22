//! Runs a closure on the main (UI) thread and waits for its result.
//! AppKit objects may only be touched there, while the lab commands run on the async runtime.
#![cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]

use std::sync::mpsc;
use std::time::Duration;
use tauri::AppHandle;

const TIMEOUT: Duration = Duration::from_secs(5);

/// On the main thread `run_on_main_thread` runs the closure right away, so this never deadlocks there.
/// Elsewhere it blocks the calling thread until the main thread has run `f` (a few ms) or `TIMEOUT` passes.
pub fn run_sync<T: Send + 'static>(app: &AppHandle, f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    let (tx, rx) = mpsc::channel();
    app.run_on_main_thread(move || { let _ = tx.send(f()); }).map_err(|e| e.to_string())?;
    rx.recv_timeout(TIMEOUT).map_err(|e| format!("main thread did not answer: {e}"))
}
