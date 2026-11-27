use super::*;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncFileStatus {
    relative_path: String,
    kind: String,
    state: String,
    checked_at: Option<i64>,
    operation: Option<String>,
    availability: Option<String>,
    pinned: bool,
    availability_checked_at: Option<i64>,
    last_read_at: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncFilePage {
    files: Vec<SyncFileStatus>,
    has_more: bool,
}

pub(super) fn migrate(connection: &Connection) -> Result<(), AppError> {
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::LocalData)?;
    transaction
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS sync_file_status (
           pair_id TEXT NOT NULL,
           relative_path TEXT NOT NULL,
           kind TEXT NOT NULL,
           state TEXT NOT NULL,
           checked_at INTEGER,
           version_json TEXT,
           availability TEXT,
           pinned INTEGER NOT NULL DEFAULT 0,
           availability_checked_at INTEGER,
           last_read_at INTEGER,
           PRIMARY KEY (pair_id, relative_path)
         );
         CREATE TABLE IF NOT EXISTS sync_native_reads (
           pair_id TEXT NOT NULL,
           relative_path TEXT NOT NULL,
           operation TEXT NOT NULL,
           token TEXT NOT NULL,
           PRIMARY KEY (pair_id, relative_path, operation)
         );
         DELETE FROM sync_native_reads;
         INSERT OR IGNORE INTO sync_file_status (pair_id, relative_path, kind, state)
         SELECT p.id, b.key, COALESCE(json_extract(b.value, '$.kind'), 'file'), 'unchecked'
         FROM sync_pairs p, json_each(CASE WHEN json_valid(p.baseline_json) THEN p.baseline_json ELSE '{}' END) b
         WHERE b.type = 'object' AND NOT EXISTS (SELECT 1 FROM sync_metadata WHERE key = 'file_status_v1');
         INSERT OR IGNORE INTO sync_file_status (pair_id, relative_path, kind, state)
         SELECT pair_id, local_relative, kind, 'unchecked' FROM sync_entries
         WHERE NOT EXISTS (SELECT 1 FROM sync_metadata WHERE key = 'file_status_v1');
         INSERT OR IGNORE INTO sync_metadata (key, value) VALUES ('file_status_v1', '1');",
        )
        .map_err(|_| AppError::LocalData)?;
    let mut columns = transaction
        .prepare("PRAGMA table_info(sync_issues)")
        .map_err(|_| AppError::LocalData)?;
    let names = columns
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|_| AppError::LocalData)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::LocalData)?;
    drop(columns);
    if !names.iter().any(|name| name == "operation") {
        transaction
            .execute_batch(
                "ALTER TABLE sync_issues ADD COLUMN operation TEXT NOT NULL DEFAULT 'sync';
             ALTER TABLE sync_issues ADD COLUMN updated_at INTEGER;
             UPDATE sync_issues SET operation = 'scan' WHERE relative_path = '';",
            )
            .map_err(|_| AppError::LocalData)?;
    }
    transaction.commit().map_err(|_| AppError::LocalData)
}

pub(super) struct ObservedScan<'a> {
    pub local: &'a BTreeMap<String, LocalEntry>,
    pub remote: &'a BTreeMap<String, RemoteEntry>,
    pub native: Option<&'a NativeLocalScan>,
}

impl ObservedScan<'_> {
    fn observes_parent(&self, path: &str) -> bool {
        let Some(native) = self.native else {
            return true;
        };
        let mut ancestor = path;
        while let Some((parent, _)) = ancestor.rsplit_once('/') {
            if native.populated_directories.contains(parent) {
                return true;
            }
            if self.local.contains_key(parent) || self.remote.contains_key(parent) {
                return false;
            }
            ancestor = parent;
        }
        true
    }

    fn agreed(&self, path: &str) -> bool {
        self.local
            .get(path)
            .zip(self.remote.get(path))
            .is_some_and(|(local, remote)| {
                local.kind == remote.kind
                    && (local.kind == "dir" || same_content(&local.identity, &remote.identity))
            })
    }

    fn removed(&self, path: &str) -> bool {
        self.observes_parent(path)
            && !self.local.contains_key(path)
            && !self.remote.contains_key(path)
    }
}

