use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::UNIX_EPOCH,
};

use futures_util::TryStreamExt;
use tauri::{AppHandle, Manager};
use tokio_util::{io::ReaderStream, sync::CancellationToken};

use crate::{
    error::AppError,
    file_ops::{LocalUploadPath, MountId, RemoteName, RemotePath},
    koofr_api::KoofrApi,
};

use super::{
    checkpoint::{TransferCheckpoint, TransferCheckpointStore, UploadCheckpoint},
    manager::TransferManager,
    model::{
        NetworkRetryPolicy, NetworkRetryRequest, TransferDirection, TransferResult, TransferState,
        UPLOAD_STALL_LIMIT, emit_progress, emit_terminal, normalize_interruption,
        should_retry_network, stall_watchdog, wait_for_network_retry,
    },
};

#[allow(clippy::too_many_arguments)]
pub async fn upload(
    app: AppHandle,
    api: &KoofrApi,
    manager: &TransferManager,
    checkpoints: &TransferCheckpointStore,
    retry_policy: NetworkRetryPolicy,
    transfer_id: String,
    mount_id: MountId,
    directory: RemotePath,
    local_path: LocalUploadPath,
) -> Result<TransferResult, AppError> {
    let metadata = tokio::fs::metadata(local_path.as_path()).await?;
    let owner_id = account_owner(api).await?;
    checkpoints
        .insert(TransferCheckpoint::Upload(UploadCheckpoint {
            transfer_id: transfer_id.clone(),
            owner_id,
            mount_id: mount_id.as_str().to_owned(),
            remote_directory: directory.as_str().to_owned(),
            local_path: local_path.as_path().to_path_buf(),
            expected_size: metadata.len(),
            modified_millis: modified_millis(&metadata)?,
            retained_at: 0,
        }))
        .await?;
    run_upload(
        app,
        api,
        manager,
        checkpoints,
        retry_policy,
        transfer_id,
        mount_id,
        directory,
        local_path,
    )
    .await
}

