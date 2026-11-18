use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use serde_json::Map;
use tauri::{AppHandle, Manager};
use tokio_util::sync::CancellationToken;

use crate::{
    error::AppError,
    file_ops::{LocalUploadPath, MountId, RemoteName, RemotePath},
    koofr_api::{FileInfo, KoofrApi},
    logging::AppLogger,
};

use super::{
    NetworkRetryPolicy, NetworkRetryRequest, SplitProgress, TransferDirection, TransferResult,
    TransferState,
    checkpoint::{SplitUploadCheckpoint, TransferCheckpoint, TransferCheckpointStore},
    emit_progress_with_split, emit_terminal,
    manager::{TransferManager, TransferRegistration},
    model::SplitPostProcessing,
    normalize_interruption, should_retry_network,
    split_package::{
        SplitManifest, SplitPart, package_directory_name, part_file_name, validate_part_bytes,
    },
    split_part_io::{committed_bytes, hash_file, hash_file_range, upload_next_part},
    split_support::upload_support_files,
    upload::modified_millis,
    wait_for_network_retry,
};

pub struct SplitTransferRuntime<'a> {
    pub app: AppHandle,
    pub api: &'a KoofrApi,
    pub manager: &'a TransferManager,
    pub checkpoints: &'a TransferCheckpointStore,
    pub retry_policy: NetworkRetryPolicy,
}

pub struct SplitUploadRequest {
    pub transfer_id: String,
    pub mount_id: MountId,
    pub directory: RemotePath,
    pub local_path: LocalUploadPath,
    pub package_name: Option<RemoteName>,
    pub part_bytes: u64,
}

pub fn validate_split_part_bytes(part_bytes: u64) -> Result<u64, AppError> {
    validate_part_bytes(part_bytes)
}

pub async fn upload_split(
    runtime: SplitTransferRuntime<'_>,
    request: SplitUploadRequest,
) -> Result<TransferResult, AppError> {
    let metadata = tokio::fs::metadata(request.local_path.as_path()).await?;
    if metadata.len() == 0 {
        return Err(AppError::InvalidInput("empty split upload"));
    }
    let transfer_uuid = uuid::Uuid::parse_str(&request.transfer_id)
        .map_err(|_| AppError::InvalidInput("transfer id"))?;
    let package_name = match request.package_name {
        Some(name) => name,
        None => RemoteName::parse(package_directory_name(
            &request.local_path.file_name()?,
            transfer_uuid,
        ))?,
    };
    let part_bytes = validate_part_bytes(request.part_bytes)?;
    let package_path = request.directory.join(&package_name)?;
    let checkpoint = SplitUploadCheckpoint {
        transfer_id: request.transfer_id,
        owner_id: runtime.api.recovery_scope().await?,
        mount_id: request.mount_id.as_str().to_owned(),
        remote_directory: request.directory.as_str().to_owned(),
        package_path: package_path.as_str().to_owned(),
        local_path: request.local_path.as_path().to_path_buf(),
        expected_size: metadata.len(),
        modified_millis: modified_millis(&metadata)?,
        chunk_size: part_bytes,
        completed_chunks: Vec::new(),
        file_sha256: None,
        retained_at: 0,
    };
    runtime
        .checkpoints
        .insert(TransferCheckpoint::SplitUpload(checkpoint.clone()))
        .await?;
    let manager = runtime.manager;
    let registration = manager.register_scoped(&checkpoint.transfer_id)?;
    let cancel = registration.token().clone();
    let preparation = prepare_new_package(
        &runtime,
        &cancel,
        &request.mount_id,
        &request.directory,
        &package_name,
        &package_path,
        &checkpoint,
    )
    .await;
    if let Err(error) = preparation {
        let paused = registration.finish();
        let result = normalize_interruption(Err(error), paused);
        if matches!(
            &result,
            Err(AppError::InvalidInput(_) | AppError::Conflict | AppError::Forbidden)
        ) {
            let _ = runtime.checkpoints.remove(&checkpoint.transfer_id).await;
        }
        emit_terminal(
            &runtime.app,
            &checkpoint.transfer_id,
            TransferDirection::Upload,
            0,
            &result,
        );
        return result;
    }
    run_registered(runtime, checkpoint, registration).await
}

