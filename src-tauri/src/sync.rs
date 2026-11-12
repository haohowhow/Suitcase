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
    cloud_files::{self, NativeCallbackRequest},
    error::AppError,
    file_ops::{MountId, RemoteName, RemotePath},
    koofr_api::KoofrApi,
    transfer::{
        NetworkRetryPolicy, NetworkRetryRequest, TransferDirection, TransferManager,
        should_retry_network, wait_for_network_retry,
    },
};

pub fn wake_native_pair(app: &AppHandle, pair_id: &str) {
    let state = app.state::<AppState>();
    if let Ok(runtimes) = state.sync.runtimes.lock()
        && let Some(runtime) = runtimes.get(pair_id)
    {
        runtime.wake.notify_one();
    }
}

pub async fn handle_native_callback(
    app: AppHandle,
    pair_id: String,
    request: NativeCallbackRequest,
    cancel: CancellationToken,
) {
    let manager = app.state::<AppState>().sync.clone();
    if let NativeCallbackRequest::Delete { operation, .. } = &request {
        let prepared = manager.prepare_native_delete(&pair_id, &request);
        let (pair, entry) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                let offline = matches!(error, AppError::Network(_) | AppError::NotAuthenticated);
                manager.set_native_provider_status(
                    &pair_id,
                    if offline {
                        cloud_files::NativeProviderStatus::Offline
                    } else {
                        cloud_files::NativeProviderStatus::Error
                    },
                );
                let acknowledgement = if offline {
                    cloud_files::complete_delete_network_error(*operation)
                } else {
                    cloud_files::complete_delete_error(*operation)
                };
                if acknowledgement.is_err() {
                    manager.set_native_provider_status(
                        &pair_id,
                        cloud_files::NativeProviderStatus::Error,
                    );
                }
                return;
            }
        };

        // The Explorer delete operation must not wait for a remote HTTP request. After
        // the pair, path, and opaque identity have been validated, acknowledge the
        // local delete immediately and continue the conditional Koofr delete in this
        // asynchronous task. The normal scanner remains the retry/reconciliation path.
        if cloud_files::complete_delete_success(*operation).is_err() {
            manager.set_native_provider_status(&pair_id, cloud_files::NativeProviderStatus::Error);
            return;
        }
        manager.set_native_provider_status(&pair_id, cloud_files::NativeProviderStatus::Syncing);
        let result = manager.delete_native_placeholder(&app, &pair, &entry).await;
        match result {
            Ok(()) => manager
                .set_native_provider_status(&pair_id, cloud_files::NativeProviderStatus::Idle),
            Err(error) => manager.set_native_provider_status(
                &pair_id,
                if matches!(error, AppError::Network(_) | AppError::NotAuthenticated) {
                    cloud_files::NativeProviderStatus::Offline
                } else {
                    cloud_files::NativeProviderStatus::Error
                },
            ),
        }
        wake_native_pair(&app, &pair_id);
        return;
    }

    let result = match &request {
        NativeCallbackRequest::FetchPlaceholders { .. } => {
            manager
                .fetch_native_placeholders(&app, &pair_id, &request, &cancel)
                .await
        }
        NativeCallbackRequest::FetchData { .. } => {
            manager
                .fetch_native_data(&app, &pair_id, &request, &cancel)
                .await
        }
        NativeCallbackRequest::Delete { .. } => unreachable!("delete handled above"),
    };
    if let Err(error) = result {
        manager.set_native_provider_status(
            &pair_id,
            if matches!(error, AppError::Network(_) | AppError::NotAuthenticated) {
                cloud_files::NativeProviderStatus::Offline
            } else {
                cloud_files::NativeProviderStatus::Error
            },
        );
        let operation = match request {
            NativeCallbackRequest::FetchPlaceholders { operation, .. } => {
                if cancel.is_cancelled() || matches!(error, AppError::Cancelled) {
                    cloud_files::complete_placeholders_cancelled(operation);
                } else {
                    cloud_files::complete_placeholders_error(operation);
                }
                return;
            }
            NativeCallbackRequest::FetchData { operation, .. } => operation,
            NativeCallbackRequest::Delete { .. } => return,
        };
        if cancel.is_cancelled() || matches!(error, AppError::Cancelled) {
            cloud_files::complete_cancelled(operation);
        } else if matches!(error, AppError::Network(_) | AppError::NotAuthenticated) {
            cloud_files::complete_network_error(operation);
        } else {
            cloud_files::complete_unsuccessful(operation);
        }
    }
}

const SYNC_EVENT: &str = "koofr://sync-state";
const REMOTE_SCAN_INTERVAL: Duration = Duration::from_secs(60);
const REMOTE_SCAN_CONCURRENCY: usize = 4;
const LOCAL_DEBOUNCE: Duration = Duration::from_secs(2);
const SYNC_PROGRESS_INTERVAL: Duration = Duration::from_secs(1);
const NATIVE_REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

