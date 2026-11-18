use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::{error::AppError, file_ops::local_path_key};

use super::{
    SplitProgress, TransferDirection, checkpoint_snapshot::snapshot, split_package::SplitPart,
};

const CHECKPOINT_VERSION: u8 = 2;
const MAX_CHECKPOINTS: usize = 128;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryKind {
    ByteResume,
    ChunkResume,
    Restart,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadCheckpoint {
    pub transfer_id: String,
    pub owner_id: String,
    pub mount_id: String,
    pub remote_path: String,
    pub local_path: PathBuf,
    pub partial_path: PathBuf,
    pub expected_size: u64,
    pub remote_hash: String,
    pub remote_modified: i64,
    #[serde(default = "default_true")]
    pub resume_allowed: bool,
    #[serde(default)]
    pub retained_at: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultDownloadCheckpoint {
    pub transfer_id: String,
    pub owner_id: String,
    pub repo_id: String,
    pub mount_id: String,
    pub remote_path: String,
    pub local_path: PathBuf,
    pub partial_path: PathBuf,
    pub expected_size: u64,
    pub remote_hash: String,
    pub remote_modified: i64,
    #[serde(default = "default_true")]
    pub resume_allowed: bool,
    #[serde(default)]
    pub retained_at: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultUploadCheckpoint {
    pub transfer_id: String,
    pub owner_id: String,
    pub repo_id: String,
    pub mount_id: String,
    pub remote_directory: String,
    pub local_path: PathBuf,
    pub expected_size: u64,
    pub modified_millis: u128,
    #[serde(default)]
    pub retained_at: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadCheckpoint {
    pub transfer_id: String,
    pub owner_id: String,
    pub mount_id: String,
    pub remote_directory: String,
    pub local_path: PathBuf,
    pub expected_size: u64,
    pub modified_millis: u128,
    #[serde(default)]
    pub retained_at: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitUploadCheckpoint {
    pub transfer_id: String,
    pub owner_id: String,
    pub mount_id: String,
    pub remote_directory: String,
    pub package_path: String,
    pub local_path: PathBuf,
    pub expected_size: u64,
    pub modified_millis: u128,
    pub chunk_size: u64,
    pub completed_chunks: Vec<SplitPart>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_sha256: Option<String>,
    #[serde(default)]
    pub retained_at: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "direction", content = "checkpoint", rename_all = "snake_case")]
pub enum TransferCheckpoint {
    Download(DownloadCheckpoint),
    VaultDownload(VaultDownloadCheckpoint),
    VaultUpload(VaultUploadCheckpoint),
    SplitUpload(SplitUploadCheckpoint),
    Upload(UploadCheckpoint),
}

impl TransferCheckpoint {
    fn transfer_id(&self) -> &str {
        match self {
            Self::Download(checkpoint) => &checkpoint.transfer_id,
            Self::VaultDownload(checkpoint) => &checkpoint.transfer_id,
            Self::VaultUpload(checkpoint) => &checkpoint.transfer_id,
            Self::SplitUpload(checkpoint) => &checkpoint.transfer_id,
            Self::Upload(checkpoint) => &checkpoint.transfer_id,
        }
    }

    pub fn owner_id(&self) -> &str {
        match self {
            Self::Download(checkpoint) => &checkpoint.owner_id,
            Self::VaultDownload(checkpoint) => &checkpoint.owner_id,
            Self::VaultUpload(checkpoint) => &checkpoint.owner_id,
            Self::SplitUpload(checkpoint) => &checkpoint.owner_id,
            Self::Upload(checkpoint) => &checkpoint.owner_id,
        }
    }

    fn resume_allowed(&self) -> bool {
        match self {
            Self::Download(checkpoint) => checkpoint.resume_allowed,
            Self::VaultDownload(checkpoint) => checkpoint.resume_allowed,
            _ => true,
        }
    }

    /// When the transfer last stopped without finishing; `0` while it is running
    /// or was paused by the user before any failure.
    fn retained_at(&self) -> u64 {
        match self {
            Self::Download(checkpoint) => checkpoint.retained_at,
            Self::VaultDownload(checkpoint) => checkpoint.retained_at,
            Self::VaultUpload(checkpoint) => checkpoint.retained_at,
            Self::SplitUpload(checkpoint) => checkpoint.retained_at,
            Self::Upload(checkpoint) => checkpoint.retained_at,
        }
    }

    fn set_retained_at(&mut self, retained_at: u64) {
        match self {
            Self::Download(checkpoint) => checkpoint.retained_at = retained_at,
            Self::VaultDownload(checkpoint) => checkpoint.retained_at = retained_at,
            Self::VaultUpload(checkpoint) => checkpoint.retained_at = retained_at,
            Self::SplitUpload(checkpoint) => checkpoint.retained_at = retained_at,
            Self::Upload(checkpoint) => checkpoint.retained_at = retained_at,
        }
    }

    /// An upload that can only start again from zero. Its checkpoint holds the
    /// request and no transferred data, so dropping it loses nothing but a shortcut.
    fn is_restart_only_upload(&self) -> bool {
        matches!(self, Self::Upload(_) | Self::VaultUpload(_))
    }

    fn is_upload(&self) -> bool {
        matches!(
            self,
            Self::Upload(_) | Self::VaultUpload(_) | Self::SplitUpload(_)
        )
    }

    /// The final local file a download will publish, which no other download may
    /// be given while this one can still finish.
    fn download_target(&self) -> Option<&Path> {
        match self {
            Self::Download(checkpoint) => Some(&checkpoint.local_path),
            Self::VaultDownload(checkpoint) => Some(&checkpoint.local_path),
            _ => None,
        }
    }

    fn retention(&self) -> Option<(u64, PathBuf)> {
        match self {
            Self::Download(checkpoint) if checkpoint.retained_at > 0 => {
                Some((checkpoint.retained_at, checkpoint.partial_path.clone()))
            }
            Self::VaultDownload(checkpoint) if checkpoint.retained_at > 0 => {
                Some((checkpoint.retained_at, checkpoint.partial_path.clone()))
            }
            _ => None,
        }
    }
}

const fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumableTransfer {
    pub transfer_id: String,
    pub name: String,
    pub direction: TransferDirection,
    pub recovery_kind: RecoveryKind,
    pub bytes_transferred: u64,
    pub total_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_progress: Option<SplitProgress>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredCheckpoints {
    version: u8,
    checkpoints: BTreeMap<String, TransferCheckpoint>,
}

impl Default for StoredCheckpoints {
    fn default() -> Self {
        Self {
            version: CHECKPOINT_VERSION,
            checkpoints: BTreeMap::new(),
        }
    }
}

#[derive(Clone)]
pub struct TransferCheckpointStore {
    path: PathBuf,
    state: Arc<RwLock<StoredCheckpoints>>,
    persist_lock: Arc<tokio::sync::Mutex<()>>,
}

impl TransferCheckpointStore {
    pub fn load(path: PathBuf) -> Self {
        let mut state = crate::atomic_file::read_json_or_quarantine::<StoredCheckpoints, _>(
            &path,
            "transfer_checkpoints",
            |stored| stored.version == CHECKPOINT_VERSION,
        )
        .unwrap_or_default();
        // Nothing is running at start-up. Uploads saved by a version that did not
        // track this, or interrupted by a crash, would otherwise look active forever
        // and could never be chosen when room has to be made.
        let now = now_millis();
        for checkpoint in state.checkpoints.values_mut() {
            if checkpoint.is_upload() && checkpoint.retained_at() == 0 {
                checkpoint.set_retained_at(now);
            }
        }
        Self {
            path,
            state: Arc::new(RwLock::new(state)),
            persist_lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    pub async fn insert(&self, checkpoint: TransferCheckpoint) -> Result<(), AppError> {
        let transfer_id = checkpoint.transfer_id().to_owned();
        validate_transfer_id(&transfer_id)?;
        {
            let mut state = self.state.write().await;
            if state.checkpoints.len() >= MAX_CHECKPOINTS
                && !state.checkpoints.contains_key(&transfer_id)
            {
                // Make room instead of refusing every new transfer: drop the oldest
                // stopped upload that would have to start from zero anyway.
                let evicted = state
                    .checkpoints
                    .iter()
                    .filter(|(_, stored)| {
                        stored.is_restart_only_upload() && stored.retained_at() > 0
                    })
                    .min_by_key(|(_, stored)| stored.retained_at())
                    .map(|(id, _)| id.clone());
                match evicted {
                    Some(id) => {
                        state.checkpoints.remove(&id);
                    }
                    None => return Err(AppError::LocalData),
                }
            }
            state.checkpoints.insert(transfer_id, checkpoint);
        }
        self.persist().await
    }

    pub async fn get(&self, transfer_id: &str) -> Result<TransferCheckpoint, AppError> {
        validate_transfer_id(transfer_id)?;
        self.state
            .read()
            .await
            .checkpoints
            .get(transfer_id)
            .cloned()
            .ok_or(AppError::NotFound)
    }

    pub async fn remove(&self, transfer_id: &str) -> Result<bool, AppError> {
        validate_transfer_id(transfer_id)?;
        let removed = self
            .state
            .write()
            .await
            .checkpoints
            .remove(transfer_id)
            .is_some();
        if removed {
            self.persist().await?;
        }
        Ok(removed)
    }

    pub async fn list(&self, owner_id: &str) -> Result<Vec<ResumableTransfer>, AppError> {
        if owner_id.is_empty() || owner_id.len() > 256 || owner_id.contains('\0') {
            return Err(AppError::InvalidInput("checkpoint owner"));
        }
        let checkpoints = self
            .state
            .read()
            .await
            .checkpoints
            .values()
            .filter(|checkpoint| checkpoint.owner_id() == owner_id && checkpoint.resume_allowed())
            .cloned()
            .collect::<Vec<_>>();
        let mut snapshots = Vec::with_capacity(checkpoints.len());
        for checkpoint in checkpoints {
            // One unreadable partial file (a removed drive, a changed permission)
            // must not hide every other resumable transfer.
            if let Ok(resumable) = snapshot(checkpoint).await {
                snapshots.push(resumable);
            }
        }
        Ok(snapshots)
    }

    /// Final file names that unfinished downloads will publish, as comparison keys
    /// from [`local_path_key`]. A new download must not be given one of them.
    pub async fn reserved_download_targets(&self) -> HashSet<String> {
        self.state
            .read()
            .await
            .checkpoints
            .values()
            .filter_map(TransferCheckpoint::download_target)
            .map(local_path_key)
            .collect()
    }

    pub async fn mark_active(&self, transfer_id: &str) -> Result<(), AppError> {
        self.update_retention(transfer_id, true, 0).await
    }

    pub async fn mark_failed(
        &self,
        transfer_id: &str,
        resume_allowed: bool,
    ) -> Result<(), AppError> {
        self.update_retention(transfer_id, resume_allowed, now_millis())
            .await
    }

    async fn update_retention(
        &self,
        transfer_id: &str,
        resume_allowed: bool,
        retained_at: u64,
    ) -> Result<(), AppError> {
        validate_transfer_id(transfer_id)?;
        {
            let mut state = self.state.write().await;
            let checkpoint = state
                .checkpoints
                .get_mut(transfer_id)
                .ok_or(AppError::NotFound)?;
            match checkpoint {
                TransferCheckpoint::Download(checkpoint) => {
                    checkpoint.resume_allowed = resume_allowed;
                }
                TransferCheckpoint::VaultDownload(checkpoint) => {
                    checkpoint.resume_allowed = resume_allowed;
                }
                // Uploads can always be tried again; only their age is tracked.
                _ => {}
            }
            checkpoint.set_retained_at(retained_at);
        }
        self.persist().await
    }

    pub async fn cleanup_expired(&self, keep_days: u32) -> Result<usize, AppError> {
        let cutoff = now_millis().saturating_sub(u64::from(keep_days) * 86_400_000);
        let candidates = self
            .state
            .read()
            .await
            .checkpoints
            .values()
            .filter_map(|checkpoint| {
                checkpoint
                    .retention()
                    .filter(|(retained_at, _)| *retained_at <= cutoff)
                    .map(|(_, path)| {
                        (
                            checkpoint.transfer_id().to_owned(),
                            checkpoint.clone(),
                            path,
                        )
                    })
            })
            .collect::<Vec<_>>();
        let mut removed = 0;
        for (transfer_id, checkpoint, partial_path) in candidates {
            // Re-run the checkpoint snapshot validation before touching its exact path.
            if snapshot(checkpoint).await.is_err() {
                continue;
            }
            match tokio::fs::symlink_metadata(&partial_path).await {
                Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                    tokio::fs::remove_file(&partial_path).await?;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                _ => continue,
            }
            if self.remove(&transfer_id).await? {
                removed += 1;
            }
        }
        Ok(removed)
    }

    async fn persist(&self) -> Result<(), AppError> {
        // Several transfers start, checkpoint and finish at the same moment. The
        // snapshot is taken under the same lock that orders the writes, so they
        // cannot interleave and an older snapshot can never land last.
        let _guard = self.persist_lock.lock().await;
        let payload = serde_json::to_vec_pretty(&*self.state.read().await)
            .map_err(|_| AppError::LocalData)?;
        crate::atomic_file::write_atomic_async(self.path.clone(), payload)
            .await
            .map_err(|_| AppError::LocalData)
    }
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn validate_transfer_id(transfer_id: &str) -> Result<(), AppError> {
    let parsed =
        uuid::Uuid::parse_str(transfer_id).map_err(|_| AppError::InvalidInput("transfer id"))?;
    if parsed.is_nil() {
        return Err(AppError::InvalidInput("transfer id"));
    }
    Ok(())
}

#[cfg(test)]
#[path = "checkpoint_tests.rs"]
mod tests;