pub async fn resume_split_upload(
    runtime: SplitTransferRuntime<'_>,
    transfer_id: String,
) -> Result<TransferResult, AppError> {
    let TransferCheckpoint::SplitUpload(checkpoint) = runtime.checkpoints.get(&transfer_id).await?
    else {
        return Err(AppError::InvalidInput("split upload checkpoint"));
    };
    verify_source(&checkpoint).await?;
    validate_part_bytes(checkpoint.chunk_size)?;
    run(runtime, checkpoint).await
}

async fn run(
    runtime: SplitTransferRuntime<'_>,
    checkpoint: SplitUploadCheckpoint,
) -> Result<TransferResult, AppError> {
    let manager = runtime.manager;
    let registration = manager.register_scoped(&checkpoint.transfer_id)?;
    run_registered(runtime, checkpoint, registration).await
}

async fn run_registered(
    runtime: SplitTransferRuntime<'_>,
    mut checkpoint: SplitUploadCheckpoint,
    registration: TransferRegistration<'_>,
) -> Result<TransferResult, AppError> {
    let cancel = registration.token().clone();
    emit_progress_with_split(
        &runtime.app,
        &checkpoint.transfer_id,
        TransferDirection::Upload,
        TransferState::Queued,
        committed_bytes(&checkpoint),
        Some(checkpoint.expected_size),
        Some(split_progress(&checkpoint)?),
    );
    let _permit = match runtime
        .manager
        .acquire(TransferDirection::Upload, &cancel)
        .await
    {
        Ok(permit) => permit,
        Err(error) => {
            let paused = registration.finish();
            let result = normalize_interruption(Err(error), paused);
            emit_terminal(
                &runtime.app,
                &checkpoint.transfer_id,
                TransferDirection::Upload,
                committed_bytes(&checkpoint),
                &result,
            );
            return result;
        }
    };
    let _ = runtime
        .checkpoints
        .mark_active(&checkpoint.transfer_id)
        .await;
    let mut retries_completed = 0_u32;
    let result = loop {
        let result = run_inner(&runtime, &cancel, &mut checkpoint).await;
        if !should_retry_network(&result, runtime.retry_policy, retries_completed) {
            break result;
        }
        retries_completed = retries_completed.saturating_add(1);
        if let Err(error) = wait_for_network_retry(NetworkRetryRequest {
            app: &runtime.app,
            cancel: &cancel,
            transfer_id: &checkpoint.transfer_id,
            direction: TransferDirection::Upload,
            retry_attempt: retries_completed,
            bytes_transferred: committed_bytes(&checkpoint),
            total_bytes: Some(checkpoint.expected_size),
            split_progress: Some(split_progress(&checkpoint)?),
            policy: runtime.retry_policy,
        })
        .await
        {
            break Err(error);
        }
    };
    let paused = registration.finish();
    let committed = checkpoint
        .completed_chunks
        .iter()
        .map(|part| part.size)
        .sum();
    let result = match result {
        Ok(result) => {
            let _ = runtime.checkpoints.remove(&checkpoint.transfer_id).await;
            Ok(result)
        }
        Err(AppError::InvalidInput(reason)) => Err(AppError::InvalidInput(reason)),
        Err(AppError::Conflict) => Err(AppError::Conflict),
        Err(AppError::Forbidden) => Err(AppError::Forbidden),
        other => {
            let result = normalize_interruption(other, paused);
            if !matches!(result, Err(AppError::TransferPaused)) {
                let _ = runtime
                    .checkpoints
                    .mark_failed(&checkpoint.transfer_id, true)
                    .await;
            }
            result
        }
    };
    emit_terminal(
        &runtime.app,
        &checkpoint.transfer_id,
        TransferDirection::Upload,
        committed,
        &result,
    );
    result
}

async fn prepare_new_package(
    runtime: &SplitTransferRuntime<'_>,
    cancel: &CancellationToken,
    mount_id: &MountId,
    directory: &RemotePath,
    package_name: &RemoteName,
    package_path: &RemotePath,
    checkpoint: &SplitUploadCheckpoint,
) -> Result<(), AppError> {
    let mut retries_completed = 0_u32;
    let mut creation_attempted = false;
    loop {
        let result = prepare_new_package_once(
            runtime.api,
            mount_id,
            directory,
            package_name,
            package_path,
            &mut creation_attempted,
        )
        .await;
        if !should_retry_network(&result, runtime.retry_policy, retries_completed) {
            return result;
        }
        retries_completed = retries_completed.saturating_add(1);
        wait_for_network_retry(NetworkRetryRequest {
            app: &runtime.app,
            cancel,
            transfer_id: &checkpoint.transfer_id,
            direction: TransferDirection::Upload,
            retry_attempt: retries_completed,
            bytes_transferred: 0,
            total_bytes: Some(checkpoint.expected_size),
            split_progress: Some(split_progress(checkpoint)?),
            policy: runtime.retry_policy,
        })
        .await?;
    }
}

