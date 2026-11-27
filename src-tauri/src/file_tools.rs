use std::{
    collections::{BTreeMap, HashSet, VecDeque},
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use futures_util::{
    StreamExt,
    stream::{self, FuturesUnordered},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{State, ipc::Channel};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    AppState,
    error::{AppError, CommandError},
    file_ops::{MountId, RemoteName, RemotePath},
    koofr_api::{FileInfo, KoofrApi, Mount, VaultRepo, next_body_chunk},
    metadata_cache::MetadataCache,
    settings::CacheMode,
    transfer::TransferDirection,
};

type CommandResult<T> = Result<T, CommandError>;

#[derive(Default)]
pub struct FileTools(Mutex<Registry>);

#[derive(Default)]
struct Registry {
    account_changes: usize,
    current: Option<(Uuid, Arc<Run>)>,
}

#[derive(Default)]
struct Run {
    closed: CancellationToken,
    data: tokio::sync::Mutex<Option<Scan>>,
    verification: Mutex<Option<CancellationToken>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

impl FileTools {
    pub fn account_change(&self) -> AccountChange<'_> {
        let mut registry = lock(&self.0);
        registry.account_changes += 1;
        if let Some((_, run)) = registry.current.take() {
            run.closed.cancel();
        }
        AccountChange(self)
    }

    fn run(&self, id: Uuid) -> Result<Arc<Run>, AppError> {
        lock(&self.0)
            .current
            .as_ref()
            .filter(|(current, _)| *current == id)
            .map(|(_, run)| Arc::clone(run))
            .ok_or(AppError::Cancelled)
    }
}

pub struct AccountChange<'a>(&'a FileTools);

