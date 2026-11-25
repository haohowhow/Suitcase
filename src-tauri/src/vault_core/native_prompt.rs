use serde_json::Map;
use tauri::{Manager, WebviewWindow};
use zeroize::Zeroizing;

use crate::{AppState, error::AppError};

#[cfg(windows)]
use windows_sys::Win32::{
    Foundation::{ERROR_CANCELLED, NO_ERROR},
    Security::Credentials::{
        CREDUI_FLAGS_ALWAYS_SHOW_UI, CREDUI_FLAGS_DO_NOT_PERSIST,
        CREDUI_FLAGS_EXCLUDE_CERTIFICATES, CREDUI_FLAGS_GENERIC_CREDENTIALS,
        CREDUI_FLAGS_KEEP_USERNAME, CREDUI_INFOW, CredUIPromptForCredentialsW,
    },
};

#[cfg(windows)]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Opens the Windows credential UI so the Safe Key never becomes an IPC argument
/// or React state value.
#[cfg(windows)]
pub async fn prompt_safe_key(
    window: &WebviewWindow,
    safe_box_id: &str,
    safe_box_name: &str,
    caption: &str,
    message: &str,
) -> Result<Option<Zeroizing<String>>, AppError> {
    let logger = window.state::<AppState>().logger.clone();
    let hwnd = match window.hwnd() {
        Ok(hwnd) => hwnd.0 as isize,
        Err(_) => {
            let mut fields = Map::new();
            fields.insert("reason".to_owned(), "hwnd_unavailable".into());
            logger.error(
                "vault",
                "native_prompt_failed",
                None,
                &AppError::VaultPrompt,
                fields,
            );
            return Err(AppError::VaultPrompt);
        }
    };
    let caption = wide(caption);
    let message = wide(message);
    let target = wide(&format!("KoofrGUI:Vault:{safe_box_id}"));
    let username_seed = safe_box_name.to_owned();
    let task_logger = logger.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let mut username = Zeroizing::new(vec![0_u16; 514]);
        for (index, unit) in username_seed
            .encode_utf16()
            .take(username.len().saturating_sub(1))
            .enumerate()
        {
            username[index] = unit;
        }
        let mut password = Zeroizing::new(vec![0_u16; 512]);
        let mut save = 0;
        let info = CREDUI_INFOW {
            cbSize: std::mem::size_of::<CREDUI_INFOW>() as u32,
            hwndParent: hwnd as _,
            pszMessageText: message.as_ptr(),
            pszCaptionText: caption.as_ptr(),
            hbmBanner: std::ptr::null_mut(),
        };
        let flags = CREDUI_FLAGS_ALWAYS_SHOW_UI
            | CREDUI_FLAGS_DO_NOT_PERSIST
            | CREDUI_FLAGS_EXCLUDE_CERTIFICATES
            | CREDUI_FLAGS_GENERIC_CREDENTIALS
            | CREDUI_FLAGS_KEEP_USERNAME;
        // SAFETY: every pointer references a live, writable, NUL-terminated buffer for the
        // duration of the synchronous Windows credential UI call.
        let status = unsafe {
            CredUIPromptForCredentialsW(
                &info,
                target.as_ptr(),
                std::ptr::null(),
                0,
                username.as_mut_ptr(),
                username.len() as u32,
                password.as_mut_ptr(),
                password.len() as u32,
                &mut save,
                flags,
            )
        };
        if status == ERROR_CANCELLED {
            return Ok(None);
        }
        if status != NO_ERROR {
            let mut fields = Map::new();
            fields.insert("reason".to_owned(), "credui_prompt_failed".into());
            fields.insert("win32Error".to_owned(), status.into());
            task_logger.error(
                "vault",
                "native_prompt_failed",
                None,
                &AppError::VaultPrompt,
                fields,
            );
            return Err(AppError::VaultPrompt);
        }
        let length = password
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(password.len());
        let value = match String::from_utf16(&password[..length]) {
            Ok(value) => value,
            Err(_) => {
                let mut fields = Map::new();
                fields.insert("reason".to_owned(), "utf16_decode_failed".into());
                task_logger.error(
                    "vault",
                    "native_prompt_failed",
                    None,
                    &AppError::VaultPrompt,
                    fields,
                );
                return Err(AppError::VaultPrompt);
            }
        };
        if value.is_empty() {
            return Ok(None);
        }
        Ok(Some(Zeroizing::new(value)))
    })
    .await
    .map_err(|_| {
        let mut fields = Map::new();
        fields.insert("reason".to_owned(), "prompt_task_join_failed".into());
        logger.error(
            "vault",
            "native_prompt_failed",
            None,
            &AppError::VaultPrompt,
            fields,
        );
        AppError::VaultPrompt
    })?
}

#[cfg(not(windows))]
pub async fn prompt_safe_key(
    _window: &WebviewWindow,
    _safe_box_id: &str,
    _safe_box_name: &str,
    _caption: &str,
    _message: &str,
) -> Result<Option<Zeroizing<String>>, AppError> {
    Err(AppError::VaultPrompt)
}