async fn prepare_new_package_once(
    api: &KoofrApi,
    mount_id: &MountId,
    directory: &RemotePath,
    package_name: &RemoteName,
    package_path: &RemotePath,
    creation_attempted: &mut bool,
) -> Result<(), AppError> {
    match api.file_info(mount_id, package_path).await {
        Ok(info) if *creation_attempted && info.entry_type == "dir" => Ok(()),
        Ok(_) => Err(AppError::Conflict),
        Err(AppError::NotFound) => {
            *creation_attempted = true;
            api.create_folder(mount_id, directory, package_name).await
        }
        Err(error) => Err(error),
    }
}

async fn run_inner(
    runtime: &SplitTransferRuntime<'_>,
    cancel: &CancellationToken,
    checkpoint: &mut SplitUploadCheckpoint,
) -> Result<TransferResult, AppError> {
    let state = runtime.app.state::<crate::AppState>();
    let logger = &state.logger;
    let transfer_id = checkpoint.transfer_id.clone();
    let mount_id = MountId::parse(checkpoint.mount_id.clone())?;
    let package_path = RemotePath::parse(checkpoint.package_path.clone())?;
    logger
        .trace(
            "transfer",
            "split_check_package",
            Some(&transfer_id),
            ensure_package(runtime.api, &mount_id, &package_path),
        )
        .await?;
    let before = checkpoint.completed_chunks.len();
    logger
        .trace(
            "transfer",
            "split_reconcile_parts",
            Some(&transfer_id),
            reconcile_parts(runtime, cancel, &mount_id, &package_path, checkpoint),
        )
        .await?;
    logger.debug(
        "transfer",
        "split_parts_reconciled",
        Some(&transfer_id),
        Map::from_iter([
            ("previousParts".to_owned(), before.into()),
            (
                "confirmedParts".to_owned(),
                checkpoint.completed_chunks.len().into(),
            ),
            (
                "bytesTransferred".to_owned(),
                committed_bytes(checkpoint).into(),
            ),
        ]),
    );
    emit_progress_with_split(
        &runtime.app,
        &checkpoint.transfer_id,
        TransferDirection::Upload,
        TransferState::Running,
        committed_bytes(checkpoint),
        Some(checkpoint.expected_size),
        Some(split_progress(checkpoint)?),
    );

    while committed_bytes(checkpoint) < checkpoint.expected_size {
        let offset = committed_bytes(checkpoint);
        let span = logger.span(
            "transfer",
            "split_upload_part",
            Some(&transfer_id),
            Map::from_iter([
                (
                    "partNumber".to_owned(),
                    (checkpoint.completed_chunks.len() + 1).into(),
                ),
                (
                    "bytes".to_owned(),
                    (checkpoint.expected_size - offset)
                        .min(checkpoint.chunk_size)
                        .into(),
                ),
            ]),
        );
        let result = upload_next_part(runtime, cancel, &mount_id, &package_path, checkpoint).await;
        if let Some(span) = span {
            span.finish(result.as_ref().err(), Map::new());
        }
        result?;
    }
    logger.info(
        "transfer",
        "split_finalization_started",
        Some(&transfer_id),
        Map::from_iter([
            (
                "bytesTransferred".to_owned(),
                checkpoint.expected_size.into(),
            ),
            (
                "confirmedParts".to_owned(),
                checkpoint.completed_chunks.len().into(),
            ),
            (
                "hasChecksum".to_owned(),
                checkpoint.file_sha256.is_some().into(),
            ),
        ]),
    );
    let base_progress = split_progress(checkpoint)?;
    let total_bytes = checkpoint.expected_size;
    let report = |post_processing| {
        emit_progress_with_split(
            &runtime.app,
            &transfer_id,
            TransferDirection::Upload,
            TransferState::Running,
            total_bytes,
            Some(total_bytes),
            Some(SplitProgress {
                post_processing: Some(post_processing),
                ..base_progress
            }),
        );
    };
    let manifest = logger
        .trace(
            "transfer",
            "split_prepare_manifest",
            Some(&transfer_id),
            prepare_manifest(
                runtime.checkpoints,
                cancel,
                checkpoint,
                Some(logger),
                report,
            ),
        )
        .await?;
    let result = tokio::select! {
        biased;
        () = cancel.cancelled() => Err(AppError::Cancelled),
        result = upload_support_files(runtime.api, &mount_id, &package_path, &manifest, Some((logger, &transfer_id)), |completed_files, total_files| {
            report(SplitPostProcessing::SupportFiles { completed_files, total_files });
        }) => result,
    };
    result?;
    Ok(TransferResult {
        transfer_id: checkpoint.transfer_id.clone(),
        bytes_transferred: checkpoint.expected_size,
        file: Some(FileInfo {
            name: manifest.file_name,
            entry_type: "split_package".to_owned(),
            modified: 0,
            size: i64::try_from(checkpoint.expected_size).unwrap_or(i64::MAX),
            content_type: "application/octet-stream".to_owned(),
            hash: manifest.file_sha256,
            path: checkpoint.package_path.clone(),
        }),
        integrity_status: super::IntegrityStatus::NotRequested,
        local_path: None,
    })
}

