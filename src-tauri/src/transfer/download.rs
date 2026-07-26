use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use futures_util::StreamExt;
use tauri::{AppHandle, Manager};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use crate::{
    error::AppError,
    file_ops::{LocalDownloadPath, MountId, RemotePath},
    koofr_api::{FileInfo, KoofrApi},
};

use super::{
    checkpoint::{DownloadCheckpoint, TransferCheckpoint, TransferCheckpointStore},
    manager::TransferManager,
    model::{
        NetworkRetryPolicy, NetworkRetryRequest, TransferDirection, TransferResult, TransferState,
        emit_progress, emit_terminal, normalize_interruption, should_retry_network,
        wait_for_network_retry,
    },
    part::{open_partial, partial_length, truncate_partial, validate_checkpoint_paths},
    range::{ResponseMode, response_mode},
};

#[allow(clippy::too_many_arguments)]
pub async fn download(
    app: AppHandle,
    api: &KoofrApi,
    manager: &TransferManager,
    checkpoints: &TransferCheckpointStore,
    retry_policy: NetworkRetryPolicy,
    transfer_id: String,
    mount_id: MountId,
    remote_path: RemotePath,
    local_path: LocalDownloadPath,
) -> Result<TransferResult, AppError> {
    let owner_id = account_owner(api).await?;
    let checkpoint = DownloadCheckpoint {
        transfer_id: transfer_id.clone(),
        owner_id,
        mount_id: mount_id.as_str().to_owned(),
        remote_path: remote_path.as_str().to_owned(),
        partial_path: local_path.resumable_temporary_path(&transfer_id)?,
        local_path: local_path.as_path().to_path_buf(),
        // The first registered run refreshes these fields. Keeping network discovery inside the
        // registered transfer makes automatic retry, pause, and cancellation apply immediately.
        expected_size: 0,
        remote_hash: String::new(),
        remote_modified: 0,
        resume_allowed: true,
        retained_at: 0,
    };
    checkpoints
        .insert(TransferCheckpoint::Download(checkpoint.clone()))
        .await?;
    run_download(app, api, manager, checkpoints, retry_policy, checkpoint).await
}

pub async fn resume_download(
    app: AppHandle,
    api: &KoofrApi,
    manager: &TransferManager,
    checkpoints: &TransferCheckpointStore,
    retry_policy: NetworkRetryPolicy,
    transfer_id: String,
) -> Result<TransferResult, AppError> {
    let TransferCheckpoint::Download(checkpoint) = checkpoints.get(&transfer_id).await? else {
        return Err(AppError::InvalidInput("download checkpoint"));
    };
    run_download(app, api, manager, checkpoints, retry_policy, checkpoint).await
}

async fn run_download(
    app: AppHandle,
    api: &KoofrApi,
    manager: &TransferManager,
    checkpoints: &TransferCheckpointStore,
    retry_policy: NetworkRetryPolicy,
    mut checkpoint: DownloadCheckpoint,
) -> Result<TransferResult, AppError> {
    let transfer_id = checkpoint.transfer_id.clone();
    validate_checkpoint_paths(&checkpoint).await?;
    let cancel = manager.register(&transfer_id)?;
    let initial_progress = partial_length(&checkpoint).await?;
    emit_progress(
        &app,
        &transfer_id,
        TransferDirection::Download,
        TransferState::Queued,
        initial_progress,
        Some(checkpoint.expected_size),
    );
    let _permit = match manager.acquire(TransferDirection::Download, &cancel).await {
        Ok(permit) => permit,
        Err(error) => {
            manager.finish(&transfer_id);
            let result = Err(error);
            emit_terminal(
                &app,
                &transfer_id,
                TransferDirection::Download,
                initial_progress,
                &result,
            );
            return result;
        }
    };
    checkpoints.mark_active(&transfer_id).await?;
    let progress = Arc::new(AtomicU64::new(initial_progress));
    let verify_hash = app
        .state::<crate::AppState>()
        .settings
        .download_behavior()
        .await
        .verify_hash_after_download;
    let mut retries_completed = 0_u32;
    let result = loop {
        let result = match refresh_remote(api, checkpoints, &mut checkpoint).await {
            Ok(()) => {
                download_inner(
                    &app,
                    api,
                    &cancel,
                    &checkpoint,
                    progress.clone(),
                    verify_hash,
                )
                .await
            }
            Err(error) => Err(error),
        };
        if !should_retry_network(&result, retry_policy, retries_completed) {
            break result;
        }
        retries_completed = retries_completed.saturating_add(1);
        if let Err(error) = wait_for_network_retry(NetworkRetryRequest {
            app: &app,
            cancel: &cancel,
            transfer_id: &transfer_id,
            direction: TransferDirection::Download,
            retry_attempt: retries_completed,
            bytes_transferred: progress.load(Ordering::Relaxed),
            total_bytes: Some(checkpoint.expected_size),
            policy: retry_policy,
        })
        .await
        {
            break Err(error);
        }
    };
    let paused = manager.was_paused(&transfer_id);
    manager.finish(&transfer_id);
    let result = match result {
        Ok(result) => {
            checkpoints.remove(&transfer_id).await?;
            Ok(result)
        }
        other => {
            let normalized = normalize_interruption(other, paused);
            match &normalized {
                Err(AppError::Cancelled) => {
                    let _ = tokio::fs::remove_file(&checkpoint.partial_path).await;
                    let _ = checkpoints.remove(&transfer_id).await;
                }
                Err(AppError::TransferPaused) => {}
                Err(AppError::Integrity) => {
                    checkpoints.mark_failed(&transfer_id, false).await?;
                }
                Err(_) => {
                    checkpoints.mark_failed(&transfer_id, true).await?;
                }
                Ok(_) => {}
            }
            normalized
        }
    };
    emit_terminal(
        &app,
        &transfer_id,
        TransferDirection::Download,
        progress.load(Ordering::Relaxed),
        &result,
    );
    result
}