impl SyncStore {
    pub(super) fn remember_native_children(
        &self,
        pair_id: &str,
        children: &[(String, &'static str)],
    ) -> Result<(), AppError> {
        let connection = self.connection.lock().map_err(|_| AppError::LocalData)?;
        let transaction = connection
            .unchecked_transaction()
            .map_err(|_| AppError::LocalData)?;
        {
            let mut insert = transaction.prepare_cached(
                "INSERT OR IGNORE INTO sync_file_status (pair_id, relative_path, kind, state)
                 SELECT ?1, ?2, ?3, 'unchecked' FROM sync_pairs WHERE id = ?1 AND sync_mode = 'onDemand'",
            ).map_err(|_| AppError::LocalData)?;
            for (path, kind) in children {
                insert
                    .execute(params![pair_id, path, kind])
                    .map_err(|_| AppError::LocalData)?;
            }
        }
        transaction.commit().map_err(|_| AppError::LocalData)
    }

    pub(super) fn remember_pending(
        &self,
        pair_id: &str,
        actions: &[Action],
        scan: &ScanResult,
    ) -> Result<(), AppError> {
        if actions.is_empty() {
            return Ok(());
        }
        let connection = self.connection.lock().map_err(|_| AppError::LocalData)?;
        let transaction = connection
            .unchecked_transaction()
            .map_err(|_| AppError::LocalData)?;
        {
            let mut upsert = transaction.prepare_cached(
                "INSERT INTO sync_file_status (pair_id, relative_path, kind, state)
                 SELECT ?1, ?2, ?3, 'unchecked' WHERE EXISTS (SELECT 1 FROM sync_pairs WHERE id = ?1)
                 ON CONFLICT (pair_id, relative_path) DO UPDATE SET state = 'unchecked'",
            ).map_err(|_| AppError::LocalData)?;
            for action in actions {
                let path = action_relative(action);
                let kind = scan
                    .local
                    .get(&path)
                    .map(|entry| entry.kind.as_str())
                    .or_else(|| scan.remote.get(&path).map(|entry| entry.kind.as_str()))
                    .unwrap_or("file");
                upsert
                    .execute(params![pair_id, path, kind])
                    .map_err(|_| AppError::LocalData)?;
            }
        }
        transaction.commit().map_err(|_| AppError::LocalData)
    }

    pub(super) fn save_observed_scan(
        &self,
        pair_id: &str,
        baseline: &BTreeMap<String, SnapshotEntry>,
        scan: ObservedScan<'_>,
        failed: &BTreeSet<String>,
    ) -> Result<u64, AppError> {
        let now = now_millis();
        let json = serde_json::to_string(baseline).map_err(|_| AppError::LocalData)?;
        let connection = self.connection.lock().map_err(|_| AppError::LocalData)?;
        let transaction = connection
            .unchecked_transaction()
            .map_err(|_| AppError::LocalData)?;
        let result = transaction
            .execute(
                "UPDATE sync_pairs SET baseline_json = ?2,
                 last_synced_at = CASE WHEN ?3 THEN ?4 ELSE last_synced_at END,
                 pending_items = CASE WHEN ?3 THEN 0 ELSE pending_items END,
                 pending_bytes = CASE WHEN ?3 THEN 0 ELSE pending_bytes END,
                 completed_items = CASE WHEN ?3 THEN total_items ELSE completed_items END,
                 completed_bytes = CASE WHEN ?3 THEN total_bytes ELSE completed_bytes END
             WHERE id = ?1",
                params![pair_id, json, failed.is_empty(), now],
            )
            .map_err(|_| AppError::LocalData)?;
        if result == 0 {
            return Err(AppError::NotFound);
        }

        {
            let mut upsert = transaction.prepare_cached(
                "INSERT INTO sync_file_status
                 (pair_id, relative_path, kind, state, checked_at, version_json, availability, pinned, availability_checked_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT (pair_id, relative_path) DO UPDATE SET
                   kind = excluded.kind, state = excluded.state,
                   checked_at = CASE WHEN sync_file_status.kind = excluded.kind THEN
                     COALESCE(excluded.checked_at, sync_file_status.checked_at) ELSE excluded.checked_at END,
                   version_json = CASE WHEN sync_file_status.kind = excluded.kind THEN
                     COALESCE(excluded.version_json, sync_file_status.version_json) ELSE excluded.version_json END,
                   availability = excluded.availability, pinned = excluded.pinned,
                   availability_checked_at = excluded.availability_checked_at",
            ).map_err(|_| AppError::LocalData)?;
            let paths: BTreeSet<_> = scan.local.keys().chain(scan.remote.keys()).collect();
            for path in paths {
                let kind = scan
                    .local
                    .get(path)
                    .map(|entry| entry.kind.as_str())
                    .or_else(|| scan.remote.get(path).map(|entry| entry.kind.as_str()))
                    .expect("path came from a scan entry");
                let agreed = scan.agreed(path) && !failed.contains(path);
                let version = if agreed {
                    baseline
                        .get(path)
                        .map(serde_json::to_string)
                        .transpose()
                        .map_err(|_| AppError::LocalData)?
                } else {
                    None
                };
                let availability = scan
                    .native
                    .and_then(|native| native.file_availability.get(path));
                let state = match (agreed, scan.native.is_some()) {
                    (true, true) => "metadata_current",
                    (true, false) => "in_sync",
                    (false, _) => "unchecked",
                };
                upsert
                    .execute(params![
                        pair_id,
                        path,
                        kind,
                        state,
                        agreed.then_some(now),
                        version,
                        availability.map(|(state, _)| state),
                        availability.is_some_and(|(_, pinned)| *pinned),
                        availability.map(|_| now),
                    ])
                    .map_err(|_| AppError::LocalData)?;
            }
        }
        let removed = {
            let mut select = transaction
                .prepare("SELECT relative_path FROM sync_file_status WHERE pair_id = ?1")
                .map_err(|_| AppError::LocalData)?;
            let rows = select
                .query_map([pair_id], |row| row.get::<_, String>(0))
                .map_err(|_| AppError::LocalData)?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|_| AppError::LocalData)?
        };
        for path in removed
            .into_iter()
            .filter(|path| scan.removed(path) && !failed.contains(path))
        {
            let deleted = transaction.execute(
                "DELETE FROM sync_file_status WHERE pair_id = ?1 AND relative_path = ?2
                 AND NOT EXISTS (SELECT 1 FROM sync_native_reads WHERE pair_id = ?1 AND relative_path = ?2)",
                params![pair_id, path],
            ).map_err(|_| AppError::LocalData)?;
            if deleted > 0 {
                transaction.execute(
                    "UPDATE sync_issues SET resolved = 1 WHERE pair_id = ?1 AND relative_path = ?2 AND resolved = 0",
                    params![pair_id, path],
                ).map_err(|_| AppError::LocalData)?;
            }
        }

        let scan_issues = {
            let mut select = transaction
                .prepare(
                    "SELECT id, relative_path, operation FROM sync_issues
                 WHERE pair_id = ?1 AND resolved = 0 AND operation IN ('sync', 'scan')",
                )
                .map_err(|_| AppError::LocalData)?;
            let rows = select
                .query_map([pair_id], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(|_| AppError::LocalData)?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|_| AppError::LocalData)?
        };
        for (id, path, operation) in scan_issues {
            if operation == "scan"
                || !failed.contains(&path) && (scan.agreed(&path) || scan.removed(&path))
            {
                transaction
                    .execute("UPDATE sync_issues SET resolved = 1 WHERE id = ?1", [id])
                    .map_err(|_| AppError::LocalData)?;
            }
        }
        let count = refresh_issue_count(&transaction, pair_id)?;
        transaction.execute(
            "UPDATE sync_pairs SET status = CASE WHEN paused THEN 'paused' ELSE ?2 END, last_error_code = ?3 WHERE id = ?1",
            params![pair_id, if count > 0 || !failed.is_empty() { "attention" } else { "idle" },
                (!failed.is_empty()).then_some("sync_attention")],
        ).map_err(|_| AppError::LocalData)?;
        transaction.commit().map_err(|_| AppError::LocalData)?;
        Ok(count)
    }

    pub(super) fn file_page(
        &self,
        pair_id: &str,
        owner_id: &str,
        search: &str,
        filter: &str,
        page: u32,
        live: &BTreeMap<String, (String, String)>,
    ) -> Result<SyncFilePage, AppError> {
        let connection = self.connection.lock().map_err(|_| AppError::LocalData)?;
        let mode: String = connection
            .query_row(
                "SELECT sync_mode FROM sync_pairs WHERE id = ?1 AND owner_id = ?2",
                params![pair_id, owner_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| AppError::LocalData)?
            .ok_or(AppError::Forbidden)?;
        let tasks = serde_json::to_string(live).map_err(|_| AppError::LocalData)?;
        let mut statement = connection.prepare_cached(
            "WITH tasks AS (
               SELECT key AS path, json_extract(value, '$[0]') AS operation, json_extract(value, '$[1]') AS state
               FROM json_each(?2)
             ), issues AS (
               SELECT relative_path AS path, MAX(operation) AS operation FROM sync_issues
               WHERE pair_id = ?1 AND resolved = 0 AND relative_path <> '' GROUP BY relative_path
             ), reads AS (
               SELECT relative_path AS path, MAX(operation) AS operation FROM sync_native_reads
               WHERE pair_id = ?1 AND relative_path <> '' GROUP BY relative_path
             ), paths AS (
               SELECT relative_path AS path FROM sync_file_status WHERE pair_id = ?1
               UNION SELECT path FROM tasks UNION SELECT path FROM issues UNION SELECT path FROM reads
             ), result AS (
               SELECT p.path, COALESCE(f.kind, e.kind, 'file') AS kind,
                 CASE WHEN i.path IS NOT NULL THEN 'failed' WHEN r.path IS NOT NULL THEN 'running'
                      ELSE COALESCE(t.state, f.state, 'unchecked') END AS state,
                 f.checked_at, COALESCE(i.operation, r.operation, t.operation) AS operation,
                 CASE WHEN e.kind = 'dir' AND e.populated = 0 THEN 'unexpanded' ELSE f.availability END AS availability,
                 COALESCE(f.pinned, 0), f.availability_checked_at, f.last_read_at
               FROM paths p LEFT JOIN sync_file_status f ON f.pair_id = ?1 AND f.relative_path = p.path
               LEFT JOIN sync_entries e ON ?6 AND e.pair_id = ?1 AND e.local_relative = p.path
               LEFT JOIN tasks t ON t.path = p.path LEFT JOIN issues i ON i.path = p.path
               LEFT JOIN reads r ON r.path = p.path
               WHERE INSTR(LOWER(p.path), LOWER(?3)) > 0 OR ?3 = ''
             )
             SELECT * FROM result WHERE ?4 = 'all'
               OR (?4 = 'pending' AND state IN ('queued', 'running', 'waiting', 'unchecked'))
               OR (?4 = 'failed' AND state = 'failed')
               OR (?4 = 'checked' AND state IN ('in_sync', 'metadata_current'))
             ORDER BY CASE state WHEN 'failed' THEN 0 WHEN 'running' THEN 1 WHEN 'waiting' THEN 2
                       WHEN 'queued' THEN 3 WHEN 'unchecked' THEN 4 ELSE 5 END, path COLLATE NOCASE, path
             LIMIT 41 OFFSET ?5",
        ).map_err(|_| AppError::LocalData)?;
        let rows = statement
            .query_map(
                params![
                    pair_id,
                    tasks,
                    search,
                    filter,
                    i64::from(page) * 40,
                    mode == "onDemand"
                ],
                |row| {
                    Ok(SyncFileStatus {
                        relative_path: row.get(0)?,
                        kind: row.get(1)?,
                        state: row.get(2)?,
                        checked_at: row.get(3)?,
                        operation: row.get(4)?,
                        availability: row.get(5)?,
                        pinned: row.get(6)?,
                        availability_checked_at: row.get(7)?,
                        last_read_at: row.get(8)?,
                    })
                },
            )
            .map_err(|_| AppError::LocalData)?;
        let mut files = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| AppError::LocalData)?;
        let has_more = files.len() > 40;
        files.truncate(40);
        Ok(SyncFilePage { files, has_more })
    }
}

pub(super) fn refresh_issue_count(connection: &Connection, pair_id: &str) -> Result<u64, AppError> {
    connection
        .query_row(
            "UPDATE sync_pairs SET issue_count = (
           SELECT COUNT(*) FROM sync_issues WHERE pair_id = ?1 AND resolved = 0
         ) WHERE id = ?1 RETURNING issue_count",
            [pair_id],
            |row| row.get(0),
        )
        .map_err(|_| AppError::LocalData)
}