async fn prepare_manifest(
    checkpoints: &TransferCheckpointStore,
    cancel: &CancellationToken,
    checkpoint: &mut SplitUploadCheckpoint,
    logger: Option<&AppLogger>,
    report: impl Fn(SplitPostProcessing),
) -> Result<SplitManifest, AppError> {
    verify_source(checkpoint).await?;
    let file_sha256 = match &checkpoint.file_sha256 {
        Some(hash) => hash.clone(),
        None => {
            let hash = hash_source(checkpoint, cancel, logger, report).await?;
            verify_source(checkpoint).await?;
            hash
        }
    };
    let manifest = SplitManifest::new(
        file_name(&checkpoint.local_path)?,
        checkpoint.expected_size,
        checkpoint.chunk_size,
        file_sha256,
        checkpoint.completed_chunks.clone(),
    )?;
    if checkpoint.file_sha256.is_none() {
        // Keep the checksum if a support file fails to upload after hashing.
        checkpoint.file_sha256 = Some(manifest.file_sha256.clone());
        let persist = checkpoints.insert(TransferCheckpoint::SplitUpload(checkpoint.clone()));
        if let Some(logger) = logger {
            logger
                .trace(
                    "transfer",
                    "split_save_checksum",
                    Some(&checkpoint.transfer_id),
                    persist,
                )
                .await?;
        } else {
            persist.await?;
        }
    }
    Ok(manifest)
}

async fn hash_source(
    checkpoint: &SplitUploadCheckpoint,
    cancel: &CancellationToken,
    logger: Option<&AppLogger>,
    report: impl Fn(SplitPostProcessing),
) -> Result<String, AppError> {
    let bytes_hashed = Arc::new(AtomicU64::new(0));
    let hash = hash_file(
        checkpoint.local_path.clone(),
        cancel.clone(),
        bytes_hashed.clone(),
    );
    let transfer_id = Some(checkpoint.transfer_id.as_str());
    let span = logger.and_then(|logger| {
        logger.span(
            "transfer",
            "split_hash_source",
            transfer_id,
            Map::from_iter([("totalBytes".to_owned(), checkpoint.expected_size.into())]),
        )
    });
    let started = Instant::now();
    let interval = Duration::from_secs(1);
    let mut heartbeat = tokio::time::interval(interval);
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_log_at = 0;
    tokio::pin!(hash);
    let result = loop {
        tokio::select! {
            result = &mut hash => break result,
            _ = heartbeat.tick() => {
                let bytes = bytes_hashed.load(Ordering::Relaxed).min(checkpoint.expected_size);
                let elapsed = started.elapsed().as_secs_f64();
                report(SplitPostProcessing::Hashing {
                    bytes_hashed: bytes,
                    total_bytes: checkpoint.expected_size,
                    remaining_seconds: (bytes > 0 && elapsed >= 5.0).then(|| {
                        ((checkpoint.expected_size - bytes) as f64 * elapsed / bytes as f64).ceil() as u64
                    }),
                });
                let elapsed = elapsed as u64;
                if elapsed.saturating_sub(last_log_at) >= 10 {
                    last_log_at = elapsed;
                    if let Some(logger) = logger {
                        logger.debug("transfer", "split_hash_progress", transfer_id, Map::from_iter([
                            ("bytesHashed".to_owned(), bytes.into()),
                            ("totalBytes".to_owned(), checkpoint.expected_size.into()),
                            ("elapsedSeconds".to_owned(), elapsed.into()),
                        ]));
                    }
                }
            }
        }
    };
    if let Some(span) = span {
        span.finish(
            result.as_ref().err(),
            Map::from_iter([(
                "bytesHashed".to_owned(),
                bytes_hashed.load(Ordering::Relaxed).into(),
            )]),
        );
    }
    result
}

