use serde::Deserialize;
use tauri::State;

use crate::{
    AppState,
    error::CommandError,
    file_ops::{MountId, RemotePath},
    koofr_api::{PublicLink, PublicLinkKind},
};

type CommandResult<T> = Result<T, CommandError>;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePublicLinkOptions {
    kind: PublicLinkKind,
    expire_days: u32,
    require_password: bool,
}

#[tauri::command]
pub async fn list_public_links(
    state: State<'_, AppState>,
    mount_id: String,
) -> CommandResult<Vec<PublicLink>> {
    let mount_id = MountId::parse(mount_id).map_err(CommandError::from)?;
    state
        .api
        .list_public_links(&mount_id)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn create_public_link(
    state: State<'_, AppState>,
    mount_id: String,
    path: String,
    options: CreatePublicLinkOptions,
) -> CommandResult<PublicLink> {
    let mount_id = MountId::parse(mount_id).map_err(CommandError::from)?;
    let path = RemotePath::parse(path).map_err(CommandError::from)?;
    if options.expire_days > 365 {
        return Err(crate::error::AppError::InvalidInput("link expiration").into());
    }
    let info = state
        .api
        .file_info(&mount_id, &path)
        .await
        .map_err(CommandError::from)?;
    let kind = if info.entry_type == "file" {
        PublicLinkKind::Download
    } else {
        options.kind
    };
    let mut link = state
        .api
        .create_public_link(&mount_id, &path, kind)
        .await
        .map_err(CommandError::from)?;
    let configure = async {
        if options.expire_days > 0 {
            let valid_to = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| crate::error::AppError::LocalData)?
                .as_millis()
                .saturating_add(u128::from(options.expire_days) * 86_400_000);
            link = state
                .api
                .set_public_link_validity(
                    &mount_id,
                    &link.id,
                    kind,
                    u64::try_from(valid_to).map_err(|_| crate::error::AppError::LocalData)?,
                )
                .await?;
        }
        if options.require_password {
            link = state
                .api
                .reset_public_link_password(&mount_id, &link.id, kind)
                .await?;
        }
        Ok::<(), crate::error::AppError>(())
    }
    .await;
    if let Err(error) = configure {
        let _ = state
            .api
            .delete_public_link(&mount_id, &link.id, kind)
            .await;
        return Err(CommandError::from(error));
    }
    Ok(link)
}

#[tauri::command]
pub async fn delete_public_link(
    state: State<'_, AppState>,
    mount_id: String,
    link_id: String,
    kind: PublicLinkKind,
) -> CommandResult<()> {
    let mount_id = MountId::parse(mount_id).map_err(CommandError::from)?;
    state
        .api
        .delete_public_link(&mount_id, &link_id, kind)
        .await
        .map_err(CommandError::from)
}
