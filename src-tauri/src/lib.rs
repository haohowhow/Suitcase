mod autostart;
mod commands;
mod credential_manager;
mod crypto;
mod desktop;
mod download_history;
mod error;
mod file_ops;
mod folder_commands;
mod folder_download;
mod koofr_api;
mod link_commands;
mod local_access;
mod local_open;
mod logging;
mod metadata_cache;
mod settings;
mod split_commands;
mod stream_commands;
mod stream_session;
mod transfer;
mod transfer_commands;
mod vault_commands;
mod vault_core;
mod work_directory;

use credential_manager::CredentialManager;
use download_history::DownloadHistoryStore;
use koofr_api::KoofrApi;
use local_access::LocalAccessManager;
use logging::{AppLogger, LogConfig};
use metadata_cache::MetadataCache;
use settings::{SettingsDefaults, SettingsStore};
use stream_session::{cap_range, parse_range_header, parse_session_id_from_path};
use tauri::Manager;
use transfer::{TransferCheckpointStore, TransferManager};
use vault_core::VaultManager;
use work_directory::WorkDirectoryStore;

pub struct AppState {
    api: KoofrApi,
    local_access: LocalAccessManager,
    download_history: DownloadHistoryStore,
    transfers: TransferManager,
    transfer_checkpoints: TransferCheckpointStore,
    settings: SettingsStore,
    cache: MetadataCache,
    credentials: CredentialManager,
    logger: AppLogger,
    vault: VaultManager,
    work_directory: WorkDirectoryStore,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .register_asynchronous_uri_scheme_protocol(
            "koofr-stream",
            move |ctx, request, responder| {
                let app_handle = ctx.app_handle().clone();
                tauri::async_runtime::spawn(async move {
                    let response = handle_stream_request(&app_handle, &request).await;
                    responder.respond(response);
                });
            },
        )
        .setup(|app| {
            autostart::initialize(app.handle().clone());
            let default_data_dir = app.path().app_local_data_dir()?;
            let work_directory = WorkDirectoryStore::initialize(default_data_dir)?;
            let data_dir = work_directory.current_directory();
            let defaults = SettingsDefaults {
                download_directory: app.path().download_dir()?,
                cache_directory: data_dir.join("cache"),
                log_directory: data_dir.join("logs"),
            };
            std::fs::create_dir_all(&defaults.cache_directory)?;
            std::fs::create_dir_all(&defaults.log_directory)?;
            let settings = SettingsStore::load(data_dir.join("settings.json"), defaults);
            let general = tauri::async_runtime::block_on(settings.general());
            let registered_autostart = autostart::is_launch_at_login_enabled();
            if general.launch_at_login != registered_autostart {
                tauri::async_runtime::block_on(settings.update_general(
                    registered_autostart,
                    general.close_to_tray,
                    general.minimize_to_tray,
                    general.confirm_quit_with_transfers,
                ))?;
            }
            let (log_directory, log_level, retention_days, max_file_size_mb) =
                settings.initial_log_config().clone();
            std::fs::create_dir_all(&log_directory)?;
            let logger = AppLogger::initialize(LogConfig {
                directory: log_directory,
                level: log_level,
                retention_days,
                max_file_bytes: u64::from(max_file_size_mb) * 1024 * 1024,
            })?;
            let cache = MetadataCache::load(
                settings
                    .initial_cache_directory()
                    .join("metadata-cache.json"),
                settings.initial_cache_mode() == settings::CacheMode::Disk,
            );
            let vault = VaultManager::default();
            let vault_auto_lock = vault.clone();
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
                loop {
                    interval.tick().await;
                    vault_auto_lock.lock_expired().await;
                }
            });
            let advanced = tauri::async_runtime::block_on(settings.transfer_advanced());
            let bandwidth = tauri::async_runtime::block_on(settings.bandwidth());
            let history_policy = tauri::async_runtime::block_on(settings.history_policy());
            let download_behavior = tauri::async_runtime::block_on(settings.download_behavior());
            let transfers = TransferManager::default();
            transfers.set_concurrency(
                advanced.upload_concurrency as usize,
                advanced.download_concurrency as usize,
            );
            transfers.set_bandwidth(
                bandwidth.mode,
                bandwidth.global_kbps,
                bandwidth.per_task_kbps,
            );
            let download_history =
                DownloadHistoryStore::load(data_dir.join("download-history.json"));
            download_history.set_policy(history_policy.keep_days, history_policy.keep_count)?;
            let transfer_checkpoints =
                TransferCheckpointStore::load(data_dir.join("transfer-checkpoints.json"));
            tauri::async_runtime::block_on(
                transfer_checkpoints.cleanup_expired(download_behavior.keep_failed_part_days),
            )?;
            app.manage(AppState {
                api: KoofrApi::production()?,
                local_access: LocalAccessManager::default(),
                download_history,
                transfers,
                transfer_checkpoints,
                settings,
                cache,
                credentials: CredentialManager::initialize()?,
                logger,
                vault,
                work_directory,
            });
            let _ = stream_session::STREAM_SESSIONS
                .set(stream_session::StreamSessionManager::default());
            desktop::setup(app)?;
            Ok(())
        })
        .on_window_event(desktop::handle_window_event)
        .invoke_handler(tauri::generate_handler![
            commands::connect_koofr,
            commands::restore_saved_login,
            commands::disconnect_koofr,
            commands::koofr_session,
            commands::get_settings,
            commands::update_settings,
            commands::update_download_settings,
            commands::update_logging_settings,
            commands::update_transfer_settings,
            commands::update_work_directory,
            commands::clear_metadata_cache,
            commands::clear_logs,
            commands::forget_saved_login,
            commands::select_upload_file,
            commands::select_download_location,
            commands::select_download_directory,
            commands::select_work_directory,
            commands::prepare_download_location,
            folder_commands::select_download_folder,
            folder_commands::prepare_download_folder,
            commands::list_mounts,
            commands::list_files,
            commands::list_recent,
            commands::list_shared,
            commands::list_trash,
            link_commands::list_public_links,
            link_commands::create_public_link,
            link_commands::delete_public_link,
            commands::restore_trash,
            commands::empty_trash,
            commands::create_folder,
            commands::rename_entry,
            commands::move_entry,
            commands::copy_entry,
            commands::delete_entry,
            commands::upload_file,
            split_commands::upload_split_file,
            commands::download_file,
            folder_commands::download_folder,
            commands::open_downloaded_file,
            commands::open_downloaded_folder,
            commands::cancel_transfer,
            commands::pause_transfer,
            commands::update_appearance_settings,
            commands::update_general_settings,
            commands::update_transfer_advanced_settings,
            commands::update_download_behavior_settings,
            commands::update_bandwidth_settings,
            commands::update_notification_settings,
            commands::update_link_defaults,
            commands::update_security_settings,
            commands::update_history_policy,
            commands::app_version_info,
            commands::open_external_page,
            desktop::set_taskbar_badge,
            desktop::show_floating_context_menu,
            transfer_commands::list_resumable_transfers,
            transfer_commands::list_download_history,
            transfer_commands::clear_finished_download_history,
            transfer_commands::resume_transfer,
            transfer_commands::discard_resumable_transfer,
            vault_commands::list_vaults,
            vault_commands::unlock_vault,
            vault_commands::lock_vault,
            vault_commands::list_vault_files,
            vault_commands::create_vault_folder,
            vault_commands::rename_vault_entry,
            vault_commands::relocate_vault_entry,
            vault_commands::delete_vault_entries,
            vault_commands::upload_vault_file,
            vault_commands::download_vault_file,
            vault_commands::create_vault,
            vault_commands::remove_vault,
            vault_commands::export_vault_rclone_config,
            vault_commands::import_vault_rclone_config,
            stream_commands::prepare_video_stream,
            stream_commands::close_video_stream,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Koofr GUI");
}

async fn handle_stream_request(
    app_handle: &tauri::AppHandle,
    request: &tauri::http::Request<Vec<u8>>,
) -> tauri::http::Response<Vec<u8>> {
    let path = request.uri().path();
    let Some(session_id) = parse_session_id_from_path(path) else {
        return error_response(tauri::http::StatusCode::FORBIDDEN, "invalid session");
    };

    let sessions = stream_session::STREAM_SESSIONS.get().unwrap();
    let Some(session) = sessions.get_session(session_id) else {
        return error_response(tauri::http::StatusCode::FORBIDDEN, "session not found");
    };

    let range_header = request.headers().get("range").and_then(|v| v.to_str().ok());

    let (start, end) = match parse_range_header(range_header, session.file_size) {
        Some((s, e)) => cap_range(s, e, session.file_size),
        None => {
            if range_header.is_some() {
                return error_response(
                    tauri::http::StatusCode::RANGE_NOT_SATISFIABLE,
                    "range not satisfiable",
                );
            }
            cap_range(0, session.file_size.saturating_sub(1), session.file_size)
        }
    };

    let state = app_handle.state::<AppState>();
    let koofr_response = match state
        .api
        .download_response_from(&session.mount_id, &session.remote_path, start)
        .await
    {
        Ok(resp) => resp,
        Err(error::AppError::NotAuthenticated) => {
            return error_response(tauri::http::StatusCode::FORBIDDEN, "not authenticated");
        }
        Err(error::AppError::NotFound) => {
            return error_response(tauri::http::StatusCode::NOT_FOUND, "not found");
        }
        Err(_) => {
            return error_response(tauri::http::StatusCode::BAD_GATEWAY, "remote error");
        }
    };

    let is_partial = koofr_response.status() == tauri::http::StatusCode::PARTIAL_CONTENT;
    let skip = if is_partial { 0 } else { start as usize };
    let needed = (end - start + 1) as usize;

    let body = match read_stream_bytes(koofr_response, skip, needed).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return error_response(tauri::http::StatusCode::BAD_GATEWAY, "stream error");
        }
    };

    let actual_end = start + body.len() as u64 - 1;
    tauri::http::Response::builder()
        .status(tauri::http::StatusCode::PARTIAL_CONTENT)
        .header("Content-Type", &session.content_type)
        .header("Content-Length", body.len().to_string())
        .header(
            "Content-Range",
            format!("bytes {start}-{actual_end}/{}", session.file_size),
        )
        .header("Accept-Ranges", "bytes")
        .header("Cache-Control", "no-store")
        .body(body)
        .unwrap_or_else(|_| {
            error_response(
                tauri::http::StatusCode::INTERNAL_SERVER_ERROR,
                "response build error",
            )
        })
}