async fn verify_source(checkpoint: &SplitUploadCheckpoint) -> Result<(), AppError> {
    let metadata = tokio::fs::metadata(&checkpoint.local_path).await?;
    if metadata.len() != checkpoint.expected_size
        || modified_millis(&metadata)? != checkpoint.modified_millis
    {
        return Err(AppError::Conflict);
    }
    Ok(())
}

pub(super) fn split_progress(
    checkpoint: &SplitUploadCheckpoint,
) -> Result<SplitProgress, AppError> {
    SplitProgress::new(
        checkpoint.expected_size,
        checkpoint.chunk_size,
        checkpoint.completed_chunks.len(),
    )
}

async fn ensure_package(
    api: &KoofrApi,
    mount_id: &MountId,
    package_path: &RemotePath,
) -> Result<(), AppError> {
    match api.file_info(mount_id, package_path).await {
        Ok(info) if info.entry_type == "dir" => Ok(()),
        Ok(_) => Err(AppError::Conflict),
        Err(AppError::NotFound) => {
            let parent = package_path.parent()?;
            let name = RemoteName::parse(package_path.file_name()?.to_owned())?;
            api.create_folder(mount_id, &parent, &name).await
        }
        Err(error) => Err(error),
    }
}

async fn reconcile_parts(
    runtime: &SplitTransferRuntime<'_>,
    cancel: &CancellationToken,
    mount_id: &MountId,
    package_path: &RemotePath,
    checkpoint: &mut SplitUploadCheckpoint,
) -> Result<(), AppError> {
    let remote = runtime.api.list_files(mount_id, package_path).await?;
    let mut parts = Vec::new();
    let mut covered = 0_u64;
    while covered < checkpoint.expected_size {
        let index =
            u32::try_from(parts.len()).map_err(|_| AppError::InvalidInput("split part index"))?;
        let expected = (checkpoint.expected_size - covered).min(checkpoint.chunk_size);
        let name = part_file_name(index);
        let Some(file) = remote.iter().find(|file| file.name == name) else {
            break;
        };
        if file.entry_type != "file" || u64::try_from(file.size).ok() != Some(expected) {
            return Err(AppError::Conflict);
        }
        let saved_hash = checkpoint
            .completed_chunks
            .iter()
            .find(|part| part.index == index)
            .map(|part| part.sha256.clone());
        let sha256 = match saved_hash {
            Some(hash) => hash,
            None => {
                hash_file_range(
                    checkpoint.local_path.clone(),
                    covered,
                    expected,
                    cancel.clone(),
                )
                .await?
            }
        };
        parts.push(SplitPart::new(index, expected, sha256));
        covered += expected;
    }
    checkpoint.completed_chunks = parts;
    runtime
        .checkpoints
        .insert(TransferCheckpoint::SplitUpload(checkpoint.clone()))
        .await
}

fn file_name(path: &std::path::Path) -> Result<String, AppError> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .ok_or(AppError::InvalidInput("local file name"))
}

#[cfg(test)]
mod tests {
    use httpmock::{Method::GET, Method::POST, MockServer};
    use serde_json::json;

    use super::*;
    use crate::transfer::split_package::{
        MIN_PART_BYTES, RESTORE_CMD_NAME, RESTORE_SH_NAME, restore_cmd,
    };