impl SyncManager {
    pub fn list_file_status(
        &self,
        owner_id: &str,
        pair_id: &str,
        search: &str,
        filter: &str,
        page: u32,
    ) -> Result<SyncFilePage, AppError> {
        if search.len() > 4096 || !matches!(filter, "all" | "pending" | "failed" | "checked") {
            return Err(AppError::InvalidInput("sync file query"));
        }
        let tracker = self.task_tracker_for(pair_id).ok();
        let tasks = tracker
            .as_ref()
            .map(|tracker| tracker.lock().map_err(|_| AppError::LocalData))
            .transpose()?;
        let mut live = BTreeMap::new();
        if let Some(tasks) = tasks.as_deref() {
            for task in tasks {
                match task.state.as_str() {
                    "running" | "waiting" => {
                        live.insert(
                            task.relative_path.clone(),
                            (task.operation.clone(), task.state.clone()),
                        );
                    }
                    "queued" => {
                        live.entry(task.relative_path.clone())
                            .or_insert((task.operation.clone(), task.state.clone()));
                    }
                    _ => {}
                }
            }
        }
        drop(tasks);
        self.store
            .file_page(pair_id, owner_id, search, filter, page, &live)
    }
}

pub(super) fn record_issue(
    connection: &Connection,
    pair_id: &str,
    path: &str,
    operation: &str,
    code: &str,
    message: &str,
) -> Result<(), AppError> {
    let now = now_millis();
    let updated = connection.execute(
        "UPDATE sync_issues SET message = ?5, updated_at = ?6
         WHERE pair_id = ?1 AND relative_path = ?2 AND operation = ?3 AND code = ?4 AND resolved = 0",
        params![pair_id, path, operation, code, message, now],
    ).map_err(|_| AppError::LocalData)?;
    if updated == 0 {
        connection.execute(
            "INSERT INTO sync_issues (pair_id, relative_path, operation, code, message, created_at, updated_at)
             SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?6 WHERE EXISTS (SELECT 1 FROM sync_pairs WHERE id = ?1)",
            params![pair_id, path, operation, code, message, now],
        ).map_err(|_| AppError::LocalData)?;
    }
    connection.execute(
        "UPDATE sync_pairs SET issue_count = (SELECT COUNT(*) FROM sync_issues WHERE pair_id = ?1 AND resolved = 0),
           status = CASE WHEN status = 'idle' AND NOT paused THEN 'attention' ELSE status END WHERE id = ?1",
        [pair_id],
    ).map_err(|_| AppError::LocalData)?;
    Ok(())
}