async fn read_stream_bytes(
    response: reqwest::Response,
    skip: usize,
    needed: usize,
) -> Result<Vec<u8>, error::AppError> {
    use futures_util::StreamExt;

    let mut stream = response.bytes_stream();
    let mut buf = Vec::with_capacity(needed);
    let mut skipped = 0usize;

    while buf.len() < needed {
        match stream.next().await {
            Some(Ok(chunk)) => {
                let mut remaining = chunk.as_ref();

                if skipped < skip {
                    let to_skip = (skip - skipped).min(remaining.len());
                    remaining = &remaining[to_skip..];
                    skipped += to_skip;
                    if remaining.is_empty() {
                        continue;
                    }
                }

                let to_read = (needed - buf.len()).min(remaining.len());
                buf.extend_from_slice(&remaining[..to_read]);
            }
            Some(Err(e)) => return Err(error::AppError::Network(e)),
            None => break,
        }
    }

    Ok(buf)
}

fn error_response(
    status: tauri::http::StatusCode,
    message: &str,
) -> tauri::http::Response<Vec<u8>> {
    tauri::http::Response::builder()
        .status(status)
        .header("Content-Type", "text/plain; charset=utf-8")
        .body(message.as_bytes().to_vec())
        .unwrap_or_else(|_| {
            tauri::http::Response::builder()
                .status(tauri::http::StatusCode::INTERNAL_SERVER_ERROR)
                .body(Vec::new())
                .unwrap()
        })
}
