use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::{
    AppState,
    download_history::{DownloadLocalKind, NewDownloadHistoryItem},
    error::{AppError, CommandError},
    file_ops::{MountId, RemoteName, RemotePath, safe_suggested_file_name},
    folder_download::{self, FolderDownloadContext, FolderDownloadRequest, FolderDownloadTarget},
    local_access::LocalFileSelection,
    transfer::{self, NetworkRetryPolicy, TransferResult},
};

type CommandResult<T> = Result<T, CommandError>;

#[tauri::command]
pub async fn select_download_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    suggested_name: String,
) -> CommandResult<Option<LocalFileSelection>> {
    let name = RemoteName::parse(suggested_name).map_err(CommandError::from)?;
    let Some(parent) = app.dialog().file().blocking_pick_folder() else {
        return Ok(None);
    };
    let parent = parent
        .into_path()
        .map_err(|_| CommandError::from(AppError::Dialog))?;
    let target = FolderDownloadTarget::from_parent(parent, &name)
        .await
        .map_err(CommandError::from)?;
    state
        .local_access
        .grant_download_directory(target.as_path().to_path_buf())
        .map(Some)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn prepare_download_folder(
    state: State<'_, AppState>,
    suggested_name: String,
    download_directory: String,
    conflict_strategy: Option<crate::settings::DownloadConflictStrategy>,
) -> CommandResult<LocalFileSelection> {
    let name = RemoteName::parse(suggested_name).map_err(CommandError::from)?;
    let parent = std::path::PathBuf::from(download_directory);
    let preferred = parent.join(safe_suggested_file_name(&name));
    let exists = tokio::fs::try_exists(&preferred).await.unwrap_or(false);
    let strategy =
        conflict_strategy.unwrap_or(state.settings.download_behavior().await.conflict_strategy);
    let target = if exists {
        match strategy {
            crate::settings::DownloadConflictStrategy::Ask
            | crate::settings::DownloadConflictStrategy::Skip => {
                return Err(AppError::Conflict.into());
            }
            crate::settings::DownloadConflictStrategy::Rename => {
                FolderDownloadTarget::from_parent(parent, &name).await
            }
        }
    } else {
        FolderDownloadTarget::from_selected(preferred).await
    }
    .map_err(CommandError::from)?;
    state
        .local_access
        .grant_download_directory(target.as_path().to_path_buf())
        .map_err(Into::into)
}

#[tauri::command]
pub async fn download_folder(
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
        .take_download_directory(&local_path_grant)
        .map_err(CommandError::from)?;
    let completed_path = selected_path.clone();
    let owner_id = transfer::current_owner(&state.api)
        .await
        .map_err(CommandError::from)?;
    let history_name = remote_path
        .file_name()
        .map_err(CommandError::from)?
        .to_owned();
    let target = FolderDownloadTarget::from_selected(selected_path)
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
            local_kind: DownloadLocalKind::Folder,
            recovery_kind: None,
        })
        .map_err(CommandError::from)?;
    let result = folder_download::download_folder(
        FolderDownloadContext {
            app,
            api: &state.api,
            manager: &state.transfers,
            retry_policy: NetworkRetryPolicy::from(state.settings.network_retry_settings().await),
        },
        FolderDownloadRequest {
            transfer_id: transfer_id.clone(),
            mount_id,
            remote_path,
            target,
        },
    )
    .await
    .map_err(CommandError::from)?;
    if let Some(path) = &result.local_path
        && *path != completed_path
    {
        let _ = state.download_history.set_local_path(&transfer_id, path);
    }
    let completed_path = result.local_path.clone().unwrap_or(completed_path);
    state
        .local_access
        .remember_download(&transfer_id, completed_path)
        .map_err(CommandError::from)?;
    Ok(result)
}
