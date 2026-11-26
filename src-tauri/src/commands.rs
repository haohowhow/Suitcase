use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use zeroize::Zeroizing;

use crate::{
    AppState, autostart,
    download_history::{DownloadLocalKind, NewDownloadHistoryItem},
    error::{AppError, CommandError},
    file_ops::{
        LocalDownloadPath, LocalUploadPath, MountId, RemoteName, RemotePath, local_path_key,
        safe_suggested_file_name,
    },
    koofr_api::{FileInfo, LocatedFile, Mount, SessionInfo, TrashList},
    local_access::LocalFileSelection,
    local_open,
    logging::LogConfig,
    settings::{
        BandwidthMode, CacheMode, DownloadConflictStrategy, InterfaceLanguage, LinkDefaultKind,
        LogLevel, UiDensity,
    },
    sync::{self, SyncIssue, SyncPairSummary, SyncTaskSummary},
    transfer::{self, TransferResult},
};

type CommandResult<T> = Result<T, CommandError>;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashRestoreTarget {
    mount_id: String,
    path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginBootstrap {
    session: SessionInfo,
    saved_email: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSnapshot {
    cache_mode: CacheMode,
    cache_ttl_minutes: u32,
    cached_items: usize,
    cache_disk_bytes: u64,
    saved_email: Option<String>,
    download_directory: String,
    ask_download_location: bool,
    work_directory: String,
    pending_work_directory: Option<String>,
    pending_work_directory_move: bool,
    work_directory_migration_failed: bool,
    log_level: LogLevel,
    log_retention_days: u32,
    log_max_file_size_mb: u32,
    log_files: usize,
    log_disk_bytes: u64,
    auto_retry_network_errors: bool,
    network_retry_limit: Option<u32>,
    network_retry_interval_seconds: u32,
    // Appearance
    language: InterfaceLanguage,
    reduce_motion: bool,
    density: UiDensity,
    // General
    launch_at_login: bool,
    close_to_tray: bool,
    native_sync_background_required: bool,
    minimize_to_tray: bool,
    confirm_quit_with_transfers: bool,
    // Transfer (advanced)
    upload_concurrency: u32,
    download_concurrency: u32,
    default_split_part_mib: u32,
    auto_split_threshold_mib: u32,
    // Download behavior
    download_conflict_strategy: DownloadConflictStrategy,
    open_folder_after_download: bool,
    verify_hash_after_download: bool,
    keep_failed_part_days: u32,
    // Bandwidth
    global_bandwidth_limit_kbps: u32,
    per_task_bandwidth_limit_kbps: u32,
    bandwidth_mode: BandwidthMode,
    // Notifications
    notify_on_complete: bool,
    notify_sound: bool,
    tray_badge: bool,
    floating_window_enabled: bool,
    floating_window_prompted: bool,
    floating_window_compact_enabled: bool,
    floating_window_click_toggle_enabled: bool,
    floating_window_double_click_open_enabled: bool,
    // Link defaults
    link_default_kind: LinkDefaultKind,
    link_default_expire_days: u32,
    link_default_require_password: bool,
    // Security
    idle_auto_logout_minutes: u32,
    // History policy
    download_history_keep_days: u32,
    download_history_keep_count: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppVersionInfo {
    version: &'static str,
    homepage_url: &'static str,
    releases_url: &'static str,
    issues_url: &'static str,
    license: &'static str,
}

/// How long signing out or quitting waits for paused transfers to save their state.
pub const TRANSFER_STOP_GRACE: std::time::Duration = std::time::Duration::from_secs(3);

#[tauri::command]
pub async fn connect_koofr(
    app: AppHandle,
    state: State<'_, AppState>,
    email: String,
    app_password: String,
    remember_password: bool,
) -> CommandResult<SessionInfo> {
    // Switching accounts pauses rather than cancels: the previous account's
    // resumable transfers keep their checkpoints and show up again on its next login.
    state.transfers.pause_all();
    state.sync.stop_all();
    state.transfers.wait_until_idle(TRANSFER_STOP_GRACE).await;
    state.local_access.clear();
    state.vault.reset().await;
    if let Some(sessions) = crate::stream_session::STREAM_SESSIONS.get() {
        sessions.close_all();
    }
    let app_password = Zeroizing::new(app_password);
    let session = state
        .api
        .authenticate(&email, app_password.as_str())
        .await
        .map_err(CommandError::from)?;
    let owner_id = state
        .api
        .recovery_scope()
        .await
        .map_err(CommandError::from)?;
    state
        .sync
        .start_for_owner(app, owner_id)
        .map_err(CommandError::from)?;
    let previous_email = state.settings.remembered_email().await;
    if remember_password {
        if let Some(previous) = previous_email.filter(|previous| previous != &email) {
            state
                .credentials
                .delete(previous)
                .await
                .map_err(CommandError::from)?;
        }
        state
            .credentials
            .save(email.clone(), app_password)
            .await
            .map_err(CommandError::from)?;
        state
            .settings
            .set_remembered_email(Some(email.clone()))
            .await
            .map_err(CommandError::from)?;
    } else if let Some(previous) = previous_email {
        state
            .credentials
            .delete(previous)
            .await
            .map_err(CommandError::from)?;
        state
            .settings
            .set_remembered_email(None)
            .await
            .map_err(CommandError::from)?;
    }
    let (mode, _) = state.settings.cache_policy().await;
    let _ = state
        .cache
        .select_account(
            session
                .user_id
                .clone()
                .unwrap_or_else(|| email.to_ascii_lowercase()),
            mode,
        )
        .await;
    Ok(session)
}

#[tauri::command]
pub async fn restore_saved_login(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<LoginBootstrap> {
    if state.api.session_info().await.authenticated {
        return Ok(LoginBootstrap {
            session: state.api.session_info().await,
            saved_email: state.settings.remembered_email().await,
        });
    }
    let Some(email) = state.settings.remembered_email().await else {
        return Ok(LoginBootstrap {
            session: state.api.session_info().await,
            saved_email: None,
        });
    };
    let Some(password) = state
        .logger
        .trace(
            "auth",
            "load_saved_credential",
            None,
            state.credentials.load(email.clone()),
        )
        .await
        .map_err(CommandError::from)?
    else {
        state
            .settings
            .set_remembered_email(None)
            .await
            .map_err(CommandError::from)?;
        return Ok(LoginBootstrap {
            session: state.api.session_info().await,
            saved_email: None,
        });
    };
    let session = state
        .api
        .authenticate(&email, password.as_str())
        .await
        .map_err(CommandError::from)?;
    let owner_id = state
        .api
        .recovery_scope()
        .await
        .map_err(CommandError::from)?;
    state
        .sync
        .start_for_owner(app, owner_id)
        .map_err(CommandError::from)?;
    state.vault.reset().await;
    if let Some(sessions) = crate::stream_session::STREAM_SESSIONS.get() {
        sessions.close_all();
    }
    let (mode, _) = state.settings.cache_policy().await;
    let _ = state
        .cache
        .select_account(
            session
                .user_id
                .clone()
                .unwrap_or_else(|| email.to_ascii_lowercase()),
            mode,
        )
        .await;
    Ok(LoginBootstrap {
        session,
        saved_email: Some(email),
    })
}

#[tauri::command]
pub async fn disconnect_koofr(state: State<'_, AppState>) -> CommandResult<()> {
    // Signing out (also the idle timeout) pauses transfers so they can be resumed
    // after the next sign-in, and lets them save their state before the session ends.
    state.transfers.pause_all();
    state.sync.stop_all();
    state.transfers.wait_until_idle(TRANSFER_STOP_GRACE).await;
    state.vault.reset().await;
    state.api.disconnect().await;
    state.local_access.clear();
    if let Some(sessions) = crate::stream_session::STREAM_SESSIONS.get() {
        sessions.close_all();
    }
    Ok(())
}

#[tauri::command]
pub async fn select_upload_file(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<Option<LocalFileSelection>> {
    let Some(file_path) = app.dialog().file().blocking_pick_file() else {
        return Ok(None);
    };
    let path = file_path
        .into_path()
        .map_err(|_| CommandError::from(AppError::Dialog))?;
    LocalUploadPath::from_selected(path.clone())
        .await
        .map_err(CommandError::from)?;
    state
        .local_access
        .grant_upload(path)
        .map(Some)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn select_download_location(
    app: AppHandle,
    state: State<'_, AppState>,
    suggested_name: String,
) -> CommandResult<Option<LocalFileSelection>> {
    let suggested_name = RemoteName::parse(suggested_name).map_err(CommandError::from)?;
    let Some(file_path) = app
        .dialog()
        .file()
        .set_file_name(safe_suggested_file_name(&suggested_name))
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let path = file_path
        .into_path()
        .map_err(|_| CommandError::from(AppError::Dialog))?;
    LocalDownloadPath::from_selected(path.clone())
        .await
        .map_err(CommandError::from)?;
    state
        .local_access
        .grant_download(path)
        .map(Some)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn select_download_directory(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<Option<String>> {
    let (initial_directory, _) = state.settings.download_policy().await;
    let Some(directory) = app
        .dialog()
        .file()
        .set_directory(initial_directory)
        .blocking_pick_folder()
    else {
        return Ok(None);
    };
    let path = directory
        .into_path()
        .map_err(|_| CommandError::from(AppError::Dialog))?;
    let metadata = tokio::fs::symlink_metadata(&path)
        .await
        .map_err(|error| CommandError::from(AppError::from(error)))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(CommandError::from(AppError::InvalidInput(
            "download directory",
        )));
    }
    path.to_str()
        .map(str::to_owned)
        .map(Some)
        .ok_or_else(|| CommandError::from(AppError::InvalidInput("download directory")))
}

#[tauri::command]
pub async fn select_work_directory(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<Option<String>> {
    let initial_directory = state.work_directory.current_directory();
    let Some(directory) = app
        .dialog()
        .file()
        .set_directory(initial_directory)
        .blocking_pick_folder()
    else {
        return Ok(None);
    };
    let path = directory
        .into_path()
        .map_err(|_| CommandError::from(AppError::Dialog))?;
    let metadata = tokio::fs::symlink_metadata(&path)
        .await
        .map_err(|error| CommandError::from(AppError::from(error)))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(CommandError::from(AppError::InvalidInput(
            "settings directory",
        )));
    }
    path.to_str()
        .map(str::to_owned)
        .map(Some)
        .ok_or_else(|| CommandError::from(AppError::InvalidInput("settings directory")))
}

#[tauri::command]
pub async fn prepare_download_location(
    state: State<'_, AppState>,
    suggested_name: String,
    download_directory: String,
    conflict_strategy: Option<DownloadConflictStrategy>,
) -> CommandResult<LocalFileSelection> {
    let name = RemoteName::parse(suggested_name).map_err(CommandError::from)?;
    let parent = PathBuf::from(download_directory);
    let strategy =
        conflict_strategy.unwrap_or(state.settings.download_behavior().await.conflict_strategy);
    // Names that running or paused downloads, or grants not used yet, will publish
    // count as taken: two downloads of the same file must not share a target.
    let mut reserved = state.transfer_checkpoints.reserved_download_targets().await;
    reserved.extend(state.local_access.pending_download_targets());
    let preferred = parent.join(safe_suggested_file_name(&name));
    let exists = reserved.contains(&local_path_key(&preferred))
        || tokio::fs::try_exists(&preferred).await.unwrap_or(false);
    if exists && strategy != DownloadConflictStrategy::Rename {
        return Err(CommandError::from(AppError::Conflict));
    }
    let target = LocalDownloadPath::from_parent_avoiding(parent, &name, &reserved)
        .await
        .map_err(CommandError::from)?;
    state
        .local_access
        .grant_download(target.as_path().to_path_buf())
        .map_err(Into::into)
}

#[tauri::command]
pub async fn koofr_session(state: State<'_, AppState>) -> CommandResult<SessionInfo> {
    Ok(state.api.session_info().await)
}

#[tauri::command]
pub async fn select_sync_folder(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<Option<LocalFileSelection>> {
    let Some(folder) = app.dialog().file().blocking_pick_folder() else {
        return Ok(None);
    };
    let folder = folder
        .into_path()
        .map_err(|_| CommandError::from(AppError::Dialog))?;
    sync::validate_local_root_sync(&folder).map_err(CommandError::from)?;
    state
        .local_access
        .grant_sync_directory(folder)
        .map(Some)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn list_sync_pairs(state: State<'_, AppState>) -> CommandResult<Vec<SyncPairSummary>> {
    let owner_id = state
        .api
        .recovery_scope()
        .await
        .map_err(CommandError::from)?;
    state.sync.list_pairs(&owner_id).map_err(CommandError::from)
}

#[tauri::command]
pub async fn create_sync_pair(
    app: AppHandle,
    state: State<'_, AppState>,
    local_path_grant: String,
    mount_id: String,
    remote_path: String,
) -> CommandResult<SyncPairSummary> {
    let owner_id = state
        .api
        .recovery_scope()
        .await
        .map_err(CommandError::from)?;
    let local_root = state
        .local_access
        .take_sync_directory(&local_path_grant)
        .map_err(CommandError::from)?;
    let mount_id = MountId::parse(mount_id).map_err(CommandError::from)?;
    let remote_path = RemotePath::parse(remote_path).map_err(CommandError::from)?;
    state
        .sync
        .create_pair(app, owner_id, local_root, mount_id, remote_path)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub fn get_native_sync_capability(
    state: State<'_, AppState>,
) -> CommandResult<crate::cloud_files::NativeSyncCapability> {
    let (capability, hresult) = state.sync.native_capability();
    if !capability.supported {
        let mut fields = serde_json::Map::new();
        fields.insert(
            "detail".to_owned(),
            serde_json::Value::from("Windows Cloud Files API (CfGetPlatformInfo) unavailable"),
        );
        if let Some(code) = hresult {
            fields.insert(
                "hresult".to_owned(),
                serde_json::Value::String(format!("0x{:08X}", code as u32)),
            );
        }
        state
            .logger
            .warn("sync", "cloud_files_api_unavailable", None, fields);
    }
    Ok(capability)
}

#[tauri::command]
pub fn suggested_on_demand_sync_root() -> CommandResult<String> {
    let profile = std::env::var_os("USERPROFILE")
        .ok_or_else(|| CommandError::from(AppError::Initialization))?;
    let path = PathBuf::from(profile).join("Koofr");
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| CommandError::from(AppError::InvalidInput("sync directory")))
}

async fn enable_native_sync_background(
    state: &AppState,
) -> Result<(bool, crate::settings::GeneralSettings), AppError> {
    let previous_autostart = autostart::is_launch_at_login_enabled();
    let general = state.settings.general().await;
    if !previous_autostart {
        autostart::set_launch_at_login(true)?;
    }
    if let Err(error) = state
        .settings
        .update_general(
            true,
            true,
            general.minimize_to_tray,
            general.confirm_quit_with_transfers,
        )
        .await
    {
        if !previous_autostart {
            let _ = autostart::set_launch_at_login(false);
        }
        let _ = state
            .settings
            .update_general(
                general.launch_at_login,
                general.close_to_tray,
                general.minimize_to_tray,
                general.confirm_quit_with_transfers,
            )
            .await;
        return Err(error);
    }
    Ok((previous_autostart, general))
}

async fn restore_native_sync_background(
    state: &AppState,
    previous: (bool, crate::settings::GeneralSettings),
) {
    let (autostart_enabled, general) = previous;
    let _ = autostart::set_launch_at_login(autostart_enabled);
    let _ = state
        .settings
        .update_general(
            general.launch_at_login,
            general.close_to_tray,
            general.minimize_to_tray,
            general.confirm_quit_with_transfers,
        )
        .await;
}

#[tauri::command]
pub async fn create_on_demand_sync_pair(
    app: AppHandle,
    state: State<'_, AppState>,
    local_path_grant: String,
    mount_id: String,
    remote_path: String,
    root_kind: String,
) -> CommandResult<SyncPairSummary> {
    let owner_id = state
        .api
        .recovery_scope()
        .await
        .map_err(CommandError::from)?;
    let local_root = state
        .local_access
        .take_sync_directory(&local_path_grant)
        .map_err(CommandError::from)?;
    let mount_id = MountId::parse(mount_id).map_err(CommandError::from)?;
    let remote_path = RemotePath::parse(remote_path).map_err(CommandError::from)?;
    let created = state
        .sync
        .create_on_demand_pair(
            app,
            owner_id.clone(),
            local_root,
            mount_id,
            remote_path,
            root_kind,
        )
        .await
        .map_err(CommandError::from)?;
    if let Err(error) = enable_native_sync_background(&state).await {
        let _ = state.sync.remove_pair(&owner_id, &created.id);
        return Err(CommandError::from(error));
    }
    Ok(created)
}

#[tauri::command]
pub async fn migrate_sync_pair_to_on_demand(
    app: AppHandle,
    state: State<'_, AppState>,
    pair_id: String,
    local_path_grant: String,
    root_kind: String,
) -> CommandResult<SyncPairSummary> {
    let owner_id = state
        .api
        .recovery_scope()
        .await
        .map_err(CommandError::from)?;
    let local_root = state
        .local_access
        .take_sync_directory(&local_path_grant)
        .map_err(CommandError::from)?;
    let background = enable_native_sync_background(&state)
        .await
        .map_err(CommandError::from)?;
    let migrated = state
        .sync
        .migrate_pair(app, owner_id.clone(), &pair_id, local_root, root_kind)
        .await;
    let migrated = match migrated {
        Ok(migrated) => migrated,
        Err(error) => {
            restore_native_sync_background(&state, background).await;
            return Err(CommandError::from(error));
        }
    };
    Ok(migrated)
}

#[tauri::command]
pub async fn open_sync_root(state: State<'_, AppState>, pair_id: String) -> CommandResult<()> {
    let owner_id = state
        .api
        .recovery_scope()
        .await
        .map_err(CommandError::from)?;
    state
        .sync
        .open_root(&owner_id, &pair_id)
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn set_sync_pair_paused(
    state: State<'_, AppState>,
    pair_id: String,
    paused: bool,
) -> CommandResult<bool> {
    let owner_id = state
        .api
        .recovery_scope()
        .await
        .map_err(CommandError::from)?;
    state
        .sync
        .set_paused(&owner_id, &pair_id, paused)
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn run_sync_pair_now(state: State<'_, AppState>, pair_id: String) -> CommandResult<bool> {
    let owner_id = state
        .api
        .recovery_scope()
        .await
        .map_err(CommandError::from)?;
    state
        .sync
        .run_now(&owner_id, &pair_id)
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn list_sync_issues(
    state: State<'_, AppState>,
    pair_id: String,
) -> CommandResult<Vec<SyncIssue>> {
    let owner_id = state
        .api
        .recovery_scope()
        .await
        .map_err(CommandError::from)?;
    state
        .sync
        .list_issues(&owner_id, &pair_id)
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn list_sync_tasks(
    state: State<'_, AppState>,
    pair_id: String,
) -> CommandResult<Vec<SyncTaskSummary>> {
    let owner_id = state
        .api
        .recovery_scope()
        .await
        .map_err(CommandError::from)?;
    state
        .sync
        .list_tasks(&owner_id, &pair_id)
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn remove_sync_pair(state: State<'_, AppState>, pair_id: String) -> CommandResult<bool> {
    let owner_id = state
        .api
        .recovery_scope()
        .await
        .map_err(CommandError::from)?;
    state
        .sync
        .remove_pair(&owner_id, &pair_id)
        .map_err(CommandError::from)
}

async fn settings_snapshot(state: &State<'_, AppState>) -> SettingsSnapshot {
    let (cache_mode, cache_ttl_minutes) = state.settings.cache_policy().await;
    let (download_directory, ask_download_location) = state.settings.download_policy().await;
    let (_, log_level, log_retention_days, log_max_file_size_mb) =
        state.settings.log_policy().await;
    let (cached_items, cache_disk_bytes) = state.cache.stats().await;
    let (log_files, log_disk_bytes) = state.logger.stats().await;
    let network_retry = state.settings.network_retry_settings().await;
    let work_directory = state.work_directory.status();
    let appearance = state.settings.appearance().await;
    let general = state.settings.general().await;
    let advanced = state.settings.transfer_advanced().await;
    let behavior = state.settings.download_behavior().await;
    let bandwidth = state.settings.bandwidth().await;
    let notifications = state.settings.notifications().await;
    let links = state.settings.link_defaults().await;
    let security = state.settings.security().await;
    let history = state.settings.history_policy().await;
    SettingsSnapshot {
        cache_mode,
        cache_ttl_minutes,
        cached_items,
        cache_disk_bytes,
        saved_email: state.settings.remembered_email().await,
        download_directory: download_directory.to_string_lossy().into_owned(),
        ask_download_location,
        work_directory: work_directory
            .current_directory
            .to_string_lossy()
            .into_owned(),
        pending_work_directory: work_directory
            .pending_directory
            .map(|directory| directory.to_string_lossy().into_owned()),
        pending_work_directory_move: work_directory.move_existing,
        work_directory_migration_failed: work_directory.migration_failed,
        log_level,
        log_retention_days,
        log_max_file_size_mb,
        log_files,
        log_disk_bytes,
        auto_retry_network_errors: network_retry.enabled,
        network_retry_limit: network_retry.max_retries,
        network_retry_interval_seconds: network_retry.interval_seconds,
        language: appearance.language,
        reduce_motion: appearance.reduce_motion,
        density: appearance.density,
        launch_at_login: general.launch_at_login && autostart::is_launch_at_login_enabled(),
        close_to_tray: general.close_to_tray,
        native_sync_background_required: state.sync.native_background_required().unwrap_or(false),
        minimize_to_tray: general.minimize_to_tray,
        confirm_quit_with_transfers: general.confirm_quit_with_transfers,
        upload_concurrency: advanced.upload_concurrency,
        download_concurrency: advanced.download_concurrency,
        default_split_part_mib: advanced.default_split_part_mib,
        auto_split_threshold_mib: advanced.auto_split_threshold_mib,
        download_conflict_strategy: behavior.conflict_strategy,
        open_folder_after_download: behavior.open_folder_after_download,
        verify_hash_after_download: behavior.verify_hash_after_download,
        keep_failed_part_days: behavior.keep_failed_part_days,
        global_bandwidth_limit_kbps: bandwidth.global_kbps,
        per_task_bandwidth_limit_kbps: bandwidth.per_task_kbps,
        bandwidth_mode: bandwidth.mode,
        notify_on_complete: notifications.notify_on_complete,
        notify_sound: notifications.notify_sound,
        tray_badge: notifications.tray_badge,
        floating_window_enabled: notifications.floating_window_enabled,
        floating_window_prompted: notifications.floating_window_prompted,
        floating_window_compact_enabled: notifications.floating_window_compact_enabled,
        floating_window_click_toggle_enabled: notifications.floating_window_click_toggle_enabled,
        floating_window_double_click_open_enabled: notifications
            .floating_window_double_click_open_enabled,
        link_default_kind: links.kind,
        link_default_expire_days: links.expire_days,
        link_default_require_password: links.require_password,
        idle_auto_logout_minutes: security.idle_auto_logout_minutes,
        download_history_keep_days: history.keep_days,
        download_history_keep_count: history.keep_count,
    }
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> CommandResult<SettingsSnapshot> {
    Ok(settings_snapshot(&state).await)
}

#[tauri::command]
pub async fn update_settings(
    state: State<'_, AppState>,
    cache_mode: CacheMode,
    cache_ttl_minutes: u32,
) -> CommandResult<SettingsSnapshot> {
    state
        .settings
        .update_cache(cache_mode, cache_ttl_minutes)
        .await
        .map_err(CommandError::from)?;
    state
        .cache
        .apply_mode(cache_mode)
        .await
        .map_err(CommandError::from)?;
    state.logger.info(
        "settings",
        "cache_settings_updated",
        None,
        serde_json::Map::new(),
    );
    Ok(settings_snapshot(&state).await)
}

#[tauri::command]
pub async fn update_logging_settings(
    state: State<'_, AppState>,
    log_level: LogLevel,
    log_retention_days: u32,
    log_max_file_size_mb: u32,
) -> CommandResult<SettingsSnapshot> {
    let log_directory = state.settings.log_policy().await.0;
    state
        .settings
        .update_logging(log_level, log_retention_days, log_max_file_size_mb)
        .await
        .map_err(CommandError::from)?;
    state
        .logger
        .configure(LogConfig {
            directory: log_directory,
            level: log_level,
            retention_days: log_retention_days,
            max_file_bytes: u64::from(log_max_file_size_mb) * 1024 * 1024,
        })
        .await
        .map_err(CommandError::from)?;
    state.logger.info(
        "settings",
        "logging_settings_updated",
        None,
        serde_json::Map::new(),
    );
    Ok(settings_snapshot(&state).await)
}

#[tauri::command]
pub async fn update_work_directory(
    state: State<'_, AppState>,
    work_directory: String,
    move_existing: bool,
) -> CommandResult<SettingsSnapshot> {
    state
        .work_directory
        .schedule_change(PathBuf::from(work_directory.trim()), move_existing)
        .map_err(CommandError::from)?;
    state.logger.info(
        "settings",
        "work_directory_change_scheduled",
        None,
        serde_json::Map::new(),
    );
    Ok(settings_snapshot(&state).await)
}

#[tauri::command]
pub async fn update_transfer_settings(
    state: State<'_, AppState>,
    auto_retry_network_errors: bool,
    network_retry_limit: Option<u32>,
    network_retry_interval_seconds: u32,
) -> CommandResult<SettingsSnapshot> {
    state
        .settings
        .update_transfer(
            auto_retry_network_errors,
            network_retry_limit,
            network_retry_interval_seconds,
        )
        .await
        .map_err(CommandError::from)?;
    state.logger.info(
        "settings",
        "transfer_settings_updated",
        None,
        serde_json::Map::new(),
    );
    Ok(settings_snapshot(&state).await)
}

#[tauri::command]
pub async fn update_download_settings(
    state: State<'_, AppState>,
    download_directory: String,
    ask_download_location: bool,
) -> CommandResult<SettingsSnapshot> {
    state
        .settings
        .update_download(
            PathBuf::from(download_directory.trim()),
            ask_download_location,
        )
        .await
        .map_err(CommandError::from)?;
    Ok(settings_snapshot(&state).await)
}

#[tauri::command]
pub async fn clear_metadata_cache(state: State<'_, AppState>) -> CommandResult<SettingsSnapshot> {
    state.cache.clear().await.map_err(CommandError::from)?;
    Ok(settings_snapshot(&state).await)
}

#[tauri::command]
pub async fn clear_logs(state: State<'_, AppState>) -> CommandResult<SettingsSnapshot> {
    state.logger.clear().await.map_err(CommandError::from)?;
    Ok(settings_snapshot(&state).await)
}

#[tauri::command]
pub async fn forget_saved_login(state: State<'_, AppState>) -> CommandResult<SettingsSnapshot> {
    if let Some(email) = state.settings.remembered_email().await {
        state
            .credentials
            .delete(email)
            .await
            .map_err(CommandError::from)?;
    }
    state
        .settings
        .set_remembered_email(None)
        .await
        .map_err(CommandError::from)?;
    Ok(settings_snapshot(&state).await)
}

#[tauri::command]
pub async fn list_mounts(state: State<'_, AppState>, refresh: bool) -> CommandResult<Vec<Mount>> {
    let (mode, ttl) = state.settings.cache_policy().await;
    if !refresh && let Some(cached) = state.cache.get("mounts", mode, ttl).await {
        return Ok(cached);
    }
    let mounts = state.api.list_mounts().await.map_err(CommandError::from)?;
    let _ = state.cache.put("mounts".to_owned(), &mounts, mode).await;
    Ok(mounts)
}

#[tauri::command]
pub async fn list_files(
    state: State<'_, AppState>,
    mount_id: String,
    path: String,
    refresh: bool,
) -> CommandResult<Vec<FileInfo>> {
    let mount_id = MountId::parse(mount_id).map_err(CommandError::from)?;
    let path = RemotePath::parse(path).map_err(CommandError::from)?;
    let cache_key = format!("files:{}:{}", mount_id.as_str(), path.as_str());
    let (mode, ttl) = state.settings.cache_policy().await;
    if !refresh && let Some(cached) = state.cache.get(&cache_key, mode, ttl).await {
        state
            .logger
            .debug("cache", "directory_cache_hit", None, serde_json::Map::new());
        return Ok(cached);
    }
    let files = state
        .api
        .list_files(&mount_id, &path)
        .await
        .map_err(CommandError::from)?;
    let _ = state.cache.put(cache_key, &files, mode).await;
    Ok(files)
}

#[tauri::command]
pub async fn list_recent(
    state: State<'_, AppState>,
    refresh: bool,
) -> CommandResult<Vec<LocatedFile>> {
    let (mode, ttl) = state.settings.cache_policy().await;
    if !refresh && let Some(cached) = state.cache.get("recent", mode, ttl).await {
        return Ok(cached);
    }
    let files = state.api.list_recent().await.map_err(CommandError::from)?;
    let _ = state.cache.put("recent".to_owned(), &files, mode).await;
    Ok(files)
}

#[tauri::command]
pub async fn list_shared(
    state: State<'_, AppState>,
    refresh: bool,
) -> CommandResult<Vec<LocatedFile>> {
    let (mode, ttl) = state.settings.cache_policy().await;
    if !refresh && let Some(cached) = state.cache.get("shared", mode, ttl).await {
        return Ok(cached);
    }
    let files = state.api.list_shared().await.map_err(CommandError::from)?;
    let _ = state.cache.put("shared".to_owned(), &files, mode).await;
    Ok(files)
}

#[tauri::command]
pub async fn list_trash(state: State<'_, AppState>, refresh: bool) -> CommandResult<TrashList> {
    let (mode, ttl) = state.settings.cache_policy().await;
    if !refresh && let Some(cached) = state.cache.get("trash", mode, ttl).await {
        return Ok(cached);
    }
    let trash = state.api.list_trash().await.map_err(CommandError::from)?;
    let _ = state.cache.put("trash".to_owned(), &trash, mode).await;
    Ok(trash)
}

#[tauri::command]
pub async fn restore_trash(
    state: State<'_, AppState>,
    files: Vec<TrashRestoreTarget>,
) -> CommandResult<()> {
    let files = files
        .into_iter()
        .map(|file| {
            let mount_id = MountId::parse(file.mount_id)?;
            let path = RemotePath::parse(file.path)?;
            path.require_non_root()?;
            Ok((mount_id, path))
        })
        .collect::<Result<Vec<_>, AppError>>()
        .map_err(CommandError::from)?;
    state
        .api
        .restore_trash(&files)
        .await
        .map_err(CommandError::from)?;
    let _ = state.cache.clear().await;
    Ok(())
}

#[tauri::command]
pub async fn empty_trash(state: State<'_, AppState>, confirmation: String) -> CommandResult<()> {
    if confirmation != "永久删除" {
        return Err(CommandError::from(AppError::InvalidInput("confirmation")));
    }
    state.api.empty_trash().await.map_err(CommandError::from)?;
    let _ = state.cache.clear().await;
    Ok(())
}

#[tauri::command]
pub async fn create_folder(
    state: State<'_, AppState>,
    mount_id: String,
    parent_path: String,
    name: String,
) -> CommandResult<()> {
    let mount_id = MountId::parse(mount_id).map_err(CommandError::from)?;
    let parent = RemotePath::parse(parent_path).map_err(CommandError::from)?;
    let name = RemoteName::parse(name).map_err(CommandError::from)?;
    state
        .api
        .create_folder(&mount_id, &parent, &name)
        .await
        .map_err(CommandError::from)?;
    let _ = state.cache.clear().await;
    Ok(())
}

#[tauri::command]
pub async fn rename_entry(
    state: State<'_, AppState>,
    mount_id: String,
    path: String,
    new_name: String,
) -> CommandResult<()> {
    let mount_id = MountId::parse(mount_id).map_err(CommandError::from)?;
    let path = RemotePath::parse(path).map_err(CommandError::from)?;
    path.require_non_root().map_err(CommandError::from)?;
    let destination = path
        .parent()
        .and_then(|parent| parent.join(&RemoteName::parse(new_name)?))
        .map_err(CommandError::from)?;
    state
        .api
        .move_to(&mount_id, &path, &mount_id, &destination)
        .await
        .map_err(CommandError::from)?;
    let _ = state.cache.clear().await;
    Ok(())
}

#[tauri::command]
pub async fn move_entry(
    state: State<'_, AppState>,
    mount_id: String,
    path: String,
    destination_mount_id: String,
    destination_directory: String,
) -> CommandResult<()> {
    relocate_entry(
        &state,
        true,
        mount_id,
        path,
        destination_mount_id,
        destination_directory,
    )
    .await
}

#[tauri::command]
pub async fn copy_entry(
    state: State<'_, AppState>,
    mount_id: String,
    path: String,
    destination_mount_id: String,
    destination_directory: String,
) -> CommandResult<()> {
    relocate_entry(
        &state,
        false,
        mount_id,
        path,
        destination_mount_id,
        destination_directory,
    )
    .await
}

async fn relocate_entry(
    state: &State<'_, AppState>,
    is_move: bool,
    mount_id: String,
    path: String,
    destination_mount_id: String,
    destination_directory: String,
) -> CommandResult<()> {
    let mount_id = MountId::parse(mount_id).map_err(CommandError::from)?;
    let path = RemotePath::parse(path).map_err(CommandError::from)?;
    path.require_non_root().map_err(CommandError::from)?;
    let destination_mount_id = MountId::parse(destination_mount_id).map_err(CommandError::from)?;
    let destination_directory =
        RemotePath::parse(destination_directory).map_err(CommandError::from)?;
    let name = RemoteName::parse(path.file_name()?.to_owned()).map_err(CommandError::from)?;
    let destination = destination_directory
        .join(&name)
        .map_err(CommandError::from)?;
    let result = if is_move {
        state
            .api
            .move_to(&mount_id, &path, &destination_mount_id, &destination)
            .await
    } else {
        state
            .api
            .copy_to(&mount_id, &path, &destination_mount_id, &destination)
            .await
    };
    result.map_err(CommandError::from)?;
    let _ = state.cache.clear().await;
    Ok(())
}

#[tauri::command]
pub async fn delete_entry(
    state: State<'_, AppState>,
    mount_id: String,
    path: String,
) -> CommandResult<()> {
    let mount_id = MountId::parse(mount_id).map_err(CommandError::from)?;
    let path = RemotePath::parse(path).map_err(CommandError::from)?;
    path.require_non_root().map_err(CommandError::from)?;
    state
        .api
        .delete(&mount_id, &path)
        .await
        .map_err(CommandError::from)?;
    let _ = state.cache.clear().await;
    Ok(())
}

#[tauri::command]
pub async fn upload_file(
    app: AppHandle,
    state: State<'_, AppState>,
    transfer_id: String,
    mount_id: String,
    remote_directory: String,
    local_path_grant: String,
) -> CommandResult<TransferResult> {
    let mount_id = MountId::parse(mount_id).map_err(CommandError::from)?;
    let directory = RemotePath::parse(remote_directory).map_err(CommandError::from)?;
    let selected_path = state
        .local_access
        .take_upload(&local_path_grant)
        .map_err(CommandError::from)?;
    let local_path = LocalUploadPath::from_selected(selected_path)
        .await
        .map_err(CommandError::from)?;
    let retry_policy =
        transfer::NetworkRetryPolicy::from(state.settings.network_retry_settings().await);
    let result = transfer::upload(
        app,
        &state.api,
        &state.transfers,
        &state.transfer_checkpoints,
        retry_policy,
        transfer_id,
        mount_id,
        directory,
        local_path,
    )
    .await
    .map_err(CommandError::from)?;
    let _ = state.cache.clear().await;
    Ok(result)
}

#[tauri::command]
pub async fn download_file(
    app: AppHandle,
    state: State<'_, AppState>,
    transfer_id: String,
    mount_id: String,
    remote_path: String,
    local_path_grant: String,
) -> CommandResult<TransferResult> {
    let mount_id = MountId::parse(mount_id).map_err(CommandError::from)?;
    let remote_path = RemotePath::parse(remote_path).map_err(CommandError::from)?;
    let selected_path = state
        .local_access
        .take_download(&local_path_grant)
        .map_err(CommandError::from)?;
    let completed_path = selected_path.clone();
    let owner_id = transfer::current_owner(&state.api)
        .await
        .map_err(CommandError::from)?;
    let history_name = remote_path
        .file_name()
        .map_err(CommandError::from)?
        .to_owned();
    let local_path = LocalDownloadPath::from_selected(selected_path)
        .await
        .map_err(CommandError::from)?;
    state
        .download_history
        .start(NewDownloadHistoryItem {
            transfer_id: &transfer_id,
            owner_id: &owner_id,
            name: &history_name,
            remote_path: remote_path.as_str(),
            local_path: &completed_path,
            local_kind: DownloadLocalKind::File,
            recovery_kind: Some(transfer::RecoveryKind::ByteResume),
        })
        .map_err(CommandError::from)?;
    let retry_policy =
        transfer::NetworkRetryPolicy::from(state.settings.network_retry_settings().await);
    let result = transfer::download(
        app,
        &state.api,
        &state.transfers,
        &state.transfer_checkpoints,
        retry_policy,
        transfer_id.clone(),
        mount_id,
        remote_path,
        local_path,
    )
    .await
    .map_err(CommandError::from)?;
    let completed_path = result.local_path.clone().unwrap_or(completed_path);
    state
        .local_access
        .remember_download(&transfer_id, completed_path)
        .map_err(CommandError::from)?;
    Ok(result)
}

#[tauri::command]
pub async fn open_downloaded_file(
    state: State<'_, AppState>,
    transfer_id: String,
) -> CommandResult<()> {
    let path = match state.local_access.completed_download(&transfer_id) {
        Ok(path) => path,
        Err(AppError::NotFound) => {
            let owner_id = transfer::current_owner(&state.api)
                .await
                .map_err(CommandError::from)?;
            state
                .download_history
                .completed_path(&owner_id, &transfer_id)
                .map_err(CommandError::from)?
        }
        Err(error) => return Err(CommandError::from(error)),
    };
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|_| CommandError::from(AppError::LocalOpen))?;
    if !metadata.is_file() {
        return Err(CommandError::from(AppError::LocalOpen));
    }
    local_open::open(&path).map_err(CommandError::from)
}

#[tauri::command]
pub async fn open_downloaded_folder(
    state: State<'_, AppState>,
    transfer_id: String,
) -> CommandResult<()> {
    let path = match state.local_access.completed_download(&transfer_id) {
        Ok(path) => path,
        Err(AppError::NotFound) => {
            let owner_id = transfer::current_owner(&state.api)
                .await
                .map_err(CommandError::from)?;
            state
                .download_history
                .completed_path(&owner_id, &transfer_id)
                .map_err(CommandError::from)?
        }
        Err(error) => return Err(CommandError::from(error)),
    };
    let parent = path
        .parent()
        .ok_or_else(|| CommandError::from(AppError::LocalOpen))?;
    let metadata = tokio::fs::metadata(parent)
        .await
        .map_err(|_| CommandError::from(AppError::LocalOpen))?;
    if !metadata.is_dir() {
        return Err(CommandError::from(AppError::LocalOpen));
    }
    local_open::open(parent).map_err(CommandError::from)
}

#[tauri::command]
pub fn cancel_transfer(state: State<'_, AppState>, transfer_id: String) -> CommandResult<bool> {
    state
        .transfers
        .cancel(&transfer_id)
        .map_err(CommandError::from)
}

#[tauri::command]
pub fn pause_transfer(state: State<'_, AppState>, transfer_id: String) -> CommandResult<bool> {
    state
        .transfers
        .pause(&transfer_id)
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn update_appearance_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    language: InterfaceLanguage,
    reduce_motion: bool,
    density: UiDensity,
) -> CommandResult<SettingsSnapshot> {
    state
        .settings
        .update_appearance(language, reduce_motion, density)
        .await
        .map_err(CommandError::from)?;
    let _ = crate::desktop::update_language(&app, language);
    Ok(settings_snapshot(&state).await)
}

#[tauri::command]
pub async fn update_general_settings(
    state: State<'_, AppState>,
    launch_at_login: bool,
    close_to_tray: bool,
    minimize_to_tray: bool,
    confirm_quit_with_transfers: bool,
) -> CommandResult<SettingsSnapshot> {
    if state
        .sync
        .native_background_required()
        .map_err(CommandError::from)?
        && (!launch_at_login || !close_to_tray)
    {
        return Err(CommandError::from(AppError::Conflict));
    }
    let previous = state.settings.general().await.launch_at_login;
    if previous != launch_at_login {
        autostart::set_launch_at_login(launch_at_login).map_err(CommandError::from)?;
    }
    let update_result = state
        .settings
        .update_general(
            launch_at_login,
            close_to_tray,
            minimize_to_tray,
            confirm_quit_with_transfers,
        )
        .await;
    if let Err(error) = update_result {
        if previous != launch_at_login {
            let _ = autostart::set_launch_at_login(previous);
        }
        return Err(CommandError::from(error));
    }
    Ok(settings_snapshot(&state).await)
}

#[tauri::command]
pub async fn update_transfer_advanced_settings(
    state: State<'_, AppState>,
    upload_concurrency: u32,
    download_concurrency: u32,
    default_split_part_mib: u32,
    auto_split_threshold_mib: u32,
) -> CommandResult<SettingsSnapshot> {
    state
        .settings
        .update_transfer_advanced(
            upload_concurrency,
            download_concurrency,
            default_split_part_mib,
            auto_split_threshold_mib,
        )
        .await
        .map_err(CommandError::from)?;
    state
        .transfers
        .set_concurrency(upload_concurrency as usize, download_concurrency as usize);
    Ok(settings_snapshot(&state).await)
}

#[tauri::command]
pub async fn update_download_behavior_settings(
    state: State<'_, AppState>,
    conflict_strategy: DownloadConflictStrategy,
    open_folder_after_download: bool,
    verify_hash_after_download: bool,
    keep_failed_part_days: u32,
) -> CommandResult<SettingsSnapshot> {
    state
        .settings
        .update_download_behavior(
            conflict_strategy,
            open_folder_after_download,
            verify_hash_after_download,
            keep_failed_part_days,
        )
        .await
        .map_err(CommandError::from)?;
    state
        .transfer_checkpoints
        .cleanup_expired(keep_failed_part_days)
        .await
        .map_err(CommandError::from)?;
    Ok(settings_snapshot(&state).await)
}

#[tauri::command]
pub async fn update_bandwidth_settings(
    state: State<'_, AppState>,
    global_kbps: u32,
    per_task_kbps: u32,
    mode: BandwidthMode,
) -> CommandResult<SettingsSnapshot> {
    state
        .settings
        .update_bandwidth(global_kbps, per_task_kbps, mode)
        .await
        .map_err(CommandError::from)?;
    state
        .transfers
        .set_bandwidth(mode, global_kbps, per_task_kbps);
    Ok(settings_snapshot(&state).await)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn update_notification_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    notify_on_complete: bool,
    notify_sound: bool,
    tray_badge: bool,
    floating_window_enabled: bool,
    floating_window_prompted: bool,
    floating_window_compact_enabled: bool,
    floating_window_click_toggle_enabled: bool,
    floating_window_double_click_open_enabled: bool,
) -> CommandResult<SettingsSnapshot> {
    state
        .settings
        .update_notifications(
            notify_on_complete,
            notify_sound,
            tray_badge,
            floating_window_enabled,
            floating_window_prompted,
            floating_window_compact_enabled,
            floating_window_click_toggle_enabled,
            floating_window_double_click_open_enabled,
        )
        .await
        .map_err(CommandError::from)?;
    crate::desktop::set_floating_window_visible(&app, floating_window_enabled)
        .map_err(|_| CommandError::from(AppError::LocalData))?;
    Ok(settings_snapshot(&state).await)
}

#[tauri::command]
pub async fn update_link_defaults(
    state: State<'_, AppState>,
    kind: LinkDefaultKind,
    expire_days: u32,
    require_password: bool,
) -> CommandResult<SettingsSnapshot> {
    state
        .settings
        .update_link_defaults(kind, expire_days, require_password)
        .await
        .map_err(CommandError::from)?;
    Ok(settings_snapshot(&state).await)
}

#[tauri::command]
pub async fn update_security_settings(
    state: State<'_, AppState>,
    idle_auto_logout_minutes: u32,
) -> CommandResult<SettingsSnapshot> {
    state
        .settings
        .update_security(idle_auto_logout_minutes)
        .await
        .map_err(CommandError::from)?;
    Ok(settings_snapshot(&state).await)
}

#[tauri::command]
pub async fn update_history_policy(
    state: State<'_, AppState>,
    keep_days: u32,
    keep_count: u32,
) -> CommandResult<SettingsSnapshot> {
    state
        .settings
        .update_history_policy(keep_days, keep_count)
        .await
        .map_err(CommandError::from)?;
    state
        .download_history
        .set_policy(keep_days, keep_count)
        .map_err(CommandError::from)?;
    Ok(settings_snapshot(&state).await)
}

#[tauri::command]
pub fn app_version_info() -> AppVersionInfo {
    AppVersionInfo {
        version: env!("CARGO_PKG_VERSION"),
        homepage_url: "https://forgejo.haohaohow.com/wenhaozheng/Suitcase",
        releases_url: "https://forgejo.haohaohow.com/wenhaozheng/Suitcase/releases",
        issues_url: "https://forgejo.haohaohow.com/wenhaozheng/Suitcase/issues",
        license: "MIT",
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExternalPage {
    Homepage,
    Releases,
    Issues,
}

#[tauri::command]
pub fn open_external_page(page: ExternalPage) -> CommandResult<()> {
    let url = match page {
        ExternalPage::Homepage => "https://forgejo.haohaohow.com/wenhaozheng/Suitcase",
        ExternalPage::Releases => "https://forgejo.haohaohow.com/wenhaozheng/Suitcase/releases",
        ExternalPage::Issues => "https://forgejo.haohaohow.com/wenhaozheng/Suitcase/issues",
    };
    local_open::open_url(url).map_err(CommandError::from)
}
use std::path::PathBuf;
