use std::path::PathBuf;

use serde::Serialize;

#[cfg(not(windows))]
use std::path::Path;

use crate::error::AppError;

const IDENTITY_PREFIX: &[u8] = b"KFG1:";

#[cfg(windows)]
fn callback_path_from_parts(volume_dos_name: &str, normalized_path: &str) -> PathBuf {
    if volume_dos_name.is_empty()
        || normalized_path
            .as_bytes()
            .get(1)
            .is_some_and(|separator| *separator == b':')
    {
        return PathBuf::from(normalized_path);
    }
    PathBuf::from(format!(
        "{}\\{}",
        volume_dos_name.trim_end_matches(['\\', '/']),
        normalized_path.trim_start_matches(['\\', '/'])
    ))
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeSyncCapability {
    pub supported: bool,
    pub reason: Option<String>,
    pub build_number: Option<u32>,
    pub integration_number: Option<u32>,
}

#[derive(Clone, Copy, Debug)]
pub enum NativeProviderStatus {
    Idle,
    Paused,
    Scanning,
    Syncing,
    Offline,
    Error,
}

#[derive(Clone, Debug)]
pub struct PlaceholderSpec {
    pub name: String,
    pub entry_key: String,
    pub is_directory: bool,
    pub size: u64,
    pub modified_millis: i64,
}

#[derive(Clone, Copy, Debug)]
pub struct NativeOperation {
    pub connection_key: i64,
    pub transfer_key: i64,
    pub request_key: i64,
}

#[derive(Clone, Debug)]
pub enum NativeCallbackRequest {
    FetchPlaceholders {
        operation: NativeOperation,
        local_path: PathBuf,
    },
    FetchData {
        operation: NativeOperation,
        entry_key: String,
        required_offset: i64,
        required_length: i64,
    },
    Delete {
        operation: NativeOperation,
        entry_key: String,
        local_path: PathBuf,
    },
}

pub fn encode_entry_identity(entry_key: &str) -> Result<Vec<u8>, AppError> {
    let key = uuid::Uuid::parse_str(entry_key)
        .map_err(|_| AppError::InvalidInput("cloud file identity"))?;
    if key.is_nil() {
        return Err(AppError::InvalidInput("cloud file identity"));
    }
    let mut identity = Vec::with_capacity(IDENTITY_PREFIX.len() + 36);
    identity.extend_from_slice(IDENTITY_PREFIX);
    identity.extend_from_slice(key.hyphenated().to_string().as_bytes());
    Ok(identity)
}

pub fn decode_entry_identity(identity: &[u8]) -> Result<String, AppError> {
    let value = identity
        .strip_prefix(IDENTITY_PREFIX)
        .ok_or(AppError::InvalidInput("cloud file identity"))?;
    let value =
        std::str::from_utf8(value).map_err(|_| AppError::InvalidInput("cloud file identity"))?;
    let key =
        uuid::Uuid::parse_str(value).map_err(|_| AppError::InvalidInput("cloud file identity"))?;
    if key.is_nil() {
        return Err(AppError::InvalidInput("cloud file identity"));
    }
    Ok(key.hyphenated().to_string())
}

#[cfg(windows)]
mod windows {
    use std::{
        collections::HashMap,
        ffi::c_void,
        mem::size_of,
        os::windows::ffi::OsStrExt,
        path::Path,
        ptr,
        sync::{Arc, Mutex},
    };

    use tauri::AppHandle;
    use tokio_util::sync::CancellationToken;
    use windows_sys::{
        Win32::{
            Foundation::{
                INVALID_HANDLE_VALUE, NTSTATUS, STATUS_CANCELLED,
                STATUS_CLOUD_FILE_NETWORK_UNAVAILABLE, STATUS_CLOUD_FILE_UNSUCCESSFUL,
                STATUS_SUCCESS,
            },
            Storage::{
                CloudFilters::*,
                FileSystem::{
                    FILE_ATTRIBUTE_ARCHIVE, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_PINNED,
                    FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS, FILE_ATTRIBUTE_RECALL_ON_OPEN,
                    FILE_ATTRIBUTE_UNPINNED, FILE_BASIC_INFO, FindClose, FindFirstFileW,
                    WIN32_FIND_DATAW,
                },
            },
        },
        core::GUID,
    };

    use super::{
        NativeCallbackRequest, NativeOperation, NativeProviderStatus, NativeSyncCapability,
        PlaceholderSpec, callback_path_from_parts, decode_entry_identity, encode_entry_identity,
    };
    use crate::error::AppError;

    const PROVIDER_GUID: GUID = GUID::from_u128(0x5d07f3f0667c4ef0a93c4941ef6a67a8);
    const WINDOWS_EPOCH_OFFSET_100NS: i64 = 116_444_736_000_000_000;

    pub struct CloudConnection {
        connection_key: CF_CONNECTION_KEY,
        context: *const ConnectionContext,
    }

    unsafe impl Send for CloudConnection {}
    unsafe impl Sync for CloudConnection {}

    impl std::fmt::Debug for CloudConnection {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter
                .debug_struct("CloudConnection")
                .field("connection_key", &self.connection_key)
                .finish_non_exhaustive()
        }
    }

    impl Drop for CloudConnection {
        fn drop(&mut self) {
            unsafe {
                let _ = CfDisconnectSyncRoot(self.connection_key);
                drop(Arc::from_raw(self.context));
            }
        }
    }

    impl CloudConnection {
        pub fn set_status(&self, status: NativeProviderStatus) {
            let status = match status {
                NativeProviderStatus::Idle => CF_PROVIDER_STATUS_IDLE,
                NativeProviderStatus::Paused => CF_PROVIDER_STATUS_DISCONNECTED,
                NativeProviderStatus::Scanning => CF_PROVIDER_STATUS_POPULATE_METADATA,
                NativeProviderStatus::Syncing => CF_PROVIDER_STATUS_SYNC_INCREMENTAL,
                NativeProviderStatus::Offline => CF_PROVIDER_STATUS_CONNECTIVITY_LOST,
                NativeProviderStatus::Error => CF_PROVIDER_STATUS_ERROR,
            };
            unsafe {
                let _ = CfUpdateSyncProviderStatus(self.connection_key, status);
            }
        }
    }

    struct ConnectionContext {
        app: AppHandle,
        pair_id: String,
        cancellations: Mutex<HashMap<i64, CancellationToken>>,
    }

    pub fn capability() -> NativeSyncCapability {
        capability_with_hresult().0
    }

    pub fn capability_with_hresult() -> (NativeSyncCapability, Option<i32>) {
        let mut info = CF_PLATFORM_INFO::default();
        let result = unsafe { CfGetPlatformInfo(&mut info) };
        if failed(result) {
            return (
                NativeSyncCapability {
                    supported: false,
                    reason: Some("当前 Windows 系统不支持按需同步功能。".to_owned()),
                    build_number: None,
                    integration_number: None,
                },
                Some(result),
            );
        }
        (
            NativeSyncCapability {
                supported: true,
                reason: None,
                build_number: Some(info.BuildNumber),
                integration_number: Some(info.IntegrationNumber),
            },
            None,
        )
    }

    pub fn register(root: &Path, pair_id: &str) -> Result<(), AppError> {
        if !capability().supported {
            return Err(AppError::Initialization);
        }
        let root_path = root;
        let root = wide(root_path);
        let provider_name = wide_str("Koofr GUI");
        let provider_version = wide_str(env!("CARGO_PKG_VERSION"));
        let root_identity = format!("KFG1:{pair_id}").into_bytes();
        let registration = CF_SYNC_REGISTRATION {
            StructSize: size_of::<CF_SYNC_REGISTRATION>() as u32,
            ProviderName: provider_name.as_ptr(),
            ProviderVersion: provider_version.as_ptr(),
            SyncRootIdentity: root_identity.as_ptr().cast(),
            SyncRootIdentityLength: root_identity.len() as u32,
            FileIdentity: ptr::null(),
            FileIdentityLength: 0,
            ProviderId: PROVIDER_GUID,
        };
        let policies = CF_SYNC_POLICIES {
            StructSize: size_of::<CF_SYNC_POLICIES>() as u32,
            Hydration: CF_HYDRATION_POLICY {
                Primary: CF_HYDRATION_POLICY_FULL,
                Modifier: CF_HYDRATION_POLICY_MODIFIER_AUTO_DEHYDRATION_ALLOWED
                    | CF_HYDRATION_POLICY_MODIFIER_ALLOW_FULL_RESTART_HYDRATION,
            },
            Population: CF_POPULATION_POLICY {
                Primary: CF_POPULATION_POLICY_PARTIAL,
                Modifier: CF_POPULATION_POLICY_MODIFIER_NONE,
            },
            InSync: CF_INSYNC_POLICY_TRACK_FILE_LAST_WRITE_TIME
                | CF_INSYNC_POLICY_TRACK_DIRECTORY_LAST_WRITE_TIME,
            HardLink: CF_HARDLINK_POLICY_NONE,
            PlaceholderManagement: CF_PLACEHOLDER_MANAGEMENT_POLICY_DEFAULT,
        };
        let mut result = unsafe {
            CfRegisterSyncRoot(
                root.as_ptr(),
                &registration,
                &policies,
                CF_REGISTER_FLAG_MARK_IN_SYNC_ON_ROOT,
            )
        };
        if failed(result) && owned_sync_root(root_path, pair_id) {
            result = unsafe {
                CfRegisterSyncRoot(
                    root.as_ptr(),
                    &registration,
                    &policies,
                    CF_REGISTER_FLAG_UPDATE | CF_REGISTER_FLAG_MARK_IN_SYNC_ON_ROOT,
                )
            };
        }
        if failed(result) {
            return Err(AppError::LocalData);
        }
        Ok(())
    }

    pub fn unregister(root: &Path, pair_id: &str) -> Result<(), AppError> {
        if !owned_sync_root(root, pair_id) {
            return Err(AppError::Forbidden);
        }
        let root = wide(root);
        let result = unsafe { CfUnregisterSyncRoot(root.as_ptr()) };
        if failed(result) {
            return Err(AppError::LocalData);
        }
        Ok(())
    }

    fn owned_sync_root(root: &Path, pair_id: &str) -> bool {
        let root = wide(root);
        let mut buffer = vec![0_usize; 512];
        let mut returned = 0_u32;
        let result = unsafe {
            CfGetSyncRootInfoByPath(
                root.as_ptr(),
                CF_SYNC_ROOT_INFO_STANDARD,
                buffer.as_mut_ptr().cast(),
                (buffer.len() * size_of::<usize>()) as u32,
                &mut returned,
            )
        };
        if failed(result) || returned < size_of::<CF_SYNC_ROOT_STANDARD_INFO>() as u32 {
            return false;
        }
        let info = unsafe { &*buffer.as_ptr().cast::<CF_SYNC_ROOT_STANDARD_INFO>() };
        let provider_length = info
            .ProviderName
            .iter()
            .position(|value| *value == 0)
            .unwrap_or(info.ProviderName.len());
        if String::from_utf16_lossy(&info.ProviderName[..provider_length]) != "Koofr GUI" {
            return false;
        }
        let identity_length = info.SyncRootIdentityLength as usize;
        let identity_offset = std::mem::offset_of!(CF_SYNC_ROOT_STANDARD_INFO, SyncRootIdentity);
        if identity_offset.saturating_add(identity_length) > returned as usize {
            return false;
        }
        let identity = unsafe {
            std::slice::from_raw_parts(
                buffer.as_ptr().cast::<u8>().add(identity_offset),
                identity_length,
            )
        };
        identity == format!("KFG1:{pair_id}").as_bytes()
    }

    pub fn is_owned_sync_root(root: &Path, pair_id: &str) -> bool {
        owned_sync_root(root, pair_id)
    }

    pub fn connect(
        root: &Path,
        app: AppHandle,
        pair_id: String,
    ) -> Result<CloudConnection, AppError> {
        let context = Arc::new(ConnectionContext {
            app,
            pair_id,
            cancellations: Mutex::new(HashMap::new()),
        });
        let context_pointer = Arc::into_raw(context);
        let callbacks = [
            callback(CF_CALLBACK_TYPE_FETCH_DATA, Some(fetch_data_callback)),
            callback(CF_CALLBACK_TYPE_CANCEL_FETCH_DATA, Some(cancel_callback)),
            callback(
                CF_CALLBACK_TYPE_FETCH_PLACEHOLDERS,
                Some(fetch_placeholders_callback),
            ),
            callback(
                CF_CALLBACK_TYPE_CANCEL_FETCH_PLACEHOLDERS,
                Some(cancel_callback),
            ),
            callback(
                CF_CALLBACK_TYPE_NOTIFY_FILE_CLOSE_COMPLETION,
                Some(wake_callback),
            ),
            callback(
                CF_CALLBACK_TYPE_NOTIFY_DEHYDRATE_COMPLETION,
                Some(wake_callback),
            ),
            callback(CF_CALLBACK_TYPE_NOTIFY_DELETE, Some(delete_callback)),
            callback(
                CF_CALLBACK_TYPE_NOTIFY_DELETE_COMPLETION,
                Some(wake_callback),
            ),
            callback(
                CF_CALLBACK_TYPE_NOTIFY_RENAME_COMPLETION,
                Some(wake_callback),
            ),
            CF_CALLBACK_REGISTRATION {
                Type: CF_CALLBACK_TYPE_NONE,
                Callback: None,
            },
        ];
        let mut connection_key = 0;
        let root = wide(root);
        let result = unsafe {
            CfConnectSyncRoot(
                root.as_ptr(),
                callbacks.as_ptr(),
                context_pointer.cast(),
                CF_CONNECT_FLAG_REQUIRE_FULL_FILE_PATH
                    | CF_CONNECT_FLAG_BLOCK_SELF_IMPLICIT_HYDRATION,
                &mut connection_key,
            )
        };
        if failed(result) {
            unsafe { drop(Arc::from_raw(context_pointer)) };
            return Err(AppError::LocalData);
        }
        unsafe {
            let _ = CfUpdateSyncProviderStatus(connection_key, CF_PROVIDER_STATUS_IDLE);
        }
        Ok(CloudConnection {
            connection_key,
            context: context_pointer,
        })
    }

    #[cfg(test)]
    pub struct TestCloudConnection {
        connection_key: i64,
    }

    #[cfg(test)]
    impl Drop for TestCloudConnection {
        fn drop(&mut self) {
            unsafe {
                let _ = CfDisconnectSyncRoot(self.connection_key);
            }
        }
    }

    #[cfg(test)]
    pub fn connect_test_provider(root: &Path) -> Result<TestCloudConnection, AppError> {
        let callbacks = [
            callback(CF_CALLBACK_TYPE_FETCH_DATA, Some(test_fetch_data_callback)),
            callback(
                CF_CALLBACK_TYPE_FETCH_PLACEHOLDERS,
                Some(test_fetch_placeholders_callback),
            ),
            callback(CF_CALLBACK_TYPE_NOTIFY_DELETE, Some(test_delete_callback)),
            CF_CALLBACK_REGISTRATION {
                Type: CF_CALLBACK_TYPE_NONE,
                Callback: None,
            },
        ];
        let mut connection_key = 0;
        let root = wide(root);
        let result = unsafe {
            CfConnectSyncRoot(
                root.as_ptr(),
                callbacks.as_ptr(),
                ptr::null(),
                CF_CONNECT_FLAG_NONE,
                &mut connection_key,
            )
        };
        if failed(result) {
            return Err(AppError::LocalData);
        }
        Ok(TestCloudConnection { connection_key })
    }

    #[cfg(test)]
    unsafe extern "system" fn test_fetch_data_callback(
        info: *const CF_CALLBACK_INFO,
        _parameters: *const CF_CALLBACK_PARAMETERS,
    ) {
        if let Some(operation) = unsafe { operation_from_callback(info) } {
            let _ = transfer_data(operation, 0, b"abc");
        }
    }

    #[cfg(test)]
    unsafe extern "system" fn test_fetch_placeholders_callback(
        info: *const CF_CALLBACK_INFO,
        _parameters: *const CF_CALLBACK_PARAMETERS,
    ) {
        if let Some(operation) = unsafe { operation_from_callback(info) } {
            complete_placeholders_error(operation);
        }
    }

    #[cfg(test)]
    unsafe extern "system" fn test_delete_callback(
        info: *const CF_CALLBACK_INFO,
        _parameters: *const CF_CALLBACK_PARAMETERS,
    ) {
        if let Some(operation) = unsafe { operation_from_callback(info) } {
            let _ = acknowledge_delete(operation, STATUS_SUCCESS);
        }
    }

    #[cfg(test)]
    unsafe fn operation_from_callback(info: *const CF_CALLBACK_INFO) -> Option<NativeOperation> {
        if info.is_null() {
            return None;
        }
        Some(NativeOperation {
            connection_key: unsafe { (*info).ConnectionKey },
            transfer_key: unsafe { (*info).TransferKey },
            request_key: unsafe { (*info).RequestKey },
        })
    }

    fn callback(kind: CF_CALLBACK_TYPE, function: CF_CALLBACK) -> CF_CALLBACK_REGISTRATION {
        CF_CALLBACK_REGISTRATION {
            Type: kind,
            Callback: function,
        }
    }

    unsafe extern "system" fn fetch_placeholders_callback(
        info: *const CF_CALLBACK_INFO,
        _parameters: *const CF_CALLBACK_PARAMETERS,
    ) {
        let Some((context, operation)) = (unsafe { callback_context(info) }) else {
            return;
        };
        // CFAPI reports NormalizedPath from the root of the callback volume (for
        // example, `\Koofr`) and exposes the DOS volume separately (`D:`).
        // Passing the normalized portion alone makes the provider reject every
        // Explorer directory-population request as being outside the sync root.
        let volume = unsafe { wide_pointer_to_string((*info).VolumeDosName) };
        let normalized = unsafe { wide_pointer_to_string((*info).NormalizedPath) };
        let path = callback_path_from_parts(&volume, &normalized);
        let token = CancellationToken::new();
        context
            .cancellations
            .lock()
            .ok()
            .map(|mut pending| pending.insert(operation.transfer_key, token.clone()));
        dispatch(
            context,
            token,
            NativeCallbackRequest::FetchPlaceholders {
                operation,
                local_path: path,
            },
        );
    }

    unsafe extern "system" fn fetch_data_callback(
        info: *const CF_CALLBACK_INFO,
        parameters: *const CF_CALLBACK_PARAMETERS,
    ) {
        let Some((context, operation)) = (unsafe { callback_context(info) }) else {
            return;
        };
        if parameters.is_null() {
            transfer_error(operation, STATUS_CLOUD_FILE_UNSUCCESSFUL);
            return;
        }
        let identity = unsafe {
            if (*info).FileIdentity.is_null() {
                &[][..]
            } else {
                std::slice::from_raw_parts(
                    (*info).FileIdentity.cast::<u8>(),
                    (*info).FileIdentityLength as usize,
                )
            }
        };
        let Ok(entry_key) = decode_entry_identity(identity) else {
            transfer_error(operation, STATUS_CLOUD_FILE_UNSUCCESSFUL);
            return;
        };
        let fetch = unsafe { (*parameters).Anonymous.FetchData };
        let token = CancellationToken::new();
        context
            .cancellations
            .lock()
            .ok()
            .map(|mut pending| pending.insert(operation.transfer_key, token.clone()));
        dispatch(
            context,
            token,
            NativeCallbackRequest::FetchData {
                operation,
                entry_key,
                required_offset: fetch.RequiredFileOffset,
                required_length: fetch.RequiredLength,
            },
        );
    }

    unsafe extern "system" fn delete_callback(
        info: *const CF_CALLBACK_INFO,
        _parameters: *const CF_CALLBACK_PARAMETERS,
    ) {
        let Some((context, operation)) = (unsafe { callback_context(info) }) else {
            return;
        };
        let identity = unsafe {
            if (*info).FileIdentity.is_null() {
                &[][..]
            } else {
                std::slice::from_raw_parts(
                    (*info).FileIdentity.cast::<u8>(),
                    (*info).FileIdentityLength as usize,
                )
            }
        };
        let Ok(entry_key) = decode_entry_identity(identity) else {
            let _ = acknowledge_delete(operation, STATUS_CLOUD_FILE_UNSUCCESSFUL);
            return;
        };
        let volume = unsafe { wide_pointer_to_string((*info).VolumeDosName) };
        let normalized = unsafe { wide_pointer_to_string((*info).NormalizedPath) };
        let local_path = callback_path_from_parts(&volume, &normalized);
        dispatch(
            context,
            CancellationToken::new(),
            NativeCallbackRequest::Delete {
                operation,
                entry_key,
                local_path,
            },
        );
    }

    unsafe extern "system" fn cancel_callback(
        info: *const CF_CALLBACK_INFO,
        _parameters: *const CF_CALLBACK_PARAMETERS,
    ) {
        let Some((context, operation)) = (unsafe { callback_context(info) }) else {
            return;
        };
        if let Ok(pending) = context.cancellations.lock()
            && let Some(token) = pending.get(&operation.transfer_key)
        {
            token.cancel();
        }
    }

    unsafe extern "system" fn wake_callback(
        info: *const CF_CALLBACK_INFO,
        _parameters: *const CF_CALLBACK_PARAMETERS,
    ) {
        let Some((context, _)) = (unsafe { callback_context(info) }) else {
            return;
        };
        crate::sync::wake_native_pair(&context.app, &context.pair_id);
    }

    fn dispatch(
        context: Arc<ConnectionContext>,
        token: CancellationToken,
        request: NativeCallbackRequest,
    ) {
        tauri::async_runtime::spawn(async move {
            let transfer_key = match &request {
                NativeCallbackRequest::FetchPlaceholders { operation, .. }
                | NativeCallbackRequest::FetchData { operation, .. }
                | NativeCallbackRequest::Delete { operation, .. } => operation.transfer_key,
            };
            crate::sync::handle_native_callback(
                context.app.clone(),
                context.pair_id.clone(),
                request,
                token,
            )
            .await;
            if let Ok(mut pending) = context.cancellations.lock() {
                pending.remove(&transfer_key);
            }
        });
    }

    unsafe fn callback_context(
        info: *const CF_CALLBACK_INFO,
    ) -> Option<(Arc<ConnectionContext>, NativeOperation)> {
        if info.is_null() || unsafe { (*info).CallbackContext.is_null() } {
            return None;
        }
        let pointer = unsafe { (*info).CallbackContext.cast::<ConnectionContext>() };
        unsafe { Arc::increment_strong_count(pointer) };
        let context = unsafe { Arc::from_raw(pointer) };
        let operation = NativeOperation {
            connection_key: unsafe { (*info).ConnectionKey },
            transfer_key: unsafe { (*info).TransferKey },
            request_key: unsafe { (*info).RequestKey },
        };
        Some((context, operation))
    }

    pub fn create_placeholders(base: &Path, specs: &[PlaceholderSpec]) -> Result<(), AppError> {
        if specs.is_empty() {
            return Ok(());
        }
        let mut names = Vec::with_capacity(specs.len());
        let mut identities = Vec::with_capacity(specs.len());
        let mut placeholders = Vec::with_capacity(specs.len());
        for spec in specs {
            names.push(wide_str(&spec.name));
            identities.push(encode_entry_identity(&spec.entry_key)?);
        }
        for (index, spec) in specs.iter().enumerate() {
            placeholders.push(placeholder_info(
                spec,
                names[index].as_ptr(),
                identities[index].as_ptr().cast(),
                identities[index].len() as u32,
            ));
        }
        let base = wide(base);
        let mut processed = 0;
        let result = unsafe {
            CfCreatePlaceholders(
                base.as_ptr(),
                placeholders.as_mut_ptr(),
                placeholders.len() as u32,
                CF_CREATE_FLAG_STOP_ON_ERROR,
                &mut processed,
            )
        };
        if failed(result) || processed != placeholders.len() as u32 {
            return Err(AppError::LocalData);
        }
        Ok(())
    }

    pub fn transfer_placeholders(
        operation: NativeOperation,
        specs: &[PlaceholderSpec],
    ) -> Result<(), AppError> {
        let mut names = Vec::with_capacity(specs.len());
        let mut identities = Vec::with_capacity(specs.len());
        let mut placeholders = Vec::with_capacity(specs.len());
        for spec in specs {
            names.push(wide_str(&spec.name));
            identities.push(encode_entry_identity(&spec.entry_key)?);
        }
        for (index, spec) in specs.iter().enumerate() {
            placeholders.push(placeholder_info(
                spec,
                names[index].as_ptr(),
                identities[index].as_ptr().cast(),
                identities[index].len() as u32,
            ));
        }
        let placeholder_array = if placeholders.is_empty() {
            ptr::null_mut()
        } else {
            placeholders.as_mut_ptr()
        };
        let info = operation_info(operation, CF_OPERATION_TYPE_TRANSFER_PLACEHOLDERS);
        let parameters = CF_OPERATION_PARAMETERS {
            ParamSize: size_of::<CF_OPERATION_PARAMETERS>() as u32,
            Anonymous: CF_OPERATION_PARAMETERS_0 {
                TransferPlaceholders: CF_OPERATION_PARAMETERS_0_4 {
                    Flags: CF_OPERATION_TRANSFER_PLACEHOLDERS_FLAG_STOP_ON_ERROR,
                    CompletionStatus: STATUS_SUCCESS,
                    PlaceholderTotalCount: specs.len() as i64,
                    PlaceholderArray: placeholder_array,
                    PlaceholderCount: placeholders.len() as u32,
                    EntriesProcessed: 0,
                },
            },
        };
        execute(&info, parameters)
    }

    fn complete_placeholders(operation: NativeOperation, status: NTSTATUS) {
        let info = operation_info(operation, CF_OPERATION_TYPE_TRANSFER_PLACEHOLDERS);
        let parameters = CF_OPERATION_PARAMETERS {
            ParamSize: size_of::<CF_OPERATION_PARAMETERS>() as u32,
            Anonymous: CF_OPERATION_PARAMETERS_0 {
                TransferPlaceholders: CF_OPERATION_PARAMETERS_0_4 {
                    Flags: CF_OPERATION_TRANSFER_PLACEHOLDERS_FLAG_NONE,
                    CompletionStatus: status,
                    PlaceholderTotalCount: 0,
                    PlaceholderArray: ptr::null_mut(),
                    PlaceholderCount: 0,
                    EntriesProcessed: 0,
                },
            },
        };
        let _ = execute(&info, parameters);
    }

    pub fn complete_placeholders_cancelled(operation: NativeOperation) {
        complete_placeholders(operation, STATUS_CANCELLED);
    }

    pub fn complete_placeholders_error(operation: NativeOperation) {
        complete_placeholders(operation, STATUS_CLOUD_FILE_UNSUCCESSFUL);
    }

    pub fn transfer_data(
        operation: NativeOperation,
        offset: i64,
        bytes: &[u8],
    ) -> Result<(), AppError> {
        let info = operation_info(operation, CF_OPERATION_TYPE_TRANSFER_DATA);
        let parameters = CF_OPERATION_PARAMETERS {
            ParamSize: size_of::<CF_OPERATION_PARAMETERS>() as u32,
            Anonymous: CF_OPERATION_PARAMETERS_0 {
                TransferData: CF_OPERATION_PARAMETERS_0_0 {
                    Flags: CF_OPERATION_TRANSFER_DATA_FLAG_NONE,
                    CompletionStatus: STATUS_SUCCESS,
                    Buffer: bytes.as_ptr().cast(),
                    Offset: offset,
                    Length: bytes.len() as i64,
                },
            },
        };
        execute(&info, parameters)
    }

    pub fn transfer_error(operation: NativeOperation, status: NTSTATUS) {
        let info = operation_info(operation, CF_OPERATION_TYPE_TRANSFER_DATA);
        let parameters = CF_OPERATION_PARAMETERS {
            ParamSize: size_of::<CF_OPERATION_PARAMETERS>() as u32,
            Anonymous: CF_OPERATION_PARAMETERS_0 {
                TransferData: CF_OPERATION_PARAMETERS_0_0 {
                    Flags: CF_OPERATION_TRANSFER_DATA_FLAG_NONE,
                    CompletionStatus: status,
                    Buffer: ptr::null(),
                    Offset: 0,
                    Length: 0,
                },
            },
        };
        let _ = execute(&info, parameters);
    }

    pub fn complete_cancelled(operation: NativeOperation) {
        transfer_error(operation, STATUS_CANCELLED);
    }

    pub fn complete_network_error(operation: NativeOperation) {
        transfer_error(operation, STATUS_CLOUD_FILE_NETWORK_UNAVAILABLE);
    }

    fn acknowledge_delete(operation: NativeOperation, status: NTSTATUS) -> Result<(), AppError> {
        let info = operation_info(operation, CF_OPERATION_TYPE_ACK_DELETE);
        let parameters = CF_OPERATION_PARAMETERS {
            ParamSize: size_of::<CF_OPERATION_PARAMETERS>() as u32,
            Anonymous: CF_OPERATION_PARAMETERS_0 {
                AckDelete: CF_OPERATION_PARAMETERS_0_7 {
                    Flags: CF_OPERATION_ACK_DELETE_FLAG_NONE,
                    CompletionStatus: status,
                },
            },
        };
        execute(&info, parameters)
    }

    pub fn complete_delete_success(operation: NativeOperation) -> Result<(), AppError> {
        acknowledge_delete(operation, STATUS_SUCCESS)
    }

    pub fn complete_delete_network_error(operation: NativeOperation) -> Result<(), AppError> {
        acknowledge_delete(operation, STATUS_CLOUD_FILE_NETWORK_UNAVAILABLE)
    }

    pub fn complete_delete_error(operation: NativeOperation) -> Result<(), AppError> {
        acknowledge_delete(operation, STATUS_CLOUD_FILE_UNSUCCESSFUL)
    }

    pub fn complete_unsuccessful(operation: NativeOperation) {
        transfer_error(operation, STATUS_CLOUD_FILE_UNSUCCESSFUL);
    }

    pub fn convert_to_placeholder(
        path: &Path,
        entry_key: &str,
        is_directory: bool,
    ) -> Result<(), AppError> {
        let identity = encode_entry_identity(entry_key)?;
        let path = wide(path);
        let mut handle = ptr::null_mut();
        let open_result = unsafe {
            CfOpenFileWithOplock(
                path.as_ptr(),
                CF_OPEN_FILE_FLAG_EXCLUSIVE | CF_OPEN_FILE_FLAG_WRITE_ACCESS,
                &mut handle,
            )
        };
        if failed(open_result) {
            return Err(AppError::LocalData);
        }
        let flags = if is_directory {
            CF_CONVERT_FLAG_MARK_IN_SYNC | CF_CONVERT_FLAG_ENABLE_ON_DEMAND_POPULATION
        } else {
            CF_CONVERT_FLAG_MARK_IN_SYNC
        };
        let result = unsafe {
            CfConvertToPlaceholder(
                handle,
                identity.as_ptr().cast(),
                identity.len() as u32,
                flags,
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };
        unsafe { CfCloseHandle(handle) };
        if failed(result) {
            return Err(AppError::LocalData);
        }
        Ok(())
    }

    pub fn update_placeholder(
        path: &Path,
        spec: &PlaceholderSpec,
        dehydrate: bool,
    ) -> Result<(), AppError> {
        let identity = encode_entry_identity(&spec.entry_key)?;
        let metadata = placeholder_metadata(spec);
        let path = wide(path);
        let mut handle = ptr::null_mut();
        let open_result = unsafe {
            CfOpenFileWithOplock(
                path.as_ptr(),
                CF_OPEN_FILE_FLAG_EXCLUSIVE | CF_OPEN_FILE_FLAG_WRITE_ACCESS,
                &mut handle,
            )
        };
        if failed(open_result) {
            return Err(AppError::LocalData);
        }
        let mut flags = CF_UPDATE_FLAG_MARK_IN_SYNC;
        if dehydrate && !spec.is_directory {
            flags |= CF_UPDATE_FLAG_DEHYDRATE | CF_UPDATE_FLAG_ALLOW_PARTIAL;
        }
        let result = unsafe {
            CfUpdatePlaceholder(
                handle,
                &metadata,
                identity.as_ptr().cast(),
                identity.len() as u32,
                ptr::null(),
                0,
                flags,
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };
        unsafe { CfCloseHandle(handle) };
        if failed(result) {
            return Err(AppError::LocalData);
        }
        Ok(())
    }

    pub fn set_pinned(path: &Path, pinned: bool) -> Result<(), AppError> {
        let path = wide(path);
        let mut handle = ptr::null_mut();
        let open_result = unsafe {
            CfOpenFileWithOplock(path.as_ptr(), CF_OPEN_FILE_FLAG_WRITE_ACCESS, &mut handle)
        };
        if failed(open_result) {
            return Err(AppError::LocalData);
        }
        let result = unsafe {
            CfSetPinState(
                handle,
                if pinned {
                    CF_PIN_STATE_PINNED
                } else {
                    CF_PIN_STATE_UNPINNED
                },
                CF_SET_PIN_FLAG_NONE,
                ptr::null_mut(),
            )
        };
        unsafe { CfCloseHandle(handle) };
        if failed(result) {
            return Err(AppError::LocalData);
        }
        Ok(())
    }

    pub fn placeholder_state(path: &Path) -> Result<Option<PlaceholderState>, AppError> {
        let path = wide(path);
        let mut data = WIN32_FIND_DATAW::default();
        let handle = unsafe { FindFirstFileW(path.as_ptr(), &mut data) };
        if handle == INVALID_HANDLE_VALUE {
            let error = std::io::Error::last_os_error();
            return if error.kind() == std::io::ErrorKind::NotFound {
                Ok(None)
            } else {
                Err(AppError::Io(error))
            };
        }
        unsafe { FindClose(handle) };
        let state = unsafe { CfGetPlaceholderStateFromFindData((&raw const data).cast()) };
        let recall_attributes = data.dwFileAttributes
            & (FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS | FILE_ATTRIBUTE_RECALL_ON_OPEN)
            != 0;
        let cloud_attributes = recall_attributes
            || data.dwFileAttributes & (FILE_ATTRIBUTE_PINNED | FILE_ATTRIBUTE_UNPINNED) != 0;
        if state & CF_PLACEHOLDER_STATE_PLACEHOLDER == 0 && !cloud_attributes {
            return Ok(None);
        }
        Ok(Some(PlaceholderState {
            in_sync: state & CF_PLACEHOLDER_STATE_IN_SYNC != 0
                || state & CF_PLACEHOLDER_STATE_PLACEHOLDER == 0 && recall_attributes,
            partial: state
                & (CF_PLACEHOLDER_STATE_PARTIAL | CF_PLACEHOLDER_STATE_PARTIALLY_ON_DISK)
                != 0
                || recall_attributes,
            pinned: data.dwFileAttributes & FILE_ATTRIBUTE_PINNED != 0,
        }))
    }

    #[derive(Clone, Copy, Debug, Default)]
    pub struct PlaceholderState {
        pub in_sync: bool,
        pub partial: bool,
        pub pinned: bool,
    }

    fn placeholder_info(
        spec: &PlaceholderSpec,
        name: *const u16,
        identity: *const c_void,
        identity_length: u32,
    ) -> CF_PLACEHOLDER_CREATE_INFO {
        CF_PLACEHOLDER_CREATE_INFO {
            RelativeFileName: name,
            FsMetadata: placeholder_metadata(spec),
            FileIdentity: identity,
            FileIdentityLength: identity_length,
            Flags: CF_PLACEHOLDER_CREATE_FLAG_MARK_IN_SYNC,
            Result: 0,
            CreateUsn: 0,
        }
    }

    fn placeholder_metadata(spec: &PlaceholderSpec) -> CF_FS_METADATA {
        let timestamp = unix_millis_to_filetime(spec.modified_millis);
        let attributes = if spec.is_directory {
            FILE_ATTRIBUTE_DIRECTORY
        } else {
            FILE_ATTRIBUTE_ARCHIVE
        };
        CF_FS_METADATA {
            BasicInfo: FILE_BASIC_INFO {
                CreationTime: timestamp,
                LastAccessTime: timestamp,
                LastWriteTime: timestamp,
                ChangeTime: timestamp,
                FileAttributes: attributes,
            },
            FileSize: if spec.is_directory {
                0
            } else {
                spec.size.min(i64::MAX as u64) as i64
            },
        }
    }

    fn operation_info(operation: NativeOperation, kind: CF_OPERATION_TYPE) -> CF_OPERATION_INFO {
        CF_OPERATION_INFO {
            StructSize: size_of::<CF_OPERATION_INFO>() as u32,
            Type: kind,
            ConnectionKey: operation.connection_key,
            TransferKey: operation.transfer_key,
            CorrelationVector: ptr::null(),
            SyncStatus: ptr::null(),
            RequestKey: operation.request_key,
        }
    }

    fn execute(
        info: &CF_OPERATION_INFO,
        mut parameters: CF_OPERATION_PARAMETERS,
    ) -> Result<(), AppError> {
        let result = unsafe { CfExecute(info, &mut parameters) };
        if failed(result) {
            return Err(AppError::LocalData);
        }
        Ok(())
    }

    fn unix_millis_to_filetime(value: i64) -> i64 {
        value
            .saturating_mul(10_000)
            .saturating_add(WINDOWS_EPOCH_OFFSET_100NS)
    }

    fn failed(result: i32) -> bool {
        result < 0
    }

    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }

    fn wide_str(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }

    unsafe fn wide_pointer_to_string(pointer: *const u16) -> String {
        if pointer.is_null() {
            return String::new();
        }
        let mut length = 0usize;
        while unsafe { *pointer.add(length) } != 0 {
            length += 1;
        }
        String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(pointer, length) })
    }
}