pub async fn retry_upload(
    app: AppHandle,
    api: &KoofrApi,
    manager: &TransferManager,
    checkpoints: &TransferCheckpointStore,
    retry_policy: NetworkRetryPolicy,
    transfer_id: String,
) -> Result<TransferResult, AppError> {
    let TransferCheckpoint::Upload(checkpoint) = checkpoints.get(&transfer_id).await? else {
        return Err(AppError::InvalidInput("upload checkpoint"));
    };
    let local_path = LocalUploadPath::from_selected(checkpoint.local_path).await?;
    let metadata = tokio::fs::metadata(local_path.as_path()).await?;
    if metadata.len() != checkpoint.expected_size
        || modified_millis(&metadata)? != checkpoint.modified_millis
    {
        return Err(AppError::Conflict);
    }
    run_upload(
        app,
        api,
        manager,
        checkpoints,
        retry_policy,
        transfer_id,
        MountId::parse(checkpoint.mount_id)?,
        RemotePath::parse(checkpoint.remote_directory)?,
        local_path,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn run_upload(
    app: AppHandle,
    api: &KoofrApi,
    manager: &TransferManager,
    checkpoints: &TransferCheckpointStore,
    retry_policy: NetworkRetryPolicy,
    transfer_id: String,
    mount_id: MountId,
    directory: RemotePath,
    local_path: LocalUploadPath,
) -> Result<TransferResult, AppError> {
    let total = tokio::fs::metadata(local_path.as_path()).await?.len();
    let registration = manager.register_scoped(&transfer_id)?;
    let cancel = registration.token().clone();
    emit_progress(
        &app,
        &transfer_id,
        TransferDirection::Upload,
        TransferState::Queued,
        0,
        Some(total),
    );
    let state = app.state::<crate::AppState>();
    let logger = &state.logger;
    let _permit = match logger
        .trace(
            "transfer",
            "wait_upload_slot",
            Some(&transfer_id),
            manager.acquire(TransferDirection::Upload, &cancel),
        )
        .await
    {
        Ok(permit) => permit,
        Err(error) => {
            let paused = registration.finish();
            let result =
                settle_upload_checkpoint(checkpoints, &transfer_id, Err(error), paused).await;
            emit_terminal(&app, &transfer_id, TransferDirection::Upload, 0, &result);
            return result;
        }
    };
    // A running upload must not be evicted to make room for another checkpoint.
    let _ = checkpoints.mark_active(&transfer_id).await;
    let progress = Arc::new(AtomicU64::new(0));
    let mut retries_completed = 0_u32;
    let result = loop {
        progress.store(0, Ordering::Relaxed);
        let result = logger
            .trace(
                "transfer",
                "upload_file_attempt",
                Some(&transfer_id),
                upload_inner(
                    &app,
                    api,
                    &transfer_id,
                    &cancel,
                    mount_id.clone(),
                    directory.clone(),
                    local_path.clone(),
                    progress.clone(),
                ),
            )
            .await;
        if !should_retry_network(&result, retry_policy, retries_completed) {
            break result;
        }
        retries_completed = retries_completed.saturating_add(1);
        if let Err(error) = wait_for_network_retry(NetworkRetryRequest {
            app: &app,
            cancel: &cancel,
            transfer_id: &transfer_id,
            direction: TransferDirection::Upload,
            retry_attempt: retries_completed,
            bytes_transferred: progress.load(Ordering::Relaxed),
            total_bytes: Some(total),
            split_progress: None,
            policy: retry_policy,
        })
        .await
        {
            break Err(error);
        }
    };
    let paused = registration.finish();
    let result = settle_upload_checkpoint(checkpoints, &transfer_id, result, paused).await;
    emit_terminal(
        &app,
        &transfer_id,
        TransferDirection::Upload,
        progress.load(Ordering::Relaxed),
        &result,
    );
    result
}

#[allow(clippy::too_many_arguments)]
async fn upload_inner(
    app: &AppHandle,
    api: &KoofrApi,
    transfer_id: &str,
    cancel: &CancellationToken,
    mount_id: MountId,
    directory: RemotePath,
    local_path: LocalUploadPath,
    progress: Arc<AtomicU64>,
) -> Result<TransferResult, AppError> {
    let file_name = RemoteName::parse(local_path.file_name()?)?;
    let file = tokio::fs::File::open(local_path.as_path()).await?;
    let total = file.metadata().await?.len();
    let app_for_stream = app.clone();
    let id_for_stream = transfer_id.to_owned();
    let cancel_for_stream = cancel.clone();
    emit_progress(
        app,
        transfer_id,
        TransferDirection::Upload,
        TransferState::Running,
        0,
        Some(total),
    );
    let watched_progress = progress.clone();
    let stream = ReaderStream::new(file).and_then(move |chunk| {
        let app = app_for_stream.clone();
        let transfer_id = id_for_stream.clone();
        let progress = progress.clone();
        let cancel = cancel_for_stream.clone();
        async move {
            app.state::<crate::AppState>()
                .transfers
                .throttle(&transfer_id, chunk.len(), &cancel)
                .await
                .map_err(|_| std::io::Error::other("upload interrupted"))?;
            let previous = progress.fetch_add(chunk.len() as u64, Ordering::Relaxed);
            emit_progress(
                &app,
                &transfer_id,
                TransferDirection::Upload,
                TransferState::Running,
                previous.saturating_add(chunk.len() as u64),
                Some(total),
            );
            Ok(chunk)
        }
    });
    let body = reqwest::Body::wrap_stream(stream);
    let file_info = tokio::select! {
        result = api.upload(&mount_id, &directory, &file_name, body, total) => result?,
        () = cancel.cancelled() => return Err(AppError::Cancelled),
        error = stall_watchdog(&watched_progress, UPLOAD_STALL_LIMIT) => return Err(error),
    };
    Ok(TransferResult {
        transfer_id: transfer_id.to_owned(),
        bytes_transferred: total,
        file: Some(file_info),
        integrity_status: super::IntegrityStatus::NotRequested,
        local_path: None,
    })
}

/// Decides what happens to the checkpoint of a whole-file upload once it stops.
/// Only a pause or a failure that trying again could fix keeps it: a cancelled
/// upload must not come back after a restart, and one the server refused will be
/// refused again.
pub(super) async fn settle_upload_checkpoint(
    checkpoints: &TransferCheckpointStore,
    transfer_id: &str,
    result: Result<TransferResult, AppError>,
    paused: bool,
) -> Result<TransferResult, AppError> {
    let result = normalize_interruption(result, paused);
    match &result {
        Err(AppError::TransferPaused) => {
            let _ = checkpoints.mark_failed(transfer_id, true).await;
        }
        Err(error) if keeps_upload_checkpoint(error) => {
            let _ = checkpoints.mark_failed(transfer_id, true).await;
        }
        Ok(_) | Err(_) => {
            let _ = checkpoints.remove(transfer_id).await;
        }
    }
    result
}

fn keeps_upload_checkpoint(error: &AppError) -> bool {
    !matches!(
        error,
        AppError::Cancelled
            | AppError::InvalidInput(_)
            | AppError::Conflict
            | AppError::Forbidden
            | AppError::NotFound
    )
}

pub(super) fn modified_millis(metadata: &std::fs::Metadata) -> Result<u128, AppError> {
    metadata
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .map_err(|_| AppError::InvalidInput("local file modification time"))
}

async fn account_owner(api: &KoofrApi) -> Result<String, AppError> {
    api.recovery_scope().await
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::settle_upload_checkpoint;
    use crate::{
        error::AppError,
        transfer::checkpoint::{TransferCheckpoint, TransferCheckpointStore, UploadCheckpoint},
    };

    async fn store_with_upload(directory: &std::path::Path) -> (TransferCheckpointStore, String) {
        let store = TransferCheckpointStore::load(directory.join("transfers.json"));
        let transfer_id = uuid::Uuid::new_v4().to_string();
        store
            .insert(TransferCheckpoint::Upload(UploadCheckpoint {
                transfer_id: transfer_id.clone(),
                owner_id: "user-1".to_owned(),
                mount_id: "mount_1".to_owned(),
                remote_directory: "/".to_owned(),
                local_path: PathBuf::from(r"C:\files\report.pdf"),
                expected_size: 1,
                modified_millis: 1,
                retained_at: 0,
            }))
            .await
            .expect("save upload checkpoint");
        (store, transfer_id)
    }

    #[tokio::test]
    async fn only_pauses_and_retryable_failures_keep_an_upload_checkpoint() {
        let directory =
            std::env::temp_dir().join(format!("koofr-upload-settle-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).expect("create test directory");
        let cases = [
            (Err(AppError::Cancelled), true, true),
            (Err(AppError::Cancelled), false, false),
            (Err(AppError::NetworkStalled), false, true),
            (Err(AppError::Forbidden), false, false),
            (Err(AppError::Conflict), false, false),
            (Err(AppError::NotFound), false, false),
        ];
        for (result, paused, kept) in cases {
            let (store, transfer_id) = store_with_upload(&directory).await;
            let _ = settle_upload_checkpoint(&store, &transfer_id, result, paused).await;
            assert_eq!(store.get(&transfer_id).await.is_ok(), kept);
        }
        std::fs::remove_dir_all(directory).expect("remove test directory");
    }
}
