use std::{
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    fs,
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[cfg(windows)]
use std::os::windows::fs::MetadataExt;

use filetime::FileTime;
use futures_util::{StreamExt, TryStreamExt};
use md5::{Digest, Md5};
use notify::{RecursiveMode, Watcher};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::Notify,
};
use tokio_util::{io::ReaderStream, sync::CancellationToken};

use crate::{
    AppState,
    error::AppError,
    file_ops::{MountId, RemoteName, RemotePath},
    koofr_api::KoofrApi,
    transfer::{TransferDirection, TransferManager},
};

const SYNC_EVENT: &str = "koofr://sync-state";
const REMOTE_SCAN_INTERVAL: Duration = Duration::from_secs(60);
const REMOTE_SCAN_CONCURRENCY: usize = 4;
const LOCAL_DEBOUNCE: Duration = Duration::from_secs(2);
const SYNC_PROGRESS_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncEndpointStats {
    pub file_count: u64,
    pub directory_count: u64,
    pub bytes: u64,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncDirectionStats {
    pub pending_items: u64,
    pub total_items: u64,
    pub completed_items: u64,
    pub pending_bytes: u64,
    pub total_bytes: u64,
    pub completed_bytes: u64,
    pub bytes_per_second: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncPairSummary {
    pub id: String,
    pub local_root: String,
    pub remote_path: String,
    pub status: String,
    pub paused: bool,
    pub pending_items: u64,
    pub pending_bytes: u64,
    pub completed_items: u64,
    pub total_items: u64,
    pub completed_bytes: u64,
    pub total_bytes: u64,
    pub last_synced_at: Option<i64>,
    pub issue_count: u64,
    pub last_error_code: Option<String>,
    pub local: SyncEndpointStats,
    pub remote: SyncEndpointStats,
    pub upload: SyncDirectionStats,
    pub download: SyncDirectionStats,
    pub last_scan_at: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncIssue {
    pub id: i64,
    pub pair_id: String,
    pub relative_path: String,
    pub code: String,
    pub message: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncStateEvent {
    pair_id: String,
    status: String,
    pending_items: u64,
    pending_bytes: u64,
    completed_items: u64,
    total_items: u64,
    completed_bytes: u64,
    total_bytes: u64,
    last_synced_at: Option<i64>,
    issue_count: u64,
    error_code: Option<String>,
    local: SyncEndpointStats,
    remote: SyncEndpointStats,
    upload: SyncDirectionStats,
    download: SyncDirectionStats,
    last_scan_at: Option<i64>,
}

#[derive(Clone, Debug)]
struct SyncPairRecord {
    id: String,
    owner_id: String,
    local_root: PathBuf,
    mount_id: MountId,
    remote_path: RemotePath,
    paused: bool,
    status: String,
    pending_items: u64,
    pending_bytes: u64,
    completed_items: u64,
    total_items: u64,
    completed_bytes: u64,
    total_bytes: u64,
    last_synced_at: Option<i64>,
    issue_count: u64,
    last_error_code: Option<String>,
    baseline: BTreeMap<String, SnapshotEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
struct SnapshotEntry {
    kind: String,
    local: Option<Identity>,
    remote: Option<Identity>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Identity {
    size: i64,
    modified: i64,
    #[serde(default)]
    hash: String,
}

#[derive(Clone, Debug)]
struct LocalEntry {
    kind: String,
    identity: Identity,
    path: PathBuf,
}

#[derive(Clone, Debug)]
struct RemoteEntry {
    kind: String,
    identity: Identity,
    path: RemotePath,
}

#[derive(Clone, Debug)]
enum Action {
    EnsureLocalDir {
        relative: String,
    },
    EnsureRemoteDir {
        relative: String,
    },
    Upload {
        relative: String,
        local: LocalEntry,
        expected_remote: Option<Identity>,
    },
    Download {
        relative: String,
        remote: RemoteEntry,
        expected_local: Option<Identity>,
    },
    DeleteLocal {
        relative: String,
        expected: Identity,
    },
    DeleteRemote {
        relative: String,
        remote: RemoteEntry,
    },
    Conflict {
        relative: String,
        local: LocalEntry,
        remote: RemoteEntry,
    },
    TypeConflict {
        relative: String,
    },
}

#[derive(Clone, Debug, Default)]
struct ScanResult {
    local: BTreeMap<String, LocalEntry>,
    remote: BTreeMap<String, RemoteEntry>,
}

#[derive(Clone, Debug)]
struct PairRuntime {
    cancel: CancellationToken,
    wake: Arc<Notify>,
    telemetry: Arc<Mutex<SyncTelemetry>>,
}

#[derive(Clone, Debug, Default)]
struct TelemetryDirection {
    total_items: u64,
    total_bytes: u64,
    completed_items: u64,
    completed_bytes: u64,
    active_bytes: u64,
    bytes_per_second: u64,
}

#[derive(Clone, Debug, Default)]
struct SyncTelemetry {
    local: SyncEndpointStats,
    remote: SyncEndpointStats,
    upload: TelemetryDirection,
    download: TelemetryDirection,
    last_scan_at: Option<i64>,
    last_sample_at: Option<std::time::Instant>,
    last_sample_upload: u64,
    last_sample_download: u64,
}

impl SyncTelemetry {
    fn begin_scanning(&mut self) {
        self.upload = TelemetryDirection::default();
        self.download = TelemetryDirection::default();
        self.last_sample_at = None;
        self.last_sample_upload = 0;
        self.last_sample_download = 0;
    }

    fn update_local_scan(&mut self, local: SyncEndpointStats) {
        self.local = local;
    }

    fn update_remote_scan(&mut self, remote: SyncEndpointStats) {
        self.remote = remote;
    }

    fn update_endpoints(&mut self, local: SyncEndpointStats, remote: SyncEndpointStats) {
        self.local = local;
        self.remote = remote;
        self.last_scan_at = Some(now_millis());
    }

    fn reset_for_scan(
        &mut self,
        local: SyncEndpointStats,
        remote: SyncEndpointStats,
        upload_items: u64,
        upload_bytes: u64,
        download_items: u64,
        download_bytes: u64,
    ) {
        self.local = local;
        self.remote = remote;
        self.upload = TelemetryDirection {
            total_items: upload_items,
            total_bytes: upload_bytes,
            ..TelemetryDirection::default()
        };
        self.download = TelemetryDirection {
            total_items: download_items,
            total_bytes: download_bytes,
            ..TelemetryDirection::default()
        };
        self.last_scan_at = Some(now_millis());
        self.last_sample_at = Some(std::time::Instant::now());
        self.last_sample_upload = 0;
        self.last_sample_download = 0;
    }

    fn record(&mut self, upload: bool, bytes: u64) {
        let direction = if upload {
            &mut self.upload
        } else {
            &mut self.download
        };
        direction.active_bytes = direction.active_bytes.saturating_add(bytes);
    }

    fn complete(&mut self, upload: bool, items: u64, bytes: u64) {
        let direction = if upload {
            &mut self.upload
        } else {
            &mut self.download
        };
        direction.active_bytes = direction.active_bytes.saturating_sub(bytes);
        direction.completed_items = direction.completed_items.saturating_add(items);
        direction.completed_bytes = direction.completed_bytes.saturating_add(bytes);
    }

    fn complete_action(
        &mut self,
        upload_items: u64,
        upload_bytes: u64,
        download_items: u64,
        download_bytes: u64,
    ) {
        self.complete(true, upload_items, upload_bytes);
        self.complete(false, download_items, download_bytes);
    }

    fn clear_active(&mut self) {
        self.upload.active_bytes = 0;
        self.download.active_bytes = 0;
        self.upload.bytes_per_second = 0;
        self.download.bytes_per_second = 0;
    }

    fn sample_rates(&mut self) {
        let now = std::time::Instant::now();
        let Some(previous) = self.last_sample_at.replace(now) else {
            return;
        };
        let elapsed = now.duration_since(previous).as_secs_f64();
        if elapsed <= 0.0 {
            return;
        }
        let upload = self
            .upload
            .completed_bytes
            .saturating_add(self.upload.active_bytes);
        let download = self
            .download
            .completed_bytes
            .saturating_add(self.download.active_bytes);
        self.upload.bytes_per_second =
            ((upload.saturating_sub(self.last_sample_upload) as f64) / elapsed).round() as u64;
        self.download.bytes_per_second =
            ((download.saturating_sub(self.last_sample_download) as f64) / elapsed).round() as u64;
        self.last_sample_upload = upload;
        self.last_sample_download = download;
    }

    fn direction_stats(direction: &TelemetryDirection) -> SyncDirectionStats {
        let transferred = direction
            .completed_bytes
            .saturating_add(direction.active_bytes);
        SyncDirectionStats {
            pending_items: direction
                .total_items
                .saturating_sub(direction.completed_items),
            total_items: direction.total_items,
            completed_items: direction.completed_items,
            pending_bytes: direction.total_bytes.saturating_sub(transferred),
            total_bytes: direction.total_bytes,
            completed_bytes: transferred,
            bytes_per_second: direction.bytes_per_second,
        }
    }

    fn public(
        &self,
    ) -> (
        SyncEndpointStats,
        SyncEndpointStats,
        SyncDirectionStats,
        SyncDirectionStats,
        Option<i64>,
    ) {
        (
            self.local.clone(),
            self.remote.clone(),
            Self::direction_stats(&self.upload),
            Self::direction_stats(&self.download),
            self.last_scan_at,
        )
    }
}

struct SyncStore {
    connection: Mutex<Connection>,
    device_id: String,
}

impl SyncStore {
    fn open(path: PathBuf) -> Result<Self, AppError> {
        let connection = Connection::open(path).map_err(|_| AppError::LocalData)?;
        connection
            .execute_batch(
                "PRAGMA journal_mode = WAL;
                 CREATE TABLE IF NOT EXISTS sync_pairs (
                   id TEXT PRIMARY KEY,
                   owner_id TEXT NOT NULL,
                   local_root TEXT NOT NULL,
                   mount_id TEXT NOT NULL,
                   remote_path TEXT NOT NULL,
                   paused INTEGER NOT NULL DEFAULT 0,
                   status TEXT NOT NULL DEFAULT 'initializing',
                   pending_items INTEGER NOT NULL DEFAULT 0,
                   pending_bytes INTEGER NOT NULL DEFAULT 0,
                   completed_items INTEGER NOT NULL DEFAULT 0,
                   total_items INTEGER NOT NULL DEFAULT 0,
                   completed_bytes INTEGER NOT NULL DEFAULT 0,
                   total_bytes INTEGER NOT NULL DEFAULT 0,
                   last_synced_at INTEGER,
                   issue_count INTEGER NOT NULL DEFAULT 0,
                   last_error_code TEXT,
                   baseline_json TEXT NOT NULL DEFAULT '{}'
                 );
                 CREATE INDEX IF NOT EXISTS sync_pairs_owner_idx ON sync_pairs(owner_id);
                 CREATE TABLE IF NOT EXISTS sync_issues (
                   id INTEGER PRIMARY KEY AUTOINCREMENT,
                   pair_id TEXT NOT NULL,
                   relative_path TEXT NOT NULL,
                   code TEXT NOT NULL,
                   message TEXT NOT NULL,
                   created_at INTEGER NOT NULL,
                   resolved INTEGER NOT NULL DEFAULT 0
                 );
                 CREATE INDEX IF NOT EXISTS sync_issues_pair_idx ON sync_issues(pair_id, resolved);
                 CREATE TABLE IF NOT EXISTS sync_operations (
                   id TEXT PRIMARY KEY,
                   pair_id TEXT NOT NULL,
                   operation TEXT NOT NULL,
                   relative_path TEXT NOT NULL,
                   state TEXT NOT NULL,
                   updated_at INTEGER NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS sync_metadata (
                   key TEXT PRIMARY KEY,
                   value TEXT NOT NULL
                 );",
            )
            .map_err(|_| AppError::LocalData)?;
        let device_id = connection
            .query_row(
                "SELECT value FROM sync_metadata WHERE key = 'device_id'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|_| AppError::LocalData)?;
        let device_id = if let Some(device_id) = device_id {
            device_id
        } else {
            let device_id = uuid::Uuid::new_v4().to_string();
            connection
                .execute(
                    "INSERT INTO sync_metadata (key, value) VALUES ('device_id', ?1)",
                    [&device_id],
                )
                .map_err(|_| AppError::LocalData)?;
            device_id
        };
        connection
            .execute(
                "UPDATE sync_operations SET state = 'retry', updated_at = ?1 WHERE state = 'running'",
                [now_millis()],
            )
            .map_err(|_| AppError::LocalData)?;
        Ok(Self {
            connection: Mutex::new(connection),
            device_id,
        })
    }

    fn device_id(&self) -> &str {
        &self.device_id
    }

    fn pairs_for_owner(&self, owner_id: &str) -> Result<Vec<SyncPairRecord>, AppError> {
        let connection = self.connection.lock().map_err(|_| AppError::LocalData)?;
        let mut statement = connection
            .prepare(
                "SELECT id, owner_id, local_root, mount_id, remote_path, paused, status,
                        pending_items, pending_bytes, completed_items, total_items,
                        completed_bytes, total_bytes, last_synced_at, issue_count,
                        last_error_code, baseline_json
                 FROM sync_pairs WHERE owner_id = ?1 ORDER BY id",
            )
            .map_err(|_| AppError::LocalData)?;
        let rows = statement
            .query_map([owner_id], decode_pair)
            .map_err(|_| AppError::LocalData)?;
        rows.map(|row| row.map_err(|_| AppError::LocalData))
            .collect()
    }

    fn pair(&self, pair_id: &str) -> Result<Option<SyncPairRecord>, AppError> {
        let connection = self.connection.lock().map_err(|_| AppError::LocalData)?;
        connection
            .query_row(
                "SELECT id, owner_id, local_root, mount_id, remote_path, paused, status,
                        pending_items, pending_bytes, completed_items, total_items,
                        completed_bytes, total_bytes, last_synced_at, issue_count,
                        last_error_code, baseline_json
                 FROM sync_pairs WHERE id = ?1",
                [pair_id],
                decode_pair,
            )
            .optional()
            .map_err(|_| AppError::LocalData)
    }

    fn insert_pair(&self, pair: &SyncPairRecord) -> Result<(), AppError> {
        let connection = self.connection.lock().map_err(|_| AppError::LocalData)?;
        connection
            .execute(
                "INSERT INTO sync_pairs
                 (id, owner_id, local_root, mount_id, remote_path, paused, status, baseline_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, 0, 'initializing', '{}')",
                params![
                    pair.id,
                    pair.owner_id,
                    pair.local_root.to_string_lossy().to_string(),
                    pair.mount_id.as_str(),
                    pair.remote_path.as_str(),
                ],
            )
            .map_err(|_| AppError::LocalData)?;
        Ok(())
    }

    fn delete_pair(&self, pair_id: &str) -> Result<(), AppError> {
        let connection = self.connection.lock().map_err(|_| AppError::LocalData)?;
        let transaction = connection
            .unchecked_transaction()
            .map_err(|_| AppError::LocalData)?;
        transaction
            .execute("DELETE FROM sync_issues WHERE pair_id = ?1", [pair_id])
            .map_err(|_| AppError::LocalData)?;
        transaction
            .execute("DELETE FROM sync_operations WHERE pair_id = ?1", [pair_id])
            .map_err(|_| AppError::LocalData)?;
        transaction
            .execute("DELETE FROM sync_pairs WHERE id = ?1", [pair_id])
            .map_err(|_| AppError::LocalData)?;
        transaction.commit().map_err(|_| AppError::LocalData)
    }

    fn set_paused(&self, pair_id: &str, paused: bool) -> Result<(), AppError> {
        let connection = self.connection.lock().map_err(|_| AppError::LocalData)?;
        connection
            .execute(
                "UPDATE sync_pairs SET paused = ?2, status = CASE WHEN ?2 = 1 THEN 'paused' ELSE 'initializing' END WHERE id = ?1",
                params![pair_id, i64::from(paused)],
            )
            .map_err(|_| AppError::LocalData)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn update_progress(
        &self,
        pair_id: &str,
        status: &str,
        pending_items: u64,
        pending_bytes: u64,
        completed_items: u64,
        total_items: u64,
        completed_bytes: u64,
        total_bytes: u64,
        last_synced_at: Option<i64>,
        error_code: Option<&str>,
    ) -> Result<(), AppError> {
        let connection = self.connection.lock().map_err(|_| AppError::LocalData)?;
        connection
            .execute(
                "UPDATE sync_pairs SET status = ?2, pending_items = ?3, pending_bytes = ?4,
                 completed_items = ?5, total_items = ?6, completed_bytes = ?7, total_bytes = ?8,
                 last_synced_at = COALESCE(?9, last_synced_at), last_error_code = ?10 WHERE id = ?1",
                params![
                    pair_id,
                    status,
                    pending_items as i64,
                    pending_bytes as i64,
                    completed_items as i64,
                    total_items as i64,
                    completed_bytes as i64,
                    total_bytes as i64,
                    last_synced_at,
                    error_code,
                ],
            )
            .map_err(|_| AppError::LocalData)?;
        Ok(())
    }

    fn save_baseline(
        &self,
        pair_id: &str,
        baseline: &BTreeMap<String, SnapshotEntry>,
        now: i64,
    ) -> Result<(), AppError> {
        let json = serde_json::to_string(baseline).map_err(|_| AppError::LocalData)?;
        let connection = self.connection.lock().map_err(|_| AppError::LocalData)?;
        let transaction = connection
            .unchecked_transaction()
            .map_err(|_| AppError::LocalData)?;
        transaction
            .execute(
                "UPDATE sync_pairs SET baseline_json = ?2, status = 'idle', last_synced_at = ?3,
                 pending_items = 0, pending_bytes = 0, completed_items = total_items,
                 completed_bytes = total_bytes, issue_count = 0, last_error_code = NULL WHERE id = ?1",
                params![pair_id, json, now],
            )
            .map_err(|_| AppError::LocalData)?;
        transaction
            .execute(
                "UPDATE sync_issues SET resolved = 1 WHERE pair_id = ?1 AND resolved = 0",
                [pair_id],
            )
            .map_err(|_| AppError::LocalData)?;
        transaction.commit().map_err(|_| AppError::LocalData)
    }

    fn add_issue(
        &self,
        pair_id: &str,
        relative_path: &str,
        code: &str,
        message: &str,
    ) -> Result<(), AppError> {
        let now = now_millis();
        let connection = self.connection.lock().map_err(|_| AppError::LocalData)?;
        let transaction = connection
            .unchecked_transaction()
            .map_err(|_| AppError::LocalData)?;
        transaction
            .execute(
                "INSERT INTO sync_issues (pair_id, relative_path, code, message, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![pair_id, relative_path, code, message, now],
            )
            .map_err(|_| AppError::LocalData)?;
        transaction
            .execute(
                "UPDATE sync_pairs SET issue_count = issue_count + 1, status = 'attention' WHERE id = ?1",
                [pair_id],
            )
            .map_err(|_| AppError::LocalData)?;
        transaction.commit().map_err(|_| AppError::LocalData)
    }

    fn issues(&self, pair_id: &str) -> Result<Vec<SyncIssue>, AppError> {
        let connection = self.connection.lock().map_err(|_| AppError::LocalData)?;
        let mut statement = connection
            .prepare(
                "SELECT id, pair_id, relative_path, code, message, created_at
                 FROM sync_issues WHERE pair_id = ?1 AND resolved = 0 ORDER BY id DESC",
            )
            .map_err(|_| AppError::LocalData)?;
        let rows = statement
            .query_map([pair_id], |row| {
                Ok(SyncIssue {
                    id: row.get(0)?,
                    pair_id: row.get(1)?,
                    relative_path: row.get(2)?,
                    code: row.get(3)?,
                    message: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })
            .map_err(|_| AppError::LocalData)?;
        rows.map(|row| row.map_err(|_| AppError::LocalData))
            .collect()
    }

    fn operation(
        &self,
        pair_id: &str,
        operation: &str,
        relative: &str,
    ) -> Result<String, AppError> {
        let id = uuid::Uuid::new_v4().to_string();
        let connection = self.connection.lock().map_err(|_| AppError::LocalData)?;
        connection
            .execute(
                "INSERT INTO sync_operations (id, pair_id, operation, relative_path, state, updated_at)
                 VALUES (?1, ?2, ?3, ?4, 'running', ?5)",
                params![id, pair_id, operation, relative, now_millis()],
            )
            .map_err(|_| AppError::LocalData)?;
        Ok(id)
    }

    fn finish_operation(&self, operation_id: &str, state: &str) -> Result<(), AppError> {
        let connection = self.connection.lock().map_err(|_| AppError::LocalData)?;
        connection
            .execute(
                "UPDATE sync_operations SET state = ?2, updated_at = ?3 WHERE id = ?1",
                params![operation_id, state, now_millis()],
            )
            .map_err(|_| AppError::LocalData)?;
        Ok(())
    }
}

fn decode_pair(row: &rusqlite::Row<'_>) -> rusqlite::Result<SyncPairRecord> {
    let mount_id: String = row.get(3)?;
    let remote_path: String = row.get(4)?;
    let baseline_json: String = row.get(16)?;
    Ok(SyncPairRecord {
        id: row.get(0)?,
        owner_id: row.get(1)?,
        local_root: PathBuf::from(row.get::<_, String>(2)?),
        mount_id: MountId::parse(mount_id).map_err(|_| rusqlite::Error::InvalidQuery)?,
        remote_path: RemotePath::parse(remote_path).map_err(|_| rusqlite::Error::InvalidQuery)?,
        paused: row.get::<_, i64>(5)? != 0,
        status: row.get(6)?,
        pending_items: row.get::<_, i64>(7)?.max(0) as u64,
        pending_bytes: row.get::<_, i64>(8)?.max(0) as u64,
        completed_items: row.get::<_, i64>(9)?.max(0) as u64,
        total_items: row.get::<_, i64>(10)?.max(0) as u64,
        completed_bytes: row.get::<_, i64>(11)?.max(0) as u64,
        total_bytes: row.get::<_, i64>(12)?.max(0) as u64,
        last_synced_at: row.get(13)?,
        issue_count: row.get::<_, i64>(14)?.max(0) as u64,
        last_error_code: row.get(15)?,
        baseline: serde_json::from_str(&baseline_json).unwrap_or_default(),
    })
}

#[derive(Clone)]
pub struct SyncManager {
    store: Arc<SyncStore>,
    runtimes: Arc<Mutex<HashMap<String, PairRuntime>>>,
}

impl SyncManager {
    pub fn load(path: PathBuf) -> Result<Self, AppError> {
        Ok(Self {
            store: Arc::new(SyncStore::open(path)?),
            runtimes: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn list_pairs(&self, owner_id: &str) -> Result<Vec<SyncPairSummary>, AppError> {
        self.store
            .pairs_for_owner(owner_id)?
            .into_iter()
            .map(|pair| {
                let telemetry = self
                    .runtimes
                    .lock()
                    .map_err(|_| AppError::LocalData)?
                    .get(&pair.id)
                    .map(|runtime| runtime.telemetry.clone());
                summary(pair, telemetry.as_deref())
            })
            .collect()
    }

    pub fn list_issues(&self, owner_id: &str, pair_id: &str) -> Result<Vec<SyncIssue>, AppError> {
        let pair = self.store.pair(pair_id)?.ok_or(AppError::NotFound)?;
        if pair.owner_id != owner_id {
            return Err(AppError::Forbidden);
        }
        self.store.issues(pair_id)
    }

    pub async fn create_pair(
        &self,
        app: AppHandle,
        owner_id: String,
        local_root: PathBuf,
        mount_id: MountId,
        remote_path: RemotePath,
    ) -> Result<SyncPairSummary, AppError> {
        validate_local_root(&local_root).await?;
        let state = app.state::<AppState>();
        let mounts = state.api.list_mounts().await?;
        let primary = mounts
            .iter()
            .find(|mount| mount.id == mount_id.as_str() && mount.is_primary && mount.online)
            .ok_or(AppError::Forbidden)?;
        if !primary.mount_type.eq_ignore_ascii_case("device")
            || primary.mount_type.eq_ignore_ascii_case("vault")
            || primary.is_shared
        {
            return Err(AppError::Forbidden);
        }
        let remote = state.api.file_info(&mount_id, &remote_path).await?;
        if !is_directory(&remote.entry_type) {
            return Err(AppError::InvalidInput("sync remote directory"));
        }
        for existing in self.store.pairs_for_owner(&owner_id)? {
            if roots_overlap(&existing.local_root, &local_root)
                || remote_roots_overlap(
                    &existing.mount_id,
                    &existing.remote_path,
                    &mount_id,
                    &remote_path,
                )
            {
                return Err(AppError::Conflict);
            }
        }
        let pair = SyncPairRecord {
            id: uuid::Uuid::new_v4().to_string(),
            owner_id,
            local_root,
            mount_id,
            remote_path,
            paused: false,
            status: "initializing".to_owned(),
            pending_items: 0,
            pending_bytes: 0,
            completed_items: 0,
            total_items: 0,
            completed_bytes: 0,
            total_bytes: 0,
            last_synced_at: None,
            issue_count: 0,
            last_error_code: None,
            baseline: BTreeMap::new(),
        };
        self.store.insert_pair(&pair)?;
        self.start_pair(app, &pair.id)?;
        summary(pair, None)
    }

    pub fn remove_pair(&self, owner_id: &str, pair_id: &str) -> Result<bool, AppError> {
        let Some(pair) = self.store.pair(pair_id)? else {
            return Ok(false);
        };
        if pair.owner_id != owner_id {
            return Err(AppError::Forbidden);
        }
        if let Some(runtime) = self
            .runtimes
            .lock()
            .map_err(|_| AppError::LocalData)?
            .remove(pair_id)
        {
            runtime.cancel.cancel();
        }
        self.store.delete_pair(pair_id)?;
        Ok(true)
    }

    pub fn set_paused(
        &self,
        owner_id: &str,
        pair_id: &str,
        paused: bool,
    ) -> Result<bool, AppError> {
        let Some(pair) = self.store.pair(pair_id)? else {
            return Ok(false);
        };
        if pair.owner_id != owner_id {
            return Err(AppError::Forbidden);
        }
        let Some(runtime) = self
            .runtimes
            .lock()
            .map_err(|_| AppError::LocalData)?
            .get(pair_id)
            .cloned()
        else {
            return Ok(false);
        };
        self.store.set_paused(pair_id, paused)?;
        if !paused {
            runtime.wake.notify_one();
        }
        Ok(true)
    }

    pub fn run_now(&self, owner_id: &str, pair_id: &str) -> Result<bool, AppError> {
        let Some(pair) = self.store.pair(pair_id)? else {
            return Ok(false);
        };
        if pair.owner_id != owner_id {
            return Err(AppError::Forbidden);
        }
        let runtime = self
            .runtimes
            .lock()
            .map_err(|_| AppError::LocalData)?
            .get(pair_id)
            .cloned();
        if let Some(runtime) = runtime {
            runtime.wake.notify_one();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn stop_all(&self) {
        if let Ok(mut runtimes) = self.runtimes.lock() {
            for runtime in runtimes.values() {
                runtime.cancel.cancel();
            }
            runtimes.clear();
        }
    }

    pub fn start_for_owner(&self, app: AppHandle, owner_id: String) -> Result<(), AppError> {
        for pair in self.store.pairs_for_owner(&owner_id)? {
            self.start_pair(app.clone(), &pair.id)?;
        }
        Ok(())
    }

    fn telemetry_for(&self, pair_id: &str) -> Result<Arc<Mutex<SyncTelemetry>>, AppError> {
        self.runtimes
            .lock()
            .map_err(|_| AppError::LocalData)?
            .get(pair_id)
            .map(|runtime| runtime.telemetry.clone())
            .ok_or(AppError::NotFound)
    }

    fn start_pair(&self, app: AppHandle, pair_id: &str) -> Result<(), AppError> {
        let pair = self.store.pair(pair_id)?.ok_or(AppError::NotFound)?;
        let mut runtimes = self.runtimes.lock().map_err(|_| AppError::LocalData)?;
        if runtimes.contains_key(pair_id) {
            return Ok(());
        }
        let cancel = CancellationToken::new();
        let wake = Arc::new(Notify::new());
        let (local, remote) = baseline_endpoint_stats(&pair.baseline);
        let telemetry = Arc::new(Mutex::new(SyncTelemetry {
            local,
            remote,
            last_scan_at: pair.last_synced_at,
            ..SyncTelemetry::default()
        }));
        let runtime = PairRuntime {
            cancel: cancel.clone(),
            wake: wake.clone(),
            telemetry: telemetry.clone(),
        };
        runtimes.insert(pair_id.to_owned(), runtime);
        drop(runtimes);

        let watcher_cancel = cancel.clone();
        let watcher_wake = wake.clone();
        let root = pair.local_root.clone();
        std::thread::spawn(move || {
            let (events_tx, events_rx) = std::sync::mpsc::channel();
            let mut watcher = match notify::recommended_watcher(move |event| {
                let _ = events_tx.send(event);
            }) {
                Ok(watcher) => watcher,
                Err(_) => return,
            };
            if watcher.watch(&root, RecursiveMode::Recursive).is_err() {
                return;
            }
            while !watcher_cancel.is_cancelled() {
                match events_rx.recv_timeout(Duration::from_millis(500)) {
                    Ok(Ok(_)) => watcher_wake.notify_one(),
                    Ok(Err(_)) => watcher_wake.notify_one(),
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        });

        let manager = self.clone();
        let pair_id = pair_id.to_owned();
        tauri::async_runtime::spawn(manager.run_pair_loop(app, pair_id, cancel, wake));
        Ok(())
    }

    fn run_pair_loop(
        self,
        app: AppHandle,
        pair_id: String,
        cancel: CancellationToken,
        wake: Arc<Notify>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        // Keep the large nested sync future off Tokio's comparatively small worker stack.
        Box::pin(async move {
            let mut remote_tick = tokio::time::interval(REMOTE_SCAN_INTERVAL);
            remote_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            // Consume Tokio's immediate first tick so startup performs exactly one scan.
            remote_tick.tick().await;
            let _ = self.run_once(&app, &pair_id, &cancel).await;
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = remote_tick.tick() => { let _ = self.run_once(&app, &pair_id, &cancel).await; },
                    _ = wake.notified() => {
                        tokio::time::sleep(LOCAL_DEBOUNCE).await;
                        if !cancel.is_cancelled() { let _ = self.run_once(&app, &pair_id, &cancel).await; }
                    }
                }
            }
            if let Ok(mut runtimes) = self.runtimes.lock() {
                runtimes.remove(&pair_id);
            }
        })
    }

    async fn scan_local_with_progress(
        &self,
        app: &AppHandle,
        pair_id: &str,
        root: PathBuf,
        known: BTreeMap<String, Identity>,
        telemetry: Arc<Mutex<SyncTelemetry>>,
    ) -> Result<BTreeMap<String, LocalEntry>, AppError> {
        let telemetry_for_scan = telemetry.clone();
        let mut task = Box::pin(tokio::task::spawn_blocking(move || {
            scan_local_cached(&root, &known, Some(&telemetry_for_scan))
        }));
        let mut tick = tokio::time::interval(SYNC_PROGRESS_INTERVAL);
        loop {
            tokio::select! {
                result = &mut task => {
                    let entries = result.map_err(|_| AppError::LocalData)??;
                    telemetry.lock().map_err(|_| AppError::LocalData)?
                        .update_local_scan(local_endpoint_stats(&entries));
                    let _ = self.emit_summary(app, pair_id);
                    return Ok(entries);
                }
                _ = tick.tick() => {
                    let _ = self.emit_summary(app, pair_id);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn scan_remote_with_progress(
        &self,
        app: &AppHandle,
        pair_id: &str,
        api: &KoofrApi,
        mount_id: &MountId,
        root: &RemotePath,
        rules: &[IgnoreRule],
        telemetry: Arc<Mutex<SyncTelemetry>>,
        cancel: &CancellationToken,
    ) -> Result<BTreeMap<String, RemoteEntry>, AppError> {
        let mut scan = Box::pin(scan_remote(
            api,
            mount_id,
            root,
            rules,
            Some(telemetry.clone()),
        ));
        let mut tick = tokio::time::interval(SYNC_PROGRESS_INTERVAL);
        loop {
            tokio::select! {
                result = &mut scan => {
                    let entries = result?;
                    telemetry.lock().map_err(|_| AppError::LocalData)?
                        .update_remote_scan(remote_endpoint_stats(&entries));
                    let _ = self.emit_summary(app, pair_id);
                    return Ok(entries);
                }
                _ = tick.tick() => {
                    let _ = self.emit_summary(app, pair_id);
                }
                () = cancel.cancelled() => return Err(AppError::Cancelled),
            }
        }
    }

    async fn run_once(
        &self,
        app: &AppHandle,
        pair_id: &str,
        cancel: &CancellationToken,
    ) -> Result<(), AppError> {
        let Some(pair) = self.store.pair(pair_id)? else {
            return Ok(());
        };
        let telemetry = self.telemetry_for(pair_id)?;
        if pair.paused {
            telemetry
                .lock()
                .map_err(|_| AppError::LocalData)?
                .clear_active();
            self.emit_summary(app, pair_id)?;
            return Ok(());
        }
        self.store
            .update_progress(pair_id, "scanning", 0, 0, 0, 0, 0, 0, None, None)?;
        telemetry
            .lock()
            .map_err(|_| AppError::LocalData)?
            .begin_scanning();
        self.emit_summary(app, pair_id)?;
        let local_root = pair.local_root.clone();
        let known_local = baseline_local_identities(&pair.baseline);
        let local = match self
            .scan_local_with_progress(app, pair_id, local_root, known_local, telemetry.clone())
            .await
        {
            Ok(local) => local,
            Err(error) => {
                if validate_local_root_sync(&pair.local_root).is_err() {
                    if pair.status != "paused" {
                        self.store.add_issue(
                            pair_id,
                            "",
                            "sync_root_unavailable",
                            "本地同步根目录不可用，已暂停同步。",
                        )?;
                    }
                    self.store.update_progress(
                        pair_id,
                        "paused",
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        None,
                        Some("sync_root_unavailable"),
                    )?;
                    self.emit_summary(app, pair_id)?;
                    return Ok(());
                }
                return Err(error);
            }
        };
        let ignore_rules = load_ignore_rules(&pair.local_root)?;
        if cancel.is_cancelled() {
            return Err(AppError::Cancelled);
        }
        let remote = match self
            .scan_remote_with_progress(
                app,
                pair_id,
                &app.state::<AppState>().api,
                &pair.mount_id,
                &pair.remote_path,
                &ignore_rules,
                telemetry.clone(),
                cancel,
            )
            .await
        {
            Ok(remote) => remote,
            Err(error @ AppError::Network(_)) => {
                self.store.update_progress(
                    pair_id,
                    "offline",
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    None,
                    Some("network_error"),
                )?;
                self.emit_summary(app, pair_id)?;
                return Err(error);
            }
            Err(error) => return Err(error),
        };
        let scan = ScanResult { local, remote };
        let actions = plan_actions(&pair.baseline, &scan);
        let (upload_items, upload_bytes, download_items, download_bytes) =
            action_direction_totals(&actions);
        telemetry
            .lock()
            .map_err(|_| AppError::LocalData)?
            .reset_for_scan(
                local_endpoint_stats(&scan.local),
                remote_endpoint_stats(&scan.remote),
                upload_items,
                upload_bytes,
                download_items,
                download_bytes,
            );
        let total_items = actions.len() as u64;
        let total_bytes = actions.iter().map(action_bytes).sum();
        self.store.update_progress(
            pair_id,
            if total_items == 0 { "idle" } else { "syncing" },
            total_items,
            total_bytes,
            0,
            total_items,
            0,
            total_bytes,
            None,
            None,
        )?;
        self.emit_summary(app, pair_id)?;

        let mut completed_items = 0_u64;
        let mut completed_bytes = 0_u64;
        let mut had_failure = false;
        for action in actions {
            if cancel.is_cancelled() {
                return Err(AppError::Cancelled);
            }
            let relative = action_relative(&action);
            let operation = self
                .store
                .operation(pair_id, action_name(&action), &relative)?;
            let result = self
                .execute_action_with_progress(
                    app,
                    pair_id,
                    &pair,
                    &action,
                    self.store.device_id(),
                    cancel,
                    telemetry.clone(),
                )
                .await;
            match result {
                Ok(bytes) => {
                    let (
                        action_upload_items,
                        action_upload_bytes,
                        action_download_items,
                        action_download_bytes,
                    ) = action_direction(&action);
                    telemetry
                        .lock()
                        .map_err(|_| AppError::LocalData)?
                        .complete_action(
                            action_upload_items,
                            action_upload_bytes,
                            action_download_items,
                            action_download_bytes,
                        );
                    self.store.finish_operation(&operation, "completed")?;
                    completed_items = completed_items.saturating_add(1);
                    completed_bytes = completed_bytes.saturating_add(bytes);
                    self.store.update_progress(
                        pair_id,
                        "syncing",
                        total_items.saturating_sub(completed_items),
                        total_bytes.saturating_sub(completed_bytes),
                        completed_items,
                        total_items,
                        completed_bytes,
                        total_bytes,
                        None,
                        None,
                    )?;
                    self.emit_summary(app, pair_id)?;
                }
                Err(error @ AppError::Network(_)) => {
                    telemetry
                        .lock()
                        .map_err(|_| AppError::LocalData)?
                        .clear_active();
                    let _ = self.store.finish_operation(&operation, "retry");
                    self.store.update_progress(
                        pair_id,
                        "offline",
                        total_items.saturating_sub(completed_items),
                        total_bytes.saturating_sub(completed_bytes),
                        completed_items,
                        total_items,
                        completed_bytes,
                        total_bytes,
                        None,
                        Some("network_error"),
                    )?;
                    self.emit_summary(app, pair_id)?;
                    return Err(error);
                }
                Err(AppError::Conflict) => {
                    telemetry
                        .lock()
                        .map_err(|_| AppError::LocalData)?
                        .clear_active();
                    let _ = self.store.finish_operation(&operation, "retry");
                    self.store.update_progress(
                        pair_id,
                        "scanning",
                        total_items.saturating_sub(completed_items),
                        total_bytes.saturating_sub(completed_bytes),
                        completed_items,
                        total_items,
                        completed_bytes,
                        total_bytes,
                        None,
                        None,
                    )?;
                    self.emit_summary(app, pair_id)?;
                    return Ok(());
                }
                Err(error @ AppError::Cancelled) => {
                    telemetry
                        .lock()
                        .map_err(|_| AppError::LocalData)?
                        .clear_active();
                    let _ = self.store.finish_operation(&operation, "retry");
                    return Err(error);
                }
                Err(error) => {
                    telemetry
                        .lock()
                        .map_err(|_| AppError::LocalData)?
                        .clear_active();
                    had_failure = true;
                    let _ = self.store.finish_operation(&operation, "failed");
                    let code = error.log_code();
                    self.store.add_issue(
                        pair_id,
                        &relative,
                        code,
                        "同步操作未完成，请检查后重试。",
                    )?;
                    self.emit_summary(app, pair_id)?;
                }
            }
        }

        if had_failure {
            self.store.update_progress(
                pair_id,
                "attention",
                total_items.saturating_sub(completed_items),
                total_bytes.saturating_sub(completed_bytes),
                completed_items,
                total_items,
                completed_bytes,
                total_bytes,
                None,
                Some("sync_attention"),
            )?;
            self.emit_summary(app, pair_id)?;
            return Ok(());
        }

        let final_local_root = pair.local_root.clone();
        let final_known = scan
            .local
            .iter()
            .map(|(relative, entry)| (relative.clone(), entry.identity.clone()))
            .collect::<BTreeMap<_, _>>();
        let final_local = tokio::task::spawn_blocking(move || {
            scan_local_cached(&final_local_root, &final_known, None)
        })
        .await
        .map_err(|_| AppError::LocalData)??;
        let final_ignore_rules = load_ignore_rules(&pair.local_root)?;
        let final_remote = scan_remote(
            &app.state::<AppState>().api,
            &pair.mount_id,
            &pair.remote_path,
            &final_ignore_rules,
            None,
        )
        .await?;
        telemetry
            .lock()
            .map_err(|_| AppError::LocalData)?
            .update_endpoints(
                local_endpoint_stats(&final_local),
                remote_endpoint_stats(&final_remote),
            );
        let mut baseline = BTreeMap::new();
        for relative in final_local.keys().chain(final_remote.keys()) {
            let local_entry = final_local.get(relative);
            let remote_entry = final_remote.get(relative);
            baseline
                .entry(relative.clone())
                .or_insert_with(|| SnapshotEntry {
                    kind: local_entry
                        .map(|entry| entry.kind.clone())
                        .or_else(|| remote_entry.map(|entry| entry.kind.clone()))
                        .unwrap_or_else(|| "file".to_owned()),
                    local: local_entry.map(|entry| entry.identity.clone()),
                    remote: remote_entry.map(|entry| entry.identity.clone()),
                });
        }
        self.store.save_baseline(pair_id, &baseline, now_millis())?;
        telemetry
            .lock()
            .map_err(|_| AppError::LocalData)?
            .clear_active();
        self.emit_summary(app, pair_id)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_action_with_progress(
        &self,
        app: &AppHandle,
        pair_id: &str,
        pair: &SyncPairRecord,
        action: &Action,
        device_id: &str,
        cancel: &CancellationToken,
        telemetry: Arc<Mutex<SyncTelemetry>>,
    ) -> Result<u64, AppError> {
        let mut action_future = Box::pin(execute_action(
            app,
            pair,
            action,
            device_id,
            cancel,
            telemetry.clone(),
        ));
        let mut tick = tokio::time::interval(SYNC_PROGRESS_INTERVAL);
        loop {
            tokio::select! {
                result = &mut action_future => {
                    if let Ok(mut telemetry) = telemetry.lock() {
                        telemetry.sample_rates();
                    }
                    let _ = self.emit_summary(app, pair_id);
                    return result;
                }
                _ = tick.tick() => {
                    if let Ok(mut telemetry) = telemetry.lock() {
                        telemetry.sample_rates();
                    }
                    let _ = self.emit_summary(app, pair_id);
                }
            }
        }
    }

    fn emit_summary(&self, app: &AppHandle, pair_id: &str) -> Result<(), AppError> {
        let Some(pair) = self.store.pair(pair_id)? else {
            return Ok(());
        };
        let (local, remote, upload, download, last_scan_at) = self
            .runtimes
            .lock()
            .map_err(|_| AppError::LocalData)?
            .get(pair_id)
            .map(|runtime| {
                runtime
                    .telemetry
                    .lock()
                    .map(|telemetry| telemetry.public())
                    .map_err(|_| AppError::LocalData)
            })
            .transpose()?
            .unwrap_or_else(|| {
                let (local, remote) = baseline_endpoint_stats(&pair.baseline);
                (
                    local,
                    remote,
                    SyncDirectionStats::default(),
                    SyncDirectionStats::default(),
                    pair.last_synced_at,
                )
            });
        let event = SyncStateEvent {
            pair_id: pair.id,
            status: if pair.paused {
                "paused".to_owned()
            } else {
                pair.status
            },
            pending_items: pair.pending_items,
            pending_bytes: pair.pending_bytes,
            completed_items: pair.completed_items,
            total_items: pair.total_items,
            completed_bytes: pair.completed_bytes,
            total_bytes: pair.total_bytes,
            last_synced_at: pair.last_synced_at,
            issue_count: pair.issue_count,
            error_code: pair.last_error_code,
            local,
            remote,
            upload,
            download,
            last_scan_at,
        };
        app.emit(SYNC_EVENT, event).map_err(|_| AppError::LocalData)
    }
}

fn summary(
    pair: SyncPairRecord,
    telemetry: Option<&Mutex<SyncTelemetry>>,
) -> Result<SyncPairSummary, AppError> {
    let (local, remote, upload, download, last_scan_at) = if let Some(telemetry) = telemetry {
        telemetry.lock().map_err(|_| AppError::LocalData)?.public()
    } else {
        let (local, remote) = baseline_endpoint_stats(&pair.baseline);
        (
            local,
            remote,
            SyncDirectionStats::default(),
            SyncDirectionStats::default(),
            pair.last_synced_at,
        )
    };
    Ok(SyncPairSummary {
        id: pair.id,
        local_root: pair.local_root.to_string_lossy().into_owned(),
        remote_path: pair.remote_path.as_str().to_owned(),
        status: if pair.paused {
            "paused".to_owned()
        } else {
            pair.status
        },
        paused: pair.paused,
        pending_items: pair.pending_items,
        pending_bytes: pair.pending_bytes,
        completed_items: pair.completed_items,
        total_items: pair.total_items,
        completed_bytes: pair.completed_bytes,
        total_bytes: pair.total_bytes,
        last_synced_at: pair.last_synced_at,
        issue_count: pair.issue_count,
        last_error_code: pair.last_error_code,
        local,
        remote,
        upload,
        download,
        last_scan_at,
    })
}

fn baseline_endpoint_stats(
    baseline: &BTreeMap<String, SnapshotEntry>,
) -> (SyncEndpointStats, SyncEndpointStats) {
    let mut local = SyncEndpointStats::default();
    let mut remote = SyncEndpointStats::default();
    for entry in baseline.values() {
        if let Some(identity) = entry.local.as_ref() {
            if entry.kind == "dir" {
                local.directory_count = local.directory_count.saturating_add(1);
            } else {
                local.file_count = local.file_count.saturating_add(1);
                local.bytes = local.bytes.saturating_add(identity.size.max(0) as u64);
            }
        }
        if let Some(identity) = entry.remote.as_ref() {
            if entry.kind == "dir" {
                remote.directory_count = remote.directory_count.saturating_add(1);
            } else {
                remote.file_count = remote.file_count.saturating_add(1);
                remote.bytes = remote.bytes.saturating_add(identity.size.max(0) as u64);
            }
        }
    }
    (local, remote)
}

pub fn validate_local_root_sync(path: &Path) -> Result<(), AppError> {
    let metadata = fs::symlink_metadata(path)?;
    if !path.is_absolute() || !metadata.is_dir() || is_reparse_point(&metadata) {
        return Err(AppError::InvalidInput("sync local directory"));
    }
    Ok(())
}

async fn validate_local_root(path: &Path) -> Result<(), AppError> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || validate_local_root_sync(&path))
        .await
        .map_err(|_| AppError::LocalData)??;
    Ok(())
}

fn roots_overlap(left: &Path, right: &Path) -> bool {
    let left = fs::canonicalize(left).unwrap_or_else(|_| left.to_path_buf());
    let right = fs::canonicalize(right).unwrap_or_else(|_| right.to_path_buf());
    left == right || left.starts_with(&right) || right.starts_with(&left)
}

fn remote_roots_overlap(
    left_mount: &MountId,
    left: &RemotePath,
    right_mount: &MountId,
    right: &RemotePath,
) -> bool {
    if left_mount != right_mount {
        return false;
    }
    let left = left.as_str();
    let right = right.as_str();
    left == right
        || left.starts_with(&format!("{right}/"))
        || right.starts_with(&format!("{left}/"))
}

fn baseline_local_identities(
    baseline: &BTreeMap<String, SnapshotEntry>,
) -> BTreeMap<String, Identity> {
    baseline
        .iter()
        .filter_map(|(relative, entry)| {
            entry
                .local
                .as_ref()
                .map(|identity| (relative.clone(), identity.clone()))
        })
        .collect()
}

fn scan_local_cached(
    root: &Path,
    known: &BTreeMap<String, Identity>,
    telemetry: Option<&Arc<Mutex<SyncTelemetry>>>,
) -> Result<BTreeMap<String, LocalEntry>, AppError> {
    validate_local_root_sync(root)?;
    let rules = load_ignore_rules(root)?;
    let mut result = BTreeMap::new();
    let mut stats = SyncEndpointStats::default();
    scan_local_directory(
        root,
        root,
        &rules,
        known,
        telemetry,
        &mut stats,
        &mut result,
    )?;
    Ok(result)
}

fn local_endpoint_stats(entries: &BTreeMap<String, LocalEntry>) -> SyncEndpointStats {
    let mut stats = SyncEndpointStats::default();
    for entry in entries.values() {
        if entry.kind == "dir" {
            stats.directory_count = stats.directory_count.saturating_add(1);
        } else {
            stats.file_count = stats.file_count.saturating_add(1);
            stats.bytes = stats
                .bytes
                .saturating_add(entry.identity.size.max(0) as u64);
        }
    }
    stats
}

fn scan_local_directory(
    root: &Path,
    directory: &Path,
    rules: &[IgnoreRule],
    known: &BTreeMap<String, Identity>,
    telemetry: Option<&Arc<Mutex<SyncTelemetry>>>,
    stats: &mut SyncEndpointStats,
    result: &mut BTreeMap<String, LocalEntry>,
) -> Result<(), AppError> {
    let mut entries = fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|_| AppError::LocalData)?
            .to_string_lossy()
            .replace('\\', "/");
        let metadata = fs::symlink_metadata(&path)?;
        if is_reparse_point(&metadata) || !metadata.is_dir() && !metadata.is_file() {
            continue;
        }
        if relative != ".syncignore"
            && (is_builtin_ignored(&relative) || matches_ignore(rules, &relative))
        {
            continue;
        }
        if metadata.is_dir() {
            let identity = Identity {
                size: 0,
                modified: modified_millis(&metadata)?,
                hash: String::new(),
            };
            result.insert(
                relative.clone(),
                LocalEntry {
                    kind: "dir".to_owned(),
                    identity,
                    path: path.clone(),
                },
            );
            stats.directory_count = stats.directory_count.saturating_add(1);
            if let Some(telemetry) = telemetry {
                telemetry
                    .lock()
                    .map_err(|_| AppError::LocalData)?
                    .update_local_scan(stats.clone());
            }
            scan_local_directory(root, &path, rules, known, telemetry, stats, result)?;
        } else {
            let size = i64::try_from(metadata.len())
                .map_err(|_| AppError::InvalidInput("local file size"))?;
            let modified = modified_millis(&metadata)?;
            stats.file_count = stats.file_count.saturating_add(1);
            stats.bytes = stats.bytes.saturating_add(metadata.len());
            if let Some(telemetry) = telemetry {
                telemetry
                    .lock()
                    .map_err(|_| AppError::LocalData)?
                    .update_local_scan(stats.clone());
            }
            let hash = known
                .get(&relative)
                .filter(|identity| {
                    identity.size == size
                        && identity.modified == modified
                        && !identity.hash.is_empty()
                })
                .map(|identity| identity.hash.clone())
                .map_or_else(|| hash_file(&path), Ok)?;
            let identity = Identity {
                size,
                modified,
                hash,
            };
            result.insert(
                relative,
                LocalEntry {
                    kind: "file".to_owned(),
                    identity,
                    path,
                },
            );
        }
    }
    Ok(())
}

fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        metadata.file_attributes() & 0x0000_0400 != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

async fn scan_remote(
    api: &KoofrApi,
    mount_id: &MountId,
    root: &RemotePath,
    rules: &[IgnoreRule],
    telemetry: Option<Arc<Mutex<SyncTelemetry>>>,
) -> Result<BTreeMap<String, RemoteEntry>, AppError> {
    let mut result = BTreeMap::new();
    let mut stats = SyncEndpointStats::default();
    let mut queue = VecDeque::from([(root.clone(), String::new())]);
    while !queue.is_empty() {
        let mut batch = Vec::with_capacity(REMOTE_SCAN_CONCURRENCY);
        for _ in 0..REMOTE_SCAN_CONCURRENCY {
            let Some(item) = queue.pop_front() else {
                break;
            };
            batch.push(item);
        }
        let listings = futures_util::future::try_join_all(batch.into_iter().map(
            |(directory, relative_directory)| async move {
                let entries = api.list_files(mount_id, &directory).await?;
                Ok::<_, AppError>((directory, relative_directory, entries))
            },
        ))
        .await?;
        for (directory, relative_directory, entries) in listings {
            for entry in entries {
                let name = RemoteName::parse(entry.name.clone())?;
                let relative = if relative_directory.is_empty() {
                    entry.name.clone()
                } else {
                    format!("{relative_directory}/{}", entry.name)
                };
                if relative != ".syncignore"
                    && (is_builtin_ignored(&relative) || matches_ignore(rules, &relative))
                {
                    continue;
                }
                let remote_path = directory.join(&name)?;
                let identity = Identity {
                    size: entry.size,
                    modified: entry.modified,
                    hash: entry.hash,
                };
                let kind = if is_directory(&entry.entry_type) {
                    "dir"
                } else {
                    "file"
                };
                if kind == "dir" {
                    stats.directory_count = stats.directory_count.saturating_add(1);
                } else {
                    stats.file_count = stats.file_count.saturating_add(1);
                    stats.bytes = stats.bytes.saturating_add(identity.size.max(0) as u64);
                }
                result.insert(
                    relative.clone(),
                    RemoteEntry {
                        kind: kind.to_owned(),
                        identity,
                        path: remote_path.clone(),
                    },
                );
                if kind == "dir" {
                    queue.push_back((remote_path, relative));
                }
            }
        }
        if let Some(telemetry) = &telemetry {
            telemetry
                .lock()
                .map_err(|_| AppError::LocalData)?
                .update_remote_scan(stats.clone());
        }
    }
    Ok(result)
}

fn remote_endpoint_stats(entries: &BTreeMap<String, RemoteEntry>) -> SyncEndpointStats {
    let mut stats = SyncEndpointStats::default();
    for entry in entries.values() {
        if entry.kind == "dir" {
            stats.directory_count = stats.directory_count.saturating_add(1);
        } else {
            stats.file_count = stats.file_count.saturating_add(1);
            stats.bytes = stats
                .bytes
                .saturating_add(entry.identity.size.max(0) as u64);
        }
    }
    stats
}

fn action_direction(action: &Action) -> (u64, u64, u64, u64) {
    match action {
        Action::Upload { local, .. } => (1, local.identity.size.max(0) as u64, 0, 0),
        Action::Download { remote, .. } => (0, 0, 1, remote.identity.size.max(0) as u64),
        Action::Conflict { local, remote, .. } => (
            1,
            local.identity.size.max(0) as u64,
            1,
            remote.identity.size.max(0) as u64,
        ),
        _ => (0, 0, 0, 0),
    }
}

fn action_direction_totals(actions: &[Action]) -> (u64, u64, u64, u64) {
    let mut totals = (0_u64, 0_u64, 0_u64, 0_u64);
    for action in actions {
        let current = action_direction(action);
        totals.0 = totals.0.saturating_add(current.0);
        totals.1 = totals.1.saturating_add(current.1);
        totals.2 = totals.2.saturating_add(current.2);
        totals.3 = totals.3.saturating_add(current.3);
    }
    totals
}

fn plan_actions(baseline: &BTreeMap<String, SnapshotEntry>, scan: &ScanResult) -> Vec<Action> {
    let mut paths = BTreeSet::new();
    paths.extend(baseline.keys().cloned());
    paths.extend(scan.local.keys().cloned());
    paths.extend(scan.remote.keys().cloned());
    let mut folded_paths = BTreeMap::new();
    let mut ambiguous_paths = BTreeSet::new();
    for relative in &paths {
        let folded = relative.to_lowercase();
        if let Some(existing) = folded_paths.insert(folded, relative.clone())
            && existing != *relative
        {
            ambiguous_paths.insert(existing);
            ambiguous_paths.insert(relative.clone());
        }
    }
    let initial = baseline.is_empty();
    let mut actions = Vec::new();
    for relative in paths {
        if ambiguous_paths.contains(&relative) {
            actions.push(Action::TypeConflict { relative });
            continue;
        }
        let local = scan.local.get(&relative);
        let remote = scan.remote.get(&relative);
        let previous = baseline.get(&relative);
        if let (Some(local), Some(remote)) = (local, remote)
            && local.kind != remote.kind
        {
            actions.push(Action::TypeConflict { relative });
            continue;
        }
        if local.is_some_and(|entry| entry.kind == "dir")
            || remote.is_some_and(|entry| entry.kind == "dir")
        {
            plan_directory_action(&mut actions, &relative, local, remote, previous, initial);
            continue;
        }
        let local_identity = local.map(|entry| entry.identity.clone());
        let remote_identity = remote.map(|entry| entry.identity.clone());
        match previous {
            None => match (local, remote) {
                (Some(local), None) => actions.push(Action::Upload {
                    relative,
                    local: local.clone(),
                    expected_remote: None,
                }),
                (None, Some(remote)) => actions.push(Action::Download {
                    relative,
                    remote: remote.clone(),
                    expected_local: None,
                }),
                (Some(local), Some(remote)) if !same_content(&local.identity, &remote.identity) => {
                    actions.push(Action::Conflict {
                        relative,
                        local: local.clone(),
                        remote: remote.clone(),
                    })
                }
                _ => {}
            },
            Some(previous) => {
                let local_changed = local_identity != previous.local;
                let remote_changed = remote_identity != previous.remote;
                match (local, remote) {
                    (Some(local), Some(remote))
                        if same_content(&local.identity, &remote.identity) => {}
                    (Some(local), Some(remote)) if local_changed && remote_changed => {
                        actions.push(Action::Conflict {
                            relative,
                            local: local.clone(),
                            remote: remote.clone(),
                        })
                    }
                    (Some(local), Some(_)) if local_changed => actions.push(Action::Upload {
                        relative,
                        local: local.clone(),
                        expected_remote: previous.remote.clone(),
                    }),
                    (Some(_), Some(remote)) if remote_changed => actions.push(Action::Download {
                        relative,
                        remote: remote.clone(),
                        expected_local: previous.local.clone(),
                    }),
                    (Some(local), None) if local_changed || previous.remote.is_none() => actions
                        .push(Action::Upload {
                            relative,
                            local: local.clone(),
                            expected_remote: None,
                        }),
                    (Some(local), None) => actions.push(Action::DeleteLocal {
                        relative,
                        expected: local.identity.clone(),
                    }),
                    (None, Some(remote)) if remote_changed || previous.local.is_none() => actions
                        .push(Action::Download {
                            relative,
                            remote: remote.clone(),
                            expected_local: None,
                        }),
                    (None, Some(remote)) => actions.push(Action::DeleteRemote {
                        relative,
                        remote: remote.clone(),
                    }),
                    (None, None) => {}
                    _ => {}
                }
            }
        }
    }
    actions.sort_by_key(|action| match action {
        Action::EnsureLocalDir { relative } | Action::EnsureRemoteDir { relative } => {
            (0_u8, relative.matches('/').count(), relative.clone())
        }
        Action::Upload { relative, .. }
        | Action::Download { relative, .. }
        | Action::Conflict { relative, .. }
        | Action::TypeConflict { relative } => (1, relative.matches('/').count(), relative.clone()),
        Action::DeleteLocal { relative, .. } | Action::DeleteRemote { relative, .. } => (
            2,
            usize::MAX - relative.matches('/').count(),
            relative.clone(),
        ),
    });
    actions
}

fn plan_directory_action(
    actions: &mut Vec<Action>,
    relative: &str,
    local: Option<&LocalEntry>,
    remote: Option<&RemoteEntry>,
    previous: Option<&SnapshotEntry>,
    initial: bool,
) {
    let local_dir = local.is_some_and(|entry| entry.kind == "dir");
    let remote_dir = remote.is_some_and(|entry| entry.kind == "dir");
    if local_dir && remote_dir {
        return;
    }
    if local_dir && !remote_dir {
        if initial || previous.is_some_and(|entry| entry.remote.is_none()) {
            actions.push(Action::EnsureRemoteDir {
                relative: relative.to_owned(),
            });
        } else {
            actions.push(Action::DeleteLocal {
                relative: relative.to_owned(),
                expected: local.unwrap().identity.clone(),
            });
        }
    } else if remote_dir && !local_dir {
        if initial || previous.is_some_and(|entry| entry.local.is_none()) {
            actions.push(Action::EnsureLocalDir {
                relative: relative.to_owned(),
            });
        } else {
            actions.push(Action::DeleteRemote {
                relative: relative.to_owned(),
                remote: remote.unwrap().clone(),
            });
        }
    }
}

async fn execute_action(
    app: &AppHandle,
    pair: &SyncPairRecord,
    action: &Action,
    device_id: &str,
    cancel: &CancellationToken,
    telemetry: Arc<Mutex<SyncTelemetry>>,
) -> Result<u64, AppError> {
    let state = app.state::<AppState>();
    match action {
        Action::EnsureLocalDir { relative } => {
            let path = local_path(&pair.local_root, relative)?;
            validate_local_parent(&pair.local_root, &path)?;
            tokio::fs::create_dir_all(path).await?;
            Ok(0)
        }
        Action::EnsureRemoteDir { relative } => {
            let target = remote_path_for(&pair.remote_path, relative)?;
            let parent = target.parent()?;
            let name = RemoteName::parse(target.file_name()?.to_owned())?;
            state
                .api
                .create_folder(&pair.mount_id, &parent, &name)
                .await?;
            Ok(0)
        }
        Action::Upload {
            relative,
            local,
            expected_remote,
        } => {
            sync_upload(
                app,
                &state.api,
                &state.transfers,
                &pair.mount_id,
                &pair.remote_path,
                relative,
                &local.path,
                Some(&local.identity),
                expected_remote.as_ref(),
                cancel,
                telemetry.clone(),
            )
            .await
        }
        Action::Download {
            relative,
            remote,
            expected_local,
        } => {
            sync_download(
                &state.api,
                &state.transfers,
                &pair.mount_id,
                &pair.local_root,
                relative,
                remote,
                expected_local.as_ref(),
                cancel,
                telemetry.clone(),
            )
            .await
        }
        Action::DeleteLocal { relative, expected } => {
            let path = local_path(&pair.local_root, relative)?;
            let current = local_identity(&path)?;
            if current.as_ref() != Some(expected) {
                return Err(AppError::Conflict);
            }
            trash::delete(path)
                .map_err(|_| AppError::Io(std::io::Error::other("recycle bin unavailable")))?;
            Ok(0)
        }
        Action::DeleteRemote { remote, .. } => {
            state
                .api
                .delete_conditional(
                    &pair.mount_id,
                    &remote.path,
                    Some((
                        &remote.identity.hash,
                        remote.identity.size,
                        remote.identity.modified,
                    )),
                )
                .await?;
            Ok(0)
        }
        Action::Conflict {
            relative,
            local,
            remote,
        } => {
            if local.kind != "file" || remote.kind != "file" {
                return Err(AppError::InvalidInput("sync type conflict"));
            }
            let conflict_path = conflict_local_path(&pair.local_root, relative, device_id)?;
            tokio::fs::copy(&local.path, &conflict_path).await?;
            let conflict_relative = conflict_path
                .strip_prefix(&pair.local_root)
                .map_err(|_| AppError::LocalData)?
                .to_string_lossy()
                .replace('\\', "/");
            sync_upload(
                app,
                &state.api,
                &state.transfers,
                &pair.mount_id,
                &pair.remote_path,
                &conflict_relative,
                &conflict_path,
                Some(&local.identity),
                None,
                cancel,
                telemetry.clone(),
            )
            .await?;
            sync_download(
                &state.api,
                &state.transfers,
                &pair.mount_id,
                &pair.local_root,
                relative,
                remote,
                Some(&local.identity),
                cancel,
                telemetry,
            )
            .await
        }
        Action::TypeConflict { .. } => Err(AppError::InvalidInput("sync type conflict")),
    }
}

#[allow(clippy::too_many_arguments)]
async fn sync_upload(
    app: &AppHandle,
    api: &KoofrApi,
    manager: &TransferManager,
    mount_id: &MountId,
    remote_root: &RemotePath,
    relative: &str,
    local_path: &Path,
    expected_local: Option<&Identity>,
    expected_remote: Option<&Identity>,
    cancel: &CancellationToken,
    telemetry: Arc<Mutex<SyncTelemetry>>,
) -> Result<u64, AppError> {
    if let Some(expected) = expected_local
        && local_identity(local_path)?.as_ref() != Some(expected)
    {
        return Err(AppError::Conflict);
    }
    let metadata = tokio::fs::metadata(local_path).await?;
    let total = metadata.len();
    let transfer_id = uuid::Uuid::new_v4().to_string();
    let _registration = manager.register(&transfer_id)?;
    let _permit = match manager.acquire(TransferDirection::Upload, cancel).await {
        Ok(permit) => permit,
        Err(error) => {
            manager.finish(&transfer_id);
            return Err(error);
        }
    };
    let file = match tokio::fs::File::open(local_path).await {
        Ok(file) => file,
        Err(error) => {
            manager.finish(&transfer_id);
            return Err(error.into());
        }
    };
    let app_for_stream = app.clone();
    let transfer_id_for_stream = transfer_id.clone();
    let cancel_for_stream = cancel.clone();
    let telemetry_for_stream = telemetry.clone();
    let stream = ReaderStream::new(file)
        .map_err(|error| std::io::Error::other(error.to_string()))
        .and_then(move |chunk| {
            let app = app_for_stream.clone();
            let transfer_id = transfer_id_for_stream.clone();
            let cancel = cancel_for_stream.clone();
            let telemetry = telemetry_for_stream.clone();
            async move {
                app.state::<AppState>()
                    .transfers
                    .throttle(&transfer_id, chunk.len(), &cancel)
                    .await
                    .map_err(|_| std::io::Error::other("sync upload interrupted"))?;
                if let Ok(mut telemetry) = telemetry.lock() {
                    telemetry.record(true, chunk.len() as u64);
                }
                Ok(chunk)
            }
        });
    let body = reqwest::Body::wrap_stream(stream);
    let name = RemoteName::parse(
        Path::new(relative)
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or(AppError::InvalidInput("sync file name"))?
            .to_owned(),
    )?;
    let directory = remote_path_for(
        remote_root,
        Path::new(relative)
            .parent()
            .and_then(|parent| parent.to_str())
            .unwrap_or(""),
    )?;
    let modified = modified_millis(&metadata)? as i64;
    let expected =
        expected_remote.map(|identity| (&identity.hash as &str, identity.size, identity.modified));
    let result = tokio::select! {
        result = api.upload_conditional(mount_id, &directory, &name, body, total, expected, modified) => result,
        () = cancel.cancelled() => Err(AppError::Cancelled),
    };
    manager.finish(&transfer_id);
    result.map(|_| total)
}

#[allow(clippy::too_many_arguments)]
async fn sync_download(
    api: &KoofrApi,
    manager: &TransferManager,
    mount_id: &MountId,
    local_root: &Path,
    relative: &str,
    remote: &RemoteEntry,
    expected_local: Option<&Identity>,
    cancel: &CancellationToken,
    telemetry: Arc<Mutex<SyncTelemetry>>,
) -> Result<u64, AppError> {
    let target = local_path(local_root, relative)?;
    validate_local_parent(local_root, &target)?;
    if let Some(expected) = expected_local
        && local_identity(&target)?.as_ref() != Some(expected)
    {
        return Err(AppError::Conflict);
    }
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let temporary = sync_temporary_path(&target);
    let transfer_id = uuid::Uuid::new_v4().to_string();
    let _permit = match manager.acquire(TransferDirection::Download, cancel).await {
        Ok(permit) => permit,
        Err(error) => {
            manager.finish(&transfer_id);
            return Err(error);
        }
    };
    let result = async {
        let mut offset = match tokio::fs::metadata(&temporary).await {
            Ok(metadata) if metadata.is_file() => metadata.len(),
            Ok(_) => 0,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
            Err(error) => return Err(error.into()),
        };
        if offset > remote.identity.size.max(0) as u64 {
            let _ = tokio::fs::remove_file(&temporary).await;
            offset = 0;
        }
        let response = api
            .download_response_from(mount_id, &remote.path, offset)
            .await?;
        if offset > 0 && response.status() == reqwest::StatusCode::OK {
            let _ = tokio::fs::remove_file(&temporary).await;
            offset = 0;
        }
        let mut digest = Md5::new();
        if offset > 0 {
            let mut existing = tokio::fs::File::open(&temporary).await?;
            let mut buffer = vec![0_u8; 1024 * 1024];
            loop {
                let read = existing.read(&mut buffer).await?;
                if read == 0 {
                    break;
                }
                digest.update(&buffer[..read]);
            }
        }
        let mut options = tokio::fs::OpenOptions::new();
        options.create(true).write(true);
        if offset == 0 {
            options.truncate(true);
        } else {
            options.append(true);
        }
        let mut file = options.open(&temporary).await?;
        let mut bytes = offset;
        let mut stream = response.bytes_stream();
        while let Some(chunk) = tokio::select! {
            item = stream.next() => item,
            () = cancel.cancelled() => return Err(AppError::Cancelled),
        } {
            let chunk = chunk.map_err(AppError::Network)?;
            manager.throttle(&transfer_id, chunk.len(), cancel).await?;
            file.write_all(&chunk).await?;
            if let Ok(mut telemetry) = telemetry.lock() {
                telemetry.record(false, chunk.len() as u64);
            }
            digest.update(&chunk);
            bytes = bytes.saturating_add(chunk.len() as u64);
        }
        file.sync_all().await?;
        drop(file);
        let hash = format!("{:x}", digest.finalize());
        if !remote.identity.hash.is_empty() && !hash.eq_ignore_ascii_case(&remote.identity.hash) {
            return Err(AppError::Integrity);
        }
        let modified = remote.identity.modified.max(0) as u64;
        filetime::set_file_mtime(
            &temporary,
            FileTime::from_unix_time(
                (modified / 1000) as i64,
                ((modified % 1000) * 1_000_000) as u32,
            ),
        )
        .map_err(AppError::Io)?;
        let backup = if tokio::fs::try_exists(&target).await? {
            let backup = target.with_file_name(format!(".koofr-sync-old-{}", uuid::Uuid::new_v4()));
            tokio::fs::rename(&target, &backup).await?;
            Some(backup)
        } else {
            None
        };
        if let Err(error) = tokio::fs::rename(&temporary, &target).await {
            if let Some(backup) = backup.as_ref() {
                let _ = tokio::fs::rename(backup, &target).await;
            }
            return Err(error.into());
        }
        if let Some(backup) = backup {
            let _ = tokio::fs::remove_file(backup).await;
        }
        Ok(bytes)
    }
    .await;
    if result
        .as_ref()
        .is_err_and(|error| matches!(error, AppError::Integrity | AppError::Conflict))
    {
        let _ = tokio::fs::remove_file(&temporary).await;
    }
    manager.finish(&transfer_id);
    result
}

fn local_path(root: &Path, relative: &str) -> Result<PathBuf, AppError> {
    if relative.is_empty()
        || Path::new(relative).is_absolute()
        || relative
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(AppError::InvalidInput("sync relative path"));
    }
    Ok(root.join(relative.replace('/', std::path::MAIN_SEPARATOR_STR)))
}

fn sync_temporary_path(target: &Path) -> PathBuf {
    let mut digest = Md5::new();
    digest.update(target.to_string_lossy().as_bytes());
    let suffix = format!("{:x}", digest.finalize());
    target.with_file_name(format!(".koofr-sync-part-{suffix}"))
}

fn validate_local_parent(root: &Path, target: &Path) -> Result<(), AppError> {
    let parent = target
        .parent()
        .ok_or(AppError::InvalidInput("sync local parent"))?;
    let relative = parent
        .strip_prefix(root)
        .map_err(|_| AppError::InvalidInput("sync local parent"))?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.is_dir() && !is_reparse_point(&metadata) => {}
            Ok(_) => return Err(AppError::InvalidInput("sync local parent")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(AppError::Io(error)),
        }
    }
    Ok(())
}

fn remote_path_for(root: &RemotePath, relative: &str) -> Result<RemotePath, AppError> {
    if relative.is_empty() {
        return Ok(root.clone());
    }
    let mut path = root.clone();
    for segment in relative.split('/') {
        path = path.join(&RemoteName::parse(segment.to_owned())?)?;
    }
    Ok(path)
}

fn conflict_local_path(root: &Path, relative: &str, device_id: &str) -> Result<PathBuf, AppError> {
    let path = local_path(root, relative)?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(AppError::InvalidInput("sync conflict name"))?;
    let stamp = now_millis();
    let stem = Path::new(name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(name);
    let extension = Path::new(name)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{value}"))
        .unwrap_or_default();
    let parent = path
        .parent()
        .ok_or(AppError::InvalidInput("sync conflict parent"))?;
    for index in 1_u64.. {
        let suffix = if index == 1 {
            String::new()
        } else {
            format!(" ({index})")
        };
        let candidate = parent.join(format!(
            "{stem} (conflict from {device_id} UTC-{stamp}){suffix}{extension}"
        ));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    unreachable!()
}

fn action_relative(action: &Action) -> String {
    match action {
        Action::EnsureLocalDir { relative }
        | Action::EnsureRemoteDir { relative }
        | Action::Upload { relative, .. }
        | Action::Download { relative, .. }
        | Action::DeleteLocal { relative, .. }
        | Action::DeleteRemote { relative, .. }
        | Action::Conflict { relative, .. }
        | Action::TypeConflict { relative } => relative.clone(),
    }
}

fn action_name(action: &Action) -> &'static str {
    match action {
        Action::EnsureLocalDir { .. } => "mkdir_local",
        Action::EnsureRemoteDir { .. } => "mkdir_remote",
        Action::Upload { .. } => "upload",
        Action::Download { .. } => "download",
        Action::DeleteLocal { .. } => "delete_local",
        Action::DeleteRemote { .. } => "delete_remote",
        Action::Conflict { .. } => "conflict",
        Action::TypeConflict { .. } => "type_conflict",
    }
}

fn action_bytes(action: &Action) -> u64 {
    match action {
        Action::Upload { local, .. } => local.identity.size.max(0) as u64,
        Action::Download { remote, .. } => remote.identity.size.max(0) as u64,
        Action::Conflict { local, remote, .. } => {
            local.identity.size.max(0) as u64 + remote.identity.size.max(0) as u64
        }
        _ => 0,
    }
}

fn same_content(left: &Identity, right: &Identity) -> bool {
    if !left.hash.is_empty() && !right.hash.is_empty() {
        left.hash.eq_ignore_ascii_case(&right.hash)
    } else {
        left.size == right.size && left.modified == right.modified
    }
}

fn local_identity(path: &Path) -> Result<Option<Identity>, AppError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(AppError::Io(error)),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Ok(None);
    }
    Ok(Some(Identity {
        size: i64::try_from(metadata.len())
            .map_err(|_| AppError::InvalidInput("local file size"))?,
        modified: modified_millis(&metadata)?,
        hash: hash_file(path)?,
    }))
}

fn hash_file(path: &Path) -> Result<String, AppError> {
    let mut file = fs::File::open(path)?;
    let mut digest = Md5::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = std::io::Read::read(&mut file, &mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn modified_millis(metadata: &fs::Metadata) -> Result<i64, AppError> {
    Ok(metadata
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map_err(|_| AppError::InvalidInput("local modified time"))?
        .as_millis()
        .min(i64::MAX as u128) as i64)
}

#[derive(Clone, Debug)]
struct IgnoreRule {
    pattern: String,
    negated: bool,
}

fn load_ignore_rules(root: &Path) -> Result<Vec<IgnoreRule>, AppError> {
    let path = root.join(".syncignore");
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(AppError::Io(error)),
    };
    Ok(content
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let negated = line.starts_with('!');
            let pattern = line
                .trim_start_matches('!')
                .trim_start_matches('/')
                .to_owned();
            (!pattern.is_empty()).then_some(IgnoreRule { pattern, negated })
        })
        .collect())
}

fn matches_ignore(rules: &[IgnoreRule], relative: &str) -> bool {
    let mut ignored = false;
    for rule in rules {
        if glob_matches(&rule.pattern, relative)
            || (!rule.pattern.contains('/')
                && relative.rsplit('/').next() == Some(rule.pattern.as_str()))
        {
            ignored = !rule.negated;
        }
    }
    ignored
}

fn glob_matches(pattern: &str, value: &str) -> bool {
    fn inner(pattern: &[u8], value: &[u8]) -> bool {
        if pattern.is_empty() {
            return value.is_empty();
        }
        if pattern[0] == b'*' {
            return inner(&pattern[1..], value)
                || (!value.is_empty() && inner(pattern, &value[1..]));
        }
        if pattern[0] == b'?' {
            return !value.is_empty() && inner(&pattern[1..], &value[1..]);
        }
        !value.is_empty() && pattern[0] == value[0] && inner(&pattern[1..], &value[1..])
    }
    inner(pattern.as_bytes(), value.as_bytes())
}

fn is_builtin_ignored(relative: &str) -> bool {
    relative
        .split('/')
        .any(|segment| matches!(segment, "$RECYCLE.BIN" | "System Volume Information"))
        || relative.starts_with(".koofr-sync-part-")
        || relative.split('/').any(|segment| {
            segment.starts_with(".koofr-part-")
                || segment.starts_with(".koofr-folder-")
                || segment.starts_with(".koofr-sync-part-")
                || segment.starts_with(".koofr-sync-old-")
        })
}

fn is_directory(entry_type: &str) -> bool {
    matches!(entry_type, "dir" | "folder" | "directory")
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(hash: &str, modified: i64) -> Identity {
        Identity {
            size: 4,
            modified,
            hash: hash.to_owned(),
        }
    }

    fn test_pair(id: &str, owner_id: &str) -> SyncPairRecord {
        SyncPairRecord {
            id: id.to_owned(),
            owner_id: owner_id.to_owned(),
            local_root: std::env::temp_dir().join(format!("koofr-sync-{id}")),
            mount_id: MountId::parse("mount-1".to_owned()).unwrap(),
            remote_path: RemotePath::parse(format!("/{id}")).unwrap(),
            paused: false,
            status: "initializing".to_owned(),
            pending_items: 0,
            pending_bytes: 0,
            completed_items: 0,
            total_items: 0,
            completed_bytes: 0,
            total_bytes: 0,
            last_synced_at: None,
            issue_count: 0,
            last_error_code: None,
            baseline: BTreeMap::new(),
        }
    }

    #[test]
    fn same_hash_is_content_equal_even_when_mtime_differs() {
        assert!(same_content(&identity("a", 1), &identity("a", 2)));
    }

    #[test]
    fn sqlite_store_isolates_accounts_and_recovers_running_operations() {
        let path =
            std::env::temp_dir().join(format!("koofr-sync-state-{}.sqlite3", uuid::Uuid::new_v4()));
        let store = SyncStore::open(path.clone()).unwrap();
        store.insert_pair(&test_pair("pair-a", "owner-a")).unwrap();
        store.insert_pair(&test_pair("pair-b", "owner-b")).unwrap();
        assert_eq!(store.pairs_for_owner("owner-a").unwrap().len(), 1);
        let operation = store.operation("pair-a", "upload", "a.txt").unwrap();
        drop(store);

        let reopened = SyncStore::open(path.clone()).unwrap();
        let connection = reopened.connection.lock().unwrap();
        let state: String = connection
            .query_row(
                "SELECT state FROM sync_operations WHERE id = ?1",
                [&operation],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(state, "retry");
        drop(connection);
        drop(reopened);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn sqlite_store_removes_pair_and_associated_rows() {
        let path = std::env::temp_dir().join(format!(
            "koofr-sync-remove-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let store = SyncStore::open(path.clone()).unwrap();
        store.insert_pair(&test_pair("pair-a", "owner-a")).unwrap();
        store
            .add_issue("pair-a", "a.txt", "test", "problem")
            .unwrap();
        let operation = store.operation("pair-a", "upload", "a.txt").unwrap();
        store.delete_pair("pair-a").unwrap();
        assert!(store.pair("pair-a").unwrap().is_none());
        let connection = store.connection.lock().unwrap();
        let issue_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sync_issues WHERE pair_id = 'pair-a'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let operation_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sync_operations WHERE id = ?1",
                [&operation],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(issue_count, 0);
        assert_eq!(operation_count, 0);
        drop(connection);
        drop(store);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn initial_sync_never_plans_deletes() {
        let mut local = BTreeMap::new();
        local.insert(
            "a.txt".to_owned(),
            LocalEntry {
                kind: "file".to_owned(),
                identity: identity("a", 1),
                path: PathBuf::from("a.txt"),
            },
        );
        let actions = plan_actions(
            &BTreeMap::new(),
            &ScanResult {
                local,
                remote: BTreeMap::new(),
            },
        );
        assert!(actions.iter().all(|action| !matches!(
            action,
            Action::DeleteLocal { .. } | Action::DeleteRemote { .. }
        )));
    }

    #[test]
    fn ignore_rules_support_negation() {
        let rules = vec![
            IgnoreRule {
                pattern: "*.tmp".to_owned(),
                negated: false,
            },
            IgnoreRule {
                pattern: "keep.tmp".to_owned(),
                negated: true,
            },
        ];
        assert!(matches_ignore(&rules, "drop.tmp"));
        assert!(!matches_ignore(&rules, "keep.tmp"));
    }

    #[test]
    fn rejects_nested_roots() {
        assert!(remote_roots_overlap(
            &MountId::parse("m".to_owned()).unwrap(),
            &RemotePath::parse("/a".to_owned()).unwrap(),
            &MountId::parse("m".to_owned()).unwrap(),
            &RemotePath::parse("/a/b".to_owned()).unwrap()
        ));
    }

    #[test]
    fn one_sided_change_propagates_with_baseline_identity() {
        let old = identity("old", 1);
        let changed = identity("new", 2);
        let mut baseline = BTreeMap::new();
        baseline.insert(
            "a.txt".to_owned(),
            SnapshotEntry {
                kind: "file".to_owned(),
                local: Some(old.clone()),
                remote: Some(old.clone()),
            },
        );
        let mut local = BTreeMap::new();
        local.insert(
            "a.txt".to_owned(),
            LocalEntry {
                kind: "file".to_owned(),
                identity: changed,
                path: PathBuf::from("a.txt"),
            },
        );
        let mut remote = BTreeMap::new();
        remote.insert(
            "a.txt".to_owned(),
            RemoteEntry {
                kind: "file".to_owned(),
                identity: old,
                path: RemotePath::parse("/a.txt".to_owned()).unwrap(),
            },
        );
        let actions = plan_actions(&baseline, &ScanResult { local, remote });
        assert!(matches!(actions.as_slice(), [Action::Upload { .. }]));
    }

    #[test]
    fn simultaneous_same_content_change_only_updates_baseline() {
        let old = identity("old", 1);
        let changed = identity("same-new", 2);
        let mut baseline = BTreeMap::new();
        baseline.insert(
            "a.txt".to_owned(),
            SnapshotEntry {
                kind: "file".to_owned(),
                local: Some(old.clone()),
                remote: Some(old),
            },
        );
        let local = LocalEntry {
            kind: "file".to_owned(),
            identity: changed.clone(),
            path: PathBuf::from("a.txt"),
        };
        let remote = RemoteEntry {
            kind: "file".to_owned(),
            identity: changed,
            path: RemotePath::parse("/a.txt".to_owned()).unwrap(),
        };
        let actions = plan_actions(
            &baseline,
            &ScanResult {
                local: BTreeMap::from([(String::from("a.txt"), local)]),
                remote: BTreeMap::from([(String::from("a.txt"), remote)]),
            },
        );
        assert!(actions.is_empty());
    }

    #[test]
    fn simultaneous_different_changes_keep_both_as_conflict() {
        let old = identity("old", 1);
        let local = LocalEntry {
            kind: "file".to_owned(),
            identity: identity("local", 2),
            path: PathBuf::from("a.txt"),
        };
        let remote = RemoteEntry {
            kind: "file".to_owned(),
            identity: identity("remote", 3),
            path: RemotePath::parse("/a.txt".to_owned()).unwrap(),
        };
        let baseline = BTreeMap::from([(
            String::from("a.txt"),
            SnapshotEntry {
                kind: "file".to_owned(),
                local: Some(old.clone()),
                remote: Some(old),
            },
        )]);
        let actions = plan_actions(
            &baseline,
            &ScanResult {
                local: BTreeMap::from([(String::from("a.txt"), local)]),
                remote: BTreeMap::from([(String::from("a.txt"), remote)]),
            },
        );
        assert!(matches!(actions.as_slice(), [Action::Conflict { .. }]));
    }

    #[test]
    fn deletion_and_change_restores_deleted_side() {
        let old = identity("old", 1);
        let changed = identity("remote-new", 2);
        let baseline = BTreeMap::from([(
            String::from("a.txt"),
            SnapshotEntry {
                kind: "file".to_owned(),
                local: Some(old.clone()),
                remote: Some(old),
            },
        )]);
        let remote = RemoteEntry {
            kind: "file".to_owned(),
            identity: changed,
            path: RemotePath::parse("/a.txt".to_owned()).unwrap(),
        };
        let actions = plan_actions(
            &baseline,
            &ScanResult {
                local: BTreeMap::new(),
                remote: BTreeMap::from([(String::from("a.txt"), remote)]),
            },
        );
        assert!(matches!(actions.as_slice(), [Action::Download { .. }]));
    }

    #[test]
    fn type_conflict_is_reported_as_a_file_action() {
        let local = LocalEntry {
            kind: "file".to_owned(),
            identity: identity("local", 1),
            path: PathBuf::from("a"),
        };
        let remote = RemoteEntry {
            kind: "dir".to_owned(),
            identity: Identity {
                size: 0,
                modified: 1,
                hash: String::new(),
            },
            path: RemotePath::parse("/a".to_owned()).unwrap(),
        };
        let actions = plan_actions(
            &BTreeMap::new(),
            &ScanResult {
                local: BTreeMap::from([(String::from("a"), local)]),
                remote: BTreeMap::from([(String::from("a"), remote)]),
            },
        );
        assert!(matches!(actions.as_slice(), [Action::TypeConflict { .. }]));
    }

    #[test]
    fn case_folded_paths_are_reported_instead_of_overwritten() {
        let make_entry = |path: &str| LocalEntry {
            kind: "file".to_owned(),
            identity: identity(path, 1),
            path: PathBuf::from(path),
        };
        let actions = plan_actions(
            &BTreeMap::new(),
            &ScanResult {
                local: BTreeMap::from([
                    (String::from("Foo.txt"), make_entry("Foo.txt")),
                    (String::from("foo.txt"), make_entry("foo.txt")),
                ]),
                remote: BTreeMap::new(),
            },
        );
        assert_eq!(actions.len(), 2);
        assert!(
            actions
                .iter()
                .all(|action| matches!(action, Action::TypeConflict { .. }))
        );
    }

    #[test]
    fn baseline_stats_include_files_and_directories_per_side() {
        let baseline = BTreeMap::from([
            (
                "folder".to_owned(),
                SnapshotEntry {
                    kind: "dir".to_owned(),
                    local: Some(identity("", 1)),
                    remote: None,
                },
            ),
            (
                "file.bin".to_owned(),
                SnapshotEntry {
                    kind: "file".to_owned(),
                    local: Some(Identity {
                        size: 9,
                        ..identity("a", 1)
                    }),
                    remote: Some(Identity {
                        size: 11,
                        ..identity("b", 1)
                    }),
                },
            ),
        ]);
        let (local, remote) = baseline_endpoint_stats(&baseline);
        assert_eq!(
            (local.file_count, local.directory_count, local.bytes),
            (1, 1, 9)
        );
        assert_eq!(
            (remote.file_count, remote.directory_count, remote.bytes),
            (1, 0, 11)
        );
    }

    #[test]
    fn local_scan_reuses_unchanged_hash_and_reports_progressive_stats() {
        let root =
            std::env::temp_dir().join(format!("koofr-sync-local-scan-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let file = root.join("sample.bin");
        fs::write(&file, b"unchanged").unwrap();

        let initial = scan_local_cached(&root, &BTreeMap::new(), None).unwrap();
        let initial_identity = initial.get("sample.bin").unwrap().identity.clone();
        let cached_hash = "cached-without-reading-file".to_owned();
        let known = BTreeMap::from([(
            "sample.bin".to_owned(),
            Identity {
                hash: cached_hash.clone(),
                ..initial_identity
            },
        )]);
        let telemetry = Arc::new(Mutex::new(SyncTelemetry::default()));

        let scanned = scan_local_cached(&root, &known, Some(&telemetry)).unwrap();
        assert_eq!(scanned["sample.bin"].identity.hash, cached_hash);
        let public = telemetry.lock().unwrap().public();
        assert_eq!(public.0.file_count, 1);
        assert_eq!(public.0.bytes, 9);

        fs::write(&file, b"changed-size").unwrap();
        let rescanned = scan_local_cached(&root, &known, None).unwrap();
        assert_ne!(rescanned["sample.bin"].identity.hash, cached_hash);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn baseline_local_identity_cache_excludes_remote_only_entries() {
        let baseline = BTreeMap::from([
            (
                "local.txt".to_owned(),
                SnapshotEntry {
                    kind: "file".to_owned(),
                    local: Some(identity("local", 1)),
                    remote: None,
                },
            ),
            (
                "remote.txt".to_owned(),
                SnapshotEntry {
                    kind: "file".to_owned(),
                    local: None,
                    remote: Some(identity("remote", 1)),
                },
            ),
        ]);

        let known = baseline_local_identities(&baseline);
        assert_eq!(known.len(), 1);
        assert_eq!(known["local.txt"].hash, "local");
    }

    #[test]
    fn direction_totals_classify_conflicts_both_ways() {
        let local = LocalEntry {
            kind: "file".to_owned(),
            identity: Identity {
                size: 7,
                ..identity("l", 1)
            },
            path: PathBuf::from("a"),
        };
        let remote = RemoteEntry {
            kind: "file".to_owned(),
            identity: Identity {
                size: 13,
                ..identity("r", 1)
            },
            path: RemotePath::parse("/a".to_owned()).unwrap(),
        };
        let actions = vec![
            Action::Upload {
                relative: "a".to_owned(),
                local: local.clone(),
                expected_remote: None,
            },
            Action::Conflict {
                relative: "b".to_owned(),
                local,
                remote,
            },
        ];
        assert_eq!(action_direction_totals(&actions), (2, 14, 1, 13));
    }

    #[test]
    fn telemetry_reports_active_bytes_then_zeroes_rate_when_idle() {
        let mut telemetry = SyncTelemetry::default();
        telemetry.reset_for_scan(
            SyncEndpointStats::default(),
            SyncEndpointStats::default(),
            1,
            100,
            0,
            0,
        );
        telemetry.record(true, 40);
        assert_eq!(telemetry.public().2.completed_bytes, 40);
        telemetry.complete_action(1, 100, 0, 0);
        telemetry.clear_active();
        assert_eq!(telemetry.public().2.completed_bytes, 100);
        assert_eq!(telemetry.public().2.bytes_per_second, 0);
    }
}