#[cfg(windows)]
pub use windows::{
    CloudConnection, capability, capability_with_hresult, complete_cancelled,
    complete_delete_error, complete_delete_network_error, complete_delete_success,
    complete_network_error, complete_placeholders_cancelled, complete_placeholders_error,
    complete_unsuccessful, connect, convert_to_placeholder, create_placeholders,
    is_owned_sync_root, placeholder_state, register, set_pinned, transfer_data,
    transfer_placeholders, unregister, update_placeholder,
};

#[cfg(all(windows, test))]
pub use windows::connect_test_provider;

#[cfg(not(windows))]
#[derive(Debug)]
pub struct CloudConnection;

#[cfg(not(windows))]
#[derive(Clone, Copy, Debug, Default)]
pub struct PlaceholderState {
    pub in_sync: bool,
    pub partial: bool,
    pub pinned: bool,
    pub unpinned: bool,
}

#[cfg(not(windows))]
pub fn capability() -> NativeSyncCapability {
    capability_with_hresult().0
}

#[cfg(not(windows))]
pub fn capability_with_hresult() -> (NativeSyncCapability, Option<i32>) {
    (
        NativeSyncCapability {
            supported: false,
            reason: Some("按需同步仅支持 Windows 10/11。".to_owned()),
            build_number: None,
            integration_number: None,
        },
        None,
    )
}