pub(super) struct NativeRead {
    path: String,
    operation: &'static str,
    entry_key: Option<String>,
    token: String,
    pub(super) version: Option<Identity>,
}

impl SyncManager {
    pub(super) fn begin_native_read(
        &self,
        pair_id: &str,
        request: &NativeCallbackRequest,
    ) -> Result<Option<NativeRead>, AppError> {
        let pair = self.store.pair(pair_id)?.ok_or(AppError::NotFound)?;
        if pair.sync_mode != "onDemand" || pair.paused {
            return Ok(None);
        }
        let (path, operation, entry_key, version) = match request {
            NativeCallbackRequest::FetchData {
                entry_key,
                required_offset,
                required_length,
                ..
            } if *required_offset >= 0 && *required_length >= 0 => {
                let entry = self
                    .store
                    .native_entry(entry_key)?
                    .filter(|entry| entry.pair_id == pair_id && entry.kind == "file")
                    .ok_or(AppError::NotFound)?;
                (
                    entry.local_relative,
                    "read_file",
                    Some(entry.entry_key),
                    Some(entry.identity),
                )
            }
            NativeCallbackRequest::FetchPlaceholders { local_path, .. } => {
                let path = native_relative_path(&pair.local_root, local_path)?;
                let key = if path.is_empty() {
                    None
                } else {
                    Some(
                        self.store
                            .native_entry_for_local(pair_id, &path)?
                            .filter(|entry| entry.kind == "dir")
                            .ok_or(AppError::NotFound)?
                            .entry_key,
                    )
                };
                (path, "read_directory", key, None)
            }
            _ => return Ok(None),
        };
        let token = uuid::Uuid::new_v4().to_string();
        let connection = self
            .store
            .connection
            .lock()
            .map_err(|_| AppError::LocalData)?;
        let changed = connection.execute(
            "INSERT INTO sync_native_reads (pair_id, relative_path, operation, token)
             SELECT ?1, ?2, ?3, ?4 FROM sync_pairs WHERE id = ?1 AND sync_mode = 'onDemand' AND NOT paused
             ON CONFLICT (pair_id, relative_path, operation) DO UPDATE SET token = excluded.token",
            params![pair_id, path, operation, token],
        ).map_err(|_| AppError::LocalData)?;
        Ok((changed > 0).then_some(NativeRead {
            path,
            operation,
            entry_key,
            token,
            version,
        }))
    }

