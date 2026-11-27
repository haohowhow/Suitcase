use std::{
    cmp::Reverse,
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{
    error::AppError,
    transfer::{IntegrityStatus, RecoveryKind, TransferState},
};

const HISTORY_VERSION: u8 = 1;
const MAX_SPEED_SAMPLES: usize = 300;
const MIN_SAMPLE_INTERVAL_MS: u64 = 1000;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadLocalKind {
    File,
    Folder,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadSpeedSample {
    pub recorded_at: u64,
    pub bytes_transferred: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredDownloadHistoryItem {
    pub transfer_id: String,
    owner_id: String,
    pub name: String,
    pub state: TransferState,
    pub bytes_transferred: u64,
    pub total_bytes: Option<u64>,
    pub local_kind: DownloadLocalKind,
    pub recovery_kind: Option<RecoveryKind>,
    #[serde(default)]
    pub mount_id: Option<String>,
    pub remote_path: String,
    pub local_path: PathBuf,
    pub started_at: u64,
    pub finished_at: Option<u64>,
    pub speed_samples: Vec<DownloadSpeedSample>,
    #[serde(default)]
    pub integrity_status: IntegrityStatus,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadHistoryItem {
    pub transfer_id: String,
    pub name: String,
    pub state: TransferState,
    pub bytes_transferred: u64,
    pub total_bytes: Option<u64>,
    pub local_kind: DownloadLocalKind,
    pub recovery_kind: Option<RecoveryKind>,
    pub mount_id: Option<String>,
    pub remote_path: String,
    pub local_path: PathBuf,
    pub started_at: u64,
    pub finished_at: Option<u64>,
    pub speed_samples: Vec<DownloadSpeedSample>,
    pub integrity_status: IntegrityStatus,
}

impl From<&StoredDownloadHistoryItem> for DownloadHistoryItem {
    fn from(item: &StoredDownloadHistoryItem) -> Self {
        Self {
            transfer_id: item.transfer_id.clone(),
            name: item.name.clone(),
            state: item.state,
            bytes_transferred: item.bytes_transferred,
            total_bytes: item.total_bytes,
            local_kind: item.local_kind,
            recovery_kind: item.recovery_kind,
            mount_id: item.mount_id.clone(),
            remote_path: item.remote_path.clone(),
            local_path: item.local_path.clone(),
            started_at: item.started_at,
            finished_at: item.finished_at,
            speed_samples: item.speed_samples.clone(),
            integrity_status: item.integrity_status,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredDownloadHistory {
    version: u8,
    items: BTreeMap<String, StoredDownloadHistoryItem>,
}

impl Default for StoredDownloadHistory {
    fn default() -> Self {
        Self {
            version: HISTORY_VERSION,
            items: BTreeMap::new(),
        }
    }
}

pub struct NewDownloadHistoryItem<'a> {
    pub transfer_id: &'a str,
    pub owner_id: &'a str,
    pub name: &'a str,
    pub mount_id: Option<&'a str>,
    pub remote_path: &'a str,
    pub local_path: &'a Path,
    pub local_kind: DownloadLocalKind,
    pub recovery_kind: Option<RecoveryKind>,
}

pub struct DownloadHistoryStore {
    path: PathBuf,
    state: Mutex<StoredDownloadHistory>,
    policy: Mutex<(u32, u32)>,
}

impl DownloadHistoryStore {
    pub fn load(path: PathBuf) -> Self {
        let state = crate::atomic_file::read_json_or_quarantine::<StoredDownloadHistory, _>(
            &path,
            "download_history",
            |stored| stored.version == HISTORY_VERSION,
        )
        .unwrap_or_default();
        Self {
            path,
            state: Mutex::new(state),
            policy: Mutex::new((30, 500)),
        }
    }

    pub fn start(&self, item: NewDownloadHistoryItem<'_>) -> Result<(), AppError> {
        validate_transfer_id(item.transfer_id)?;
        validate_owner_id(item.owner_id)?;
        let now = now_millis();
        let history_item = StoredDownloadHistoryItem {
            transfer_id: item.transfer_id.to_owned(),
            owner_id: item.owner_id.to_owned(),
            name: item.name.to_owned(),
            state: TransferState::Running,
            bytes_transferred: 0,
            total_bytes: None,
            local_kind: item.local_kind,
            recovery_kind: item.recovery_kind,
            mount_id: item.mount_id.map(str::to_owned),
            remote_path: item.remote_path.to_owned(),
            local_path: item.local_path.to_path_buf(),
            started_at: now,
            finished_at: None,
            speed_samples: vec![DownloadSpeedSample {
                recorded_at: now,
                bytes_transferred: 0,
            }],
            integrity_status: IntegrityStatus::NotRequested,
        };
        let mut state = self.state();
        state
            .items
            .insert(item.transfer_id.to_owned(), history_item);
        self.prune(&mut state.items);
        self.persist(&state)
    }

    pub fn record_progress(
        &self,
        transfer_id: &str,
        state_value: TransferState,
        bytes_transferred: u64,
        total_bytes: Option<u64>,
    ) {
        let now = now_millis();
        let mut state = self.state();
        let Some(item) = state.items.get_mut(transfer_id) else {
            return;
        };
        item.state = state_value;
        item.bytes_transferred = bytes_transferred;
        if total_bytes.is_some() {
            item.total_bytes = total_bytes;
        }
        let should_sample = item.speed_samples.last().is_none_or(|sample| {
            now.saturating_sub(sample.recorded_at) >= MIN_SAMPLE_INTERVAL_MS
                || is_terminal(state_value)
        });
        if should_sample {
            item.speed_samples.push(DownloadSpeedSample {
                recorded_at: now,
                bytes_transferred,
            });
            if item.speed_samples.len() > MAX_SPEED_SAMPLES {
                item.speed_samples.remove(1);
            }
        }
        if is_terminal(state_value) {
            if state_value != TransferState::Paused {
                item.finished_at = Some(now);
            }
            if state_value != TransferState::Paused && state_value != TransferState::Failed {
                item.recovery_kind = None;
            }
            self.prune(&mut state.items);
            let _ = self.persist(&state);
        }
    }

    pub fn record_integrity(&self, transfer_id: &str, integrity_status: IntegrityStatus) {
        if let Some(item) = self.state().items.get_mut(transfer_id) {
            item.integrity_status = integrity_status;
        }
    }

    /// Records where a download was actually published. The name can differ from
    /// the planned one when another file took it while the transfer was running.
    pub fn set_local_path(&self, transfer_id: &str, local_path: &Path) -> Result<(), AppError> {
        validate_transfer_id(transfer_id)?;
        let mut state = self.state();
        let Some(item) = state.items.get_mut(transfer_id) else {
            return Ok(());
        };
        if item.local_path == local_path {
            return Ok(());
        }
        item.local_path = local_path.to_path_buf();
        self.persist(&state)
    }

    pub fn set_policy(&self, keep_days: u32, keep_count: u32) -> Result<usize, AppError> {
        *lock_mutex(&self.policy) = (keep_days, keep_count);
        let mut state = self.state();
        let before = state.items.len();
        self.prune(&mut state.items);
        let removed = before.saturating_sub(state.items.len());
        if removed > 0 {
            self.persist(&state)?;
        }
        Ok(removed)
    }

    pub fn list(&self, owner_id: &str) -> Result<Vec<DownloadHistoryItem>, AppError> {
        validate_owner_id(owner_id)?;
        let mut items = self
            .state()
            .items
            .values()
            .filter(|item| item.owner_id == owner_id)
            .map(DownloadHistoryItem::from)
            .collect::<Vec<_>>();
        items.sort_by_key(|item| Reverse(item.started_at));
        Ok(items)
    }

    pub fn reconcile_interrupted(
        &self,
        owner_id: &str,
        resumable: &BTreeMap<String, RecoveryKind>,
        is_active: impl Fn(&str) -> bool,
    ) -> Result<(), AppError> {
        validate_owner_id(owner_id)?;
        let mut state = self.state();
        let mut changed = false;
        for item in state.items.values_mut().filter(|item| {
            item.owner_id == owner_id
                && !is_active(&item.transfer_id)
                && matches!(
                    item.state,
                    TransferState::Queued
                        | TransferState::Running
                        | TransferState::Retrying
                        | TransferState::Paused
                )
        }) {
            // A paused entry without a checkpoint (a folder download, or one whose
            // checkpoint is gone) can never be resumed. Leaving it paused would keep
            // it in the list for good, because only finished entries can be cleared.
            let (next_state, next_recovery) = match resumable.get(&item.transfer_id) {
                Some(recovery_kind) => (TransferState::Paused, Some(*recovery_kind)),
                None => (TransferState::Failed, None),
            };
            if item.state != next_state || item.recovery_kind != next_recovery {
                item.state = next_state;
                item.recovery_kind = next_recovery;
                changed = true;
            }
        }
        if changed {
            self.persist(&state)?;
        }
        Ok(())
    }

    pub fn clear_finished(&self, owner_id: &str) -> Result<usize, AppError> {
        validate_owner_id(owner_id)?;
        let mut state = self.state();
        let before = state.items.len();
        state.items.retain(|_, item| {
            item.owner_id != owner_id || !is_finished_for_clearing(item.state, item.recovery_kind)
        });
        let removed = before.saturating_sub(state.items.len());
        if removed > 0 {
            self.persist(&state)?;
        }
        Ok(removed)
    }

    pub fn remove(&self, owner_id: &str, transfer_id: &str) -> Result<bool, AppError> {
        validate_owner_id(owner_id)?;
        validate_transfer_id(transfer_id)?;
        let mut state = self.state();
        let removable = state
            .items
            .get(transfer_id)
            .is_some_and(|item| item.owner_id == owner_id);
        if !removable {
            return Ok(false);
        }
        state.items.remove(transfer_id);
        self.persist(&state)?;
        Ok(true)
    }

    pub fn completed_path(&self, owner_id: &str, transfer_id: &str) -> Result<PathBuf, AppError> {
        validate_owner_id(owner_id)?;
        validate_transfer_id(transfer_id)?;
        self.state()
            .items
            .get(transfer_id)
            .filter(|item| item.owner_id == owner_id && item.state == TransferState::Completed)
            .map(|item| item.local_path.clone())
            .ok_or(AppError::NotFound)
    }

    fn persist(&self, state: &StoredDownloadHistory) -> Result<(), AppError> {
        // Callers hold the state lock, which also serialises these writes.
        let payload = serde_json::to_vec_pretty(state).map_err(|_| AppError::LocalData)?;
        crate::atomic_file::write_atomic(&self.path, &payload).map_err(|_| AppError::LocalData)
    }

    fn state(&self) -> MutexGuard<'_, StoredDownloadHistory> {
        match self.state.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn prune(&self, items: &mut BTreeMap<String, StoredDownloadHistoryItem>) {
        let (keep_days, keep_count) = *lock_mutex(&self.policy);
        let now = now_millis();
        if keep_days > 0 {
            let cutoff = now.saturating_sub(u64::from(keep_days) * 24 * 60 * 60 * 1000);
            items.retain(|_, item| {
                !is_finished_for_clearing(item.state, item.recovery_kind)
                    || item.finished_at.is_none_or(|finished| finished >= cutoff)
            });
        }
        if keep_count == 0 {
            return;
        }
        let owners = items
            .values()
            .map(|item| item.owner_id.clone())
            .collect::<std::collections::BTreeSet<_>>();
        for owner in owners {
            let mut eligible = items
                .values()
                .filter(|item| {
                    item.owner_id == owner
                        && is_finished_for_clearing(item.state, item.recovery_kind)
                })
                .map(|item| (item.started_at, item.transfer_id.clone()))
                .collect::<Vec<_>>();
            eligible.sort_by_key(|(started_at, _)| Reverse(*started_at));
            for (_, transfer_id) in eligible.into_iter().skip(keep_count as usize) {
                items.remove(&transfer_id);
            }
        }
    }
}

fn lock_mutex<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(value) => value,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn is_terminal(state: TransferState) -> bool {
    matches!(
        state,
        TransferState::Paused
            | TransferState::Completed
            | TransferState::Cancelled
            | TransferState::Failed
    )
}

fn is_finished_for_clearing(state: TransferState, recovery_kind: Option<RecoveryKind>) -> bool {
    state == TransferState::Completed
        || state == TransferState::Cancelled
        || (state == TransferState::Failed && recovery_kind.is_none())
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
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

fn validate_owner_id(owner_id: &str) -> Result<(), AppError> {
    if owner_id.is_empty() || owner_id.len() > 256 || owner_id.contains('\0') {
        return Err(AppError::InvalidInput("download history owner"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{DownloadHistoryStore, DownloadLocalKind, NewDownloadHistoryItem};
    use crate::transfer::{RecoveryKind, TransferState};

    #[test]
    fn persists_completed_downloads_and_isolates_accounts() {
        let directory =
            std::env::temp_dir().join(format!("koofr-download-history-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).expect("create history directory");
        let path = directory.join("history.json");
        let transfer_id = uuid::Uuid::new_v4().to_string();
        let local_path = directory.join("report.pdf");
        let store = DownloadHistoryStore::load(path.clone());
        store
            .start(NewDownloadHistoryItem {
                transfer_id: &transfer_id,
                owner_id: "owner-a",
                name: "report.pdf",
                mount_id: Some("onedrive-mount"),
                remote_path: "/reports/report.pdf",
                local_path: &local_path,
                local_kind: DownloadLocalKind::File,
                recovery_kind: Some(RecoveryKind::ByteResume),
            })
            .expect("start history");
        store.record_progress(&transfer_id, TransferState::Completed, 1024, Some(1024));

        let reloaded = DownloadHistoryStore::load(path);
        let items = reloaded.list("owner-a").expect("list owner history");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].bytes_transferred, 1024);
        assert_eq!(items[0].mount_id.as_deref(), Some("onedrive-mount"));
        assert!(
            reloaded
                .list("owner-b")
                .expect("list other owner")
                .is_empty()
        );
        assert_eq!(
            reloaded
                .completed_path("owner-a", &transfer_id)
                .expect("completed path"),
            local_path
        );

        std::fs::remove_dir_all(directory).expect("remove history directory");
    }

    #[test]
    fn reconciles_interrupted_downloads_and_clears_non_resumable_failures() {
        let directory =
            std::env::temp_dir().join(format!("koofr-download-history-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).expect("create history directory");
        let store = DownloadHistoryStore::load(directory.join("history.json"));
        let resumable_id = uuid::Uuid::new_v4().to_string();
        let failed_id = uuid::Uuid::new_v4().to_string();
        for transfer_id in [&resumable_id, &failed_id] {
            store
                .start(NewDownloadHistoryItem {
                    transfer_id,
                    owner_id: "owner-a",
                    name: "archive.zip",
                    mount_id: None,
                    remote_path: "/archive.zip",
                    local_path: &directory.join("archive.zip"),
                    local_kind: DownloadLocalKind::File,
                    recovery_kind: Some(RecoveryKind::ByteResume),
                })
                .expect("start history");
        }
        let resumable = BTreeMap::from([(resumable_id.clone(), RecoveryKind::ByteResume)]);

        store
            .reconcile_interrupted("owner-a", &resumable, |_| false)
            .expect("reconcile history");
        let items = store.list("owner-a").expect("list history");
        let resumable_item = items
            .iter()
            .find(|item| item.transfer_id == resumable_id)
            .expect("resumable item");
        let failed_item = items
            .iter()
            .find(|item| item.transfer_id == failed_id)
            .expect("failed item");
        assert_eq!(resumable_item.state, TransferState::Paused);
        assert_eq!(failed_item.state, TransferState::Failed);
        assert!(failed_item.recovery_kind.is_none());
        assert_eq!(
            store
                .clear_finished("owner-a")
                .expect("clear failed history"),
            1
        );
        assert!(
            store
                .remove("owner-a", &resumable_id)
                .expect("remove resumable history")
        );
        assert!(
            store
                .list("owner-a")
                .expect("empty owner history")
                .is_empty()
        );

        std::fs::remove_dir_all(directory).expect("remove history directory");
    }

    #[test]
    fn a_paused_entry_without_checkpoint_becomes_clearable_but_running_ones_stay() {
        let directory =
            std::env::temp_dir().join(format!("koofr-download-history-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).expect("create history directory");
        let store = DownloadHistoryStore::load(directory.join("history.json"));
        let paused_folder = uuid::Uuid::new_v4().to_string();
        let running = uuid::Uuid::new_v4().to_string();
        for transfer_id in [&paused_folder, &running] {
            store
                .start(NewDownloadHistoryItem {
                    transfer_id,
                    owner_id: "owner-a",
                    name: "Photos",
                    mount_id: None,
                    remote_path: "/Photos",
                    local_path: &directory.join("Photos"),
                    local_kind: DownloadLocalKind::Folder,
                    recovery_kind: None,
                })
                .expect("start history");
        }
        store.record_progress(&paused_folder, TransferState::Paused, 10, None);
        store.record_progress(&running, TransferState::Running, 10, None);

        store
            .reconcile_interrupted("owner-a", &BTreeMap::new(), |id| id == running)
            .expect("reconcile history");

        let items = store.list("owner-a").expect("list history");
        let state_of = |id: &str| {
            items
                .iter()
                .find(|item| item.transfer_id == id)
                .map(|item| item.state)
        };
        assert_eq!(state_of(&paused_folder), Some(TransferState::Failed));
        assert_eq!(state_of(&running), Some(TransferState::Running));
        assert_eq!(store.clear_finished("owner-a").expect("clear"), 1);
        std::fs::remove_dir_all(directory).expect("remove history directory");
    }
}
