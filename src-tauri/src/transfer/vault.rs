use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use futures_util::TryStreamExt;
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::{
    compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt},
    io::ReaderStream,
    sync::CancellationToken,
};

use crate::{
    crypto::VaultCipher,
    error::AppError,
    file_ops::{
        LocalDownloadPath, LocalUploadPath, MountId, RemoteName, RemotePath,
        publish_without_overwrite,
    },
    koofr_api::{FileInfo, KoofrApi, next_body_chunk},
    vault_core::VaultManager,
};

use super::{
    checkpoint::{
        TransferCheckpoint, TransferCheckpointStore, VaultDownloadCheckpoint, VaultUploadCheckpoint,
    },
    manager::TransferManager,
    model::{
        NetworkRetryPolicy, NetworkRetryRequest, TransferDirection, TransferResult, TransferState,
        UPLOAD_STALL_LIMIT, emit_progress, emit_terminal, normalize_interruption,
        should_retry_network, stall_watchdog, wait_for_network_retry,
    },
    range::{ResponseMode, response_mode},
    upload::settle_upload_checkpoint,
};

#[allow(clippy::too_many_arguments)]
pub async fn upload_vault_file(
    app: AppHandle,
    api: &KoofrApi,
    manager: &TransferManager,
    checkpoints: &TransferCheckpointStore,
    retry_policy: NetworkRetryPolicy,
    transfer_id: String,
    repo_id: String,
    mount_id: MountId,
    remote_directory: RemotePath,
    cipher: Arc<VaultCipher>,
    local_path: LocalUploadPath,
) -> Result<TransferResult, AppError> {
    let metadata = tokio::fs::metadata(local_path.as_path()).await?;
    let plain_size = metadata.len();
    checkpoints
        .insert(TransferCheckpoint::VaultUpload(VaultUploadCheckpoint {
            transfer_id: transfer_id.clone(),
            owner_id: super::current_owner(api).await?,
            repo_id,
            mount_id: mount_id.as_str().to_owned(),
            remote_directory: remote_directory.as_str().to_owned(),
            local_path: local_path.as_path().to_path_buf(),
            expected_size: plain_size,
            modified_millis: super::upload::modified_millis(&metadata)?,
            retained_at: 0,
        }))
        .await?;
    run_vault_upload(
        app,
        api,
        manager,
        checkpoints,
        retry_policy,
        transfer_id,
        mount_id,
        remote_directory,
        cipher,
        local_path,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn resume_vault_upload(
    app: AppHandle,
    api: &KoofrApi,
    manager: &TransferManager,
    checkpoints: &TransferCheckpointStore,
    vault: &VaultManager,
    retry_policy: NetworkRetryPolicy,
    checkpoint: VaultUploadCheckpoint,
) -> Result<TransferResult, AppError> {
    let cipher = vault
        .resume_download_target(
            &checkpoint.repo_id,
            &checkpoint.mount_id,
            &checkpoint.remote_directory,
        )
        .await?;
    let local_path = LocalUploadPath::from_selected(checkpoint.local_path).await?;
    let metadata = tokio::fs::metadata(local_path.as_path()).await?;
    if metadata.len() != checkpoint.expected_size
        || super::upload::modified_millis(&metadata)? != checkpoint.modified_millis
    {
        return Err(AppError::Conflict);
    }
    run_vault_upload(
        app,
        api,
        manager,
        checkpoints,
        retry_policy,
        checkpoint.transfer_id,
        MountId::parse(checkpoint.mount_id)?,
        RemotePath::parse(checkpoint.remote_directory)?,
        cipher,
        local_path,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn run_vault_upload(
    app: AppHandle,
    api: &KoofrApi,
    manager: &TransferManager,
    checkpoints: &TransferCheckpointStore,
    retry_policy: NetworkRetryPolicy,
    transfer_id: String,
    mount_id: MountId,
    remote_directory: RemotePath,
    cipher: Arc<VaultCipher>,
    local_path: LocalUploadPath,
) -> Result<TransferResult, AppError> {
    let plain_size = tokio::fs::metadata(local_path.as_path()).await?.len();
    let encrypted_size = VaultCipher::encrypted_size(plain_size)?;
    let plain_name = RemoteName::parse(local_path.file_name()?)?;
    let encrypted_name = RemoteName::parse(cipher.encrypt_name(plain_name.as_str()))?;
    let registration = manager.register_scoped(&transfer_id)?;
    let cancel = registration.token().clone();
    emit_progress(
        &app,
        &transfer_id,
        TransferDirection::Upload,
        TransferState::Queued,
        0,
        Some(plain_size),
    );
    let _permit = match manager.acquire(TransferDirection::Upload, &cancel).await {
        Ok(permit) => permit,
        Err(error) => {
            let paused = registration.finish();
            let result =
                settle_upload_checkpoint(checkpoints, &transfer_id, Err(error), paused).await;
            emit_terminal(&app, &transfer_id, TransferDirection::Upload, 0, &result);
            return result;
        }
    };
    let _ = checkpoints.mark_active(&transfer_id).await;
    let progress = Arc::new(AtomicU64::new(0));
    let mut retries_completed = 0;
    let result = loop {
        progress.store(0, Ordering::Relaxed);
        let result = upload_once(
            &app,
            api,
            &transfer_id,
            &cancel,
            &mount_id,
            &remote_directory,
            &encrypted_name,
            cipher.clone(),
            local_path.clone(),
            plain_size,
            encrypted_size,
            progress.clone(),
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
            total_bytes: Some(plain_size),
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
async fn upload_once(
    app: &AppHandle,
    api: &KoofrApi,
    transfer_id: &str,
    cancel: &CancellationToken,
    mount_id: &MountId,
    remote_directory: &RemotePath,
    encrypted_name: &RemoteName,
    cipher: Arc<VaultCipher>,
    local_path: LocalUploadPath,
    plain_size: u64,
    encrypted_size: u64,
    progress: Arc<AtomicU64>,
) -> Result<TransferResult, AppError> {
    let file = tokio::fs::File::open(local_path.as_path()).await?;
    let reader = cipher.encrypt_reader(file.compat()).compat();
    let app_for_stream = app.clone();
    let id_for_stream = transfer_id.to_owned();
    let cancel_for_stream = cancel.clone();
    let encrypted_progress = Arc::new(AtomicU64::new(0));
    let encrypted_progress_for_stream = encrypted_progress.clone();
    let progress_for_stream = progress.clone();
    emit_progress(
        app,
        transfer_id,
        TransferDirection::Upload,
        TransferState::Running,
        0,
        Some(plain_size),
    );
    let stream = ReaderStream::new(reader).and_then(move |chunk| {
        let app = app_for_stream.clone();
        let transfer_id = id_for_stream.clone();
        let cancel = cancel_for_stream.clone();
        let encrypted_progress = encrypted_progress_for_stream.clone();
        let progress = progress_for_stream.clone();
        async move {
            app.state::<crate::AppState>()
                .transfers
                .throttle(&transfer_id, chunk.len(), &cancel)
                .await
                .map_err(|_| std::io::Error::other("vault upload interrupted"))?;
            let encrypted = encrypted_progress
                .fetch_add(chunk.len() as u64, Ordering::Relaxed)
                .saturating_add(chunk.len() as u64);
            let plain = plaintext_progress(encrypted, plain_size);
            progress.store(plain, Ordering::Relaxed);
            emit_progress(
                &app,
                &transfer_id,
                TransferDirection::Upload,
                TransferState::Running,
                plain,
                Some(plain_size),
            );
            Ok(chunk)
        }
    });
    let body = reqwest::Body::wrap_stream(stream);
    tokio::select! {
        result = api.upload(mount_id, remote_directory, encrypted_name, body, encrypted_size) => {
            result?;
        }
        () = cancel.cancelled() => return Err(AppError::Cancelled),
        error = stall_watchdog(&encrypted_progress, UPLOAD_STALL_LIMIT) => return Err(error),
    }
    progress.store(plain_size, Ordering::Relaxed);
    Ok(TransferResult {
        transfer_id: transfer_id.to_owned(),
        bytes_transferred: plain_size,
        file: None,
        integrity_status: super::IntegrityStatus::NotRequested,
        local_path: None,
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn download_vault_file(
    app: AppHandle,
    api: &KoofrApi,
    manager: &TransferManager,
    checkpoints: &TransferCheckpointStore,
    retry_policy: NetworkRetryPolicy,
    transfer_id: String,
    owner_id: String,
    repo_id: String,
    mount_id: MountId,
    remote_path: RemotePath,
    cipher: Arc<VaultCipher>,
    local_path: LocalDownloadPath,
) -> Result<TransferResult, AppError> {
    let checkpoint = VaultDownloadCheckpoint {
        transfer_id: transfer_id.clone(),
        owner_id,
        repo_id,
        mount_id: mount_id.as_str().to_owned(),
        remote_path: remote_path.as_str().to_owned(),
        partial_path: local_path.resumable_temporary_path(&transfer_id)?,
        local_path: local_path.as_path().to_path_buf(),
        expected_size: 0,
        remote_hash: String::new(),
        remote_modified: 0,
        resume_allowed: true,
        retained_at: 0,
    };
    checkpoints
        .insert(TransferCheckpoint::VaultDownload(checkpoint.clone()))
        .await?;
    run_vault_download(
        app,
        api,
        manager,
        checkpoints,
        retry_policy,
        checkpoint,
        cipher,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn resume_vault_download(
    app: AppHandle,
    api: &KoofrApi,
    manager: &TransferManager,
    checkpoints: &TransferCheckpointStore,
    vault: &VaultManager,
    retry_policy: NetworkRetryPolicy,
    checkpoint: VaultDownloadCheckpoint,
) -> Result<TransferResult, AppError> {
    let cipher = vault
        .resume_download_target(
            &checkpoint.repo_id,
            &checkpoint.mount_id,
            &checkpoint.remote_path,
        )
        .await?;
    run_vault_download(
        app,
        api,
        manager,
        checkpoints,
        retry_policy,
        checkpoint,
        cipher,
    )
    .await
}

async fn run_vault_download(
    app: AppHandle,
    api: &KoofrApi,
    manager: &TransferManager,
    checkpoints: &TransferCheckpointStore,
    retry_policy: NetworkRetryPolicy,
    mut checkpoint: VaultDownloadCheckpoint,
    cipher: Arc<VaultCipher>,
) -> Result<TransferResult, AppError> {
    validate_paths(&checkpoint).await?;
    let transfer_id = checkpoint.transfer_id.clone();
    let initial_encrypted = partial_length(&checkpoint).await?;
    let initial_plain = decrypted_total(initial_encrypted).unwrap_or(0);
    let registration = manager.register_scoped(&transfer_id)?;
    let cancel = registration.token().clone();
    emit_progress(
        &app,
        &transfer_id,
        TransferDirection::Download,
        TransferState::Queued,
        initial_plain,
        decrypted_total(checkpoint.expected_size).ok(),
    );
    let permit = manager.acquire(TransferDirection::Download, &cancel).await;
    let encrypted_progress = Arc::new(AtomicU64::new(initial_encrypted));
    let plain_progress = Arc::new(AtomicU64::new(initial_plain));
    let verify_hash = app
        .state::<crate::AppState>()
        .settings
        .download_behavior()
        .await
        .verify_hash_after_download;
    let mut retries_completed = 0;
    let started = match permit {
        Ok(permit) => checkpoints.mark_active(&transfer_id).await.map(|()| permit),
        Err(error) => Err(error),
    };
    let result = match started {
        Err(error) => Err(error),
        Ok(_permit) => loop {
            let result = match refresh_remote(api, checkpoints, &mut checkpoint).await {
                Ok(()) => {
                    let total_plain = VaultCipher::decrypted_size(
                        i64::try_from(checkpoint.expected_size)
                            .map_err(|_| AppError::VaultCrypto)?,
                    )
                    .and_then(|size| u64::try_from(size).map_err(|_| AppError::VaultCrypto))?;
                    download_once(
                        &app,
                        api,
                        &cancel,
                        &checkpoint,
                        cipher.clone(),
                        encrypted_progress.clone(),
                        plain_progress.clone(),
                        total_plain,
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
            let total_plain = decrypted_total(checkpoint.expected_size).unwrap_or(0);
            if let Err(error) = wait_for_network_retry(NetworkRetryRequest {
                app: &app,
                cancel: &cancel,
                transfer_id: &transfer_id,
                direction: TransferDirection::Download,
                retry_attempt: retries_completed,
                bytes_transferred: plain_progress.load(Ordering::Relaxed),
                total_bytes: Some(total_plain),
                split_progress: None,
                policy: retry_policy,
            })
            .await
            {
                break Err(error);
            }
        },
    };
    let paused = registration.finish();
    let result = match result {
        Ok(result) => {
            let _ = checkpoints.remove(&transfer_id).await;
            if let Some(path) = &result.local_path
                && *path != checkpoint.local_path
            {
                let _ = app
                    .state::<crate::AppState>()
                    .download_history
                    .set_local_path(&transfer_id, path);
            }
            Ok(result)
        }
        Err(error @ AppError::VaultCrypto) => {
            let _ = tokio::fs::remove_file(&checkpoint.partial_path).await;
            let _ = checkpoints.remove(&transfer_id).await;
            Err(error)
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
                    let _ = checkpoints.mark_failed(&transfer_id, false).await;
                }
                Err(_) => {
                    let _ = checkpoints.mark_failed(&transfer_id, true).await;
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
        plain_progress.load(Ordering::Relaxed),
        &result,
    );
    result
}

async fn refresh_remote(
    api: &KoofrApi,
    checkpoints: &TransferCheckpointStore,
    checkpoint: &mut VaultDownloadCheckpoint,
) -> Result<(), AppError> {
    let mount_id = MountId::parse(checkpoint.mount_id.clone())?;
    let remote_path = RemotePath::parse(checkpoint.remote_path.clone())?;
    let info = api.file_info(&mount_id, &remote_path).await?;
    let size = encrypted_file_size(&info)?;
    let uninitialized = checkpoint.expected_size == 0
        && checkpoint.remote_modified == 0
        && checkpoint.remote_hash.is_empty();
    if uninitialized {
        truncate_partial(checkpoint).await?;
        checkpoint.expected_size = size;
        checkpoint.remote_hash = info.hash;
        checkpoint.remote_modified = info.modified;
        checkpoints
            .insert(TransferCheckpoint::VaultDownload(checkpoint.clone()))
            .await?;
    } else if remote_identity_changed(checkpoint, size, info.modified, &info.hash) {
        return Err(AppError::Conflict);
    }
    Ok(())
}

fn remote_identity_changed(
    checkpoint: &VaultDownloadCheckpoint,
    size: u64,
    modified: i64,
    hash: &str,
) -> bool {
    let hash_changed =
        !checkpoint.remote_hash.is_empty() && !hash.is_empty() && checkpoint.remote_hash != hash;
    checkpoint.expected_size != size || checkpoint.remote_modified != modified || hash_changed
}

#[allow(clippy::too_many_arguments)]
async fn download_once(
    app: &AppHandle,
    api: &KoofrApi,
    cancel: &CancellationToken,
    checkpoint: &VaultDownloadCheckpoint,
    cipher: Arc<VaultCipher>,
    encrypted_progress: Arc<AtomicU64>,
    plain_progress: Arc<AtomicU64>,
    total_plain: u64,
    verify_hash: bool,
) -> Result<TransferResult, AppError> {
    let mount_id = MountId::parse(checkpoint.mount_id.clone())?;
    let remote_path = RemotePath::parse(checkpoint.remote_path.clone())?;
    let offset = partial_length(checkpoint).await?;
    if offset < checkpoint.expected_size {
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
        let mut transferred = if mode == ResponseMode::Append {
            offset
        } else {
            0
        };
        let mut output = open_partial(checkpoint, mode).await?;
        encrypted_progress.store(transferred, Ordering::Relaxed);
        let initial_plain = plaintext_progress(transferred, total_plain);
        plain_progress.store(initial_plain, Ordering::Relaxed);
        emit_progress(
            app,
            &checkpoint.transfer_id,
            TransferDirection::Download,
            TransferState::Running,
            initial_plain,
            Some(total_plain),
        );
        let mut stream = response.bytes_stream();
        loop {
            let next = tokio::select! {
                item = next_body_chunk(&mut stream) => item?,
                () = cancel.cancelled() => return Err(AppError::Cancelled),
            };
            match next {
                Some(chunk) => {
                    app.state::<crate::AppState>()
                        .transfers
                        .throttle(&checkpoint.transfer_id, chunk.len(), cancel)
                        .await?;
                    output.write_all(&chunk).await?;
                    transferred = transferred.saturating_add(chunk.len() as u64);
                    encrypted_progress.store(transferred, Ordering::Relaxed);
                    let plain = plaintext_progress(transferred, total_plain);
                    plain_progress.store(plain, Ordering::Relaxed);
                    emit_progress(
                        app,
                        &checkpoint.transfer_id,
                        TransferDirection::Download,
                        TransferState::Running,
                        plain,
                        Some(total_plain),
                    );
                }
                None => break,
            }
        }
        if transferred != checkpoint.expected_size {
            return Err(AppError::IncompleteTransfer);
        }
        output.flush().await?;
        output.sync_all().await?;
    }
    let integrity_status = super::verify_file_integrity(
        &checkpoint.partial_path,
        &checkpoint.remote_hash,
        cancel,
        verify_hash,
    )
    .await?;
    let published = decrypt_to_destination(checkpoint, cipher, cancel, total_plain).await?;
    plain_progress.store(total_plain, Ordering::Relaxed);
    Ok(TransferResult {
        transfer_id: checkpoint.transfer_id.clone(),
        bytes_transferred: total_plain,
        file: None,
        integrity_status,
        local_path: Some(published),
    })
}

async fn decrypt_to_destination(
    checkpoint: &VaultDownloadCheckpoint,
    cipher: Arc<VaultCipher>,
    cancel: &CancellationToken,
    expected_plain: u64,
) -> Result<std::path::PathBuf, AppError> {
    let file_name = checkpoint
        .local_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(AppError::InvalidInput("vault local file name"))?;
    let temporary = checkpoint.local_path.with_file_name(format!(
        ".{file_name}.koofr-vault-plain-{}",
        checkpoint.transfer_id
    ));
    let input = tokio::fs::File::open(&checkpoint.partial_path).await?;
    let mut reader = cipher.decrypt_reader(input.compat()).compat();
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create_new(true);
    let mut output = match options.open(&temporary).await {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(AppError::Conflict);
        }
        Err(error) => return Err(error.into()),
    };
    let result = async {
        let mut written = 0_u64;
        let mut buffer = vec![0_u8; 64 * 1024];
        loop {
            let read = tokio::select! {
                result = reader.read(&mut buffer) => result.map_err(|_| AppError::VaultCrypto)?,
                () = cancel.cancelled() => return Err(AppError::Cancelled),
            };
            if read == 0 {
                break;
            }
            output.write_all(&buffer[..read]).await?;
            written = written.saturating_add(read as u64);
        }
        if written != expected_plain {
            return Err(AppError::VaultCrypto);
        }
        output.flush().await?;
        output.sync_all().await?;
        drop(output);
        // Publish the plaintext first: if that fails, the downloaded ciphertext is
        // still there and the transfer can be resumed without fetching it again.
        let published =
            publish_without_overwrite(temporary.clone(), checkpoint.local_path.clone()).await?;
        let _ = tokio::fs::remove_file(&checkpoint.partial_path).await;
        Ok(published)
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&temporary).await;
    }
    result
}

async fn validate_paths(checkpoint: &VaultDownloadCheckpoint) -> Result<(), AppError> {
    let local = LocalDownloadPath::from_selected(checkpoint.local_path.clone()).await?;
    if local.resumable_temporary_path(&checkpoint.transfer_id)? != checkpoint.partial_path {
        return Err(AppError::InvalidInput("vault partial path"));
    }
    let _ = partial_length(checkpoint).await?;
    Ok(())
}

async fn open_partial(
    checkpoint: &VaultDownloadCheckpoint,
    mode: ResponseMode,
) -> Result<tokio::fs::File, AppError> {
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create(true);
    match mode {
        ResponseMode::Append => {
            options.append(true);
        }
        ResponseMode::Restart => {
            options.truncate(true);
        }
    }
    Ok(options.open(&checkpoint.partial_path).await?)
}

async fn partial_length(checkpoint: &VaultDownloadCheckpoint) -> Result<u64, AppError> {
    match tokio::fs::symlink_metadata(&checkpoint.partial_path).await {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            if metadata.len() > checkpoint.expected_size && checkpoint.expected_size != 0 {
                truncate_partial(checkpoint).await?;
                Ok(0)
            } else {
                Ok(metadata.len())
            }
        }
        Ok(_) => Err(AppError::InvalidInput("vault partial path")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error.into()),
    }
}

async fn truncate_partial(checkpoint: &VaultDownloadCheckpoint) -> Result<(), AppError> {
    tokio::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&checkpoint.partial_path)
        .await?;
    Ok(())
}

fn encrypted_file_size(info: &FileInfo) -> Result<u64, AppError> {
    if info.entry_type != "file" {
        return Err(AppError::InvalidInput("vault download file"));
    }
    let size = u64::try_from(info.size).map_err(|_| AppError::VaultCrypto)?;
    let _ = decrypted_total(size)?;
    Ok(size)
}

pub(super) fn decrypted_total(encrypted: u64) -> Result<u64, AppError> {
    let encrypted = i64::try_from(encrypted).map_err(|_| AppError::VaultCrypto)?;
    let plain = VaultCipher::decrypted_size(encrypted)?;
    u64::try_from(plain).map_err(|_| AppError::VaultCrypto)
}

pub(super) fn plaintext_progress(encrypted: u64, total_plain: u64) -> u64 {
    const HEADER: u64 = 32;
    const BLOCK_DATA: u64 = 64 * 1024;
    const BLOCK_TAG: u64 = 16;
    const BLOCK: u64 = BLOCK_DATA + BLOCK_TAG;
    let body = encrypted.saturating_sub(HEADER);
    let blocks = body / BLOCK;
    let remainder = body % BLOCK;
    blocks
        .saturating_mul(BLOCK_DATA)
        .saturating_add(remainder.saturating_sub(BLOCK_TAG))
        .min(total_plain)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{plaintext_progress, remote_identity_changed};
    use crate::transfer::checkpoint::VaultDownloadCheckpoint;

    #[test]
    fn maps_rclone_ciphertext_progress_to_plaintext() {
        assert_eq!(plaintext_progress(0, 100_000), 0);
        assert_eq!(plaintext_progress(32, 100_000), 0);
        assert_eq!(plaintext_progress(32 + 16 + 123, 100_000), 123);
        assert_eq!(
            plaintext_progress(32 + 16 + 65_536 + 16 + 10, 100_000),
            65_546
        );
    }

    #[test]
    fn refuses_to_stitch_ciphertext_after_remote_identity_changes() {
        let checkpoint = VaultDownloadCheckpoint {
            transfer_id: "transfer-1".to_owned(),
            owner_id: "owner-1".to_owned(),
            repo_id: "repo-1".to_owned(),
            mount_id: "mount-1".to_owned(),
            remote_path: "/vault/cipher".to_owned(),
            local_path: PathBuf::from("plain.bin"),
            partial_path: PathBuf::from("cipher.part"),
            expected_size: 100,
            remote_hash: "hash-a".to_owned(),
            remote_modified: 10,
            resume_allowed: true,
            retained_at: 0,
        };
        assert!(!remote_identity_changed(&checkpoint, 100, 10, "hash-a"));
        assert!(remote_identity_changed(&checkpoint, 101, 10, "hash-a"));
        assert!(remote_identity_changed(&checkpoint, 100, 11, "hash-a"));
        assert!(remote_identity_changed(&checkpoint, 100, 10, "hash-b"));
    }
}