pub fn cleanup_native_roots_for_uninstall(database_path: &Path) -> Result<(), AppError> {
    if !database_path.is_file() {
        return Ok(());
    }
    let connection = Connection::open_with_flags(
        database_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| AppError::LocalData)?;
    let columns = connection
        .prepare("PRAGMA table_info(sync_pairs)")
        .and_then(|mut statement| {
            statement
                .query_map([], |row| row.get::<_, String>(1))?
                .collect::<Result<Vec<_>, _>>()
        })
        .map_err(|_| AppError::LocalData)?;
    if !columns.iter().any(|column| column == "sync_mode") {
        return Ok(());
    }
    let mut statement = connection
        .prepare("SELECT id, local_root FROM sync_pairs WHERE sync_mode = 'onDemand'")
        .map_err(|_| AppError::LocalData)?;
    let roots = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                PathBuf::from(row.get::<_, String>(1)?),
            ))
        })
        .map_err(|_| AppError::LocalData)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::LocalData)?;
    for (pair_id, root) in roots {
        if cloud_files::is_owned_sync_root(&root, &pair_id) {
            cloud_files::unregister(&root, &pair_id)?;
        }
    }
    Ok(())
}

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
    pub sync_mode: String,
    pub root_kind: String,
    pub migration_state: String,
    pub native_connected: bool,
    pub online_only_files: u64,
    pub locally_available_files: u64,
    pub pinned_files: u64,
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
pub struct SyncTaskSummary {
    pub id: String,
    pub operation: String,
    pub direction: String,
    pub display_name: String,
    pub state: String,
    pub bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncStateEvent {
    pair_id: String,
    sync_mode: String,
    root_kind: String,
    migration_state: String,
    native_connected: bool,
    online_only_files: u64,
    locally_available_files: u64,
    pinned_files: u64,
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
    sync_mode: String,
    root_kind: String,
    migration_state: String,
    online_only_files: u64,
    locally_available_files: u64,
    pinned_files: u64,
}

#[derive(Clone, Debug)]
struct NativeSyncEntry {
    entry_key: String,
    pair_id: String,
    local_relative: String,
    remote_path: RemotePath,
    kind: String,
    identity: Identity,
    populated: bool,
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
    MoveRemote {
        from_relative: String,
        to_relative: String,
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
    tasks: Arc<Mutex<Vec<SyncTaskSummary>>>,
    native_connection: Option<Arc<cloud_files::CloudConnection>>,
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

fn ensure_sync_pair_column(
    connection: &Connection,
    name: &str,
    definition: &str,
) -> Result<(), AppError> {
    let mut statement = connection
        .prepare("PRAGMA table_info(sync_pairs)")
        .map_err(|_| AppError::LocalData)?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|_| AppError::LocalData)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::LocalData)?;
    if columns.iter().any(|column| column == name) {
        return Ok(());
    }
    connection
        .execute(
            &format!("ALTER TABLE sync_pairs ADD COLUMN {name} {definition}"),
            [],
        )
        .map_err(|_| AppError::LocalData)?;
    Ok(())
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
                 );
                 CREATE TABLE IF NOT EXISTS sync_entries (
                   entry_key TEXT PRIMARY KEY,
                   pair_id TEXT NOT NULL,
                   local_relative TEXT NOT NULL,
                   remote_path TEXT NOT NULL,
                   kind TEXT NOT NULL,
                   size INTEGER NOT NULL,
                   modified INTEGER NOT NULL,
                   hash TEXT NOT NULL,
                   populated INTEGER NOT NULL DEFAULT 0,
                   UNIQUE(pair_id, local_relative)
                 );",
            )
            .map_err(|_| AppError::LocalData)?;
        ensure_sync_pair_column(&connection, "sync_mode", "TEXT NOT NULL DEFAULT 'classic'")?;
        ensure_sync_pair_column(&connection, "root_kind", "TEXT NOT NULL DEFAULT 'custom'")?;
        ensure_sync_pair_column(
            &connection,
            "migration_state",
            "TEXT NOT NULL DEFAULT 'none'",
        )?;
        ensure_sync_pair_column(
            &connection,
            "online_only_files",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        ensure_sync_pair_column(
            &connection,
            "locally_available_files",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        ensure_sync_pair_column(&connection, "pinned_files", "INTEGER NOT NULL DEFAULT 0")?;
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
                        last_error_code, baseline_json, sync_mode, root_kind, migration_state,
                        online_only_files, locally_available_files, pinned_files
                 FROM sync_pairs WHERE owner_id = ?1 ORDER BY id",
            )
            .map_err(|_| AppError::LocalData)?;
        let rows = statement
            .query_map([owner_id], decode_pair)
            .map_err(|_| AppError::LocalData)?;
        rows.map(|row| row.map_err(|_| AppError::LocalData))
            .collect()
    }

    fn has_on_demand_pairs(&self) -> Result<bool, AppError> {
        let connection = self.connection.lock().map_err(|_| AppError::LocalData)?;
        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sync_pairs WHERE sync_mode = 'onDemand'",
                [],
                |row| row.get(0),
            )
            .map_err(|_| AppError::LocalData)?;
        Ok(count > 0)
    }

    fn pair(&self, pair_id: &str) -> Result<Option<SyncPairRecord>, AppError> {
        let connection = self.connection.lock().map_err(|_| AppError::LocalData)?;
        connection
            .query_row(
                "SELECT id, owner_id, local_root, mount_id, remote_path, paused, status,
                        pending_items, pending_bytes, completed_items, total_items,
                        completed_bytes, total_bytes, last_synced_at, issue_count,
                        last_error_code, baseline_json, sync_mode, root_kind, migration_state,
                        online_only_files, locally_available_files, pinned_files
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
                 (id, owner_id, local_root, mount_id, remote_path, paused, status, baseline_json,
                  sync_mode, root_kind, migration_state)
                 VALUES (?1, ?2, ?3, ?4, ?5, 0, 'initializing', '{}', ?6, ?7, ?8)",
                params![
                    pair.id,
                    pair.owner_id,
                    pair.local_root.to_string_lossy().to_string(),
                    pair.mount_id.as_str(),
                    pair.remote_path.as_str(),
                    pair.sync_mode,
                    pair.root_kind,
                    pair.migration_state,
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
            .execute("DELETE FROM sync_entries WHERE pair_id = ?1", [pair_id])
            .map_err(|_| AppError::LocalData)?;
        transaction
            .execute("DELETE FROM sync_pairs WHERE id = ?1", [pair_id])
            .map_err(|_| AppError::LocalData)?;
        transaction.commit().map_err(|_| AppError::LocalData)
    }

    fn native_entry(&self, entry_key: &str) -> Result<Option<NativeSyncEntry>, AppError> {
        let connection = self.connection.lock().map_err(|_| AppError::LocalData)?;
        connection
            .query_row(
                "SELECT entry_key, pair_id, local_relative, remote_path, kind, size, modified, hash, populated
                 FROM sync_entries WHERE entry_key = ?1",
                [entry_key],
                decode_native_entry,
            )
            .optional()
            .map_err(|_| AppError::LocalData)
    }

    fn native_entry_for_local(
        &self,
        pair_id: &str,
        local_relative: &str,
    ) -> Result<Option<NativeSyncEntry>, AppError> {
        let connection = self.connection.lock().map_err(|_| AppError::LocalData)?;
        connection
            .query_row(
                "SELECT entry_key, pair_id, local_relative, remote_path, kind, size, modified, hash, populated
                 FROM sync_entries WHERE pair_id = ?1 AND local_relative = ?2",
                params![pair_id, local_relative],
                decode_native_entry,
            )
            .optional()
            .map_err(|_| AppError::LocalData)
    }

    fn native_entries_for_pair(&self, pair_id: &str) -> Result<Vec<NativeSyncEntry>, AppError> {
        let connection = self.connection.lock().map_err(|_| AppError::LocalData)?;
        let mut statement = connection
            .prepare(
                "SELECT entry_key, pair_id, local_relative, remote_path, kind, size, modified, hash, populated
                 FROM sync_entries WHERE pair_id = ?1 ORDER BY local_relative",
            )
            .map_err(|_| AppError::LocalData)?;
        let rows = statement
            .query_map([pair_id], decode_native_entry)
            .map_err(|_| AppError::LocalData)?;
        rows.map(|row| row.map_err(|_| AppError::LocalData))
            .collect()
    }

    fn upsert_native_entry(&self, mut entry: NativeSyncEntry) -> Result<NativeSyncEntry, AppError> {
        if let Some(existing) =
            self.native_entry_for_local(&entry.pair_id, &entry.local_relative)?
        {
            entry.entry_key = existing.entry_key;
            entry.populated |= existing.populated;
        }
        let connection = self.connection.lock().map_err(|_| AppError::LocalData)?;
        connection
            .execute(
                "INSERT INTO sync_entries
                 (entry_key, pair_id, local_relative, remote_path, kind, size, modified, hash, populated)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(pair_id, local_relative) DO UPDATE SET
                   remote_path = excluded.remote_path,
                   kind = excluded.kind,
                   size = excluded.size,
                   modified = excluded.modified,
                   hash = excluded.hash,
                   populated = MAX(sync_entries.populated, excluded.populated)",
                params![
                    entry.entry_key,
                    entry.pair_id,
                    entry.local_relative,
                    entry.remote_path.as_str(),
                    entry.kind,
                    entry.identity.size,
                    entry.identity.modified,
                    entry.identity.hash,
                    i64::from(entry.populated),
                ],
            )
            .map_err(|_| AppError::LocalData)?;
        Ok(entry)
    }

    fn set_native_directory_populated(
        &self,
        pair_id: &str,
        local_relative: &str,
    ) -> Result<(), AppError> {
        if local_relative.is_empty() {
            return Ok(());
        }
        let connection = self.connection.lock().map_err(|_| AppError::LocalData)?;
        connection
            .execute(
                "UPDATE sync_entries SET populated = 1 WHERE pair_id = ?1 AND local_relative = ?2 AND kind = 'dir'",
                params![pair_id, local_relative],
            )
            .map_err(|_| AppError::LocalData)?;
        Ok(())
    }

    fn rename_native_entry(
        &self,
        pair_id: &str,
        from_relative: &str,
        to_relative: &str,
        remote_path: &RemotePath,
    ) -> Result<(), AppError> {
        let connection = self.connection.lock().map_err(|_| AppError::LocalData)?;
        connection
            .execute(
                "UPDATE sync_entries SET local_relative = ?3, remote_path = ?4
                 WHERE pair_id = ?1 AND local_relative = ?2",
                params![pair_id, from_relative, to_relative, remote_path.as_str()],
            )
            .map_err(|_| AppError::LocalData)?;
        Ok(())
    }

    fn update_native_counts(
        &self,
        pair_id: &str,
        online_only: u64,
        available: u64,
        pinned: u64,
    ) -> Result<(), AppError> {
        let connection = self.connection.lock().map_err(|_| AppError::LocalData)?;
        connection
            .execute(
                "UPDATE sync_pairs SET online_only_files = ?2, locally_available_files = ?3, pinned_files = ?4 WHERE id = ?1",
                params![pair_id, online_only as i64, available as i64, pinned as i64],
            )
            .map_err(|_| AppError::LocalData)?;
        Ok(())
    }

    fn set_migration_state(&self, pair_id: &str, state: &str) -> Result<(), AppError> {
        if !matches!(state, "none" | "preflight" | "staging" | "ready" | "failed") {
            return Err(AppError::InvalidInput("sync migration state"));
        }
        let connection = self.connection.lock().map_err(|_| AppError::LocalData)?;
        connection
            .execute(
                "UPDATE sync_pairs SET migration_state = ?2 WHERE id = ?1",
                params![pair_id, state],
            )
            .map_err(|_| AppError::LocalData)?;
        Ok(())
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
        sync_mode: row.get(17)?,
        root_kind: row.get(18)?,
        migration_state: row.get(19)?,
        online_only_files: row.get::<_, i64>(20)?.max(0) as u64,
        locally_available_files: row.get::<_, i64>(21)?.max(0) as u64,
        pinned_files: row.get::<_, i64>(22)?.max(0) as u64,
    })
}

