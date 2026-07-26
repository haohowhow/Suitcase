use tauri::{
    App, AppHandle, Manager, Runtime,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

use crate::{AppState, settings::InterfaceLanguage};

#[tauri::command]
pub fn set_taskbar_badge(
    app: AppHandle,
    count: u32,
    enabled: bool,
) -> Result<(), crate::error::CommandError> {
    let window = app
        .get_webview_window("main")
        .ok_or(crate::error::AppError::Initialization)?;
    if !enabled || count == 0 {
        window
            .set_overlay_icon(None)
            .map_err(|_| crate::error::AppError::LocalData)?;
        return Ok(());
    }
    let label = if count > 9 {
        "9+"
    } else {
        match count {
            1 => "1",
            2 => "2",
            3 => "3",
            4 => "4",
            5 => "5",
            6 => "6",
            7 => "7",
            8 => "8",
            _ => "9",
        }
    };
    let image = badge_image(label);
    window
        .set_overlay_icon(Some(image))
        .map_err(|_| crate::error::AppError::LocalData)?;
    Ok(())
}

pub fn setup(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    let appearance = tauri::async_runtime::block_on(app.state::<AppState>().settings.appearance());
    let english = resolved_english(appearance.language);
    let menu = tray_menu(app.handle(), english)?;
    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or("application icon is unavailable")?;

    TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .tooltip("Koofr GUI")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "quit" => request_quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

pub fn update_language(app: &AppHandle, language: InterfaceLanguage) -> tauri::Result<()> {
    let Some(tray) = app.tray_by_id("main-tray") else {
        return Ok(());
    };
    tray.set_menu(Some(tray_menu(app, resolved_english(language))?))
}

fn tray_menu<R: Runtime>(app: &AppHandle<R>, english: bool) -> tauri::Result<Menu<R>> {
    let show = MenuItem::with_id(
        app,
        "show",
        if english {
            "Show Koofr GUI"
        } else {
            "显示 Koofr GUI"
        },
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(
        app,
        "quit",
        if english { "Quit" } else { "退出" },
        true,
        None::<&str>,
    )?;
    Menu::with_items(app, &[&show, &quit])
}

pub fn handle_window_event<R: Runtime>(window: &tauri::Window<R>, event: &tauri::WindowEvent) {
    let app = window.app_handle();
    match event {
        tauri::WindowEvent::CloseRequested { api, .. } => {
            let general =
                tauri::async_runtime::block_on(app.state::<AppState>().settings.general());
            if general.close_to_tray {
                api.prevent_close();
                let _ = window.hide();
                return;
            }
            if general.confirm_quit_with_transfers
                && app.state::<AppState>().transfers.active_count() > 0
            {
                api.prevent_close();
                request_quit(app);
            }
        }
        tauri::WindowEvent::Resized(_) if window.is_minimized().unwrap_or(false) => {
            let general =
                tauri::async_runtime::block_on(app.state::<AppState>().settings.general());
            if general.minimize_to_tray {
                let _ = window.hide();
            }
        }
        _ => {}
    }
}

fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn request_quit<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<AppState>();
    let general = tauri::async_runtime::block_on(state.settings.general());
    if general.confirm_quit_with_transfers && state.transfers.active_count() > 0 {
        let appearance = tauri::async_runtime::block_on(state.settings.appearance());
        let english = resolved_english(appearance.language);
        let confirmed = app
            .dialog()
            .message(if english {
                "Transfers are still queued or running. Quit and cancel them?"
            } else {
                "仍有排队或运行中的传输。要退出并取消这些任务吗？"
            })
            .title(if english {
                "Quit Koofr GUI"
            } else {
                "退出 Koofr GUI"
            })
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::OkCancel)
            .blocking_show();
        if !confirmed {
            return;
        }
    }
    state.transfers.cancel_all();
    app.exit(0);
}

fn resolved_english(language: InterfaceLanguage) -> bool {
    match language {
        InterfaceLanguage::EnUs => true,
        InterfaceLanguage::ZhCn => false,
        InterfaceLanguage::System => system_ui_is_english(),
    }
}

#[cfg(windows)]
fn system_ui_is_english() -> bool {
    unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() & 0x03ff == 0x0009 }
}

fn badge_image(label: &str) -> tauri::image::Image<'static> {
    const SIZE: usize = 32;
    let mut rgba = vec![0_u8; SIZE * SIZE * 4];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as i32 - 16;
            let dy = y as i32 - 16;
            if dx * dx + dy * dy <= 15 * 15 {
                let pixel = (y * SIZE + x) * 4;
                rgba[pixel] = 211;
                rgba[pixel + 1] = 47;
                rgba[pixel + 2] = 47;
                rgba[pixel + 3] = 255;
            }
        }
    }
    let glyphs = label.chars().collect::<Vec<_>>();
    let width = glyphs.len() * 6 - 1;
    let start_x = (SIZE - width) / 2;
    for (index, character) in glyphs.into_iter().enumerate() {
        draw_glyph(&mut rgba, start_x + index * 6, 12, character);
    }
    tauri::image::Image::new_owned(rgba, SIZE as u32, SIZE as u32)
}

fn draw_glyph(rgba: &mut [u8], start_x: usize, start_y: usize, character: char) {
    const SIZE: usize = 32;
    let rows: [u8; 7] = match character {
        '1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        '2' => [
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
        ],
        '3' => [
            0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        '4' => [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
        '5' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b00001, 0b00001, 0b11110,
        ],
        '6' => [
            0b01110, 0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
        '7' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
        '8' => [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
        '9' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b01110,
        ],
        '+' => [0, 0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0],
        _ => [0; 7],
    };
    for (y, row) in rows.into_iter().enumerate() {
        for x in 0..5 {
            if row & (1 << (4 - x)) != 0 {
                let pixel = ((start_y + y) * SIZE + start_x + x) * 4;
                rgba[pixel..pixel + 4].copy_from_slice(&[255, 255, 255, 255]);
            }
        }
    }
}

#[cfg(not(windows))]
fn system_ui_is_english() -> bool {
    true
}
