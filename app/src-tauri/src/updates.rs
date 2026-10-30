//! In-app updates: GitHub's `latest.json` → download the signed package → checkpoint every
//! download → install → restart into the new version.

#[cfg(desktop)]
use std::sync::atomic::Ordering;

#[cfg(desktop)]
use prism_core::manager::Core;
use serde::Serialize;
use tauri::AppHandle;
#[cfg(desktop)]
use tauri::{Emitter, Manager};
#[cfg(desktop)]
use tauri_plugin_updater::UpdaterExt;

use crate::commands::CmdResult;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current: String,
    pub notes: Option<String>,
    /// `false` where the app can't replace itself (deb/rpm, portable): offer the website instead.
    pub can_install: bool,
}

/// Only installs the updater can replace in place: the per-user setup (NSIS), the macOS app
/// and the AppImage. MSI (per-machine) would get a second, per-user copy, deb/rpm belong to the
/// package manager and the portable zip must stay portable — those get the website instead.
#[cfg(desktop)]
fn can_self_update() -> bool {
    use tauri::utils::config::BundleType;
    let portable = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join("portable.txt").exists()))
        .unwrap_or(false);
    if portable {
        return false;
    }
    match tauri::utils::platform::bundle_type() {
        Some(BundleType::Nsis | BundleType::App) => true,
        Some(BundleType::AppImage) => std::env::var_os("APPIMAGE").is_some(),
        _ => false,
    }
}

#[cfg(desktop)]
#[tauri::command]
pub async fn check_update(app: AppHandle) -> CmdResult<Option<UpdateInfo>> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    let update = updater.check().await.map_err(|e| {
        tracing::info!("update check failed: {e}");
        e.to_string()
    })?;
    Ok(update.map(|u| UpdateInfo {
        version: u.version.clone(),
        current: u.current_version.clone(),
        notes: u.body.clone(),
        can_install: can_self_update(),
    }))
}

#[cfg(desktop)]
#[tauri::command]
pub async fn install_update(app: AppHandle) -> CmdResult<()> {
    if !can_self_update() {
        return Err("This installation is updated by your package manager or by hand.".into());
    }
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Prism is up to date.")?;
    tracing::info!("downloading update {}", update.version);
    let mut received = 0u64;
    let bytes = update
        .download(
            |chunk, total| {
                received += chunk as u64;
                let _ = app.emit("update-progress", (received, total));
            },
            || {},
        )
        .await
        .map_err(|e| format!("Download failed: {e}"))?;
    install_and_restart(&app, update, bytes).await
}

/// Windows: the installer replaces the running app and exits it, so everything is saved
/// first (and the app restarts itself if the installer can't be started). macOS/Linux: the
/// bundle is replaced in place first, so a failure leaves the app fully working.
#[cfg(desktop)]
async fn install_and_restart(
    app: &AppHandle,
    update: tauri_plugin_updater::Update,
    bytes: Vec<u8>,
) -> CmdResult<()> {
    #[cfg(windows)]
    {
        shutdown_for_restart(app).await;
        tracing::info!("installing update {}", update.version);
        if let Err(e) = update.install(bytes) {
            tracing::error!("installing the update failed: {e}");
        }
        app.restart();
    }
    #[cfg(not(windows))]
    {
        tracing::info!("installing update {}", update.version);
        update
            .install(bytes)
            .map_err(|e| format!("Install failed: {e}"))?;
        shutdown_for_restart(app).await;
        app.restart();
    }
}

/// Checkpoint every download and remember what was running, so it continues after the restart.
#[cfg(desktop)]
async fn shutdown_for_restart(app: &AppHandle) {
    crate::QUITTING.store(true, Ordering::SeqCst);
    if let Some(core) = app.try_state::<Core>() {
        core.shutdown_for_restart().await;
    }
}

/// Android: updates come with a new APK, there is nothing to check in the app.
#[cfg(mobile)]
#[tauri::command]
pub async fn check_update(_app: AppHandle) -> CmdResult<Option<UpdateInfo>> {
    Ok(None)
}

#[cfg(mobile)]
#[tauri::command]
pub async fn install_update(_app: AppHandle) -> CmdResult<()> {
    Err("Install the new APK to update Prism on Android.".into())
}