fn decode_native_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<NativeSyncEntry> {
    let remote_path =
        RemotePath::parse(row.get::<_, String>(3)?).map_err(|_| rusqlite::Error::InvalidQuery)?;
    Ok(NativeSyncEntry {
        entry_key: row.get(0)?,
        pair_id: row.get(1)?,
        local_relative: row.get(2)?,
        remote_path,
        kind: row.get(4)?,
        identity: Identity {
            size: row.get(5)?,
            modified: row.get(6)?,
            hash: row.get(7)?,
        },
        populated: row.get::<_, i64>(8)? != 0,
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
                let runtime = self
                    .runtimes
                    .lock()
                    .map_err(|_| AppError::LocalData)?
                    .get(&pair.id)
                    .map(|runtime| {
                        (
                            runtime.telemetry.clone(),
                            runtime.native_connection.is_some(),
                        )
                    });
                let native_connected = runtime.as_ref().is_some_and(|runtime| runtime.1);
                summary(
                    pair,
                    runtime.as_ref().map(|runtime| runtime.0.as_ref()),
                    native_connected,
                )
            })
            .collect()
    }

    pub fn native_background_required(&self) -> Result<bool, AppError> {
        self.store.has_on_demand_pairs()
    }

    pub fn list_issues(&self, owner_id: &str, pair_id: &str) -> Result<Vec<SyncIssue>, AppError> {
        let pair = self.store.pair(pair_id)?.ok_or(AppError::NotFound)?;
        if pair.owner_id != owner_id {
            return Err(AppError::Forbidden);
        }
        self.store.issues(pair_id)
    }

    pub fn list_tasks(
        &self,
        owner_id: &str,
        pair_id: &str,
    ) -> Result<Vec<SyncTaskSummary>, AppError> {
        let pair = self.store.pair(pair_id)?.ok_or(AppError::NotFound)?;
        if pair.owner_id != owner_id {
            return Err(AppError::Forbidden);
        }
        let tasks = self.task_tracker_for(pair_id)?;
        let tasks = tasks.lock().map_err(|_| AppError::LocalData)?;
        let focus = tasks
            .iter()
            .position(|task| matches!(task.state.as_str(), "running" | "waiting"))
            .or_else(|| tasks.iter().position(|task| task.state == "queued"))
            .or_else(|| tasks.iter().position(|task| task.state == "failed"))
            .unwrap_or_else(|| tasks.len().saturating_sub(10));
        let start = focus.saturating_sub(2);
        Ok(tasks.iter().skip(start).take(30).cloned().collect())
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
            sync_mode: "classic".to_owned(),
            root_kind: "custom".to_owned(),
            migration_state: "none".to_owned(),
            online_only_files: 0,
            locally_available_files: 0,
            pinned_files: 0,
        };
        self.store.insert_pair(&pair)?;
        self.start_pair(app, &pair.id)?;
        summary(pair, None, false)
    }

    pub fn native_capability(&self) -> cloud_files::NativeSyncCapability {
        cloud_files::capability()
    }

    pub async fn create_on_demand_pair(
        &self,
        app: AppHandle,
        owner_id: String,
        local_root: PathBuf,
        mount_id: MountId,
        remote_path: RemotePath,
        root_kind: String,
    ) -> Result<SyncPairSummary, AppError> {
        self.create_on_demand_pair_inner(
            app,
            owner_id,
            local_root,
            mount_id,
            remote_path,
            root_kind,
            "none",
            None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn create_on_demand_pair_inner(
        &self,
        app: AppHandle,
        owner_id: String,
        local_root: PathBuf,
        mount_id: MountId,
        remote_path: RemotePath,
        root_kind: String,
        migration_state: &str,
        ignored_pair_id: Option<&str>,
    ) -> Result<SyncPairSummary, AppError> {
        if !cloud_files::capability().supported {
            return Err(AppError::Initialization);
        }
        if !matches!(root_kind.as_str(), "unified" | "custom")
            || root_kind == "unified" && remote_path.as_str() != "/"
            || root_kind == "custom" && remote_path.as_str() == "/"
        {
            return Err(AppError::InvalidInput("sync root kind"));
        }
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
            if ignored_pair_id.is_some_and(|ignored| existing.id == ignored) {
                continue;
            }
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
            sync_mode: "onDemand".to_owned(),
            root_kind,
            migration_state: migration_state.to_owned(),
            online_only_files: 0,
            locally_available_files: 0,
            pinned_files: 0,
        };
        self.store.insert_pair(&pair)?;
        if let Err(error) = cloud_files::register(&pair.local_root, &pair.id) {
            let _ = self.store.delete_pair(&pair.id);
            return Err(error);
        }
        if let Err(error) = self.start_pair(app, &pair.id) {
            let _ = cloud_files::unregister(&pair.local_root, &pair.id);
            let _ = self.store.delete_pair(&pair.id);
            return Err(error);
        }
        summary(pair, None, true)
    }

    pub async fn migrate_pair(
        &self,
        app: AppHandle,
        owner_id: String,
        source_pair_id: &str,
        local_root: PathBuf,
        root_kind: String,
    ) -> Result<SyncPairSummary, AppError> {
        let source = self.store.pair(source_pair_id)?.ok_or(AppError::NotFound)?;
        if source.owner_id != owner_id {
            return Err(AppError::Forbidden);
        }
        validate_local_root(&local_root).await?;
        if source.sync_mode != "classic" || source.status != "idle" || source.issue_count != 0 {
            return Err(AppError::Conflict);
        }
        if std::fs::read_dir(&local_root)
            .map_err(AppError::Io)?
            .next()
            .is_some()
        {
            return Err(AppError::Conflict);
        }
        self.store
            .set_migration_state(source_pair_id, "preflight")?;
        match self.set_paused(&owner_id, source_pair_id, true) {
            Ok(true) => {}
            Ok(false) => {
                let _ = self.store.set_migration_state(source_pair_id, "none");
                return Err(AppError::Conflict);
            }
            Err(error) => {
                let _ = self.store.set_migration_state(source_pair_id, "none");
                return Err(error);
            }
        }
        let created = self
            .create_on_demand_pair_inner(
                app,
                owner_id.clone(),
                local_root,
                source.mount_id.clone(),
                source.remote_path.clone(),
                root_kind,
                "staging",
                Some(source_pair_id),
            )
            .await;
        let created = match created {
            Ok(created) => created,
            Err(error) => {
                let _ = self.store.set_migration_state(source_pair_id, "none");
                let _ = self.set_paused(&owner_id, source_pair_id, false);
                return Err(error);
            }
        };
        let validation = tokio::time::timeout(Duration::from_secs(60), async {
            loop {
                let current = self.store.pair(&created.id)?.ok_or(AppError::NotFound)?;
                match current.status.as_str() {
                    "idle" => return Ok::<(), AppError>(()),
                    "attention" | "offline" | "paused" => return Err(AppError::Conflict),
                    _ => tokio::time::sleep(Duration::from_millis(250)).await,
                }
            }
        })
        .await
        .map_err(|_| AppError::LocalData)
        .and_then(|result| result);
        if let Err(error) = validation {
            let _ = self.store.set_migration_state(&created.id, "failed");
            let _ = self.remove_pair(&owner_id, &created.id);
            let _ = self.store.set_migration_state(source_pair_id, "none");
            let _ = self.set_paused(&owner_id, source_pair_id, false);
            return Err(error);
        }
        self.store.set_migration_state(&created.id, "ready")?;
        if let Err(error) = self.remove_pair(&owner_id, source_pair_id) {
            let _ = self.remove_pair(&owner_id, &created.id);
            let _ = self.store.set_migration_state(source_pair_id, "none");
            let _ = self.set_paused(&owner_id, source_pair_id, false);
            return Err(error);
        }
        let migrated = self.store.pair(&created.id)?.ok_or(AppError::NotFound)?;
        summary(migrated, None, true)
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
            drop(runtime);
        }
        if pair.sync_mode == "onDemand" {
            cloud_files::unregister(&pair.local_root, &pair.id)?;
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
        if pair.sync_mode == "onDemand"
            && let Some(connection) = runtime.native_connection.as_ref()
        {
            connection.set_status(if paused {
                cloud_files::NativeProviderStatus::Paused
            } else {
                cloud_files::NativeProviderStatus::Idle
            });
        }
        runtime.wake.notify_one();
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

    pub fn open_root(&self, owner_id: &str, pair_id: &str) -> Result<(), AppError> {
        let pair = self.store.pair(pair_id)?.ok_or(AppError::NotFound)?;
        if pair.owner_id != owner_id {
            return Err(AppError::Forbidden);
        }
        crate::local_open::open(&pair.local_root)
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

    fn task_tracker_for(
        &self,
        pair_id: &str,
    ) -> Result<Arc<Mutex<Vec<SyncTaskSummary>>>, AppError> {
        self.runtimes
            .lock()
            .map_err(|_| AppError::LocalData)?
            .get(pair_id)
            .map(|runtime| runtime.tasks.clone())
            .ok_or(AppError::NotFound)
    }

    fn clear_tasks(&self, pair_id: &str) -> Result<(), AppError> {
        self.task_tracker_for(pair_id)?
            .lock()
            .map_err(|_| AppError::LocalData)?
            .clear();
        Ok(())
    }

    fn set_task_plan(&self, pair_id: &str, actions: &[Action]) -> Result<(), AppError> {
        let tasks = actions
            .iter()
            .enumerate()
            .map(|(index, action)| sync_task_summary(index, action))
            .collect();
        *self
            .task_tracker_for(pair_id)?
            .lock()
            .map_err(|_| AppError::LocalData)? = tasks;
        Ok(())
    }

    fn set_task_state(&self, pair_id: &str, index: usize, state: &str) -> Result<(), AppError> {
        if let Some(task) = self
            .task_tracker_for(pair_id)?
            .lock()
            .map_err(|_| AppError::LocalData)?
            .get_mut(index)
        {
            task.state = state.to_owned();
        }
        Ok(())
    }

    fn set_native_provider_status(&self, pair_id: &str, status: cloud_files::NativeProviderStatus) {
        if let Ok(runtimes) = self.runtimes.lock()
            && let Some(connection) = runtimes
                .get(pair_id)
                .and_then(|runtime| runtime.native_connection.as_ref())
        {
            connection.set_status(status);
        }
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
        let native_connection = if pair.sync_mode == "onDemand" {
            cloud_files::register(&pair.local_root, &pair.id)?;
            Some(Arc::new(cloud_files::connect(
                &pair.local_root,
                app.clone(),
                pair.id.clone(),
            )?))
        } else {
            None
        };
        let runtime = PairRuntime {
            cancel: cancel.clone(),
            wake: wake.clone(),
            telemetry: telemetry.clone(),
            tasks: Arc::new(Mutex::new(Vec::new())),
            native_connection,
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
        if pair.sync_mode == "onDemand" {
            tauri::async_runtime::spawn(manager.run_native_pair_loop(app, pair_id, cancel, wake));
        } else {
            tauri::async_runtime::spawn(manager.run_pair_loop(app, pair_id, cancel, wake));
        }
        Ok(())
    }

    async fn fetch_native_placeholders(
        &self,
        app: &AppHandle,
        pair_id: &str,
        request: &NativeCallbackRequest,
        cancel: &CancellationToken,
    ) -> Result<(), AppError> {
        let NativeCallbackRequest::FetchPlaceholders {
            operation,
            local_path: local_directory,
        } = request
        else {
            return Err(AppError::InvalidInput("native callback"));
        };
        let pair = self.store.pair(pair_id)?.ok_or(AppError::NotFound)?;
        if pair.sync_mode != "onDemand" || pair.paused {
            return Err(AppError::Cancelled);
        }
        self.set_native_provider_status(pair_id, cloud_files::NativeProviderStatus::Syncing);
        let relative = native_relative_path(&pair.local_root, local_directory)?;
        let remote_directory = if relative.is_empty() {
            pair.remote_path.clone()
        } else {
            let entry = self
                .store
                .native_entry_for_local(pair_id, &relative)?
                .ok_or(AppError::NotFound)?;
            if entry.kind != "dir" {
                return Err(AppError::InvalidInput("native directory"));
            }
            entry.remote_path
        };
        let rules = load_ignore_rules(&pair.local_root)?;
        let state = app.state::<AppState>();
        let files = tokio::select! {
            result = tokio::time::timeout(NATIVE_REQUEST_TIMEOUT, state.api.list_files(&pair.mount_id, &remote_directory)) => result.map_err(|_| native_timeout_error())??,
            () = cancel.cancelled() => return Err(AppError::Cancelled),
        };
        let mut specs = Vec::new();
        for file in files {
            if cancel.is_cancelled() {
                return Err(AppError::Cancelled);
            }
            let child_relative = if relative.is_empty() {
                file.name.clone()
            } else {
                format!("{relative}/{}", file.name)
            };
            if child_relative != ".syncignore"
                && (is_builtin_ignored(&child_relative) || matches_ignore(&rules, &child_relative))
            {
                continue;
            }
            let name = RemoteName::parse(file.name.clone())?;
            let remote_path = remote_directory.join(&name)?;
            let is_directory = is_directory(&file.entry_type);
            let entry = self.store.upsert_native_entry(NativeSyncEntry {
                entry_key: uuid::Uuid::new_v4().to_string(),
                pair_id: pair_id.to_owned(),
                local_relative: child_relative.clone(),
                remote_path,
                kind: if is_directory { "dir" } else { "file" }.to_owned(),
                identity: Identity {
                    size: file.size,
                    modified: file.modified,
                    hash: file.hash,
                },
                populated: false,
            })?;
            let target = local_path(&pair.local_root, &child_relative)?;
            if cloud_files::placeholder_state(&target)?.is_none() {
                match fs::symlink_metadata(&target) {
                    Ok(_) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        specs.push(native_placeholder_spec(&file.name, &entry));
                    }
                    Err(error) => return Err(AppError::Io(error)),
                }
            }
        }
        cloud_files::transfer_placeholders(*operation, &specs)?;
        self.store
            .set_native_directory_populated(pair_id, &relative)?;
        self.refresh_native_counts(pair_id)?;
        self.emit_summary(app, pair_id)?;
        self.set_native_provider_status(pair_id, cloud_files::NativeProviderStatus::Idle);
        Ok(())
    }

    async fn fetch_native_data(
        &self,
        app: &AppHandle,
        pair_id: &str,
        request: &NativeCallbackRequest,
        cancel: &CancellationToken,
    ) -> Result<(), AppError> {
        let NativeCallbackRequest::FetchData {
            operation,
            entry_key,
            required_offset,
            required_length,
        } = request
        else {
            return Err(AppError::InvalidInput("native callback"));
        };
        if *required_offset < 0 || *required_length < 0 {
            return Err(AppError::InvalidInput("native data range"));
        }
        let pair = self.store.pair(pair_id)?.ok_or(AppError::NotFound)?;
        let mut entry = self
            .store
            .native_entry(entry_key)?
            .filter(|entry| entry.pair_id == pair_id && entry.kind == "file")
            .ok_or(AppError::NotFound)?;
        if pair.paused {
            return Err(AppError::Cancelled);
        }
        self.set_native_provider_status(pair_id, cloud_files::NativeProviderStatus::Syncing);
        let state = app.state::<AppState>();
        let remote = tokio::select! {
            result = tokio::time::timeout(NATIVE_REQUEST_TIMEOUT, state.api.file_info(&pair.mount_id, &entry.remote_path)) => result.map_err(|_| native_timeout_error())??,
            () = cancel.cancelled() => return Err(AppError::Cancelled),
        };
        if is_directory(&remote.entry_type) {
            return Err(AppError::Conflict);
        }
        entry.identity = Identity {
            size: remote.size,
            modified: remote.modified,
            hash: remote.hash,
        };
        entry = self.store.upsert_native_entry(entry)?;

        let transfer_id = uuid::Uuid::new_v4().to_string();
        let _permit = state
            .transfers
            .acquire(TransferDirection::Download, cancel)
            .await?;
        let retry_policy = NetworkRetryPolicy::from(state.settings.network_retry_settings().await);
        let telemetry = self.telemetry_for(pair_id)?;
        let mut digest = Md5::new();
        let mut pending = Vec::with_capacity(1024 * 1024 + 4096);
        let mut offset = 0_i64;
        let mut total = 0_u64;
        let mut retries_completed = 0_u32;
        'download: loop {
            let response_result = tokio::select! {
                result = tokio::time::timeout(
                    NATIVE_REQUEST_TIMEOUT,
                    state.api.download_response_from(&pair.mount_id, &entry.remote_path, total),
                ) => result.map_err(|_| native_timeout_error())?,
                () = cancel.cancelled() => return Err(AppError::Cancelled),
            };
            let response = match response_result {
                Ok(response) => response,
                Err(error) => {
                    let failure = Err::<(), AppError>(error);
                    if !should_retry_network(&failure, retry_policy, retries_completed) {
                        return failure;
                    }
                    retries_completed = retries_completed.saturating_add(1);
                    wait_for_network_retry(NetworkRetryRequest {
                        app,
                        cancel,
                        transfer_id: &transfer_id,
                        direction: TransferDirection::Download,
                        retry_attempt: retries_completed,
                        bytes_transferred: total,
                        total_bytes: Some(entry.identity.size.max(0) as u64),
                        split_progress: None,
                        policy: retry_policy,
                    })
                    .await?;
                    continue;
                }
            };
            if total > 0 && response.status() == reqwest::StatusCode::OK {
                digest = Md5::new();
                pending.clear();
                offset = 0;
                total = 0;
            } else if total > 0 && response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
                return Err(AppError::IncompleteTransfer);
            }
            let mut stream = response.bytes_stream();
            loop {
                let next = tokio::select! {
                    item = tokio::time::timeout(NATIVE_REQUEST_TIMEOUT, stream.next()) => item.map_err(|_| native_timeout_error())?,
                    () = cancel.cancelled() => return Err(AppError::Cancelled),
                };
                match next {
                    Some(Ok(chunk)) => {
                        state
                            .transfers
                            .throttle(&transfer_id, chunk.len(), cancel)
                            .await?;
                        digest.update(&chunk);
                        total = total.saturating_add(chunk.len() as u64);
                        pending.extend_from_slice(&chunk);
                        let aligned = aligned_native_chunk_length(pending.len());
                        if aligned > 0 {
                            cloud_files::transfer_data(*operation, offset, &pending[..aligned])?;
                            offset = offset.saturating_add(aligned as i64);
                            pending.drain(..aligned);
                        }
                        if let Ok(mut telemetry) = telemetry.lock() {
                            telemetry.record(false, chunk.len() as u64);
                        }
                    }
                    Some(Err(error)) => {
                        let failure = Err::<(), AppError>(AppError::Network(error));
                        if !should_retry_network(&failure, retry_policy, retries_completed) {
                            return failure;
                        }
                        retries_completed = retries_completed.saturating_add(1);
                        wait_for_network_retry(NetworkRetryRequest {
                            app,
                            cancel,
                            transfer_id: &transfer_id,
                            direction: TransferDirection::Download,
                            retry_attempt: retries_completed,
                            bytes_transferred: total,
                            total_bytes: Some(entry.identity.size.max(0) as u64),
                            split_progress: None,
                            policy: retry_policy,
                        })
                        .await?;
                        continue 'download;
                    }
                    None => break 'download,
                }
            }
        }
        if !pending.is_empty() || total == 0 {
            cloud_files::transfer_data(*operation, offset, &pending)?;
        }
        if total != entry.identity.size.max(0) as u64 {
            return Err(AppError::IncompleteTransfer);
        }
        let hash = format!("{:x}", digest.finalize());
        if !entry.identity.hash.is_empty() && !hash.eq_ignore_ascii_case(&entry.identity.hash) {
            return Err(AppError::Integrity);
        }
        self.refresh_native_counts(pair_id)?;
        self.emit_summary(app, pair_id)?;
        self.set_native_provider_status(pair_id, cloud_files::NativeProviderStatus::Idle);
        Ok(())
    }

    fn prepare_native_delete(
        &self,
        pair_id: &str,
        request: &NativeCallbackRequest,
    ) -> Result<(SyncPairRecord, NativeSyncEntry), AppError> {
        let NativeCallbackRequest::Delete {
            entry_key,
            local_path: callback_path,
            ..
        } = request
        else {
            return Err(AppError::InvalidInput("native callback"));
        };
        let pair = self.store.pair(pair_id)?.ok_or(AppError::NotFound)?;
        if pair.sync_mode != "onDemand" || pair.paused {
            return Err(AppError::Cancelled);
        }
        let entry = self
            .store
            .native_entry(entry_key)?
            .filter(|entry| entry.pair_id == pair_id)
            .ok_or(AppError::NotFound)?;
        let callback_relative = native_relative_path(&pair.local_root, callback_path)?;
        if callback_relative != entry.local_relative || callback_relative.is_empty() {
            return Err(AppError::Forbidden);
        }
        Ok((pair, entry))
    }

    async fn delete_native_placeholder(
        &self,
        app: &AppHandle,
        pair: &SyncPairRecord,
        entry: &NativeSyncEntry,
    ) -> Result<(), AppError> {
        let state = app.state::<AppState>();
        let deletion = tokio::time::timeout(
            NATIVE_REQUEST_TIMEOUT,
            state.api.delete_conditional(
                &pair.mount_id,
                &entry.remote_path,
                Some((
                    &entry.identity.hash,
                    entry.identity.size,
                    entry.identity.modified,
                )),
            ),
        )
        .await
        .map_err(|_| native_timeout_error())?;
        match deletion {
            Ok(()) | Err(AppError::NotFound) => Ok(()),
            Err(error) => Err(error),
        }
    }

    fn ensure_native_placeholder(
        &self,
        pair: &SyncPairRecord,
        relative: &str,
        remote: &RemoteEntry,
    ) -> Result<(), AppError> {
        let old = self.store.native_entry_for_local(&pair.id, relative)?;
        let entry = self.store.upsert_native_entry(NativeSyncEntry {
            entry_key: uuid::Uuid::new_v4().to_string(),
            pair_id: pair.id.clone(),
            local_relative: relative.to_owned(),
            remote_path: remote.path.clone(),
            kind: remote.kind.clone(),
            identity: remote.identity.clone(),
            populated: old.as_ref().is_some_and(|entry| entry.populated),
        })?;
        let target = local_path(&pair.local_root, relative)?;
        validate_native_local_parent(&pair.local_root, &target, &pair.id)?;
        let name = target
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or(AppError::InvalidInput("sync file name"))?;
        let spec = native_placeholder_spec(name, &entry);
        if let Some(state) = cloud_files::placeholder_state(&target)? {
            let changed = old
                .as_ref()
                .is_none_or(|old| old.kind != entry.kind || old.identity != entry.identity);
            if changed {
                if entry.kind == "file" && state.pinned && state.in_sync {
                    cloud_files::set_pinned(&target, false)?;
                    cloud_files::update_placeholder(&target, &spec, true)?;
                    cloud_files::set_pinned(&target, true)?;
                } else {
                    cloud_files::update_placeholder(
                        &target,
                        &spec,
                        entry.kind == "file" && state.in_sync && !state.partial,
                    )?;
                }
            }
            return Ok(());
        }
        match fs::symlink_metadata(&target) {
            Ok(metadata) => {
                if is_reparse_point(&metadata) {
                    return Err(AppError::InvalidInput("native sync local entry"));
                }
                cloud_files::convert_to_placeholder(
                    &target,
                    &entry.entry_key,
                    entry.kind == "dir",
                )?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let parent = target
                    .parent()
                    .ok_or(AppError::InvalidInput("sync local path"))?;
                fs::create_dir_all(parent)?;
                cloud_files::create_placeholders(parent, &[spec])?;
            }
            Err(error) => return Err(AppError::Io(error)),
        }
        Ok(())
    }

    fn refresh_native_counts(&self, pair_id: &str) -> Result<(), AppError> {
        let pair = self.store.pair(pair_id)?.ok_or(AppError::NotFound)?;
        let mut online_only = 0_u64;
        let mut available = 0_u64;
        let mut pinned = 0_u64;
        for entry in self.store.native_entries_for_pair(pair_id)? {
            if entry.kind != "file" {
                continue;
            }
            let target = local_path(&pair.local_root, &entry.local_relative)?;
            match cloud_files::placeholder_state(&target)? {
                Some(state) => {
                    if state.pinned {
                        pinned = pinned.saturating_add(1);
                    }
                    if state.partial {
                        online_only = online_only.saturating_add(1);
                    } else {
                        available = available.saturating_add(1);
                    }
                }
                None => match fs::symlink_metadata(&target) {
                    Ok(metadata) if metadata.is_file() => {
                        available = available.saturating_add(1);
                    }
                    Ok(_) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(AppError::Io(error)),
                },
            }
        }
        self.store
            .update_native_counts(pair_id, online_only, available, pinned)
    }

    async fn execute_native_action(
        &self,
        app: &AppHandle,
        pair: &SyncPairRecord,
        action: &Action,
        remote_entries: &BTreeMap<String, RemoteEntry>,
        cancel: &CancellationToken,
        telemetry: Arc<Mutex<SyncTelemetry>>,
    ) -> Result<u64, AppError> {
        match action {
            Action::EnsureLocalDir { relative } => {
                let remote = remote_entries.get(relative).ok_or(AppError::NotFound)?;
                self.ensure_native_placeholder(pair, relative, remote)?;
                Ok(0)
            }
            Action::Download {
                relative, remote, ..
            } => {
                self.ensure_native_placeholder(pair, relative, remote)?;
                Ok(0)
            }
            Action::DeleteLocal { relative, expected } => {
                let path = local_path(&pair.local_root, relative)?;
                if let Some(state) = cloud_files::placeholder_state(&path)? {
                    let entry = self
                        .store
                        .native_entry_for_local(&pair.id, relative)?
                        .ok_or(AppError::Conflict)?;
                    if entry.identity != *expected || !state.in_sync {
                        return Err(AppError::Conflict);
                    }
                    if entry.kind == "dir" {
                        fs::remove_dir_all(path)?;
                    } else {
                        fs::remove_file(path)?;
                    }
                    Ok(0)
                } else {
                    execute_action(app, pair, action, self.store.device_id(), cancel, telemetry)
                        .await
                }
            }
            Action::MoveRemote {
                from_relative,
                to_relative,
                remote,
            } => {
                let state = app.state::<AppState>();
                let current = state.api.file_info(&pair.mount_id, &remote.path).await?;
                let current_identity = Identity {
                    size: current.size,
                    modified: current.modified,
                    hash: current.hash,
                };
                if !same_content(&current_identity, &remote.identity) {
                    return Err(AppError::Conflict);
                }
                let destination = remote_path_for(&pair.remote_path, to_relative)?;
                state
                    .api
                    .move_to(&pair.mount_id, &remote.path, &pair.mount_id, &destination)
                    .await?;
                self.store.rename_native_entry(
                    &pair.id,
                    from_relative,
                    to_relative,
                    &destination,
                )?;
                Ok(0)
            }
            _ => execute_action(app, pair, action, self.store.device_id(), cancel, telemetry).await,
        }
    }

    async fn run_native_once(
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
            self.set_native_provider_status(pair_id, cloud_files::NativeProviderStatus::Paused);
            telemetry
                .lock()
                .map_err(|_| AppError::LocalData)?
                .clear_active();
            self.emit_summary(app, pair_id)?;
            return Ok(());
        }
        self.store
            .update_progress(pair_id, "scanning", 0, 0, 0, 0, 0, 0, None, None)?;
        self.clear_tasks(pair_id)?;
        self.set_native_provider_status(pair_id, cloud_files::NativeProviderStatus::Scanning);
        telemetry
            .lock()
            .map_err(|_| AppError::LocalData)?
            .begin_scanning();
        self.emit_summary(app, pair_id)?;

        let native_entries = self.store.native_entries_for_pair(pair_id)?;
        let known = baseline_local_identities(&pair.baseline);
        let root = pair.local_root.clone();
        let scan_pair_id = pair.id.clone();
        let local_scan = tokio::task::spawn_blocking(move || {
            scan_native_local(&root, &scan_pair_id, &native_entries, &known)
        })
        .await
        .map_err(|_| AppError::LocalData)??;
        if cancel.is_cancelled() {
            return Err(AppError::Cancelled);
        }
        let rules = load_ignore_rules(&pair.local_root)?;
        let remote = match scan_native_remote(
            &app.state::<AppState>().api,
            &pair.mount_id,
            &pair.remote_path,
            &rules,
            &local_scan.populated_directories,
        )
        .await
        {
            Ok(remote) => remote,
            Err(error @ (AppError::Network(_) | AppError::NotAuthenticated)) => {
                self.set_native_provider_status(
                    pair_id,
                    cloud_files::NativeProviderStatus::Offline,
                );
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
                    Some(error.log_code()),
                )?;
                self.emit_summary(app, pair_id)?;
                return Err(error);
            }
            Err(error) => return Err(error),
        };
        telemetry
            .lock()
            .map_err(|_| AppError::LocalData)?
            .update_endpoints(
                local_endpoint_stats(&local_scan.entries),
                remote_endpoint_stats(&remote),
            );
        let scan = ScanResult {
            local: local_scan.entries,
            remote,
        };
        let actions = coalesce_native_renames(plan_actions(&pair.baseline, &scan));
        self.set_task_plan(pair_id, &actions)?;
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
        let total_bytes = actions.iter().map(action_bytes).sum::<u64>();
        self.store.update_progress(
            pair_id,
            if actions.is_empty() {
                "idle"
            } else {
                "syncing"
            },
            total_items,
            total_bytes,
            0,
            total_items,
            0,
            total_bytes,
            None,
            None,
        )?;
        if !actions.is_empty() {
            self.set_native_provider_status(pair_id, cloud_files::NativeProviderStatus::Syncing);
        }
        self.emit_summary(app, pair_id)?;

        let mut completed_items = 0_u64;
        let mut completed_bytes = 0_u64;
        for (task_index, action) in actions.iter().enumerate() {
            if cancel.is_cancelled() {
                return Err(AppError::Cancelled);
            }
            self.set_task_state(pair_id, task_index, "running")?;
            let relative = action_relative(action);
            let result = self
                .execute_native_action(app, &pair, action, &scan.remote, cancel, telemetry.clone())
                .await;
            match result {
                Ok(bytes) => {
                    self.set_task_state(pair_id, task_index, "completed")?;
                    completed_items = completed_items.saturating_add(1);
                    completed_bytes = completed_bytes.saturating_add(bytes);
                    let direction = action_direction(action);
                    telemetry
                        .lock()
                        .map_err(|_| AppError::LocalData)?
                        .complete_action(direction.0, direction.1, direction.2, direction.3);
                }
                Err(error @ (AppError::Network(_) | AppError::NotAuthenticated)) => {
                    self.set_task_state(pair_id, task_index, "waiting")?;
                    self.set_native_provider_status(
                        pair_id,
                        cloud_files::NativeProviderStatus::Offline,
                    );
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
                        Some(error.log_code()),
                    )?;
                    self.emit_summary(app, pair_id)?;
                    return Err(error);
                }
                Err(AppError::Conflict) => {
                    self.set_task_state(pair_id, task_index, "queued")?;
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
                    self.set_task_state(pair_id, task_index, "queued")?;
                    return Err(error);
                }
                Err(error) => {
                    self.set_task_state(pair_id, task_index, "failed")?;
                    self.set_native_provider_status(
                        pair_id,
                        cloud_files::NativeProviderStatus::Error,
                    );
                    self.store.add_issue(
                        pair_id,
                        &relative,
                        error.log_code(),
                        "按需同步操作未完成，请检查后重试。",
                    )?;
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
            }
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

        let current_native = self.store.native_entries_for_pair(pair_id)?;
        let current_known = scan
            .local
            .iter()
            .map(|(relative, entry)| (relative.clone(), entry.identity.clone()))
            .collect::<BTreeMap<_, _>>();
        let root = pair.local_root.clone();
        let scan_pair_id = pair.id.clone();
        let local_after_actions = tokio::task::spawn_blocking(move || {
            scan_native_local(&root, &scan_pair_id, &current_native, &current_known)
        })
        .await
        .map_err(|_| AppError::LocalData)??;
        let final_remote = scan_native_remote(
            &app.state::<AppState>().api,
            &pair.mount_id,
            &pair.remote_path,
            &rules,
            &local_after_actions.populated_directories,
        )
        .await?;
        for (relative, remote) in &final_remote {
            if let Some(local) = local_after_actions.entries.get(relative)
                && (remote.kind == "dir" || same_content(&local.identity, &remote.identity))
            {
                self.ensure_native_placeholder(&pair, relative, remote)?;
            }
        }
        let final_native = self.store.native_entries_for_pair(pair_id)?;
        let final_known = local_after_actions
            .entries
            .iter()
            .map(|(relative, entry)| (relative.clone(), entry.identity.clone()))
            .collect::<BTreeMap<_, _>>();
        let root = pair.local_root.clone();
        let scan_pair_id = pair.id.clone();
        let final_local = tokio::task::spawn_blocking(move || {
            scan_native_local(&root, &scan_pair_id, &final_native, &final_known)
        })
        .await
        .map_err(|_| AppError::LocalData)??;
        let baseline = merge_baseline(&pair.baseline, &final_local.entries, &final_remote);
        telemetry
            .lock()
            .map_err(|_| AppError::LocalData)?
            .update_endpoints(
                local_endpoint_stats(&final_local.entries),
                remote_endpoint_stats(&final_remote),
            );
        self.store.save_baseline(pair_id, &baseline, now_millis())?;
        self.refresh_native_counts(pair_id)?;
        telemetry
            .lock()
            .map_err(|_| AppError::LocalData)?
            .clear_active();
        self.emit_summary(app, pair_id)?;
        self.set_native_provider_status(pair_id, cloud_files::NativeProviderStatus::Idle);
        Ok(())
    }

    fn run_native_pair_loop(
        self,
        app: AppHandle,
        pair_id: String,
        cancel: CancellationToken,
        wake: Arc<Notify>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        Box::pin(async move {
            let mut remote_tick = tokio::time::interval(REMOTE_SCAN_INTERVAL);
            remote_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            remote_tick.tick().await;
            let _ = self.run_native_once(&app, &pair_id, &cancel).await;
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = remote_tick.tick() => { let _ = self.run_native_once(&app, &pair_id, &cancel).await; },
                    _ = wake.notified() => {
                        tokio::time::sleep(LOCAL_DEBOUNCE).await;
                        if !cancel.is_cancelled() { let _ = self.run_native_once(&app, &pair_id, &cancel).await; }
                    }
                }
            }
            if let Ok(mut runtimes) = self.runtimes.lock() {
                runtimes.remove(&pair_id);
            }
        })
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
        self.clear_tasks(pair_id)?;
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
        self.set_task_plan(pair_id, &actions)?;
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
        for (task_index, action) in actions.into_iter().enumerate() {
            if cancel.is_cancelled() {
                return Err(AppError::Cancelled);
            }
            self.set_task_state(pair_id, task_index, "running")?;
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
                    self.set_task_state(pair_id, task_index, "completed")?;
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
                    self.set_task_state(pair_id, task_index, "waiting")?;
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
                    self.set_task_state(pair_id, task_index, "queued")?;
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
                    self.set_task_state(pair_id, task_index, "queued")?;
                    telemetry
                        .lock()
                        .map_err(|_| AppError::LocalData)?
                        .clear_active();
                    let _ = self.store.finish_operation(&operation, "retry");
                    return Err(error);
                }
                Err(error) => {
                    self.set_task_state(pair_id, task_index, "failed")?;
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
        let baseline = merge_baseline(&pair.baseline, &final_local, &final_remote);
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
        let (local, remote, upload, download, last_scan_at, native_connected) = self
            .runtimes
            .lock()
            .map_err(|_| AppError::LocalData)?
            .get(pair_id)
            .map(|runtime| {
                runtime
                    .telemetry
                    .lock()
                    .map(|telemetry| {
                        let (local, remote, upload, download, last_scan_at) = telemetry.public();
                        (
                            local,
                            remote,
                            upload,
                            download,
                            last_scan_at,
                            runtime.native_connection.is_some(),
                        )
                    })
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
                    false,
                )
            });
        let event = SyncStateEvent {
            pair_id: pair.id,
            sync_mode: pair.sync_mode,
            root_kind: pair.root_kind,
            migration_state: pair.migration_state,
            native_connected,
            online_only_files: pair.online_only_files,
            locally_available_files: pair.locally_available_files,
            pinned_files: pair.pinned_files,
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
    native_connected: bool,
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
        sync_mode: pair.sync_mode,
        root_kind: pair.root_kind,
        migration_state: pair.migration_state,
        native_connected,
        online_only_files: pair.online_only_files,
        locally_available_files: pair.locally_available_files,
        pinned_files: pair.pinned_files,
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

fn validate_native_local_root(path: &Path, pair_id: &str) -> Result<(), AppError> {
    let metadata = fs::symlink_metadata(path)?;
    if !path.is_absolute() || !metadata.is_dir() || !cloud_files::is_owned_sync_root(path, pair_id)
    {
        return Err(AppError::InvalidInput("native sync local directory"));
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
    left == "/"
        || right == "/"
        || left == right
        || left.starts_with(&format!("{right}/"))
        || right.starts_with(&format!("{left}/"))
}

fn aligned_native_chunk_length(buffered: usize) -> usize {
    buffered / 4096 * 4096
}

fn native_timeout_error() -> AppError {
    AppError::Io(std::io::Error::new(
        std::io::ErrorKind::TimedOut,
        "native sync request timed out",
    ))
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

#[derive(Default)]
struct NativeLocalScan {
    entries: BTreeMap<String, LocalEntry>,
    populated_directories: BTreeSet<String>,
}

fn scan_native_local(
    root: &Path,
    pair_id: &str,
    native_entries: &[NativeSyncEntry],
    known: &BTreeMap<String, Identity>,
) -> Result<NativeLocalScan, AppError> {
    validate_native_local_root(root, pair_id)?;
    let rules = load_ignore_rules(root)?;
    let native = native_entries
        .iter()
        .map(|entry| (entry.local_relative.clone(), entry.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut result = NativeLocalScan::default();
    scan_native_local_directory(root, root, &rules, &native, known, &mut result)?;
    Ok(result)
}

fn scan_native_local_directory(
    root: &Path,
    directory: &Path,
    rules: &[IgnoreRule],
    native: &BTreeMap<String, NativeSyncEntry>,
    known: &BTreeMap<String, Identity>,
    result: &mut NativeLocalScan,
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
        if relative != ".syncignore"
            && (is_builtin_ignored(&relative) || matches_ignore(rules, &relative))
        {
            continue;
        }
        if let Some(state) = cloud_files::placeholder_state(&path)? {
            let Some(native_entry) = native.get(&relative) else {
                continue;
            };
            let identity = if native_entry.kind == "file" && !state.in_sync && !state.partial {
                let metadata = fs::symlink_metadata(&path)?;
                Identity {
                    size: i64::try_from(metadata.len())
                        .map_err(|_| AppError::InvalidInput("local file size"))?,
                    modified: modified_millis(&metadata)?,
                    hash: hash_file(&path)?,
                }
            } else {
                native_entry.identity.clone()
            };
            result.entries.insert(
                relative.clone(),
                LocalEntry {
                    kind: native_entry.kind.clone(),
                    identity,
                    path: path.clone(),
                },
            );
            if native_entry.kind == "dir" && native_entry.populated {
                result.populated_directories.insert(relative);
                scan_native_local_directory(root, &path, rules, native, known, result)?;
            }
            continue;
        }
        let metadata = fs::symlink_metadata(&path)?;
        if is_reparse_point(&metadata) {
            continue;
        }
        if metadata.is_dir() {
            result.populated_directories.insert(relative.clone());
            result.entries.insert(
                relative.clone(),
                LocalEntry {
                    kind: "dir".to_owned(),
                    identity: Identity {
                        size: 0,
                        modified: modified_millis(&metadata)?,
                        hash: String::new(),
                    },
                    path: path.clone(),
                },
            );
            scan_native_local_directory(root, &path, rules, native, known, result)?;
        } else if metadata.is_file() {
            let size = i64::try_from(metadata.len())
                .map_err(|_| AppError::InvalidInput("local file size"))?;
            let modified = modified_millis(&metadata)?;
            let hash = known
                .get(&relative)
                .filter(|identity| {
                    identity.size == size
                        && identity.modified == modified
                        && !identity.hash.is_empty()
                })
                .map(|identity| identity.hash.clone())
                .map_or_else(|| hash_file(&path), Ok)?;
            result.entries.insert(
                relative,
                LocalEntry {
                    kind: "file".to_owned(),
                    identity: Identity {
                        size,
                        modified,
                        hash,
                    },
                    path,
                },
            );
        }
    }
    Ok(())
}

fn native_relative_path(root: &Path, path: &Path) -> Result<String, AppError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| AppError::Forbidden)?
        .to_string_lossy()
        .replace('\\', "/");
    if relative.split('/').any(|segment| segment == "..") {
        return Err(AppError::Forbidden);
    }
    Ok(relative.trim_matches('/').to_owned())
}

fn native_placeholder_spec(name: &str, entry: &NativeSyncEntry) -> cloud_files::PlaceholderSpec {
    cloud_files::PlaceholderSpec {
        name: name.to_owned(),
        entry_key: entry.entry_key.clone(),
        is_directory: entry.kind == "dir",
        size: entry.identity.size.max(0) as u64,
        modified_millis: entry.identity.modified,
    }
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

async fn scan_native_remote(
    api: &KoofrApi,
    mount_id: &MountId,
    root: &RemotePath,
    rules: &[IgnoreRule],
    populated_directories: &BTreeSet<String>,
) -> Result<BTreeMap<String, RemoteEntry>, AppError> {
    let mut result = BTreeMap::new();
    let mut queue = VecDeque::from([(root.clone(), String::new())]);
    while let Some((directory, relative_directory)) = queue.pop_front() {
        for entry in api.list_files(mount_id, &directory).await? {
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
            let kind = if is_directory(&entry.entry_type) {
                "dir"
            } else {
                "file"
            };
            result.insert(
                relative.clone(),
                RemoteEntry {
                    kind: kind.to_owned(),
                    identity: Identity {
                        size: entry.size,
                        modified: entry.modified,
                        hash: entry.hash,
                    },
                    path: remote_path.clone(),
                },
            );
            if kind == "dir" && populated_directories.contains(&relative) {
                queue.push_back((remote_path, relative));
            }
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
        Action::MoveRemote { to_relative, .. } => {
            (1, to_relative.matches('/').count(), to_relative.clone())
        }
        Action::DeleteLocal { relative, .. } | Action::DeleteRemote { relative, .. } => (
            2,
            usize::MAX - relative.matches('/').count(),
            relative.clone(),
        ),
    });
    actions
}

/// Builds the next three-way baseline from the post-sync scans.
///
/// A path is recorded only when both sides agree (same kind and, for files, same
/// content). Anything that diverges — usually because it changed while the run was
/// in progress — keeps its previous baseline entry (or stays absent), so the next
/// scan still sees the change instead of silently accepting the divergence.
fn merge_baseline(
    previous: &BTreeMap<String, SnapshotEntry>,
    local: &BTreeMap<String, LocalEntry>,
    remote: &BTreeMap<String, RemoteEntry>,
) -> BTreeMap<String, SnapshotEntry> {
    let mut baseline = BTreeMap::new();
    for relative in local.keys().chain(remote.keys()) {
        if baseline.contains_key(relative) {
            continue;
        }
        let agreed = match (local.get(relative), remote.get(relative)) {
            (Some(local), Some(remote))
                if local.kind == remote.kind
                    && (local.kind == "dir" || same_content(&local.identity, &remote.identity)) =>
            {
                Some(SnapshotEntry {
                    kind: local.kind.clone(),
                    local: Some(local.identity.clone()),
                    remote: Some(remote.identity.clone()),
                })
            }
            _ => None,
        };
        if let Some(entry) = agreed.or_else(|| previous.get(relative).cloned()) {
            baseline.insert(relative.clone(), entry);
        }
    }
    baseline
}

fn coalesce_native_renames(actions: Vec<Action>) -> Vec<Action> {
    let mut used = vec![false; actions.len()];
    let mut moves = Vec::new();
    for (upload_index, upload) in actions.iter().enumerate() {
        let Action::Upload {
            relative: to_relative,
            local,
            expected_remote: None,
        } = upload
        else {
            continue;
        };
        if local.kind != "file" {
            continue;
        }
        let matching_deletes = actions
            .iter()
            .enumerate()
            .filter(|(_, action)| {
                matches!(action, Action::DeleteRemote { remote, .. }
                    if remote.kind == "file" && same_content(&local.identity, &remote.identity))
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        if matching_deletes.len() != 1 {
            continue;
        }
        let delete_index = matching_deletes[0];
        let competing_uploads = actions
            .iter()
            .filter(|action| {
                matches!(action, Action::Upload { local, expected_remote: None, .. }
                    if local.kind == "file"
                        && matches!(&actions[delete_index], Action::DeleteRemote { remote, .. }
                            if same_content(&local.identity, &remote.identity)))
            })
            .count();
        if competing_uploads != 1 || used[delete_index] {
            continue;
        }
        let Action::DeleteRemote {
            relative: from_relative,
            remote,
        } = &actions[delete_index]
        else {
            continue;
        };
        used[upload_index] = true;
        used[delete_index] = true;
        moves.push(Action::MoveRemote {
            from_relative: from_relative.clone(),
            to_relative: to_relative.clone(),
            remote: remote.clone(),
        });
    }
    let mut result = actions
        .into_iter()
        .enumerate()
        .filter_map(|(index, action)| (!used[index]).then_some(action))
        .collect::<Vec<_>>();
    result.extend(moves);
    result.sort_by_key(|action| match action {
        Action::EnsureLocalDir { relative } | Action::EnsureRemoteDir { relative } => {
            (0_u8, relative.matches('/').count(), relative.clone())
        }
        Action::Upload { relative, .. }
        | Action::Download { relative, .. }
        | Action::Conflict { relative, .. }
        | Action::TypeConflict { relative } => (1, relative.matches('/').count(), relative.clone()),
        Action::MoveRemote { to_relative, .. } => {
            (1, to_relative.matches('/').count(), to_relative.clone())
        }
        Action::DeleteLocal { relative, .. } | Action::DeleteRemote { relative, .. } => (
            2,
            usize::MAX - relative.matches('/').count(),
            relative.clone(),
        ),
    });
    result
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
    // A directory without a usable two-sided baseline is new on the side where it
    // exists. Only a directory that both sides previously agreed on can propagate a
    // deletion; otherwise a freshly created folder would be planned as a delete.
    let new_on_one_side =
        initial || previous.is_none_or(|entry| entry.local.is_none() || entry.remote.is_none());
    if local_dir && !remote_dir {
        if new_on_one_side {
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
        if new_on_one_side {
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
            if let Some(outcome) = delete_local_directory_if_empty(&path)? {
                return match outcome {
                    LocalDirectoryDelete::Removed => Ok(0),
                    // Local content that was not part of the remote deletion (new or
                    // ignored files) wins: restore the remote folder instead of
                    // throwing that content away or stalling on a permanent conflict.
                    LocalDirectoryDelete::NotEmpty => {
                        ensure_remote_directory_tree(
                            &state.api,
                            &pair.mount_id,
                            &pair.remote_path,
                            relative,
                        )
                        .await?;
                        Ok(0)
                    }
                };
            }
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
        Action::MoveRemote {
            to_relative,
            remote,
            ..
        } => {
            let destination = remote_path_for(&pair.remote_path, to_relative)?;
            state
                .api
                .move_to(&pair.mount_id, &remote.path, &pair.mount_id, &destination)
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

#[derive(Debug, PartialEq, Eq)]
enum LocalDirectoryDelete {
    Removed,
    NotEmpty,
}

/// Directory deletions are planned after every child action, deepest first, so by
/// the time a directory itself is processed it should be empty. Directory mtimes
/// change whenever children are removed, which is why they are not compared here.
/// Returns `None` when the path is not a plain directory, so callers fall back to
/// the file identity check.
fn delete_local_directory_if_empty(path: &Path) -> Result<Option<LocalDirectoryDelete>, AppError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(AppError::Io(error)),
    };
    if !metadata.is_dir() {
        return Ok(None);
    }
    if is_reparse_point(&metadata) {
        return Err(AppError::InvalidInput("sync local directory"));
    }
    if fs::read_dir(path)?.next().is_some() {
        return Ok(Some(LocalDirectoryDelete::NotEmpty));
    }
    fs::remove_dir(path)?;
    Ok(Some(LocalDirectoryDelete::Removed))
}

async fn ensure_remote_directory_tree(
    api: &KoofrApi,
    mount_id: &MountId,
    remote_root: &RemotePath,
    relative: &str,
) -> Result<(), AppError> {
    let mut current = remote_root.clone();
    for segment in relative.split('/') {
        let name = RemoteName::parse(segment.to_owned())?;
        let next = current.join(&name)?;
        match api.file_info(mount_id, &next).await {
            Ok(info) if is_directory(&info.entry_type) => {}
            Ok(_) => return Err(AppError::Conflict),
            Err(AppError::NotFound) => api.create_folder(mount_id, &current, &name).await?,
            Err(error) => return Err(error),
        }
        current = next;
    }
    Ok(())
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

fn validate_native_local_parent(root: &Path, target: &Path, pair_id: &str) -> Result<(), AppError> {
    validate_native_local_root(root, pair_id)?;
    let parent = target
        .parent()
        .ok_or(AppError::InvalidInput("native sync local parent"))?;
    let relative = parent
        .strip_prefix(root)
        .map_err(|_| AppError::InvalidInput("native sync local parent"))?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component.as_os_str());
        if cloud_files::placeholder_state(&current)?.is_some() {
            continue;
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.is_dir() && !is_reparse_point(&metadata) => {}
            Ok(_) => return Err(AppError::InvalidInput("native sync local parent")),
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
        Action::MoveRemote { to_relative, .. } => to_relative.clone(),
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
        Action::MoveRemote { .. } => "move_remote",
        Action::Conflict { .. } => "conflict",
        Action::TypeConflict { .. } => "type_conflict",
    }
}

fn sync_task_summary(index: usize, action: &Action) -> SyncTaskSummary {
    let relative = action_relative(action);
    let display_name = relative
        .rsplit('/')
        .find(|segment| !segment.is_empty())
        .unwrap_or("同步根")
        .to_owned();
    let direction = match action {
        Action::Upload { .. }
        | Action::EnsureRemoteDir { .. }
        | Action::DeleteRemote { .. }
        | Action::MoveRemote { .. } => "upload",
        Action::Download { .. } | Action::EnsureLocalDir { .. } | Action::DeleteLocal { .. } => {
            "download"
        }
        Action::Conflict { .. } | Action::TypeConflict { .. } => "sync",
    };
    SyncTaskSummary {
        id: format!("task-{index}"),
        operation: action_name(action).to_owned(),
        direction: direction.to_owned(),
        display_name,
        state: "queued".to_owned(),
        bytes: action_bytes(action),
    }
}

fn action_bytes(action: &Action) -> u64 {
    match action {
        Action::Upload { local, .. } => local.identity.size.max(0) as u64,
        Action::Download { remote, .. } => remote.identity.size.max(0) as u64,
        Action::Conflict { local, remote, .. } => {
            local.identity.size.max(0) as u64 + remote.identity.size.max(0) as u64
        }
        Action::MoveRemote { .. } => 0,
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
            sync_mode: "classic".to_owned(),
            root_kind: "custom".to_owned(),
            migration_state: "none".to_owned(),
            online_only_files: 0,
            locally_available_files: 0,
            pinned_files: 0,
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
        assert!(remote_roots_overlap(
            &MountId::parse("m".to_owned()).unwrap(),
            &RemotePath::parse("/".to_owned()).unwrap(),
            &MountId::parse("m".to_owned()).unwrap(),
            &RemotePath::parse("/documents".to_owned()).unwrap()
        ));
        assert!(!remote_roots_overlap(
            &MountId::parse("m".to_owned()).unwrap(),
            &RemotePath::parse("/a".to_owned()).unwrap(),
            &MountId::parse("other".to_owned()).unwrap(),
            &RemotePath::parse("/a".to_owned()).unwrap()
        ));
    }

    #[test]
    fn native_transfer_chunks_are_four_kib_aligned() {
        assert_eq!(aligned_native_chunk_length(0), 0);
        assert_eq!(aligned_native_chunk_length(4095), 0);
        assert_eq!(aligned_native_chunk_length(4096), 4096);
        assert_eq!(aligned_native_chunk_length(8193), 8192);
    }

    #[test]
    fn task_summary_exposes_only_the_display_name() {
        let task = sync_task_summary(
            3,
            &Action::Upload {
                relative: "private/reports/年度报告.pdf".to_owned(),
                local: LocalEntry {
                    kind: "file".to_owned(),
                    identity: identity("hash", 1),
                    path: PathBuf::from("private/reports/年度报告.pdf"),
                },
                expected_remote: None,
            },
        );
        assert_eq!(task.id, "task-3");
        assert_eq!(task.operation, "upload");
        assert_eq!(task.direction, "upload");
        assert_eq!(task.display_name, "年度报告.pdf");
        assert!(!task.display_name.contains("private"));
        assert_eq!(task.state, "queued");
    }

    #[test]
    fn native_local_rename_becomes_remote_move() {
        let local = LocalEntry {
            kind: "file".to_owned(),
            identity: identity("same", 2),
            path: PathBuf::from("new.txt"),
        };
        let remote = RemoteEntry {
            kind: "file".to_owned(),
            identity: identity("same", 1),
            path: RemotePath::parse("/old.txt".to_owned()).unwrap(),
        };
        let actions = coalesce_native_renames(vec![
            Action::Upload {
                relative: "new.txt".to_owned(),
                local,
                expected_remote: None,
            },
            Action::DeleteRemote {
                relative: "old.txt".to_owned(),
                remote,
            },
        ]);
        assert!(matches!(
            actions.as_slice(),
            [Action::MoveRemote {
                from_relative,
                to_relative,
                ..
            }] if from_relative == "old.txt" && to_relative == "new.txt"
        ));
    }

    #[test]
    fn sqlite_upgrade_adds_native_sync_columns_and_entry_table() {
        let path = std::env::temp_dir().join(format!(
            "koofr-sync-upgrade-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        {
            let connection = Connection::open(&path).unwrap();
            connection
                .execute_batch(
                    "CREATE TABLE sync_pairs (
                       id TEXT PRIMARY KEY, owner_id TEXT NOT NULL, local_root TEXT NOT NULL,
                       mount_id TEXT NOT NULL, remote_path TEXT NOT NULL,
                       paused INTEGER NOT NULL DEFAULT 0, status TEXT NOT NULL DEFAULT 'initializing',
                       pending_items INTEGER NOT NULL DEFAULT 0, pending_bytes INTEGER NOT NULL DEFAULT 0,
                       completed_items INTEGER NOT NULL DEFAULT 0, total_items INTEGER NOT NULL DEFAULT 0,
                       completed_bytes INTEGER NOT NULL DEFAULT 0, total_bytes INTEGER NOT NULL DEFAULT 0,
                       last_synced_at INTEGER, issue_count INTEGER NOT NULL DEFAULT 0,
                       last_error_code TEXT, baseline_json TEXT NOT NULL DEFAULT '{}'
                     );",
                )
                .unwrap();
        }
        let store = SyncStore::open(path.clone()).unwrap();
        let connection = store.connection.lock().unwrap();
        let columns = connection
            .prepare("PRAGMA table_info(sync_pairs)")
            .and_then(|mut statement| {
                statement
                    .query_map([], |row| row.get::<_, String>(1))?
                    .collect::<Result<Vec<_>, _>>()
            })
            .unwrap();
        assert!(columns.iter().any(|column| column == "sync_mode"));
        assert!(columns.iter().any(|column| column == "migration_state"));
        let entry_table: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='sync_entries'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(entry_table, 1);
        drop(connection);
        drop(store);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn native_entries_keep_paths_out_of_placeholder_identity() {
        let path = std::env::temp_dir().join(format!(
            "koofr-sync-native-entry-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let store = SyncStore::open(path.clone()).unwrap();
        let mut pair = test_pair("pair-native", "owner-a");
        pair.sync_mode = "onDemand".to_owned();
        store.insert_pair(&pair).unwrap();
        let entry = store
            .upsert_native_entry(NativeSyncEntry {
                entry_key: uuid::Uuid::new_v4().to_string(),
                pair_id: pair.id.clone(),
                local_relative: "private/report.txt".to_owned(),
                remote_path: RemotePath::parse("/private/report.txt".to_owned()).unwrap(),
                kind: "file".to_owned(),
                identity: identity("abcd", 42),
                populated: false,
            })
            .unwrap();
        let shell_identity = cloud_files::encode_entry_identity(&entry.entry_key).unwrap();
        assert!(!String::from_utf8_lossy(&shell_identity).contains("private"));
        let loaded = store.native_entry(&entry.entry_key).unwrap().unwrap();
        assert_eq!(loaded.remote_path.as_str(), "/private/report.txt");
        drop(store);
        let _ = fs::remove_file(path);
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
    fn unchanged_remote_is_deleted_after_a_local_placeholder_delete() {
        let old = identity("old", 1);
        let baseline = BTreeMap::from([(
            String::from("a.txt"),
            SnapshotEntry {
                kind: "file".to_owned(),
                local: Some(old.clone()),
                remote: Some(old.clone()),
            },
        )]);
        let remote = RemoteEntry {
            kind: "file".to_owned(),
            identity: old,
            path: RemotePath::parse("/a.txt".to_owned()).unwrap(),
        };
        let actions = plan_actions(
            &baseline,
            &ScanResult {
                local: BTreeMap::new(),
                remote: BTreeMap::from([(String::from("a.txt"), remote)]),
            },
        );
        assert!(matches!(actions.as_slice(), [Action::DeleteRemote { .. }]));
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

    fn file_baseline() -> BTreeMap<String, SnapshotEntry> {
        BTreeMap::from([(
            "a.txt".to_owned(),
            SnapshotEntry {
                kind: "file".to_owned(),
                local: Some(identity("a", 1)),
                remote: Some(identity("a", 1)),
            },
        )])
    }

    fn dir_identity(modified: i64) -> Identity {
        Identity {
            size: 0,
            modified,
            hash: String::new(),
        }
    }

    fn local_file(relative: &str, hash: &str) -> (String, LocalEntry) {
        (
            relative.to_owned(),
            LocalEntry {
                kind: "file".to_owned(),
                identity: identity(hash, 1),
                path: PathBuf::from(relative),
            },
        )
    }

    fn remote_file(relative: &str, hash: &str) -> (String, RemoteEntry) {
        (
            relative.to_owned(),
            RemoteEntry {
                kind: "file".to_owned(),
                identity: identity(hash, 1),
                path: RemotePath::parse(format!("/root/{relative}")).unwrap(),
            },
        )
    }

    fn planned(actions: &[Action]) -> Vec<String> {
        actions
            .iter()
            .map(|action| format!("{}:{}", action_name(action), action_relative(action)))
            .collect()
    }

    #[test]
    fn folder_created_locally_after_first_sync_is_created_remotely() {
        let local = BTreeMap::from([(
            "New folder".to_owned(),
            LocalEntry {
                kind: "dir".to_owned(),
                identity: dir_identity(5),
                path: PathBuf::from("New folder"),
            },
        )]);
        let actions = plan_actions(
            &file_baseline(),
            &ScanResult {
                local,
                remote: BTreeMap::new(),
            },
        );
        assert_eq!(planned(&actions), ["mkdir_remote:New folder"]);
    }

    #[test]
    fn folder_created_remotely_after_first_sync_is_created_locally() {
        let remote = BTreeMap::from([(
            "Web folder".to_owned(),
            RemoteEntry {
                kind: "dir".to_owned(),
                identity: dir_identity(5),
                path: RemotePath::parse("/root/Web folder".to_owned()).unwrap(),
            },
        )]);
        let actions = plan_actions(
            &file_baseline(),
            &ScanResult {
                local: BTreeMap::new(),
                remote,
            },
        );
        assert_eq!(planned(&actions), ["mkdir_local:Web folder"]);
    }

    #[test]
    fn folder_deleted_on_one_side_after_agreement_still_propagates() {
        let mut baseline = file_baseline();
        baseline.insert(
            "docs".to_owned(),
            SnapshotEntry {
                kind: "dir".to_owned(),
                local: Some(dir_identity(1)),
                remote: Some(dir_identity(2)),
            },
        );
        let local = BTreeMap::from([(
            "docs".to_owned(),
            LocalEntry {
                kind: "dir".to_owned(),
                identity: dir_identity(9),
                path: PathBuf::from("docs"),
            },
        )]);
        let actions = plan_actions(
            &baseline,
            &ScanResult {
                local,
                remote: BTreeMap::new(),
            },
        );
        assert!(planned(&actions).contains(&"delete_local:docs".to_owned()));
    }

    #[test]
    fn local_directory_delete_removes_only_empty_plain_directories() {
        let root = std::env::temp_dir().join(format!("koofr-sync-dir-{}", uuid::Uuid::new_v4()));
        let empty = root.join("empty");
        let full = root.join("full");
        fs::create_dir_all(&empty).unwrap();
        fs::create_dir_all(&full).unwrap();
        fs::write(full.join("keep.txt"), b"keep").unwrap();
        fs::write(root.join("file.txt"), b"file").unwrap();

        assert_eq!(
            delete_local_directory_if_empty(&empty).unwrap(),
            Some(LocalDirectoryDelete::Removed)
        );
        assert!(!empty.exists());
        assert_eq!(
            delete_local_directory_if_empty(&full).unwrap(),
            Some(LocalDirectoryDelete::NotEmpty)
        );
        assert!(full.join("keep.txt").exists());
        assert_eq!(
            delete_local_directory_if_empty(&root.join("file.txt")).unwrap(),
            None
        );
        assert_eq!(
            delete_local_directory_if_empty(&root.join("missing")).unwrap(),
            None
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn baseline_records_only_paths_where_both_sides_agree() {
        let previous = file_baseline();
        let local = BTreeMap::from([
            local_file("a.txt", "edited-during-run"),
            local_file("same.txt", "s"),
            local_file("new-during-run.txt", "n"),
        ]);
        let remote = BTreeMap::from([remote_file("a.txt", "a"), remote_file("same.txt", "s")]);
        let baseline = merge_baseline(&previous, &local, &remote);

        assert_eq!(baseline.get("a.txt").unwrap().local, Some(identity("a", 1)));
        assert_eq!(
            baseline.get("same.txt").unwrap().remote,
            Some(identity("s", 1))
        );
        assert!(!baseline.contains_key("new-during-run.txt"));

        let actions = plan_actions(&baseline, &ScanResult { local, remote });
        let planned = planned(&actions);
        assert!(planned.contains(&"upload:a.txt".to_owned()));
        assert!(planned.contains(&"upload:new-during-run.txt".to_owned()));
        assert!(!planned.iter().any(|action| action.ends_with(":same.txt")));
    }

    #[test]
    fn deletion_during_run_is_propagated_instead_of_resurrected() {
        let previous = file_baseline();
        let remote = BTreeMap::from([remote_file("a.txt", "a")]);
        let baseline = merge_baseline(&previous, &BTreeMap::new(), &remote);
        let actions = plan_actions(
            &baseline,
            &ScanResult {
                local: BTreeMap::new(),
                remote,
            },
        );
        assert_eq!(planned(&actions), ["delete_remote:a.txt"]);
    }
}
