use std::sync::OnceLock;

use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

use crate::error::AppError;

static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

pub fn initialize(app: AppHandle) {
    let _ = APP_HANDLE.set(app);
}

pub fn set_launch_at_login(enabled: bool) -> Result<(), AppError> {
    let app = APP_HANDLE.get().ok_or(AppError::Initialization)?;
    if enabled {
        app.autolaunch().enable()
    } else {
        app.autolaunch().disable()
    }
    .map_err(|_| AppError::LocalData)
}

pub fn is_launch_at_login_enabled() -> bool {
    APP_HANDLE
        .get()
        .and_then(|app| app.autolaunch().is_enabled().ok())
        .unwrap_or(false)
}
