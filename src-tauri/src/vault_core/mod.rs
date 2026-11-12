mod native_prompt;

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use serde::Serialize;
use tokio::sync::RwLock;

use crate::{
    crypto::VaultCipher,
    error::AppError,
    file_ops::{MountId, RemoteName, RemotePath},
    koofr_api::{KoofrApi, VaultRepo},
};

pub use native_prompt::prompt_safe_key;

const ROOT_HANDLE: &str = "root";
const DEFAULT_AUTO_LOCK: Duration = Duration::from_secs(60 * 60);
const MAX_HANDLES_PER_REPO: usize = 4096;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultSummary {
    pub id: String,
    pub name: String,
    pub locked: bool,
    pub added: i64,
    pub auto_lock_seconds: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultBreadcrumb {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultEntry {
    pub id: String,
    pub name: String,
    pub entry_type: String,
    pub modified: i64,
    pub size: i64,
    pub content_type: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultDirectory {
    pub repo_id: String,
    pub repo_name: String,
    pub directory_id: String,
    pub breadcrumbs: Vec<VaultBreadcrumb>,
    pub entries: Vec<VaultEntry>,
    /// Items in this folder that could not be decrypted (not created by a Vault,
    /// a different Safe Key, or a damaged name). They are left out of `entries`
    /// so one bad name does not hide the whole folder.
    pub skipped_entries: usize,
}

#[derive(Clone)]
pub struct VaultManager {
    state: Arc<RwLock<VaultState>>,
    auto_lock_after: Duration,
}

#[derive(Default)]
struct VaultState {
    repos: HashMap<String, ManagedRepo>,
}

struct ManagedRepo {
    metadata: VaultRepo,
    unlocked: Option<UnlockedRepo>,
}

struct UnlockedRepo {
    cipher: Arc<VaultCipher>,
    handles: HashMap<String, VaultHandle>,
    /// Encrypted path to handle ID, so listing a folder again hands out the same
    /// IDs instead of growing the table on every refresh.
    ids_by_path: HashMap<String, String>,
    last_activity: Instant,
}

/// A listed item before it has a handle ID.
struct ListedEntry {
    name: String,
    encrypted_path: RemotePath,
    entry_type: HandleType,
    remote_type: String,
    modified: i64,
    size: i64,
    content_type: String,
}

#[derive(Clone)]
struct VaultHandle {
    encrypted_path: RemotePath,
    entry_type: HandleType,
    breadcrumbs: Vec<VaultBreadcrumb>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum HandleType {
    File,
    Directory,
}

impl Default for VaultManager {
    fn default() -> Self {
        Self {
            state: Arc::new(RwLock::new(VaultState::default())),
            auto_lock_after: DEFAULT_AUTO_LOCK,
        }
    }
}

impl VaultManager {
    pub async fn reset(&self) {
        *self.state.write().await = VaultState::default();
    }

    pub async fn sync_repos(&self, repos: Vec<VaultRepo>) -> Vec<VaultSummary> {
        let mut state = self.state.write().await;
        prune_expired(&mut state, self.auto_lock_after);
        let mut previous = std::mem::take(&mut state.repos);
        for repo in repos {
            let unlocked = previous
                .remove(&repo.id)
                .filter(|old| same_crypto_identity(&old.metadata, &repo))
                .and_then(|old| old.unlocked);
            state.repos.insert(
                repo.id.clone(),
                ManagedRepo {
                    metadata: repo,
                    unlocked,
                },
            );
        }
        summaries(&state, self.auto_lock_after)
    }

    pub async fn summaries(&self) -> Vec<VaultSummary> {
        let mut state = self.state.write().await;
        prune_expired(&mut state, self.auto_lock_after);
        summaries(&state, self.auto_lock_after)
    }

    pub async fn repo_for_unlock(&self, repo_id: &str) -> Result<VaultRepo, AppError> {
        validate_repo_id(repo_id)?;
        self.state
            .read()
            .await
            .repos
            .get(repo_id)
            .map(|repo| repo.metadata.clone())
            .ok_or(AppError::NotFound)
    }

    pub async fn unlock(&self, repo_id: &str, safe_key: &str) -> Result<VaultSummary, AppError> {
        validate_repo_id(repo_id)?;
        let repo = self.repo_for_unlock(repo_id).await?;
        let cipher = VaultCipher::new(safe_key, repo.salt.as_deref());
        if !cipher.validates(&repo.password_validator, &repo.password_validator_encrypted)? {
            return Err(AppError::VaultInvalidKey);
        }
        let mut handles = HashMap::new();
        handles.insert(
            ROOT_HANDLE.to_owned(),
            VaultHandle {
                encrypted_path: RemotePath::parse("/".to_owned())?,
                entry_type: HandleType::Directory,
                breadcrumbs: vec![VaultBreadcrumb {
                    id: ROOT_HANDLE.to_owned(),
                    name: repo.name.clone(),
                }],
            },
        );
        let mut state = self.state.write().await;
        let managed = state.repos.get_mut(repo_id).ok_or(AppError::NotFound)?;
        let ids_by_path = HashMap::from([("/".to_owned(), ROOT_HANDLE.to_owned())]);
        managed.unlocked = Some(UnlockedRepo {
            cipher: Arc::new(cipher),
            handles,
            ids_by_path,
            last_activity: Instant::now(),
        });
        Ok(summary(managed, self.auto_lock_after))
    }

    pub async fn lock(&self, repo_id: &str) -> Result<(), AppError> {
        validate_repo_id(repo_id)?;
        let mut state = self.state.write().await;
        let repo = state.repos.get_mut(repo_id).ok_or(AppError::NotFound)?;
        repo.unlocked = None;
        Ok(())
    }

    pub async fn lock_expired(&self) -> Vec<String> {
        let mut state = self.state.write().await;
        let expired = expired_repo_ids(&state, self.auto_lock_after);
        prune_expired(&mut state, self.auto_lock_after);
        expired
    }

    pub async fn touch_preview(&self, repo_id: &str) -> Result<(), AppError> {
        validate_repo_id(repo_id)?;
        let mut state = self.state.write().await;
        prune_expired(&mut state, self.auto_lock_after);
        let repo = state.repos.get_mut(repo_id).ok_or(AppError::NotFound)?;
        let unlocked = repo.unlocked.as_mut().ok_or(AppError::VaultLocked)?;
        unlocked.last_activity = Instant::now();
        Ok(())
    }

    pub async fn list_directory(
        &self,
        api: &KoofrApi,
        repo_id: &str,
        directory_id: &str,
    ) -> Result<VaultDirectory, AppError> {
        let (repo, cipher, parent) = self
            .operation_snapshot(repo_id, directory_id, Some(HandleType::Directory))
            .await?;
        let mount_id = MountId::parse(repo.mount_id.clone())?;
        let remote_directory = mount_path(&repo.path, &parent.encrypted_path)?;
        let files = api.list_files(&mount_id, &remote_directory).await?;
        let mut listed = Vec::with_capacity(files.len());
        let mut skipped_entries = 0_usize;
        for file in files {
            match listed_entry(&cipher, &parent.encrypted_path, file) {
                Some(entry) => listed.push(entry),
                None => skipped_entries += 1,
            }
        }
        let mut entries = self
            .install_handles(repo_id, directory_id, &parent, listed)
            .await?;
        entries.sort_by(|left, right| {
            let left_dir = left.entry_type == "dir";
            let right_dir = right.entry_type == "dir";
            right_dir
                .cmp(&left_dir)
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        });
        Ok(VaultDirectory {
            repo_id: repo.id,
            repo_name: repo.name,
            directory_id: directory_id.to_owned(),
            breadcrumbs: parent.breadcrumbs,
            entries,
            skipped_entries,
        })
    }

    pub async fn create_directory(
        &self,
        api: &KoofrApi,
        repo_id: &str,
        parent_id: &str,
        name: String,
    ) -> Result<(), AppError> {
        let name = RemoteName::parse(name)?;
        let (repo, cipher, parent) = self
            .operation_snapshot(repo_id, parent_id, Some(HandleType::Directory))
            .await?;
        let encrypted_name = RemoteName::parse(cipher.encrypt_name(name.as_str()))?;
        let mount_id = MountId::parse(repo.mount_id)?;
        let remote_parent = mount_path(&repo.path, &parent.encrypted_path)?;
        api.create_folder(&mount_id, &remote_parent, &encrypted_name)
            .await
    }

    pub async fn rename(
        &self,
        api: &KoofrApi,
        repo_id: &str,
        entry_id: &str,
        new_name: String,
    ) -> Result<(), AppError> {
        let new_name = RemoteName::parse(new_name)?;
        let (repo, cipher, entry) = self.operation_snapshot(repo_id, entry_id, None).await?;
        if entry.encrypted_path.as_str() == "/" {
            return Err(AppError::InvalidInput("vault root rename"));
        }
        let encrypted_name = RemoteName::parse(cipher.encrypt_name(new_name.as_str()))?;
        let parent = entry.encrypted_path.parent()?;
        let destination_relative = parent.join(&encrypted_name)?;
        let mount_id = MountId::parse(repo.mount_id)?;
        let source = mount_path(&repo.path, &entry.encrypted_path)?;
        let destination = mount_path(&repo.path, &destination_relative)?;
        api.move_to(&mount_id, &source, &mount_id, &destination)
            .await
    }

    pub async fn relocate(
        &self,
        api: &KoofrApi,
        repo_id: &str,
        entry_id: &str,
        destination_id: &str,
        is_move: bool,
    ) -> Result<(), AppError> {
        let (repo, _cipher, entry) = self.operation_snapshot(repo_id, entry_id, None).await?;
        let (_, _, destination) = self
            .operation_snapshot(repo_id, destination_id, Some(HandleType::Directory))
            .await?;
        if entry.encrypted_path.as_str() == "/" {
            return Err(AppError::InvalidInput("vault root relocate"));
        }
        let name = RemoteName::parse(entry.encrypted_path.file_name()?.to_owned())?;
        let destination_relative = destination.encrypted_path.join(&name)?;
        if entry.entry_type == HandleType::Directory
            && destination_relative
                .as_str()
                .starts_with(&format!("{}/", entry.encrypted_path.as_str()))
        {
            return Err(AppError::InvalidInput("vault move into self"));
        }
        let mount_id = MountId::parse(repo.mount_id)?;
        let source = mount_path(&repo.path, &entry.encrypted_path)?;
        let target = mount_path(&repo.path, &destination_relative)?;
        if is_move {
            api.move_to(&mount_id, &source, &mount_id, &target).await
        } else {
            api.copy_to(&mount_id, &source, &mount_id, &target).await
        }
    }

    pub async fn delete(
        &self,
        api: &KoofrApi,
        repo_id: &str,
        entry_ids: &[String],
    ) -> Result<(), AppError> {
        if entry_ids.is_empty() || entry_ids.len() > 256 {
            return Err(AppError::InvalidInput("vault delete selection"));
        }
        for entry_id in entry_ids {
            let (repo, _, entry) = self.operation_snapshot(repo_id, entry_id, None).await?;
            if entry.encrypted_path.as_str() == "/" {
                return Err(AppError::InvalidInput("vault root delete"));
            }
            let mount_id = MountId::parse(repo.mount_id)?;
            let remote_path = mount_path(&repo.path, &entry.encrypted_path)?;
            api.delete(&mount_id, &remote_path).await?;
        }
        Ok(())
    }

    pub async fn operation_target(
        &self,
        repo_id: &str,
        entry_id: &str,
        expected: Option<&str>,
    ) -> Result<(VaultRepo, Arc<VaultCipher>, RemotePath), AppError> {
        let expected = match expected {
            Some("file") => Some(HandleType::File),
            Some("dir") => Some(HandleType::Directory),
            Some(_) => return Err(AppError::InvalidInput("vault entry type")),
            None => None,
        };
        let (repo, cipher, handle) = self.operation_snapshot(repo_id, entry_id, expected).await?;
        let path = mount_path(&repo.path, &handle.encrypted_path)?;
        Ok((repo, cipher, path))
    }

    pub async fn resume_download_target(
        &self,
        repo_id: &str,
        mount_id: &str,
        remote_path: &str,
    ) -> Result<Arc<VaultCipher>, AppError> {
        validate_repo_id(repo_id)?;
        let mount_id = MountId::parse(mount_id.to_owned())?;
        let remote_path = RemotePath::parse(remote_path.to_owned())?;
        let mut state = self.state.write().await;
        prune_expired(&mut state, self.auto_lock_after);
        let repo = state.repos.get_mut(repo_id).ok_or(AppError::NotFound)?;
        if repo.metadata.mount_id != mount_id.as_str() {
            return Err(AppError::InvalidInput("vault checkpoint mount"));
        }
        let base = RemotePath::parse(repo.metadata.path.clone())?;
        let inside = remote_path.as_str() == base.as_str()
            || (base.as_str() == "/" && remote_path.as_str().starts_with('/'))
            || remote_path
                .as_str()
                .strip_prefix(base.as_str())
                .is_some_and(|suffix| suffix.starts_with('/'));
        if !inside {
            return Err(AppError::InvalidInput("vault checkpoint path"));
        }
        let unlocked = repo.unlocked.as_mut().ok_or(AppError::VaultLocked)?;
        unlocked.last_activity = Instant::now();
        Ok(unlocked.cipher.clone())
    }

    async fn operation_snapshot(
        &self,
        repo_id: &str,
        handle_id: &str,
        expected_type: Option<HandleType>,
    ) -> Result<(VaultRepo, Arc<VaultCipher>, VaultHandle), AppError> {
        validate_repo_id(repo_id)?;
        validate_handle_id(handle_id)?;
        let mut state = self.state.write().await;
        prune_expired(&mut state, self.auto_lock_after);
        let repo = state.repos.get_mut(repo_id).ok_or(AppError::NotFound)?;
        let unlocked = repo.unlocked.as_mut().ok_or(AppError::VaultLocked)?;
        let handle = unlocked
            .handles
            .get(handle_id)
            .cloned()
            .ok_or(AppError::NotFound)?;
        if expected_type.is_some_and(|expected| handle.entry_type != expected) {
            return Err(AppError::InvalidInput("vault entry type"));
        }
        unlocked.last_activity = Instant::now();
        Ok((repo.metadata.clone(), unlocked.cipher.clone(), handle))
    }

    /// Gives every listed item a handle, reusing the ID an item already had, and
    /// returns the entries for the UI.
    async fn install_handles(
        &self,
        repo_id: &str,
        directory_id: &str,
        parent: &VaultHandle,
        listed: Vec<ListedEntry>,
    ) -> Result<Vec<VaultEntry>, AppError> {
        let mut state = self.state.write().await;
        let repo = state.repos.get_mut(repo_id).ok_or(AppError::NotFound)?;
        let unlocked = repo.unlocked.as_mut().ok_or(AppError::VaultLocked)?;
        let new_handles = listed
            .iter()
            .filter(|entry| {
                !unlocked
                    .ids_by_path
                    .contains_key(entry.encrypted_path.as_str())
            })
            .count();
        if unlocked.handles.len().saturating_add(new_handles) > MAX_HANDLES_PER_REPO {
            // Forget handles of other folders, but keep the folder being listed and
            // every folder above it so the breadcrumbs and going back keep working.
            let mut keep = parent
                .breadcrumbs
                .iter()
                .map(|crumb| crumb.id.clone())
                .collect::<std::collections::HashSet<_>>();
            keep.insert(ROOT_HANDLE.to_owned());
            keep.insert(directory_id.to_owned());
            unlocked.handles.retain(|id, _| keep.contains(id));
            let handles = &unlocked.handles;
            unlocked
                .ids_by_path
                .retain(|_, id| handles.contains_key(id));
        }
        let mut entries = Vec::with_capacity(listed.len());
        for entry in listed {
            let reused = unlocked
                .ids_by_path
                .get(entry.encrypted_path.as_str())
                .filter(|id| {
                    unlocked
                        .handles
                        .get(*id)
                        .is_some_and(|handle| handle.entry_type == entry.entry_type)
                })
                .cloned();
            let id = reused.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let mut breadcrumbs = parent.breadcrumbs.clone();
            if entry.entry_type == HandleType::Directory {
                breadcrumbs.push(VaultBreadcrumb {
                    id: id.clone(),
                    name: entry.name.clone(),
                });
            }
            unlocked
                .ids_by_path
                .insert(entry.encrypted_path.as_str().to_owned(), id.clone());
            unlocked.handles.insert(
                id.clone(),
                VaultHandle {
                    encrypted_path: entry.encrypted_path,
                    entry_type: entry.entry_type,
                    breadcrumbs,
                },
            );
            entries.push(VaultEntry {
                id,
                name: entry.name,
                entry_type: entry.remote_type,
                modified: entry.modified,
                size: entry.size,
                content_type: entry.content_type,
            });
        }
        unlocked.last_activity = Instant::now();
        Ok(entries)
    }
}

/// Decrypts one listed item. `None` when it is not a valid Vault item.
fn listed_entry(
    cipher: &VaultCipher,
    parent: &RemotePath,
    file: crate::koofr_api::FileInfo,
) -> Option<ListedEntry> {
    let name = cipher.decrypt_name(&file.name).ok()?;
    RemoteName::parse(name.clone()).ok()?;
    let encrypted_name = RemoteName::parse(file.name).ok()?;
    let encrypted_path = parent.join(&encrypted_name).ok()?;
    let (entry_type, size) = match file.entry_type.as_str() {
        "dir" => (HandleType::Directory, 0),
        "file" => (
            HandleType::File,
            VaultCipher::decrypted_size(file.size).ok()?,
        ),
        _ => return None,
    };
    Some(ListedEntry {
        name,
        encrypted_path,
        entry_type,
        remote_type: file.entry_type,
        modified: file.modified,
        size,
        content_type: file.content_type,
    })
}

fn same_crypto_identity(left: &VaultRepo, right: &VaultRepo) -> bool {
    left.mount_id == right.mount_id
        && left.path == right.path
        && left.salt == right.salt
        && left.password_validator == right.password_validator
        && left.password_validator_encrypted == right.password_validator_encrypted
}

fn prune_expired(state: &mut VaultState, after: Duration) {
    let now = Instant::now();
    for repo in state.repos.values_mut() {
        let expired = repo
            .unlocked
            .as_ref()
            .is_some_and(|unlocked| now.duration_since(unlocked.last_activity) >= after);
        if expired {
            repo.unlocked = None;
        }
    }
}

fn expired_repo_ids(state: &VaultState, after: Duration) -> Vec<String> {
    let now = Instant::now();
    state
        .repos
        .iter()
        .filter(|(_, repo)| {
            repo.unlocked
                .as_ref()
                .is_some_and(|unlocked| now.duration_since(unlocked.last_activity) >= after)
        })
        .map(|(id, _)| id.clone())
        .collect()
}

fn summaries(state: &VaultState, after: Duration) -> Vec<VaultSummary> {
    let mut values = state
        .repos
        .values()
        .map(|repo| summary(repo, after))
        .collect::<Vec<_>>();
    values.sort_by(|left, right| {
        left.added
            .cmp(&right.added)
            .then_with(|| left.name.cmp(&right.name))
    });
    values
}

fn summary(repo: &ManagedRepo, after: Duration) -> VaultSummary {
    VaultSummary {
        id: repo.metadata.id.clone(),
        name: repo.metadata.name.clone(),
        locked: repo.unlocked.is_none(),
        added: repo.metadata.added,
        auto_lock_seconds: after.as_secs(),
    }
}

fn mount_path(base: &str, relative: &RemotePath) -> Result<RemotePath, AppError> {
    let base = RemotePath::parse(base.to_owned())?;
    if relative.as_str() == "/" {
        return Ok(base);
    }
    let joined = if base.as_str() == "/" {
        relative.as_str().to_owned()
    } else {
        format!("{}{}", base.as_str(), relative.as_str())
    };
    RemotePath::parse(joined)
}

fn validate_repo_id(value: &str) -> Result<(), AppError> {
    if !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        Ok(())
    } else {
        Err(AppError::InvalidInput("vault repo id"))
    }
}

fn validate_handle_id(value: &str) -> Result<(), AppError> {
    if value == ROOT_HANDLE || uuid::Uuid::parse_str(value).is_ok() {
        Ok(())
    } else {
        Err(AppError::InvalidInput("vault handle"))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{VaultManager, mount_path};
    use crate::crypto::VaultCipher;
    use crate::file_ops::RemotePath;
    use crate::{error::AppError, koofr_api::VaultRepo};

    fn test_repo() -> VaultRepo {
        VaultRepo {
            id: "repo-1".to_owned(),
            name: "Safe Box".to_owned(),
            mount_id: "mount-1".to_owned(),
            path: "/Safe Box".to_owned(),
            salt: None,
            password_validator: "508ddd3f-f18e-4514-932b-b2c1f0c8b291".to_owned(),
            password_validator_encrypted: "v2:UkNMT05FAAA-YjvGKKxTpiFekFYVMNO2UnG2u-Z16MMHAB-ipQYycVTmPSNk0mbnYeZrZ2I-Kh0lTmh4Kt2UxhdYWEXd9YQvyODrWMWWHZaLhL7e".to_owned(),
            added: 1,
        }
    }

    #[test]
    fn joins_opaque_relative_paths_under_the_registered_safe_box() {
        let relative = RemotePath::parse("/encrypted/child".to_owned()).expect("relative path");
        assert_eq!(
            mount_path("/Safe Box", &relative)
                .expect("mount path")
                .as_str(),
            "/Safe Box/encrypted/child"
        );
        assert_eq!(
            mount_path("/", &relative)
                .expect("root mount path")
                .as_str(),
            "/encrypted/child"
        );
    }

    #[tokio::test]
    async fn unlocks_only_with_a_valid_safe_key_and_locks_explicitly() {
        let manager = VaultManager::default();
        manager.sync_repos(vec![test_repo()]).await;
        assert!(matches!(
            manager.unlock("repo-1", "wrong").await,
            Err(AppError::VaultInvalidKey)
        ));
        let unlocked = manager
            .unlock("repo-1", "testpassword")
            .await
            .expect("unlock valid key");
        assert!(!unlocked.locked);
        manager.lock("repo-1").await.expect("lock");
        assert!(manager.summaries().await[0].locked);
    }

    async fn unlocked_manager_and_api(
        server: &httpmock::MockServer,
    ) -> (VaultManager, crate::koofr_api::KoofrApi) {
        let manager = VaultManager::default();
        manager.sync_repos(vec![test_repo()]).await;
        manager
            .unlock("repo-1", "testpassword")
            .await
            .expect("unlock valid key");
        let api = crate::koofr_api::KoofrApi::new(&server.base_url()).expect("create API client");
        api.set_test_session("test-token").await;
        (manager, api)
    }

    fn listing(entries: serde_json::Value) -> serde_json::Value {
        serde_json::json!({ "files": entries })
    }

    #[tokio::test]
    async fn relisting_reuses_handles_and_skips_undecryptable_names() {
        // Given
        let server = httpmock::MockServer::start_async().await;
        let cipher = VaultCipher::new("testpassword", None);
        let folder = cipher.encrypt_name("Docs");
        let file = cipher.encrypt_name("報告.txt");
        let encrypted_size = VaultCipher::encrypted_size(10).expect("encrypted size");
        server
            .mock_async(|when, then| {
                when.method(httpmock::Method::GET)
                    .path("/api/v2/mounts/mount-1/files/list")
                    .query_param("path", "/Safe Box");
                then.status(200).json_body(listing(serde_json::json!([
                    { "name": folder, "type": "dir", "modified": 1, "size": 0,
                      "contentType": "", "hash": "" },
                    { "name": file, "type": "file", "modified": 2, "size": encrypted_size,
                      "contentType": "text/plain", "hash": "" },
                    { "name": "not-encrypted.txt", "type": "file", "modified": 3, "size": 5,
                      "contentType": "text/plain", "hash": "" }
                ])));
            })
            .await;
        let (manager, api) = unlocked_manager_and_api(&server).await;

        // When
        let first = manager
            .list_directory(&api, "repo-1", "root")
            .await
            .expect("list vault root");
        let second = manager
            .list_directory(&api, "repo-1", "root")
            .await
            .expect("list vault root again");

        // Then
        assert_eq!(first.skipped_entries, 1);
        let names = first
            .entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, ["Docs", "報告.txt"]);
        assert_eq!(first.entries[1].size, 10);
        let ids = |directory: &super::VaultDirectory| {
            directory
                .entries
                .iter()
                .map(|entry| entry.id.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&first), ids(&second));
        let state = manager.state.read().await;
        let handles = &state.repos["repo-1"]
            .unlocked
            .as_ref()
            .expect("unlocked")
            .handles;
        assert_eq!(handles.len(), 3);
    }

    #[tokio::test]
    async fn handle_overflow_keeps_the_current_folder_and_its_ancestors() {
        // Given
        let server = httpmock::MockServer::start_async().await;
        let cipher = VaultCipher::new("testpassword", None);
        let folder = cipher.encrypt_name("Docs");
        let child = cipher.encrypt_name("Inner");
        let folder_path = format!("/Safe Box/{folder}");
        server
            .mock_async(|when, then| {
                when.method(httpmock::Method::GET)
                    .path("/api/v2/mounts/mount-1/files/list")
                    .query_param("path", "/Safe Box");
                then.status(200).json_body(listing(serde_json::json!([
                    { "name": folder, "type": "dir", "modified": 1, "size": 0,
                      "contentType": "", "hash": "" }
                ])));
            })
            .await;
        server
            .mock_async(|when, then| {
                when.method(httpmock::Method::GET)
                    .path("/api/v2/mounts/mount-1/files/list")
                    .query_param("path", folder_path.as_str());
                then.status(200).json_body(listing(serde_json::json!([
                    { "name": child, "type": "dir", "modified": 1, "size": 0,
                      "contentType": "", "hash": "" }
                ])));
            })
            .await;
        let (manager, api) = unlocked_manager_and_api(&server).await;
        let root = manager
            .list_directory(&api, "repo-1", "root")
            .await
            .expect("list vault root");
        let docs_id = root.entries[0].id.clone();
        {
            let mut state = manager.state.write().await;
            let unlocked = state
                .repos
                .get_mut("repo-1")
                .and_then(|repo| repo.unlocked.as_mut())
                .expect("unlocked");
            let filler = unlocked.handles[&docs_id].clone();
            while unlocked.handles.len() < super::MAX_HANDLES_PER_REPO {
                unlocked
                    .handles
                    .insert(uuid::Uuid::new_v4().to_string(), filler.clone());
            }
        }

        // When
        let docs = manager
            .list_directory(&api, "repo-1", &docs_id)
            .await
            .expect("list Docs after the table filled up");

        // Then
        let breadcrumb_ids = docs
            .breadcrumbs
            .iter()
            .map(|crumb| crumb.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(breadcrumb_ids, ["root", docs_id.as_str()]);
        for id in ["root", docs_id.as_str(), docs.entries[0].id.as_str()] {
            manager
                .operation_target("repo-1", id, Some("dir"))
                .await
                .expect("handle survived the overflow");
        }
        let state = manager.state.read().await;
        let handles = &state.repos["repo-1"]
            .unlocked
            .as_ref()
            .expect("unlocked")
            .handles;
        assert!(
            handles.len() <= 4,
            "stale handles were kept: {}",
            handles.len()
        );
    }

    #[tokio::test]
    async fn expires_idle_unlocked_sessions() {
        let manager = VaultManager {
            auto_lock_after: Duration::from_millis(1),
            ..VaultManager::default()
        };
        manager.sync_repos(vec![test_repo()]).await;
        manager
            .unlock("repo-1", "testpassword")
            .await
            .expect("unlock valid key");
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert!(manager.summaries().await[0].locked);
    }
}
