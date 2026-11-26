use std::sync::atomic::{AtomicU64, Ordering};

use tauri::{
    App, AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Runtime, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

use crate::{AppState, settings::InterfaceLanguage};

pub const FLOATING_WINDOW_LABEL: &str = "transfer-float";
pub const FLOATING_WINDOW_DISABLED_EVENT: &str = "koofr://floating-window-disabled";
const FLOATING_MENU_OPEN_ID: &str = "floating-open-app";
const FLOATING_MENU_CLOSE_ID: &str = "floating-close-window";
const FLOATING_MENU_QUIT_ID: &str = "floating-quit-app";
const FLOATING_WINDOW_WIDTH: f64 = 300.0;
const FLOATING_WINDOW_HEIGHT: f64 = 72.0;
const FLOATING_WINDOW_COMPACT_SIZE: f64 = 52.0;
const FLOATING_WINDOW_MARGIN: i32 = 20;
static FLOATING_MOVE_GENERATION: AtomicU64 = AtomicU64::new(0);
static FLOATING_RESIZE_GENERATION: AtomicU64 = AtomicU64::new(0);
static FLOATING_RESIZE_ACTIVE: AtomicU64 = AtomicU64::new(0);

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
    setup_floating_window(app)?;
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

fn setup_floating_window(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    let notifications =
        tauri::async_runtime::block_on(app.state::<AppState>().settings.notifications());
    let saved_position =
        tauri::async_runtime::block_on(app.state::<AppState>().settings.floating_window_position());
    let window = WebviewWindowBuilder::new(
        app,
        FLOATING_WINDOW_LABEL,
        WebviewUrl::App("index.html?view=transfer-float".into()),
    )
    .title("Koofr Transfers")
    .inner_size(FLOATING_WINDOW_WIDTH, FLOATING_WINDOW_HEIGHT)
    .decorations(false)
    .resizable(false)
    .minimizable(false)
    .maximizable(false)
    .closable(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .transparent(true)
    .focused(false)
    .shadow(false)
    .visible(false)
    .build()?;
    window.on_menu_event(|window, event| match event.id.as_ref() {
        FLOATING_MENU_OPEN_ID => show_main_window(window.app_handle()),
        FLOATING_MENU_CLOSE_ID => disable_floating_window(window.app_handle()),
        FLOATING_MENU_QUIT_ID => request_quit(window.app_handle()),
        _ => {}
    });
    place_floating_window(&window, saved_position)?;
    if notifications.floating_window_enabled {
        window.show()?;
    }
    Ok(())
}

fn place_floating_window<R: Runtime>(
    window: &WebviewWindow<R>,
    saved_position: Option<(i32, i32)>,
) -> tauri::Result<()> {
    let monitors = window.available_monitors()?;
    if let Some(((x, y), monitor)) = saved_position.and_then(|position| {
        monitors.iter().find_map(|monitor| {
            let (x, y) = position;
            let position = monitor.position();
            let size = monitor.size();
            let right = position
                .x
                .saturating_add(i32::try_from(size.width).unwrap_or(i32::MAX));
            let bottom = position
                .y
                .saturating_add(i32::try_from(size.height).unwrap_or(i32::MAX));
            (x >= position.x && x < right && y >= position.y && y < bottom)
                .then_some(((x, y), monitor))
        })
    }) {
        let scale = monitor.scale_factor();
        let width = (FLOATING_WINDOW_WIDTH * scale).round() as i32;
        let height = (FLOATING_WINDOW_HEIGHT * scale).round() as i32;
        let position = monitor.position();
        let size = monitor.size();
        let right = position
            .x
            .saturating_add(i32::try_from(size.width).unwrap_or(i32::MAX));
        let bottom = position
            .y
            .saturating_add(i32::try_from(size.height).unwrap_or(i32::MAX));
        let x = x.clamp(
            position.x.saturating_add(FLOATING_WINDOW_MARGIN),
            right
                .saturating_sub(width)
                .saturating_sub(FLOATING_WINDOW_MARGIN),
        );
        let y = y.clamp(
            position.y.saturating_add(FLOATING_WINDOW_MARGIN),
            bottom
                .saturating_sub(height)
                .saturating_sub(FLOATING_WINDOW_MARGIN),
        );
        return window.set_position(PhysicalPosition::new(x, y));
    }
    let Some(monitor) = window.primary_monitor()? else {
        return Ok(());
    };
    let scale = monitor.scale_factor();
    let width = (FLOATING_WINDOW_WIDTH * scale).round() as i32;
    let position = monitor.position();
    let size = monitor.size();
    let x = position
        .x
        .saturating_add(i32::try_from(size.width).unwrap_or(i32::MAX))
        .saturating_sub(width)
        .saturating_sub(FLOATING_WINDOW_MARGIN);
    let y = position.y.saturating_add(FLOATING_WINDOW_MARGIN);
    window.set_position(PhysicalPosition::new(x, y))
}

pub fn set_floating_window_visible<R: Runtime>(
    app: &AppHandle<R>,
    visible: bool,
) -> tauri::Result<()> {
    let Some(window) = app.get_webview_window(FLOATING_WINDOW_LABEL) else {
        return Ok(());
    };
    if visible {
        window.show()
    } else {
        window.hide()
    }
}

#[tauri::command]
pub fn show_floating_context_menu(window: WebviewWindow) -> Result<(), crate::error::CommandError> {
    if window.label() != FLOATING_WINDOW_LABEL {
        return Err(crate::error::AppError::Forbidden.into());
    }
    let appearance =
        tauri::async_runtime::block_on(window.state::<AppState>().settings.appearance());
    let menu = floating_context_menu(window.app_handle(), resolved_english(appearance.language))
        .map_err(|_| crate::error::AppError::LocalData)?;
    window
        .popup_menu(&menu)
        .map_err(|_| crate::error::AppError::LocalData.into())
}

#[tauri::command]
pub fn open_main_window_from_floating(
    window: WebviewWindow,
) -> Result<(), crate::error::CommandError> {
    if window.label() != FLOATING_WINDOW_LABEL {
        return Err(crate::error::AppError::Forbidden.into());
    }
    show_main_window(window.app_handle());
    Ok(())
}

#[tauri::command]
pub fn set_floating_window_compact(
    window: WebviewWindow,
    compact: bool,
    reduce_motion: bool,
) -> Result<(), crate::error::CommandError> {
    if window.label() != FLOATING_WINDOW_LABEL {
        return Err(crate::error::AppError::Forbidden.into());
    }
    let current_size = window
        .inner_size()
        .map_err(|_| crate::error::AppError::LocalData)?;
    let current_position = window
        .outer_position()
        .map_err(|_| crate::error::AppError::LocalData)?;
    let monitor = window
        .current_monitor()
        .map_err(|_| crate::error::AppError::LocalData)?
        .or_else(|| window.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return Ok(());
    };
    let scale = monitor.scale_factor();
    let logical_size = if compact {
        FLOATING_WINDOW_COMPACT_SIZE
    } else {
        FLOATING_WINDOW_WIDTH
    };
    let logical_height = if compact {
        FLOATING_WINDOW_COMPACT_SIZE
    } else {
        FLOATING_WINDOW_HEIGHT
    };
    let target_size = PhysicalSize::new(
        (logical_size * scale).round() as u32,
        (logical_height * scale).round() as u32,
    );
    if current_size == target_size {
        return Ok(());
    }
    let target_position = floating_resize_target(
        current_position,
        current_size,
        target_size,
        *monitor.position(),
        *monitor.size(),
        (FLOATING_WINDOW_MARGIN as f64 * scale).round() as i32,
    );
    let generation = FLOATING_RESIZE_GENERATION
        .fetch_add(1, Ordering::Relaxed)
        .saturating_add(1);
    FLOATING_RESIZE_ACTIVE.store(generation, Ordering::Release);
    let app = window.app_handle().clone();
    tauri::async_runtime::spawn(async move {
        let duration_ms = if reduce_motion { 1.0 } else { 260.0 };
        let started = std::time::Instant::now();
        loop {
            if FLOATING_RESIZE_GENERATION.load(Ordering::Acquire) != generation {
                return;
            }
            let elapsed = started.elapsed().as_secs_f64() * 1000.0;
            let progress = (elapsed / duration_ms).clamp(0.0, 1.0);
            // Critically damped response, normalized so the last frame is exact.
            let response = 1.0 - (1.0 + 6.0 * progress) * (-6.0 * progress).exp();
            let end_response = 1.0 - 7.0 * (-6.0_f64).exp();
            let t = if reduce_motion {
                1.0
            } else {
                response / end_response
            };
            let lerp = |from: i64, to: i64| (from as f64 + (to - from) as f64 * t).round() as i64;
            let size = PhysicalSize::new(
                lerp(i64::from(current_size.width), i64::from(target_size.width)) as u32,
                lerp(
                    i64::from(current_size.height),
                    i64::from(target_size.height),
                ) as u32,
            );
            let position = PhysicalPosition::new(
                lerp(i64::from(current_position.x), i64::from(target_position.x)) as i32,
                lerp(i64::from(current_position.y), i64::from(target_position.y)) as i32,
            );
            let _ = window.set_size(size);
            let _ = window.set_position(position);
            if progress >= 1.0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(16)).await;
        }
        if FLOATING_RESIZE_GENERATION.load(Ordering::Acquire) == generation {
            FLOATING_RESIZE_ACTIVE.store(0, Ordering::Release);
            persist_floating_position(app, target_position);
        }
    });
    Ok(())
}

fn floating_resize_target(
    current_position: PhysicalPosition<i32>,
    current_size: PhysicalSize<u32>,
    target_size: PhysicalSize<u32>,
    monitor_position: PhysicalPosition<i32>,
    monitor_size: PhysicalSize<u32>,
    margin: i32,
) -> PhysicalPosition<i32> {
    let monitor_right = monitor_position
        .x
        .saturating_add(i32::try_from(monitor_size.width).unwrap_or(i32::MAX));
    let monitor_bottom = monitor_position
        .y
        .saturating_add(i32::try_from(monitor_size.height).unwrap_or(i32::MAX));
    let current_right = current_position
        .x
        .saturating_add(i32::try_from(current_size.width).unwrap_or(i32::MAX));
    let left_distance = current_position.x.saturating_sub(monitor_position.x);
    let right_distance = monitor_right.saturating_sub(current_right);
    let target_width = i32::try_from(target_size.width).unwrap_or(i32::MAX);
    let target_height = i32::try_from(target_size.height).unwrap_or(i32::MAX);
    let x = if right_distance < left_distance {
        current_right.saturating_sub(target_width)
    } else {
        current_position.x
    };
    let y = current_position.y.saturating_add(
        (i32::try_from(current_size.height)
            .unwrap_or(i32::MAX)
            .saturating_sub(target_height))
            / 2,
    );
    PhysicalPosition::new(
        x.clamp(
            monitor_position.x.saturating_add(margin),
            monitor_right
                .saturating_sub(target_width)
                .saturating_sub(margin),
        ),
        y.clamp(
            monitor_position.y.saturating_add(margin),
            monitor_bottom
                .saturating_sub(target_height)
                .saturating_sub(margin),
        ),
    )
}

pub fn update_language(app: &AppHandle, language: InterfaceLanguage) -> tauri::Result<()> {
    let Some(tray) = app.tray_by_id("main-tray") else {
        return Ok(());
    };
    tray.set_menu(Some(tray_menu(app, resolved_english(language))?))
}

fn floating_context_menu<R: Runtime>(app: &AppHandle<R>, english: bool) -> tauri::Result<Menu<R>> {
    let open = MenuItem::with_id(
        app,
        FLOATING_MENU_OPEN_ID,
        if english {
            "Open Koofr GUI"
        } else {
            "打开应用"
        },
        true,
        None::<&str>,
    )?;
    let close = MenuItem::with_id(
        app,
        FLOATING_MENU_CLOSE_ID,
        if english {
            "Close Floating Window"
        } else {
            "关闭悬浮窗"
        },
        true,
        None::<&str>,
    )?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(
        app,
        FLOATING_MENU_QUIT_ID,
        if english {
            "Quit Koofr GUI"
        } else {
            "退出应用"
        },
        true,
        None::<&str>,
    )?;
    Menu::with_items(app, &[&open, &close, &separator, &quit])
}

fn disable_floating_window<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if app
            .state::<AppState>()
            .settings
            .disable_floating_window()
            .await
            .is_err()
        {
            return;
        }
        let _ = set_floating_window_visible(&app, false);
        let _ = app.emit_to("main", FLOATING_WINDOW_DISABLED_EVENT, ());
    });
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
    if window.label() == FLOATING_WINDOW_LABEL {
        match event {
            tauri::WindowEvent::Moved(position)
                if FLOATING_RESIZE_ACTIVE.load(Ordering::Acquire) == 0 =>
            {
                persist_floating_position(app.clone(), *position);
            }
            tauri::WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = window.hide();
            }
            _ => {}
        }
        return;
    }
    if window.label() != "main" {
        return;
    }
    match event {
        tauri::WindowEvent::CloseRequested { api, .. } => {
            // The tray icon and the hidden floating window keep the process alive,
            // so letting only the main window close would leave an invisible app
            // running. Without close-to-tray the close button quits the whole app.
            api.prevent_close();
            let general =
                tauri::async_runtime::block_on(app.state::<AppState>().settings.general());
            if general.close_to_tray {
                let _ = window.hide();
                return;
            }
            request_quit(app);
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

fn persist_floating_position<R: Runtime>(app: AppHandle<R>, position: PhysicalPosition<i32>) {
    let generation = FLOATING_MOVE_GENERATION
        .fetch_add(1, Ordering::Relaxed)
        .saturating_add(1);
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(350)).await;
        if FLOATING_MOVE_GENERATION.load(Ordering::Relaxed) != generation {
            return;
        }
        let _ = app
            .state::<AppState>()
            .settings
            .update_floating_window_position(position.x, position.y)
            .await;
    });
}