    #[tokio::test]
    async fn resumes_finalization_with_persisted_checksum_and_existing_support_file() {
        let directory =
            std::env::temp_dir().join(format!("koofr-finalize-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).expect("create test directory");
        let local_path = directory.join("archive.bin");
        std::fs::write(&local_path, b"abc").expect("write source");
        let metadata = std::fs::metadata(&local_path).expect("source metadata");
        let hash = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        let checkpoint = SplitUploadCheckpoint {
            transfer_id: uuid::Uuid::new_v4().to_string(),
            owner_id: "user-1".to_owned(),
            mount_id: "mount_1".to_owned(),
            remote_directory: "/".to_owned(),
            package_path: "/parts".to_owned(),
            local_path: local_path.clone(),
            expected_size: 3,
            modified_millis: modified_millis(&metadata).expect("source mtime"),
            chunk_size: MIN_PART_BYTES,
            completed_chunks: vec![SplitPart::new(0, 3, hash.to_owned())],
            file_sha256: None,
            retained_at: 0,
        };
        let checkpoint_path = directory.join("transfers.json");
        TransferCheckpointStore::load(checkpoint_path.clone())
            .insert(TransferCheckpoint::SplitUpload(checkpoint.clone()))
            .await
            .expect("save checkpoint without a full-file checksum");
        let store = TransferCheckpointStore::load(checkpoint_path.clone());
        let TransferCheckpoint::SplitUpload(mut checkpoint) = store
            .get(&checkpoint.transfer_id)
            .await
            .expect("load legacy checkpoint")
        else {
            panic!("expected split upload checkpoint");
        };
        let cancel = CancellationToken::new();
        let manifest = prepare_manifest(&store, &cancel, &mut checkpoint, None, |_| {})
            .await
            .expect("prepare manifest from legacy checkpoint");
        assert_eq!(manifest.file_sha256, hash);

        let server = MockServer::start_async().await;
        let api = KoofrApi::new(&server.base_url()).expect("create API client");
        api.set_test_session("test-session").await;
        let mount = MountId::parse("mount_1".to_owned()).expect("valid mount");
        let package = RemotePath::parse("/parts".to_owned()).expect("valid path");
        let list = server
            .mock_async(|when, then| {
                when.method(GET).path("/api/v2/mounts/mount_1/files/list");
                then.status(200).json_body(json!({ "files": [
                    { "name": RESTORE_CMD_NAME, "type": "file", "size": restore_cmd().len(), "modified": 0 }
                ] }));
            })
            .await;
        let existing = server
            .mock_async(|when, then| {
                when.method(GET)
                    .path("/content/api/v2/mounts/mount_1/files/get")
                    .query_param("path", "/parts/restore.cmd");
                then.status(200).body(restore_cmd());
            })
            .await;
        let failure = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/content/api/v2/mounts/mount_1/files/put")
                    .query_param("filename", RESTORE_SH_NAME);
                then.status(503);
            })
            .await;
        assert!(matches!(
            upload_support_files(&api, &mount, &package, &manifest, None, |_, _| {}).await,
            Err(AppError::RemoteStatus { status }) if status == reqwest::StatusCode::SERVICE_UNAVAILABLE
        ));
        failure.delete_async().await;

        let reloaded = TransferCheckpointStore::load(checkpoint_path);
        let TransferCheckpoint::SplitUpload(mut checkpoint) = reloaded
            .get(&checkpoint.transfer_id)
            .await
            .expect("load saved finalization checksum")
        else {
            panic!("expected split upload checkpoint");
        };
        assert_eq!(checkpoint.file_sha256.as_deref(), Some(hash));
        let resumed = prepare_manifest(&reloaded, &cancel, &mut checkpoint, None, |_| {})
            .await
            .expect("reuse saved checksum");
        assert_eq!(resumed, manifest);
        let uploads = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/content/api/v2/mounts/mount_1/files/put")
                    .query_param_missing("overwrite");
                then.status(200).json_body(json!({
                    "name": "support-file", "type": "file", "size": 1, "modified": 0
                }));
            })
            .await;
        upload_support_files(&api, &mount, &package, &resumed, None, |_, _| {})
            .await
            .expect("upload remaining support files");
        list.assert_calls_async(2).await;
        existing.assert_calls_async(2).await;
        uploads.assert_calls_async(4).await;

        std::fs::write(local_path, b"changed").expect("change source");
        assert!(matches!(
            prepare_manifest(&reloaded, &cancel, &mut checkpoint, None, |_| {}).await,
            Err(AppError::Conflict)
        ));
        std::fs::remove_dir_all(directory).expect("remove test directory");
    }
}