    pub(super) fn finish_native_read(
        &self,
        pair_id: &str,
        read: NativeRead,
        result: &Result<(), AppError>,
        cancel: &CancellationToken,
    ) -> Result<(), AppError> {
        let connection = self
            .store
            .connection
            .lock()
            .map_err(|_| AppError::LocalData)?;
        let transaction = connection
            .unchecked_transaction()
            .map_err(|_| AppError::LocalData)?;
        let current = transaction
            .execute(
                "DELETE FROM sync_native_reads WHERE pair_id = ?1 AND relative_path = ?2
             AND operation = ?3 AND token = ?4",
                params![pair_id, read.path, read.operation, read.token],
            )
            .map_err(|_| AppError::LocalData)?;
        if current > 0 && !cancel.is_cancelled() && !matches!(result, Err(AppError::Cancelled)) {
            let valid: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM sync_pairs WHERE id = ?1 AND sync_mode = 'onDemand' AND NOT paused)
                  AND (?3 IS NULL OR EXISTS(SELECT 1 FROM sync_entries WHERE pair_id = ?1 AND local_relative = ?2 AND entry_key = ?3
                       AND (?4 IS NULL OR (size = ?4 AND modified = ?5 AND hash = ?6))))",
                params![pair_id, read.path, read.entry_key, read.version.as_ref().map(|id| id.size),
                    read.version.as_ref().map(|id| id.modified), read.version.as_ref().map(|id| &id.hash)], |row| row.get(0),
            ).map_err(|_| AppError::LocalData)?;
            if valid {
                match result {
                    Ok(()) => {
                        transaction.execute(
                            "UPDATE sync_issues SET resolved = 1 WHERE pair_id = ?1 AND relative_path = ?2 AND operation = ?3 AND resolved = 0",
                            params![pair_id, read.path, read.operation],
                        ).map_err(|_| AppError::LocalData)?;
                        if read.operation == "read_file" {
                            transaction.execute(
                                "INSERT INTO sync_file_status (pair_id, relative_path, kind, state, last_read_at)
                                 VALUES (?1, ?2, 'file', 'unchecked', ?3)
                                 ON CONFLICT(pair_id, relative_path) DO UPDATE SET last_read_at = excluded.last_read_at",
                                params![pair_id, read.path, now_millis()],
                            ).map_err(|_| AppError::LocalData)?;
                        }
                        if refresh_issue_count(&transaction, pair_id)? == 0 {
                            transaction.execute(
                                "UPDATE sync_pairs SET status = 'idle' WHERE id = ?1 AND status = 'attention' AND last_error_code IS NULL AND NOT paused",
                                [pair_id],
                            ).map_err(|_| AppError::LocalData)?;
                        }
                    }
                    Err(error) => {
                        let message = if read.operation == "read_file" {
                            "资源管理器未能下载文件内容。"
                        } else {
                            "资源管理器未能读取目录。"
                        };
                        let code = match error {
                            AppError::Io(error)
                                if error.kind() == std::io::ErrorKind::StorageFull =>
                            {
                                "disk_full"
                            }
                            error => error.log_code(),
                        };
                        record_issue(
                            &transaction,
                            pair_id,
                            &read.path,
                            read.operation,
                            code,
                            message,
                        )?;
                    }
                }
            }
        }
        transaction.commit().map_err(|_| AppError::LocalData)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::tests::{local_file, remote_file, test_pair};

    #[test]
    fn file_details_page_past_queue_window_and_keep_observations_on_restart() {
        let path = std::env::temp_dir().join(format!(
            "suitcase-file-status-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let store = SyncStore::open(path.clone()).unwrap();
        store.insert_pair(&test_pair("pair-a", "owner-a")).unwrap();
        let local = (0..45)
            .map(|i| local_file(&format!("folder-{i:02}/notes.txt"), "same"))
            .collect();
        let remote = (0..45)
            .map(|i| remote_file(&format!("folder-{i:02}/notes.txt"), "same"))
            .collect();
        let baseline = merge_baseline(&BTreeMap::new(), &local, &remote);
        store
            .save_observed_scan(
                "pair-a",
                &baseline,
                ObservedScan {
                    local: &local,
                    remote: &remote,
                    native: None,
                },
                &BTreeSet::new(),
            )
            .unwrap();
        drop(store);
        let store = SyncStore::open(path.clone()).unwrap();
        let live = BTreeMap::from([(
            "folder-44/notes.txt".to_owned(),
            ("upload".to_owned(), "queued".to_owned()),
        )]);
        let first = store
            .file_page("pair-a", "owner-a", "notes.txt", "all", 0, &live)
            .unwrap();
        assert_eq!(first.files.len(), 40);
        assert!(first.has_more);
        assert_eq!(first.files[0].relative_path, "folder-44/notes.txt");
        assert_eq!(first.files[0].state, "queued");
        let second = store
            .file_page("pair-a", "owner-a", "notes.txt", "all", 1, &live)
            .unwrap();
        assert_eq!(second.files.len(), 5);
        assert!(!second.has_more);
        assert!(
            second
                .files
                .iter()
                .all(|file| file.state == "in_sync" && file.checked_at.is_some())
        );
        assert!(matches!(
            store.file_page("pair-a", "owner-b", "", "all", 0, &live),
            Err(AppError::Forbidden)
        ));
        drop(store);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn legacy_baselines_remain_searchable_without_inventing_check_times() {
        let path = std::env::temp_dir().join(format!(
            "suitcase-file-status-migration-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        {
            let store = SyncStore::open(path.clone()).unwrap();
            store.insert_pair(&test_pair("pair-a", "owner-a")).unwrap();
            store
                .add_issue("pair-a", "old.txt", "local_io_error", "problem")
                .unwrap();
            let connection = store.connection.lock().unwrap();
            connection
                .execute_batch(
                    "DROP TABLE sync_file_status;
                 DELETE FROM sync_metadata WHERE key = 'file_status_v1';
                 ALTER TABLE sync_issues DROP COLUMN updated_at;
                 ALTER TABLE sync_issues DROP COLUMN operation;",
                )
                .unwrap();
            connection
                .execute(
                    "UPDATE sync_pairs SET baseline_json = ?1 WHERE id = 'pair-a'",
                    [serde_json::json!({ "old.txt": { "kind": "file" } }).to_string()],
                )
                .unwrap();
        }
        let store = SyncStore::open(path.clone()).unwrap();
        let page = store
            .file_page("pair-a", "owner-a", "old.txt", "all", 0, &BTreeMap::new())
            .unwrap();
        assert_eq!(page.files.len(), 1);
        assert_eq!(page.files[0].state, "failed");
        assert!(page.files[0].checked_at.is_none());
        let issue = &store.issues("pair-a").unwrap()[0];
        assert_eq!(issue.operation, "sync");
        assert_eq!(issue.updated_at, issue.created_at);
        drop(store);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn native_read_failures_survive_scans_and_ignore_stale_callbacks() {
        let manager = SyncManager::load(PathBuf::from(":memory:")).unwrap();
        let mut pair = test_pair("pair-a", "owner-a");
        pair.sync_mode = "onDemand".to_owned();
        manager.store.insert_pair(&pair).unwrap();
        let request = NativeCallbackRequest::FetchPlaceholders {
            local_path: pair.local_root.clone(),
            operation: cloud_files::NativeOperation {
                connection_key: 0,
                transfer_key: 0,
                request_key: 0,
            },
        };
        let first = manager
            .begin_native_read("pair-a", &request)
            .unwrap()
            .unwrap();
        let latest = manager
            .begin_native_read("pair-a", &request)
            .unwrap()
            .unwrap();
        let cancel = CancellationToken::new();
        manager
            .finish_native_read("pair-a", latest, &Err(AppError::NetworkStalled), &cancel)
            .unwrap();
        manager
            .finish_native_read("pair-a", first, &Ok(()), &cancel)
            .unwrap();
        let native = NativeLocalScan::default();
        manager
            .store
            .save_observed_scan(
                "pair-a",
                &BTreeMap::new(),
                ObservedScan {
                    local: &native.entries,
                    remote: &BTreeMap::new(),
                    native: Some(&native),
                },
                &BTreeSet::new(),
            )
            .unwrap();
        let issues = manager.list_issues("owner-a", "pair-a").unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].operation, "read_directory");

        let retry = manager
            .begin_native_read("pair-a", &request)
            .unwrap()
            .unwrap();
        let cancelled = CancellationToken::new();
        cancelled.cancel();
        manager
            .finish_native_read("pair-a", retry, &Ok(()), &cancelled)
            .unwrap();
        assert_eq!(manager.list_issues("owner-a", "pair-a").unwrap().len(), 1);

        let retry = manager
            .begin_native_read("pair-a", &request)
            .unwrap()
            .unwrap();
        manager
            .finish_native_read("pair-a", retry, &Ok(()), &cancel)
            .unwrap();
        assert!(manager.list_issues("owner-a", "pair-a").unwrap().is_empty());

        let late = manager
            .begin_native_read("pair-a", &request)
            .unwrap()
            .unwrap();
        manager.store.delete_pair("pair-a").unwrap();
        manager
            .finish_native_read("pair-a", late, &Err(AppError::NetworkStalled), &cancel)
            .unwrap();
        assert!(manager.store.issues("pair-a").unwrap().is_empty());
    }
}