async fn refresh_remote(
    api: &KoofrApi,
    checkpoints: &TransferCheckpointStore,
    checkpoint: &mut DownloadCheckpoint,
) -> Result<(), AppError> {
    let mount_id = MountId::parse(checkpoint.mount_id.clone())?;
    let remote_path = RemotePath::parse(checkpoint.remote_path.clone())?;
    let info = api.file_info(&mount_id, &remote_path).await?;
    let size = expected_size(&info)?;
    let hash_changed = !checkpoint.remote_hash.is_empty()
        && !info.hash.is_empty()
        && checkpoint.remote_hash != info.hash;
    if checkpoint.expected_size != size
        || checkpoint.remote_modified != info.modified
        || hash_changed
    {
        truncate_partial(checkpoint).await?;
        checkpoint.expected_size = size;
        checkpoint.remote_hash = info.hash;
        checkpoint.remote_modified = info.modified;
        checkpoints
            .insert(TransferCheckpoint::Download(checkpoint.clone()))
            .await?;
    }
    Ok(())
}

async fn download_inner(
    app: &AppHandle,
    api: &KoofrApi,
    cancel: &CancellationToken,
    checkpoint: &DownloadCheckpoint,
    progress: Arc<AtomicU64>,
    verify_hash: bool,
) -> Result<TransferResult, AppError> {
    let mount_id = MountId::parse(checkpoint.mount_id.clone())?;
    let remote_path = RemotePath::parse(checkpoint.remote_path.clone())?;
    let offset = partial_length(checkpoint).await?;
    if offset == checkpoint.expected_size {
        let output = open_partial(checkpoint, ResponseMode::Append).await?;
        output.sync_all().await?;
        drop(output);
        let integrity_status = super::verify_file_integrity(
            &checkpoint.partial_path,
            &checkpoint.remote_hash,
            cancel,
            verify_hash,
        )
        .await?;
        tokio::fs::rename(&checkpoint.partial_path, &checkpoint.local_path).await?;
        return Ok(TransferResult {
            transfer_id: checkpoint.transfer_id.clone(),
            bytes_transferred: offset,
            file: None,
            integrity_status,
        });
    }
    let response = tokio::select! {
        result = api.download_response_from(&mount_id, &remote_path, offset) => result?,
        () = cancel.cancelled() => return Err(AppError::Cancelled),
    };
    let mode = response_mode(
        offset,
        response.status(),
        response.headers().get(reqwest::header::CONTENT_RANGE),
        checkpoint.expected_size,
    )?;
    let transferred = if mode == ResponseMode::Append {
        offset
    } else {
        0
    };
    let mut output = open_partial(checkpoint, mode).await?;
    progress.store(transferred, Ordering::Relaxed);
    emit_progress(
        app,
        &checkpoint.transfer_id,
        TransferDirection::Download,
        TransferState::Running,
        transferred,
        Some(checkpoint.expected_size),
    );
    let mut stream = response.bytes_stream();
    let mut transferred = transferred;
    loop {
        let next = tokio::select! {
            item = stream.next() => item,
            () = cancel.cancelled() => return Err(AppError::Cancelled),
        };
        match next {
            Some(Ok(chunk)) => {
                app.state::<crate::AppState>()
                    .transfers
                    .throttle(&checkpoint.transfer_id, chunk.len(), cancel)
                    .await?;
                output.write_all(&chunk).await?;
                transferred = transferred.saturating_add(chunk.len() as u64);
                progress.store(transferred, Ordering::Relaxed);
                emit_progress(
                    app,
                    &checkpoint.transfer_id,
                    TransferDirection::Download,
                    TransferState::Running,
                    transferred,
                    Some(checkpoint.expected_size),
                );
            }
            Some(Err(error)) => return Err(AppError::Network(error)),
            None => break,
        }
    }
    if transferred != checkpoint.expected_size || cancel.is_cancelled() {
        return Err(AppError::IncompleteTransfer);
    }
    output.flush().await?;
    output.sync_all().await?;
    drop(output);
    let integrity_status = super::verify_file_integrity(
        &checkpoint.partial_path,
        &checkpoint.remote_hash,
        cancel,
        verify_hash,
    )
    .await?;
    tokio::fs::rename(&checkpoint.partial_path, &checkpoint.local_path).await?;
    Ok(TransferResult {
        transfer_id: checkpoint.transfer_id.clone(),
        bytes_transferred: transferred,
        file: None,
        integrity_status,
    })
}

fn expected_size(info: &FileInfo) -> Result<u64, AppError> {
    if info.entry_type != "file" {
        return Err(AppError::InvalidInput("remote download file"));
    }
    u64::try_from(info.size).map_err(|_| AppError::InvalidInput("remote file size"))
}

async fn account_owner(api: &KoofrApi) -> Result<String, AppError> {
    api.recovery_scope().await
}