#[cfg(not(windows))]
pub fn register(_root: &Path, _pair_id: &str) -> Result<(), AppError> {
    Err(AppError::Initialization)
}

#[cfg(not(windows))]
pub fn unregister(_root: &Path, _pair_id: &str) -> Result<(), AppError> {
    Err(AppError::Initialization)
}

#[cfg(not(windows))]
pub fn is_owned_sync_root(_root: &Path, _pair_id: &str) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::{decode_entry_identity, encode_entry_identity};

    #[cfg(windows)]
    #[test]
    fn callback_path_includes_the_cfapi_callback_volume() {
        use std::path::PathBuf;

        assert_eq!(
            super::callback_path_from_parts("D:", r"\sync-test"),
            PathBuf::from(r"D:\sync-test")
        );
        assert_eq!(
            super::callback_path_from_parts("D:", r"\sync-test\子目录"),
            PathBuf::from(r"D:\sync-test\子目录")
        );
        assert_eq!(
            super::callback_path_from_parts("D:", r"D:\sync-test"),
            PathBuf::from(r"D:\sync-test")
        );
    }

    #[test]
    fn entry_identity_is_versioned_and_opaque() {
        let key = "81a9dbe3-df49-4515-ab3b-35187e37075d";
        let identity = encode_entry_identity(key).expect("encode identity");
        assert!(identity.starts_with(b"KFG1:"));
        assert_eq!(
            decode_entry_identity(&identity).expect("decode identity"),
            key
        );
        assert!(!String::from_utf8_lossy(&identity).contains("/remote/path"));
    }

    #[test]
    fn rejects_unversioned_or_nil_identities() {
        assert!(decode_entry_identity(b"81a9dbe3-df49-4515-ab3b-35187e37075d").is_err());
        assert!(encode_entry_identity("00000000-0000-0000-0000-000000000000").is_err());
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "mutates a temporary Windows Cloud Files sync root"]
    fn windows_registers_updates_and_unregisters_an_owned_placeholder() {
        use std::{fs, path::PathBuf};

        use super::{
            PlaceholderSpec, connect_test_provider, create_placeholders, is_owned_sync_root,
            placeholder_state, register, set_pinned, unregister, update_placeholder,
        };

        struct Cleanup {
            root: PathBuf,
            pair_id: String,
        }

        impl Drop for Cleanup {
            fn drop(&mut self) {
                if is_owned_sync_root(&self.root, &self.pair_id) {
                    let _ = unregister(&self.root, &self.pair_id);
                }
                let expected_name = format!("koofr-cfapi-test-{}", self.pair_id);
                if self.root.parent() == Some(std::env::temp_dir().as_path())
                    && self.root.file_name().and_then(|name| name.to_str())
                        == Some(expected_name.as_str())
                {
                    let _ = fs::remove_file(self.root.join("probe.bin"));
                    let _ = fs::remove_dir(&self.root);
                }
            }
        }

        let pair_id = uuid::Uuid::new_v4().to_string();
        let root = std::env::temp_dir().join(format!("koofr-cfapi-test-{pair_id}"));
        fs::create_dir(&root).expect("create isolated temporary sync root");
        let cleanup = Cleanup {
            root: root.clone(),
            pair_id: pair_id.clone(),
        };
        register(&root, &pair_id).expect("register sync root");
        assert!(is_owned_sync_root(&root, &pair_id));
        let connection = connect_test_provider(&root).expect("connect test provider");

        let entry_key = uuid::Uuid::new_v4().to_string();
        let mut placeholder = PlaceholderSpec {
            name: "probe.bin".to_owned(),
            entry_key,
            is_directory: false,
            size: 3,
            modified_millis: 1_700_000_000_000,
        };
        create_placeholders(&root, &[placeholder.clone()]).expect("create placeholder");
        let target = root.join(&placeholder.name);
        assert!(
            placeholder_state(&target)
                .expect("read placeholder state")
                .is_some()
        );
        assert_eq!(fs::read(&target).expect("hydrate placeholder"), b"abc");
        let hydrated = placeholder_state(&target)
            .expect("read hydrated placeholder state")
            .expect("hydrated file remains a placeholder");
        assert!(!hydrated.partial);
        drop(connection);
        register(&root, &pair_id).expect("re-register owned root after provider restart");
        let connection = connect_test_provider(&root).expect("reconnect test provider");

        set_pinned(&target, true).expect("pin placeholder");
        assert!(
            placeholder_state(&target)
                .expect("read pinned placeholder state")
                .expect("pinned file remains a placeholder")
                .pinned
        );
        set_pinned(&target, false).expect("unpin placeholder");
        placeholder.modified_millis += 1_000;
        update_placeholder(&target, &placeholder, true).expect("update and dehydrate placeholder");
        assert!(
            placeholder_state(&target)
                .expect("read dehydrated placeholder state")
                .expect("dehydrated file remains a placeholder")
                .partial
        );
        fs::remove_file(&target).expect("delete placeholder after provider acknowledgement");
        assert!(!target.exists());
        drop(connection);
        unregister(&root, &pair_id).expect("unregister owned sync root");
        assert!(!is_owned_sync_root(&root, &pair_id));
        drop(cleanup);
    }
}