pub fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
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
                "Transfers are still queued or running. Quit anyway? Resumable transfers are paused and can be continued from the Transfers panel after you sign in again; folder downloads stop."
            } else {
                "仍有排队或运行中的传输。仍要退出吗？可续传的任务会暂停，再次登录后可在传输面板继续；文件夹下载会停止。"
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
    state.transfers.pause_all();
    state.sync.stop_all();
    // Give the paused transfers a moment to record where they stopped before the
    // process ends; never block the UI thread for it.
    for label in ["main", FLOATING_WINDOW_LABEL] {
        if let Some(window) = app.get_webview_window(label) {
            let _ = window.hide();
        }
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        if state
            .transfers
            .wait_until_idle(crate::commands::TRANSFER_STOP_GRACE)
            .await
        {
            // The terminal events and history updates follow the ID release.
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
        app.exit(0);
    });
}

pub(crate) fn resolved_english(language: InterfaceLanguage) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_resize_keeps_the_nearest_right_edge_and_vertical_center() {
        let target = floating_resize_target(
            PhysicalPosition::new(1648, 100),
            PhysicalSize::new(252, 64),
            PhysicalSize::new(52, 52),
            PhysicalPosition::new(0, 0),
            PhysicalSize::new(1920, 1080),
            20,
        );

        assert_eq!(target, PhysicalPosition::new(1848, 106));
    }

    #[test]
    fn compact_resize_clamps_to_the_visible_monitor_margin() {
        let target = floating_resize_target(
            PhysicalPosition::new(0, 0),
            PhysicalSize::new(252, 64),
            PhysicalSize::new(52, 52),
            PhysicalPosition::new(0, 0),
            PhysicalSize::new(1920, 1080),
            20,
        );

        assert_eq!(target, PhysicalPosition::new(20, 20));
    }
}