impl Drop for AccountChange<'_> {
    fn drop(&mut self) {
        lock(&self.0.0).account_changes -= 1;
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsMount {
    id: String,
    name: String,
    online: bool,
    primary: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsMountListing {
    mounts: Vec<ToolsMount>,
    shared_unavailable: bool,
}

async fn available_mounts(state: &AppState) -> Result<ToolsMountListing, AppError> {
    let (mounts, shares) = tokio::join!(state.api.list_mounts(), state.api.list_shared());
    let mut result: Vec<_> = mounts?
        .into_iter()
        .map(|mount| ToolsMount {
            id: mount.id,
            name: mount.name,
            online: mount.online,
            primary: mount.is_primary && !mount.is_shared,
        })
        .collect();
    let mut seen: HashSet<_> = result.iter().map(|mount| mount.id.clone()).collect();
    let shared_unavailable = shares.is_err();
    if let Ok(shares) = shares {
        result.extend(shares.into_iter().filter_map(|share| {
            (share.share_direction.as_deref() == Some("received")
                && matches!(share.entry_type.as_str(), "dir" | "folder")
                && seen.insert(share.mount_id.clone()))
            .then_some(ToolsMount {
                id: share.mount_id,
                name: share.name,
                online: true,
                primary: false,
            })
        }));
    }
    Ok(ToolsMountListing {
        mounts: result,
        shared_unavailable,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanScope {
    mount_id: String,
    path: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryId {
    scope: usize,
    entry: usize,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    id: EntryId,
    path: RemotePath,
    size: u64,
    files: u64,
    directory: bool,
    can_browse: bool,
    complete: bool,
    removable: bool,
}

struct Node {
    path: RemotePath,
    parent: Option<usize>,
    children: Vec<usize>,
    size: u64,
    files: u64,
    complete: bool,
    empty: bool,
    listed: bool,
    modified: Option<i64>,
    protected: bool,
    info: Option<FileInfo>,
    digest: Option<[u8; 32]>,
    unverified: bool,
}

impl Node {
    fn directory(path: RemotePath, parent: Option<usize>) -> Self {
        Self {
            path,
            parent,
            children: Vec::new(),
            size: 0,
            files: 0,
            complete: true,
            empty: false,
            listed: false,
            modified: None,
            protected: false,
            info: None,
            digest: None,
            unverified: false,
        }
    }
}

struct Scope {
    mount: ToolsMount,
    id: MountId,
    nodes: Vec<Node>,
    cached_folders: usize,
}

impl Scope {
    fn entry(&self, scope: usize, entry: usize) -> Entry {
        let node = &self.nodes[entry];
        Entry {
            id: EntryId { scope, entry },
            path: node.path.clone(),
            size: node.size,
            files: node.files,
            directory: node.info.is_none(),
            can_browse: node.listed,
            complete: node.complete,
            removable: self.mount.primary
                && !node.protected
                && entry != 0
                && (node.empty || (!node.unverified && koofr_hash(self, node).is_some())),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsProgress {
    folders: u64,
    bytes: u64,
}

struct Progress {
    channel: Channel<ToolsProgress>,
    last_update: Instant,
    folders: u64,
    bytes: u64,
}

impl Progress {
    fn new(channel: Channel<ToolsProgress>) -> Self {
        Self {
            channel,
            last_update: Instant::now(),
            folders: 0,
            bytes: 0,
        }
    }

    fn update(&mut self, folders: u64, bytes: u64) {
        self.folders += folders;
        self.bytes = self.bytes.saturating_add(bytes);
        if self.last_update.elapsed() >= Duration::from_millis(200) {
            let _ = self.channel.send(ToolsProgress {
                folders: self.folders,
                bytes: self.bytes,
            });
            self.last_update = Instant::now();
        }
    }
}

fn within(path: &str, root: &str) -> bool {
    root == "/"
        || path == root
        || path
            .strip_prefix(root)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn is_vault_path(repos: &[VaultRepo], mount: &MountId, path: &RemotePath) -> bool {
    repos.iter().any(|repo| {
        repo.mount_id == mount.as_str()
            && within(&path.as_str().to_lowercase(), &repo.path.to_lowercase())
    })
}

fn split_part(name: &str) -> bool {
    name.strip_prefix("part-")
        .and_then(|name| name.strip_suffix(".bin"))
        .is_some_and(|number| {
            number.len() == 10 && number.bytes().all(|byte| byte.is_ascii_digit())
        })
}

fn split_support_file(name: &str) -> bool {
    matches!(
        name,
        "manifest.json" | "sha256sums" | "restore.cmd" | "restore.sh" | "readme.txt"
    )
}

struct DirectoryReader<'a> {
    api: &'a KoofrApi,
    slots: Semaphore,
    concurrency: usize,
    changed: AtomicBool,
    cache: &'a MetadataCache,
    mode: CacheMode,
    ttl: u32,
    generation: u64,
    refresh: bool,
}

impl DirectoryReader<'_> {
    async fn list(
        &self,
        mount: &MountId,
        path: RemotePath,
        index: usize,
    ) -> (usize, Result<Vec<FileInfo>, AppError>, bool) {
        let key = format!("files:{}:{}", mount.as_str(), path.as_str());
        if !self.refresh
            && let Some(entries) = self.cache.get(&key, self.mode, self.ttl).await
        {
            return (index, Ok(entries), true);
        }
        let _permit = self
            .slots
            .acquire()
            .await
            .expect("scan semaphore remains open");
        let result = self.api.list_files(mount, &path).await;
        if self.mode != CacheMode::Off {
            let changed = match &result {
                Ok(entries) => self
                    .cache
                    .store(key, entries, Some(self.generation))
                    .await
                    .unwrap_or(false),
                Err(_) if self.refresh => self.cache.forget(&key, self.generation).await,
                Err(_) => false,
            };
            if changed {
                self.changed.store(true, Ordering::Relaxed);
            }
        }
        (index, result, false)
    }
}

async fn scan_scope(
    reader: &DirectoryReader<'_>,
    mut scope: Scope,
    vaults: Option<&[VaultRepo]>,
    progress: &Mutex<Progress>,
) -> Scope {
    if !scope.mount.online {
        scope.nodes[0].complete = false;
        return scope;
    }
    // Append a child only after its parent. Reverse iteration can then sum subtree sizes
    // independently of the order in which concurrent directory requests complete.
    let mut pending = VecDeque::from([0usize]);
    let mut requests = FuturesUnordered::new();
    loop {
        while requests.len() < reader.concurrency
            && let Some(index) = pending.pop_front()
        {
            let path = scope.nodes[index].path.clone();
            if vaults.is_some_and(|repos| is_vault_path(repos, &scope.id, &path)) {
                scope.nodes[index].protected = true;
                scope.nodes[index].complete = false;
                continue;
            }
            requests.push(reader.list(&scope.id, path, index));
        }
        let Some((current, result, cached)) = requests.next().await else {
            break;
        };
        let Ok(entries) = result else {
            scope.nodes[current].complete = false;
            continue;
        };
        scope.cached_folders += usize::from(cached);
        lock(progress).update(1, 0);
        scope.nodes[current].listed = true;
        scope.nodes[current].empty = entries.is_empty();
        let is_split_package = entries
            .iter()
            .any(|entry| split_part(&entry.name.to_ascii_lowercase()));
        let mut seen = HashSet::new();
        for file in entries {
            let Ok(path) = RemoteName::parse(file.name.clone())
                .and_then(|name| scope.nodes[current].path.join(&name))
            else {
                scope.nodes[current].complete = false;
                continue;
            };
            if path.as_str() != file.path || !seen.insert(file.path.clone()) {
                scope.nodes[current].complete = false;
                continue;
            }
            let mut node = Node::directory(path, Some(current));
            node.modified = Some(file.modified);
            match file.entry_type.as_str() {
                "dir" | "folder" => {}
                "file" => {
                    node.size = u64::try_from(file.size).unwrap_or(0);
                    node.files = 1;
                    node.complete = file.size >= 0;
                    let vault_check_unavailable = vaults.is_none() && scope.mount.primary;
                    node.protected = vault_check_unavailable
                        || split_part(&file.name.to_ascii_lowercase())
                        || (is_split_package
                            && split_support_file(&file.name.to_ascii_lowercase()))
                        || vaults
                            .is_some_and(|vaults| is_vault_path(vaults, &scope.id, &node.path));
                    node.info = Some(file);
                }
                _ => {
                    scope.nodes[current].complete = false;
                    continue;
                }
            }
            let next = scope.nodes.len();
            if node.info.is_none() {
                pending.push_back(next);
            }
            scope.nodes[current].children.push(next);
            scope.nodes.push(node);
        }
    }
    drop(requests);
    for child in (1..scope.nodes.len()).rev() {
        let node = &scope.nodes[child];
        let parent = node.parent.expect("scanned children have a parent");
        let (size, files, complete) = (node.size, node.files, node.complete);
        let parent = &mut scope.nodes[parent];
        parent.size = parent.size.saturating_add(size);
        parent.files += files;
        parent.complete &= complete;
    }
    if vaults.is_none() && scope.mount.primary {
        for node in &mut scope.nodes {
            node.protected = true;
        }
    }
    scope
}

struct Scan {
    scopes: Vec<Scope>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsResults {
    cached_folders: usize,
    mounts: Vec<ToolsMount>,
    roots: Vec<Entry>,
    empty_folders: Vec<Entry>,
    duplicates: Vec<Vec<Entry>>,
    candidates: Vec<CandidateGroup>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CandidateGroup {
    size: u64,
    estimated_bytes: u64,
    failed_entries: usize,
    entries: Vec<Entry>,
}

fn koofr_hash(scope: &Scope, node: &Node) -> Option<String> {
    let file = node.info.as_ref()?;
    (scope.mount.primary
        && file.hash.len() == 32
        && file.hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
    .then(|| file.hash.to_ascii_lowercase())
}

fn same_metadata(left: &FileInfo, right: &FileInfo) -> bool {
    left.entry_type == right.entry_type
        && left.size == right.size
        && left.modified == right.modified
        && left.hash == right.hash
}

impl Scan {
    fn get(&self, id: EntryId) -> Result<(&Scope, &Node), AppError> {
        let scope = self.scopes.get(id.scope).ok_or(AppError::NotFound)?;
        Ok((scope, scope.nodes.get(id.entry).ok_or(AppError::NotFound)?))
    }

    fn buckets(&self) -> BTreeMap<u64, Vec<EntryId>> {
        let mut sizes = BTreeMap::<u64, Vec<EntryId>>::new();
        for (scope, mount) in self.scopes.iter().enumerate() {
            for (entry, node) in mount.nodes.iter().enumerate() {
                if node.info.is_some() && !node.protected && node.complete {
                    sizes
                        .entry(node.size)
                        .or_default()
                        .push(EntryId { scope, entry });
                }
            }
        }
        sizes.retain(|_, entries| entries.len() > 1);
        sizes
    }

    fn results(&self) -> ToolsResults {
        let mut results = ToolsResults {
            cached_folders: self.scopes.iter().map(|scope| scope.cached_folders).sum(),
            mounts: self
                .scopes
                .iter()
                .map(|scope| scope.mount.clone())
                .collect(),
            roots: Vec::new(),
            empty_folders: Vec::new(),
            duplicates: Vec::new(),
            candidates: Vec::new(),
        };
        for (index, scope) in self.scopes.iter().enumerate() {
            results.roots.push(scope.entry(index, 0));
            for (entry, node) in scope.nodes.iter().enumerate().skip(1) {
                if node.empty && node.complete && !node.protected {
                    results.empty_folders.push(scope.entry(index, entry));
                }
            }
        }
        for (size, entries) in self.buckets().into_iter().rev() {
            let mut sha_groups = BTreeMap::<[u8; 32], Vec<Entry>>::new();
            let mut md5_groups = BTreeMap::<String, Vec<Entry>>::new();
            let mut missing = 0u64;
            let mut any_unknown = false;
            let mut failed_entries = 0;
            for &id in &entries {
                let scope = &self.scopes[id.scope];
                let node = &scope.nodes[id.entry];
                let entry = scope.entry(id.scope, id.entry);
                failed_entries += usize::from(node.unverified);
                if let Some(digest) = node.digest {
                    sha_groups.entry(digest).or_default().push(entry);
                } else {
                    missing += 1;
                    match koofr_hash(scope, node).filter(|_| !node.unverified) {
                        Some(hash) => md5_groups.entry(hash).or_default().push(entry),
                        None => any_unknown = true,
                    }
                }
            }
            if any_unknown || (!sha_groups.is_empty() && missing > 0) {
                results.candidates.push(CandidateGroup {
                    size,
                    estimated_bytes: size.saturating_mul(missing),
                    failed_entries,
                    entries: entries
                        .iter()
                        .map(|id| self.scopes[id.scope].entry(id.scope, id.entry))
                        .collect(),
                });
            }
            results.duplicates.extend(
                sha_groups
                    .into_values()
                    .chain(md5_groups.into_values())
                    .filter(|group| group.len() > 1),
            );
        }
        results
    }

    fn mark_removed(&mut self, id: EntryId) {
        let nodes = &mut self.scopes[id.scope].nodes;
        let (size, files) = (nodes[id.entry].size, nodes[id.entry].files);
        nodes[id.entry].protected = true;
        let mut child = id.entry;
        while let Some(parent) = nodes[child].parent {
            nodes[parent].size -= size;
            nodes[parent].files -= files;
            if child == id.entry {
                nodes[parent].children.retain(|&entry| entry != child);
                // A subsequent scan confirms any newly empty parents.
            }
            child = parent;
        }
    }
}

#[tauri::command]
pub async fn file_tools_mounts(state: State<'_, AppState>) -> CommandResult<ToolsMountListing> {
    Ok(available_mounts(&state).await?)
}

#[tauri::command]
pub async fn begin_file_tools(state: State<'_, AppState>) -> CommandResult<Uuid> {
    if !state.api.session_info().await.authenticated {
        return Err(AppError::NotAuthenticated.into());
    }
    let mut registry = lock(&state.file_tools.0);
    if registry.account_changes != 0 {
        return Err(AppError::Cancelled.into());
    }
    let id = Uuid::new_v4();
    if let Some((_, run)) = registry.current.replace((id, Arc::new(Run::default()))) {
        run.closed.cancel();
    }
    Ok(id)
}

#[tauri::command]
pub fn close_file_tools(state: State<'_, AppState>, id: Uuid) {
    let mut registry = lock(&state.file_tools.0);
    if registry
        .current
        .as_ref()
        .is_some_and(|(current, _)| *current == id)
        && let Some((_, run)) = registry.current.take()
    {
        run.closed.cancel();
    }
}

#[tauri::command]
pub async fn scan_file_tools(
    state: State<'_, AppState>,
    id: Uuid,
    scopes: Vec<ScanScope>,
    refresh: Option<bool>,
    channel: Channel<ToolsProgress>,
) -> CommandResult<ToolsResults> {
    let run = state.file_tools.run(id)?;
    let mut data = run.data.try_lock().map_err(|_| AppError::Conflict)?;
    let scan = async {
        let (mounts, vaults) = tokio::join!(available_mounts(&state), state.api.list_vault_repos());
        let mounts = mounts?.mounts;
        let vaults = vaults.ok();
        let mut seen = HashSet::new();
        let scopes = scopes
            .into_iter()
            .map(|target| {
                if !seen.insert(target.mount_id.clone()) {
                    return Err(AppError::InvalidInput("duplicate scan scope"));
                }
                let mount = mounts
                    .iter()
                    .find(|mount| mount.id == target.mount_id)
                    .ok_or(AppError::NotFound)?
                    .clone();
                let id = MountId::parse(target.mount_id)?;
                let path = RemotePath::parse(target.path)?;
                Ok(Scope {
                    mount,
                    id,
                    cached_folders: 0,
                    nodes: vec![Node::directory(path, None)],
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?;
        if scopes.is_empty() {
            return Err(AppError::InvalidInput("scan scope"));
        }
        let generation = state.cache.generation().await;
        let (mode, ttl) = state.settings.cache_policy().await;
        let concurrency = state.settings.file_scan_concurrency().await as usize;
        let reader = DirectoryReader {
            api: &state.api,
            slots: Semaphore::new(concurrency),
            concurrency,
            changed: AtomicBool::new(false),
            cache: &state.cache,
            mode,
            ttl,
            generation,
            refresh: refresh.unwrap_or(false),
        };
        let progress = Mutex::new(Progress::new(channel));
        let mut ordered = stream::iter(scopes.into_iter().enumerate())
            .map(|(index, scope)| {
                let reader = &reader;
                let vaults = vaults.as_deref();
                let progress = &progress;
                async move { (index, scan_scope(reader, scope, vaults, progress).await) }
            })
            .buffer_unordered(concurrency)
            .collect::<Vec<_>>()
            .await;
        if mode == CacheMode::Disk && reader.changed.load(Ordering::Relaxed) {
            let _ = state.cache.flush_scan(generation).await;
        }
        ordered.sort_unstable_by_key(|(index, _)| *index);
        Ok(Scan {
            scopes: ordered.into_iter().map(|(_, scope)| scope).collect(),
        })
    };
    let scan = tokio::select! {
        biased;
        _ = run.closed.cancelled() => return Err(AppError::Cancelled.into()),
        result = scan => result?,
    };
    let results = scan.results();
    *data = Some(scan);
    Ok(results)
}

#[tauri::command]
pub async fn file_tools_usage(
    state: State<'_, AppState>,
    id: Uuid,
    folder: EntryId,
) -> CommandResult<Vec<Entry>> {
    let run = state.file_tools.run(id)?;
    let data = run.data.try_lock().map_err(|_| AppError::Conflict)?;
    let scan = data.as_ref().ok_or(AppError::NotFound)?;
    let (scope, node) = scan.get(folder)?;
    if !node.listed {
        return Err(AppError::Forbidden.into());
    }
    let mut entries: Vec<_> = node
        .children
        .iter()
        .map(|&index| scope.entry(folder.scope, index))
        .collect();
    entries.sort_unstable_by(|a, b| {
        b.size
            .cmp(&a.size)
            .then_with(|| a.path.as_str().cmp(b.path.as_str()))
    });
    Ok(entries)
}

#[tauri::command]
pub fn cancel_file_tools_verification(state: State<'_, AppState>, id: Uuid) -> CommandResult<()> {
    let run = state.file_tools.run(id)?;
    if let Some(token) = lock(&run.verification).as_ref() {
        token.cancel();
    }
    Ok(())
}

#[tauri::command]
pub async fn verify_file_tools(
    state: State<'_, AppState>,
    id: Uuid,
    size: u64,
    channel: Channel<ToolsProgress>,
) -> CommandResult<ToolsResults> {
    let run = state.file_tools.run(id)?;
    let mut data = run.data.try_lock().map_err(|_| AppError::Conflict)?;
    let scan = data.as_mut().ok_or(AppError::NotFound)?;
    let entries = scan.buckets().remove(&size).ok_or(AppError::NotFound)?;
    let cancel = run.closed.child_token();
    *lock(&run.verification) = Some(cancel.clone());
    let mut progress = Progress::new(channel);
    let result = verify_entries(&state, scan, entries, &cancel, &mut progress).await;
    *lock(&run.verification) = None;
    result?;
    Ok(scan.results())
}

async fn verify_entries(
    state: &AppState,
    scan: &mut Scan,
    entries: Vec<EntryId>,
    cancel: &CancellationToken,
    progress: &mut Progress,
) -> Result<(), AppError> {
    let transfer_id = Uuid::new_v4().to_string();
    let registration = state.transfers.register_scoped(&transfer_id)?;
    let verify = async {
        let _permit = state
            .transfers
            .acquire(TransferDirection::Download, registration.token())
            .await?;
        for id in entries {
            let (scope, node) = scan.get(id)?;
            if node.digest.is_some() {
                continue;
            }
            let digest = read_digest(
                state,
                scope,
                node,
                &transfer_id,
                registration.token(),
                progress,
            )
            .await;
            let node = &mut scan.scopes[id.scope].nodes[id.entry];
            node.unverified = digest.is_err();
            node.digest = digest.ok();
        }
        Ok(())
    };
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Ok(()),
        _ = registration.token().cancelled() => Ok(()),
        result = verify => result,
    }
}

async fn read_digest(
    state: &AppState,
    scope: &Scope,
    node: &Node,
    transfer_id: &str,
    cancel: &CancellationToken,
    progress: &mut Progress,
) -> Result<[u8; 32], AppError> {
    let expected = node.info.as_ref().ok_or(AppError::NotFound)?;
    let before = state.api.file_info(&scope.id, &node.path).await?;
    if !same_metadata(expected, &before) {
        return Err(AppError::Conflict);
    }
    let response = state.api.download_response(&scope.id, &node.path).await?;
    if response.status() != reqwest::StatusCode::OK
        || response
            .content_length()
            .is_some_and(|length| length != node.size)
    {
        return Err(AppError::IncompleteTransfer);
    }
    let mut stream = response.bytes_stream();
    let mut bytes = 0;
    let mut hash = Sha256::new();
    while let Some(chunk) = next_body_chunk(&mut stream).await? {
        bytes += chunk.len() as u64;
        if bytes > node.size {
            return Err(AppError::IncompleteTransfer);
        }
        state
            .transfers
            .throttle(transfer_id, chunk.len(), cancel)
            .await?;
        progress.update(0, chunk.len() as u64);
        hash.update(chunk);
    }
    let after = state.api.file_info(&scope.id, &node.path).await?;
    if bytes != node.size || !same_metadata(expected, &after) {
        return Err(AppError::Conflict);
    }
    Ok(hash.finalize().into())
}

#[tauri::command]
pub async fn remove_file_tools_entry(
    state: State<'_, AppState>,
    id: Uuid,
    target: EntryId,
    keep: Option<EntryId>,
) -> CommandResult<ToolsResults> {
    let run = state.file_tools.run(id)?;
    let mut data = run.data.try_lock().map_err(|_| AppError::Conflict)?;
    let scan = data.as_mut().ok_or(AppError::NotFound)?;
    let remove = async {
        let (scope, node) = scan.get(target)?;
        node.path.require_non_root()?;
        if target.entry == 0 || node.protected || !node.complete || !scope.mount.primary {
            return Err(AppError::Forbidden);
        }
        let (mounts, vaults) =
            tokio::try_join!(state.api.list_mounts(), state.api.list_vault_repos())?;
        if !mounts.iter().any(|mount: &Mount| {
            mount.id == scope.id.as_str() && mount.online && mount.is_primary && !mount.is_shared
        }) || is_vault_path(&vaults, &scope.id, &node.path)
        {
            return Err(AppError::Forbidden);
        }
        let info = state.api.file_info(&scope.id, &node.path).await?;
        match (&node.info, keep) {
            (None, None)
                if node.empty
                    && node.modified == Some(info.modified)
                    && matches!(info.entry_type.as_str(), "dir" | "folder") =>
            {
                state
                    .api
                    .delete_empty_folder(&scope.id, &node.path, info.modified)
                    .await?;
            }
            (Some(expected), Some(keep)) if same_metadata(expected, &info) => {
                let (_, keeper) = scan.get(keep)?;
                let filename = node.path.file_name()?.to_ascii_lowercase();
                if split_support_file(&filename)
                    && state
                        .api
                        .list_files(&scope.id, &node.path.parent()?)
                        .await?
                        .iter()
                        .any(|item| split_part(&item.name.to_ascii_lowercase()))
                {
                    return Err(AppError::Forbidden);
                }
                // Distinct paths in the same native mount establish independence.
                // Shared/external APIs do not reliably expose canonical object IDs.
                if keep.scope != target.scope
                    || keeper.path.as_str().to_lowercase() == node.path.as_str().to_lowercase()
                    || keeper.protected
                    || is_vault_path(&vaults, &scope.id, &keeper.path)
                    || !content_matches(scope, node, keeper)
                    || koofr_hash(scope, node).is_none()
                    || koofr_hash(scope, keeper).is_none()
                {
                    return Err(AppError::Forbidden);
                }
                let current_keeper = state.api.file_info(&scope.id, &keeper.path).await?;
                if !keeper
                    .info
                    .as_ref()
                    .is_some_and(|expected| same_metadata(expected, &current_keeper))
                {
                    return Err(AppError::Conflict);
                }
                state
                    .api
                    .delete_conditional(
                        &scope.id,
                        &node.path,
                        Some((&info.hash, info.size, info.modified)),
                    )
                    .await?;
            }
            _ => return Err(AppError::Conflict),
        }
        Ok(())
    };
    let result = tokio::select! {
        biased;
        _ = run.closed.cancelled() => Err(AppError::Cancelled),
        result = remove => result,
    };
    let _ = state.cache.clear().await;
    result?;
    scan.mark_removed(target);
    Ok(scan.results())
}

fn content_matches(scope: &Scope, a: &Node, b: &Node) -> bool {
    if a.size != b.size || a.unverified || b.unverified {
        return false;
    }
    match (a.digest, b.digest) {
        (Some(a), Some(b)) => a == b,
        _ => koofr_hash(scope, a).is_some_and(|hash| Some(hash) == koofr_hash(scope, b)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::{Method::GET, MockServer};
    use serde_json::json;

    fn scope(id: &str, primary: bool) -> Scope {
        Scope {
            mount: ToolsMount {
                id: id.into(),
                name: id.into(),
                online: true,
                primary,
            },
            id: MountId::parse(id.into()).unwrap(),
            cached_folders: 0,
            nodes: vec![Node::directory(
                RemotePath::parse("/".into()).unwrap(),
                None,
            )],
        }
    }

    #[tokio::test]
    async fn directory_totals_and_empty_results_respect_incomplete_subtrees() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(GET)
                    .path("/api/v2/mounts/primary/files/list")
                    .query_param("path", "/");
                then.status(200).json_body(json!({"files":[
                    {"name":"empty","type":"dir","modified":1,"size":0},
                    {"name":"unreadable","type":"dir","modified":1,"size":0},
                    {"name":".hidden","type":"file","modified":1,"size":42,"hash":null}
                ]}));
            })
            .await;
        server
            .mock_async(|when, then| {
                when.method(GET)
                    .path("/api/v2/mounts/primary/files/list")
                    .query_param("path", "/empty");
                then.status(200).json_body(json!({"files":[]}));
            })
            .await;
        server
            .mock_async(|when, then| {
                when.method(GET)
                    .path("/api/v2/mounts/primary/files/list")
                    .query_param("path", "/unreadable");
                then.status(403);
            })
            .await;
        let api = KoofrApi::new(&server.base_url()).unwrap();
        api.set_test_session("test-token").await;
        let progress = Mutex::new(Progress::new(Channel::new(|_| Ok(()))));
        let cache = MetadataCache::load(std::env::temp_dir().join("unused-scan-test-cache"), false);
        let mut reader = DirectoryReader {
            api: &api,
            slots: Semaphore::new(6),
            concurrency: 6,
            changed: AtomicBool::new(false),
            cache: &cache,
            mode: CacheMode::Memory,
            ttl: 15,
            generation: 0,
            refresh: false,
        };
        let scanned = scan_scope(&reader, scope("primary", true), Some(&[]), &progress).await;
        let results = Scan {
            scopes: vec![scanned],
        }
        .results();
        assert_eq!(results.roots[0].size, 42);
        assert_eq!(results.roots[0].files, 1);
        assert!(!results.roots[0].complete);
        assert_eq!(results.empty_folders.len(), 1);
        assert_eq!(results.empty_folders[0].path.as_str(), "/empty");
        assert_eq!(results.cached_folders, 0);
        let cached = scan_scope(&reader, scope("primary", true), Some(&[]), &progress).await;
        assert_eq!(cached.cached_folders, 2);
        assert_eq!(cached.nodes[0].size, 42);
        reader.refresh = true;
        let fresh = scan_scope(&reader, scope("primary", true), Some(&[]), &progress).await;
        assert_eq!(fresh.cached_folders, 0);
        assert_eq!(fresh.nodes[0].size, 42);
    }

    #[tokio::test]
    async fn repeated_scan_reuses_directory_listings_without_network_requests() {
        let server = MockServer::start_async().await;
        let children: Vec<_> = (0..12)
            .map(|n| {
                json!({
                    "name": format!("dir{n}"), "type":"dir", "modified":1, "size":0
                })
            })
            .collect();
        let root = server
            .mock_async(|when, then| {
                when.method(GET)
                    .path("/api/v2/mounts/main/files/list")
                    .query_param("path", "/");
                then.delay(Duration::from_millis(50))
                    .json_body(json!({"files":children}));
            })
            .await;
        let mut child_mocks = Vec::new();
        for n in 0..12 {
            child_mocks.push(
                server
                    .mock_async(|when, then| {
                        when.method(GET)
                            .path("/api/v2/mounts/main/files/list")
                            .query_param("path", format!("/dir{n}"));
                        then.delay(Duration::from_millis(50))
                            .json_body(json!({"files":[]}));
                    })
                    .await,
            );
        }
        let api = KoofrApi::new(&server.base_url()).unwrap();
        api.set_test_session("test-token").await;
        let progress = Mutex::new(Progress::new(Channel::new(|_| Ok(()))));
        let cache = MetadataCache::load(std::env::temp_dir().join("unused-scan-test-cache"), false);
        let mut reader = DirectoryReader {
            api: &api,
            slots: Semaphore::new(1),
            concurrency: 1,
            cache: &cache,
            mode: CacheMode::Memory,
            ttl: 15,
            generation: 0,
            refresh: true,
            changed: AtomicBool::new(false),
        };
        for (label, slots, refresh, expected_hits) in [
            ("serial", 1, true, 0),
            ("parallel", 12, true, 0),
            ("cached", 12, false, 13),
        ] {
            reader.slots = Semaphore::new(slots);
            reader.concurrency = slots;
            reader.refresh = refresh;
            let start = Instant::now();
            let result = scan_scope(&reader, scope("main", true), Some(&[]), &progress).await;
            eprintln!(
                "{label}: {:?}, {} cached directories",
                start.elapsed(),
                result.cached_folders
            );
            assert_eq!(result.cached_folders, expected_hits);
            assert_eq!(result.nodes.iter().filter(|n| n.empty).count(), 12);
        }
        root.assert_calls_async(2).await;
        for child in child_mocks {
            child.assert_calls_async(2).await;
        }
    }

    #[test]
    fn foreign_hashes_require_reading_and_only_matching_digests_group() {
        fn add_file(scope: &mut Scope, name: &str) {
            let path = format!("/{name}");
            let mut node = Node::directory(RemotePath::parse(path.clone()).unwrap(), Some(0));
            node.size = 4;
            node.files = 1;
            node.info = Some(FileInfo {
                name: name.into(),
                entry_type: "file".into(),
                modified: 1,
                size: 4,
                content_type: String::new(),
                hash: "a".repeat(32),
                path,
            });
            scope.nodes.push(node);
        }
        let mut native = scope("primary", true);
        let mut external = scope("external", false);
        add_file(&mut native, "native");
        add_file(&mut external, "copy");
        add_file(&mut external, "different");
        let mut scan = Scan {
            scopes: vec![native, external],
        };
        let initial = scan.results();
        assert!(initial.duplicates.is_empty());
        assert_eq!(initial.candidates[0].estimated_bytes, 12);

        scan.scopes[0].nodes[1].digest = Some([1; 32]);
        scan.scopes[1].nodes[1].digest = Some([1; 32]);
        scan.scopes[1].nodes[2].digest = Some([2; 32]);
        let verified = scan.results();
        assert!(verified.candidates.is_empty());
        assert_eq!(verified.duplicates.len(), 1);
        assert_eq!(verified.duplicates[0].len(), 2);
        assert!(!verified.duplicates[0][1].removable);
    }
}
