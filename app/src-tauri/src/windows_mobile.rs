//! Android: one activity, one web view. The dialogs that are separate windows on desktop
//! (Download File, details, Add URL, ...) become routes of the main view: the page listens to
//! `hashchange` (see `app/src/App.tsx`), and `win.close()` navigates back to the list.

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Manager};

/// Kept for parity with the desktop module (`commands::main_window_starts_visible`).
pub const SHOW_ON_CREATE: bool = true;
pub static MAIN_WANTED: AtomicBool = AtomicBool::new(true);

/// Show the dialog for `kind` (with an optional entity id) in the main view.
pub fn open(app: &AppHandle, kind: &str, id: Option<&str>) -> tauri::Result<()> {
    let route = match id {
        Some(id) => format!("#/{kind}/{id}"),
        None => format!("#/{kind}"),
    };
    let main = app
        .get_webview_window("main")
        .ok_or(tauri::Error::WindowNotFound)?;
    // serde_json quotes the route as a JS string literal.
    let literal = serde_json::to_string(&route).unwrap_or_else(|_| "\"#/\"".into());
    main.eval(format!("location.hash = {literal};"))?;
    Ok(())
}

pub fn open_details(app: &AppHandle, id: &str) -> tauri::Result<()> {
    open(app, "details", Some(id))
}

/// The auto-opened progress popup would take the screen over on every new download: the list
/// already shows progress, so nothing opens here.
pub fn open_progress(_app: &AppHandle, _id: &str) -> tauri::Result<()> {
    Ok(())
}

pub fn close_progress(_app: &AppHandle, _id: &str) {}

/// The mini window and the drop zone are desktop-only ideas.
pub fn toggle_mini(_app: &AppHandle) -> tauri::Result<bool> {
    Ok(false)
}

pub fn toggle_dropzone(_app: &AppHandle) -> tauri::Result<bool> {
    Ok(false)
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// There is no tray to wait in on Android; the view stays.
pub fn release_main_later(_app: &AppHandle) {}
