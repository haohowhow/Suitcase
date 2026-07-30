use std::{path::PathBuf, sync::Arc};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::error::AppError;

const SETTINGS_VERSION: u8 = 1;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CacheMode {
    Off,
    #[default]
    Memory,
    Disk,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Error,
    Warn,
    #[default]
    Info,
    Debug,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InterfaceLanguage {
    #[default]
    System,
    ZhCn,
    EnUs,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UiDensity {
    #[default]
    Standard,
    Compact,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DownloadConflictStrategy {
    Ask,
    Skip,
    #[default]
    #[serde(alias = "overwrite")]
    Rename,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BandwidthMode {
    #[default]
    Full,
    Limited,
    IdleFull,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LinkDefaultKind {
    #[default]
    Download,
    Upload,
}

#[derive(Clone, Debug)]
pub struct SettingsDefaults {
    pub download_directory: PathBuf,
    pub cache_directory: PathBuf,
    pub log_directory: PathBuf,
}

#[derive(Clone, Copy, Debug)]
pub struct NetworkRetrySettings {
    pub enabled: bool,
    pub max_retries: Option<u32>,
    pub interval_seconds: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct TransferAdvancedSettings {
    pub upload_concurrency: u32,
    pub download_concurrency: u32,
    pub default_split_part_mib: u32,
    pub auto_split_threshold_mib: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct DownloadBehaviorSettings {
    pub conflict_strategy: DownloadConflictStrategy,
    pub open_folder_after_download: bool,
    pub verify_hash_after_download: bool,
    pub keep_failed_part_days: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct BandwidthSettings {
    pub global_kbps: u32,
    pub per_task_kbps: u32,
    pub mode: BandwidthMode,
}

#[derive(Clone, Copy, Debug)]
pub struct NotificationSettings {
    pub notify_on_complete: bool,
    pub notify_sound: bool,
    pub tray_badge: bool,
    pub floating_window_enabled: bool,
    pub floating_window_prompted: bool,
    pub floating_window_compact_enabled: bool,
    pub floating_window_click_toggle_enabled: bool,
    pub floating_window_double_click_open_enabled: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct LinkDefaults {
    pub kind: LinkDefaultKind,
    pub expire_days: u32,
    pub require_password: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct SecuritySettings {
    pub idle_auto_logout_minutes: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct HistoryPolicy {
    pub keep_days: u32,
    pub keep_count: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct AppearanceSettings {
    pub language: InterfaceLanguage,
    pub reduce_motion: bool,
    pub density: UiDensity,
}

#[derive(Clone, Copy, Debug)]
pub struct GeneralSettings {
    pub launch_at_login: bool,
    pub close_to_tray: bool,
    pub minimize_to_tray: bool,
    pub confirm_quit_with_transfers: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredSettings {
    version: u8,
    cache_mode: CacheMode,
    cache_ttl_minutes: u32,
    remembered_email: Option<String>,
    #[serde(default)]
    download_directory: Option<PathBuf>,
    #[serde(default = "default_ask_download_location")]
    ask_download_location: bool,
    #[serde(default)]
    log_level: LogLevel,
    #[serde(default = "default_log_retention_days")]
    log_retention_days: u32,
    #[serde(default = "default_log_max_file_size_mb")]
    log_max_file_size_mb: u32,
    #[serde(default)]
    auto_retry_network_errors: bool,
    #[serde(default = "default_network_retry_limit")]
    network_retry_limit: Option<u32>,
    #[serde(default = "default_network_retry_interval_seconds")]
    network_retry_interval_seconds: u32,
    #[serde(default)]
    language: InterfaceLanguage,
    #[serde(default)]
    reduce_motion: bool,
    #[serde(default)]
    density: UiDensity,
    #[serde(default)]
    launch_at_login: bool,
    #[serde(default = "default_close_to_tray")]
    close_to_tray: bool,
    #[serde(default)]
    minimize_to_tray: bool,
    #[serde(default = "default_confirm_quit")]
    confirm_quit_with_transfers: bool,
    #[serde(default = "default_upload_concurrency")]
    upload_concurrency: u32,
    #[serde(default = "default_download_concurrency")]
    download_concurrency: u32,
    #[serde(default = "default_split_part_mib")]
    default_split_part_mib: u32,
    #[serde(default = "default_auto_split_threshold_mib")]
    auto_split_threshold_mib: u32,
    #[serde(default)]
    download_conflict_strategy: DownloadConflictStrategy,
    #[serde(default)]
    open_folder_after_download: bool,
    #[serde(default = "default_verify_hash")]
    verify_hash_after_download: bool,
    #[serde(default = "default_keep_failed_part_days")]
    keep_failed_part_days: u32,
    #[serde(default)]
    global_bandwidth_limit_kbps: u32,
    #[serde(default)]
    per_task_bandwidth_limit_kbps: u32,
    #[serde(default)]
    bandwidth_mode: BandwidthMode,
    #[serde(default = "default_notify_on_complete")]
    notify_on_complete: bool,
    #[serde(default)]
    notify_sound: bool,
    #[serde(default = "default_tray_badge")]
    tray_badge: bool,
    #[serde(default)]
    floating_window_enabled: bool,
    #[serde(default)]
    floating_window_prompted: bool,
    #[serde(default = "default_floating_window_compact_enabled")]
    floating_window_compact_enabled: bool,
    #[serde(default = "default_floating_window_click_toggle_enabled")]
    floating_window_click_toggle_enabled: bool,
    #[serde(default = "default_floating_window_double_click_open_enabled")]
    floating_window_double_click_open_enabled: bool,
    #[serde(default)]
    floating_window_x: Option<i32>,
    #[serde(default)]
    floating_window_y: Option<i32>,
    #[serde(default)]
    link_default_kind: LinkDefaultKind,
    #[serde(default)]
    link_default_expire_days: u32,
    #[serde(default)]
    link_default_require_password: bool,
    #[serde(default)]
    idle_auto_logout_minutes: u32,
    #[serde(default = "default_history_keep_days")]
    download_history_keep_days: u32,
    #[serde(default = "default_history_keep_count")]
    download_history_keep_count: u32,
}

const fn default_ask_download_location() -> bool {
    true
}
const fn default_log_retention_days() -> u32 {
    14
}
const fn default_log_max_file_size_mb() -> u32 {
    10
}
const fn default_network_retry_limit() -> Option<u32> {
    Some(8)
}
const fn default_network_retry_interval_seconds() -> u32 {
    5
}
const fn default_close_to_tray() -> bool {
    true
}
const fn default_confirm_quit() -> bool {
    true
}
const fn default_upload_concurrency() -> u32 {
    2
}
const fn default_download_concurrency() -> u32 {
    3
}
const fn default_split_part_mib() -> u32 {
    64
}
const fn default_auto_split_threshold_mib() -> u32 {
    512
}
const fn default_verify_hash() -> bool {
    true
}
const fn default_keep_failed_part_days() -> u32 {
    3
}
const fn default_notify_on_complete() -> bool {
    true
}
const fn default_tray_badge() -> bool {
    true
}
const fn default_floating_window_compact_enabled() -> bool {
    true
}
const fn default_floating_window_click_toggle_enabled() -> bool {
    true
}
const fn default_floating_window_double_click_open_enabled() -> bool {
    true
}
const fn default_history_keep_days() -> u32 {
    30
}
const fn default_history_keep_count() -> u32 {
    500
}

impl Default for StoredSettings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            cache_mode: CacheMode::Memory,
            cache_ttl_minutes: 15,
            remembered_email: None,
            download_directory: None,
            ask_download_location: default_ask_download_location(),
            log_level: LogLevel::Info,
            log_retention_days: default_log_retention_days(),
            log_max_file_size_mb: default_log_max_file_size_mb(),
            auto_retry_network_errors: false,
            network_retry_limit: default_network_retry_limit(),
            network_retry_interval_seconds: default_network_retry_interval_seconds(),
            language: InterfaceLanguage::default(),
            reduce_motion: false,
            density: UiDensity::default(),
            launch_at_login: false,
            close_to_tray: default_close_to_tray(),
            minimize_to_tray: false,
            confirm_quit_with_transfers: default_confirm_quit(),
            upload_concurrency: default_upload_concurrency(),
            download_concurrency: default_download_concurrency(),
            default_split_part_mib: default_split_part_mib(),
            auto_split_threshold_mib: default_auto_split_threshold_mib(),
            download_conflict_strategy: DownloadConflictStrategy::default(),
            open_folder_after_download: false,
            verify_hash_after_download: default_verify_hash(),
            keep_failed_part_days: default_keep_failed_part_days(),
            global_bandwidth_limit_kbps: 0,
            per_task_bandwidth_limit_kbps: 0,
            bandwidth_mode: BandwidthMode::default(),
            notify_on_complete: default_notify_on_complete(),
            notify_sound: false,
            tray_badge: default_tray_badge(),
            floating_window_enabled: false,
            floating_window_prompted: false,
            floating_window_compact_enabled: default_floating_window_compact_enabled(),
            floating_window_click_toggle_enabled: default_floating_window_click_toggle_enabled(),
            floating_window_double_click_open_enabled:
                default_floating_window_double_click_open_enabled(),
            floating_window_x: None,
            floating_window_y: None,
            link_default_kind: LinkDefaultKind::default(),
            link_default_expire_days: 0,
            link_default_require_password: false,
            idle_auto_logout_minutes: 0,
            download_history_keep_days: default_history_keep_days(),
            download_history_keep_count: default_history_keep_count(),
        }
    }
}

#[derive(Clone)]
pub struct SettingsStore {
    path: PathBuf,
    state: Arc<RwLock<StoredSettings>>,
    initial_cache_mode: CacheMode,
    defaults: SettingsDefaults,
    initial_cache_directory: PathBuf,
    initial_log_config: (PathBuf, LogLevel, u32, u32),
}

impl SettingsStore {
    pub fn load(path: PathBuf, defaults: SettingsDefaults) -> Self {
        let mut state = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<StoredSettings>(&bytes).ok())
            .filter(|settings| settings.version == SETTINGS_VERSION)
            .unwrap_or_default();
        if state
            .network_retry_limit
            .is_some_and(|limit| !(1..=10_000).contains(&limit))
        {
            state.network_retry_limit = default_network_retry_limit();
        }
        if !(1..=3_600).contains(&state.network_retry_interval_seconds) {
            state.network_retry_interval_seconds = default_network_retry_interval_seconds();
        }
        if !(1..=16).contains(&state.upload_concurrency) {
            state.upload_concurrency = default_upload_concurrency();
        }
        if !(1..=16).contains(&state.download_concurrency) {
            state.download_concurrency = default_download_concurrency();
        }
        if !(1..=1024).contains(&state.default_split_part_mib) {
            state.default_split_part_mib = default_split_part_mib();
        }
        if !(16..=4096).contains(&state.auto_split_threshold_mib) {
            state.auto_split_threshold_mib = default_auto_split_threshold_mib();
        }
        if state.keep_failed_part_days > 30 {
            state.keep_failed_part_days = default_keep_failed_part_days();
        }
        if state.global_bandwidth_limit_kbps > 10_000_000 {
            state.global_bandwidth_limit_kbps = 0;
        }
        if state.per_task_bandwidth_limit_kbps > 10_000_000 {
            state.per_task_bandwidth_limit_kbps = 0;
        }
        if state.link_default_expire_days > 365 {
            state.link_default_expire_days = 0;
        }
        if state.idle_auto_logout_minutes > 1440 {
            state.idle_auto_logout_minutes = 0;
        }
        if state.download_history_keep_days > 3650 {
            state.download_history_keep_days = default_history_keep_days();
        }
        if state.download_history_keep_count > 100_000 {
            state.download_history_keep_count = default_history_keep_count();
        }
        let initial_cache_directory = defaults.cache_directory.clone();
        let initial_log_config = (
            defaults.log_directory.clone(),
            state.log_level,
            state.log_retention_days,
            state.log_max_file_size_mb,
        );
        Self {
            path,
            initial_cache_mode: state.cache_mode,
            defaults,
            initial_cache_directory,
            initial_log_config,
            state: Arc::new(RwLock::new(state)),
        }
    }

    pub const fn initial_cache_mode(&self) -> CacheMode {
        self.initial_cache_mode
    }

    pub fn initial_cache_directory(&self) -> &PathBuf {
        &self.initial_cache_directory
    }

    pub fn initial_log_config(&self) -> &(PathBuf, LogLevel, u32, u32) {
        &self.initial_log_config
    }

    pub async fn cache_policy(&self) -> (CacheMode, u32) {
        let state = self.state.read().await;
        (state.cache_mode, state.cache_ttl_minutes)
    }

    pub async fn remembered_email(&self) -> Option<String> {
        self.state.read().await.remembered_email.clone()
    }

    pub async fn download_policy(&self) -> (PathBuf, bool) {
        let state = self.state.read().await;
        (
            state
                .download_directory
                .clone()
                .unwrap_or_else(|| self.defaults.download_directory.clone()),
            state.ask_download_location,
        )
    }

    pub async fn log_policy(&self) -> (PathBuf, LogLevel, u32, u32) {
        let state = self.state.read().await;
        (
            self.defaults.log_directory.clone(),
            state.log_level,
            state.log_retention_days,
            state.log_max_file_size_mb,
        )
    }

    pub async fn network_retry_settings(&self) -> NetworkRetrySettings {
        let state = self.state.read().await;
        NetworkRetrySettings {
            enabled: state.auto_retry_network_errors,
            max_retries: state.network_retry_limit,
            interval_seconds: state.network_retry_interval_seconds,
        }
    }

    pub async fn appearance(&self) -> AppearanceSettings {
        let state = self.state.read().await;
        AppearanceSettings {
            language: state.language,
            reduce_motion: state.reduce_motion,
            density: state.density,
        }
    }

    pub async fn general(&self) -> GeneralSettings {
        let state = self.state.read().await;
        GeneralSettings {
            launch_at_login: state.launch_at_login,
            close_to_tray: state.close_to_tray,
            minimize_to_tray: state.minimize_to_tray,
            confirm_quit_with_transfers: state.confirm_quit_with_transfers,
        }
    }

    pub async fn transfer_advanced(&self) -> TransferAdvancedSettings {
        let state = self.state.read().await;
        TransferAdvancedSettings {
            upload_concurrency: state.upload_concurrency,
            download_concurrency: state.download_concurrency,
            default_split_part_mib: state.default_split_part_mib,
            auto_split_threshold_mib: state.auto_split_threshold_mib,
        }
    }

    pub async fn download_behavior(&self) -> DownloadBehaviorSettings {
        let state = self.state.read().await;
        DownloadBehaviorSettings {
            conflict_strategy: state.download_conflict_strategy,
            open_folder_after_download: state.open_folder_after_download,
            verify_hash_after_download: state.verify_hash_after_download,
            keep_failed_part_days: state.keep_failed_part_days,
        }
    }

    pub async fn bandwidth(&self) -> BandwidthSettings {
        let state = self.state.read().await;
        BandwidthSettings {
            global_kbps: state.global_bandwidth_limit_kbps,
            per_task_kbps: state.per_task_bandwidth_limit_kbps,
            mode: state.bandwidth_mode,
        }
    }

    pub async fn notifications(&self) -> NotificationSettings {
        let state = self.state.read().await;
        NotificationSettings {
            notify_on_complete: state.notify_on_complete,
            notify_sound: state.notify_sound,
            tray_badge: state.tray_badge,
            floating_window_enabled: state.floating_window_enabled,
            floating_window_prompted: state.floating_window_prompted,
            floating_window_compact_enabled: state.floating_window_compact_enabled,
            floating_window_click_toggle_enabled: state.floating_window_click_toggle_enabled,
            floating_window_double_click_open_enabled: state
                .floating_window_double_click_open_enabled,
        }
    }

    pub async fn floating_window_position(&self) -> Option<(i32, i32)> {
        let state = self.state.read().await;
        state.floating_window_x.zip(state.floating_window_y)
    }

    pub async fn link_defaults(&self) -> LinkDefaults {
        let state = self.state.read().await;
        LinkDefaults {
            kind: state.link_default_kind,
            expire_days: state.link_default_expire_days,
            require_password: state.link_default_require_password,
        }
    }

    pub async fn security(&self) -> SecuritySettings {
        let state = self.state.read().await;
        SecuritySettings {
            idle_auto_logout_minutes: state.idle_auto_logout_minutes,
        }
    }

    pub async fn history_policy(&self) -> HistoryPolicy {
        let state = self.state.read().await;
        HistoryPolicy {
            keep_days: state.download_history_keep_days,
            keep_count: state.download_history_keep_count,
        }
    }

    pub async fn update_cache(
        &self,
        cache_mode: CacheMode,
        cache_ttl_minutes: u32,
    ) -> Result<(), AppError> {
        if !(1..=1440).contains(&cache_ttl_minutes) {
            return Err(AppError::InvalidInput("cache_ttl_minutes"));
        }
        {
            let mut state = self.state.write().await;
            state.cache_mode = cache_mode;
            state.cache_ttl_minutes = cache_ttl_minutes;
        }
        self.persist().await
    }

    pub async fn update_logging(
        &self,
        log_level: LogLevel,
        log_retention_days: u32,
        log_max_file_size_mb: u32,
    ) -> Result<(), AppError> {
        if !(1..=365).contains(&log_retention_days) {
            return Err(AppError::InvalidInput("log retention days"));
        }
        if !(1..=100).contains(&log_max_file_size_mb) {
            return Err(AppError::InvalidInput("log max file size"));
        }
        {
            let mut state = self.state.write().await;
            state.log_level = log_level;
            state.log_retention_days = log_retention_days;
            state.log_max_file_size_mb = log_max_file_size_mb;
        }
        self.persist().await
    }

    pub async fn update_transfer(
        &self,
        auto_retry_network_errors: bool,
        network_retry_limit: Option<u32>,
        network_retry_interval_seconds: u32,
    ) -> Result<(), AppError> {
        if network_retry_limit.is_some_and(|limit| !(1..=10_000).contains(&limit)) {
            return Err(AppError::InvalidInput("network retry limit"));
        }
        if !(1..=3_600).contains(&network_retry_interval_seconds) {
            return Err(AppError::InvalidInput("network retry interval"));
        }
        {
            let mut state = self.state.write().await;
            state.auto_retry_network_errors = auto_retry_network_errors;
            state.network_retry_limit = network_retry_limit;
            state.network_retry_interval_seconds = network_retry_interval_seconds;
        }
        self.persist().await
    }

    pub async fn set_remembered_email(&self, email: Option<String>) -> Result<(), AppError> {
        self.state.write().await.remembered_email = email;
        self.persist().await
    }

    pub async fn update_download(
        &self,
        download_directory: PathBuf,
        ask_download_location: bool,
    ) -> Result<(), AppError> {
        if !download_directory.is_absolute() {
            return Err(AppError::InvalidInput("download directory"));
        }
        let metadata = tokio::fs::symlink_metadata(&download_directory).await?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(AppError::InvalidInput("download directory"));
        }
        {
            let mut state = self.state.write().await;
            state.download_directory = Some(download_directory);
            state.ask_download_location = ask_download_location;
        }
        self.persist().await
    }

    pub async fn update_appearance(
        &self,
        language: InterfaceLanguage,
        reduce_motion: bool,
        density: UiDensity,
    ) -> Result<(), AppError> {
        {
            let mut state = self.state.write().await;
            state.language = language;
            state.reduce_motion = reduce_motion;
            state.density = density;
        }
        self.persist().await
    }

    pub async fn update_general(
        &self,
        launch_at_login: bool,
        close_to_tray: bool,
        minimize_to_tray: bool,
        confirm_quit_with_transfers: bool,
    ) -> Result<(), AppError> {
        {
            let mut state = self.state.write().await;
            state.launch_at_login = launch_at_login;
            state.close_to_tray = close_to_tray;
            state.minimize_to_tray = minimize_to_tray;
            state.confirm_quit_with_transfers = confirm_quit_with_transfers;
        }
        self.persist().await
    }

    pub async fn update_transfer_advanced(
        &self,
        upload_concurrency: u32,
        download_concurrency: u32,
        default_split_part_mib: u32,
        auto_split_threshold_mib: u32,
    ) -> Result<(), AppError> {
        if !(1..=16).contains(&upload_concurrency) {
            return Err(AppError::InvalidInput("upload_concurrency"));
        }
        if !(1..=16).contains(&download_concurrency) {
            return Err(AppError::InvalidInput("download_concurrency"));
        }
        if !(1..=1024).contains(&default_split_part_mib) {
            return Err(AppError::InvalidInput("default_split_part_mib"));
        }
        if !(16..=4096).contains(&auto_split_threshold_mib) {
            return Err(AppError::InvalidInput("auto_split_threshold_mib"));
        }
        {
            let mut state = self.state.write().await;
            state.upload_concurrency = upload_concurrency;
            state.download_concurrency = download_concurrency;
            state.default_split_part_mib = default_split_part_mib;
            state.auto_split_threshold_mib = auto_split_threshold_mib;
        }
        self.persist().await
    }

    pub async fn update_download_behavior(
        &self,
        conflict_strategy: DownloadConflictStrategy,
        open_folder_after_download: bool,
        verify_hash_after_download: bool,
        keep_failed_part_days: u32,
    ) -> Result<(), AppError> {
        if keep_failed_part_days > 30 {
            return Err(AppError::InvalidInput("keep_failed_part_days"));
        }
        {
            let mut state = self.state.write().await;
            state.download_conflict_strategy = conflict_strategy;
            state.open_folder_after_download = open_folder_after_download;
            state.verify_hash_after_download = verify_hash_after_download;
            state.keep_failed_part_days = keep_failed_part_days;
        }
        self.persist().await
    }

    pub async fn update_bandwidth(
        &self,
        global_kbps: u32,
        per_task_kbps: u32,
        mode: BandwidthMode,
    ) -> Result<(), AppError> {
        if global_kbps > 10_000_000 {
            return Err(AppError::InvalidInput("global_bandwidth_limit_kbps"));
        }
        if per_task_kbps > 10_000_000 {
            return Err(AppError::InvalidInput("per_task_bandwidth_limit_kbps"));
        }
        {
            let mut state = self.state.write().await;
            state.global_bandwidth_limit_kbps = global_kbps;
            state.per_task_bandwidth_limit_kbps = per_task_kbps;
            state.bandwidth_mode = mode;
        }
        self.persist().await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_notifications(
        &self,
        notify_on_complete: bool,
        notify_sound: bool,
        tray_badge: bool,
        floating_window_enabled: bool,
        floating_window_prompted: bool,
        floating_window_compact_enabled: bool,
        floating_window_click_toggle_enabled: bool,
        floating_window_double_click_open_enabled: bool,
    ) -> Result<(), AppError> {
        {
            let mut state = self.state.write().await;
            state.notify_on_complete = notify_on_complete;
            state.notify_sound = notify_sound;
            state.tray_badge = tray_badge;
            state.floating_window_enabled = floating_window_enabled;
            state.floating_window_prompted = floating_window_prompted;
            state.floating_window_compact_enabled = floating_window_compact_enabled;
            state.floating_window_click_toggle_enabled = floating_window_click_toggle_enabled;
            state.floating_window_double_click_open_enabled =
                floating_window_double_click_open_enabled;
        }
        self.persist().await
    }

    pub async fn update_floating_window_position(&self, x: i32, y: i32) -> Result<(), AppError> {
        {
            let mut state = self.state.write().await;
            state.floating_window_x = Some(x);
            state.floating_window_y = Some(y);
        }
        self.persist().await
    }

    pub async fn disable_floating_window(&self) -> Result<(), AppError> {
        {
            let mut state = self.state.write().await;
            state.floating_window_enabled = false;
            state.floating_window_prompted = true;
        }
        self.persist().await
    }

    pub async fn update_link_defaults(
        &self,
        kind: LinkDefaultKind,
        expire_days: u32,
        require_password: bool,
    ) -> Result<(), AppError> {
        if expire_days > 365 {
            return Err(AppError::InvalidInput("link_default_expire_days"));
        }
        {
            let mut state = self.state.write().await;
            state.link_default_kind = kind;
            state.link_default_expire_days = expire_days;
            state.link_default_require_password = require_password;
        }
        self.persist().await
    }

    pub async fn update_security(&self, idle_auto_logout_minutes: u32) -> Result<(), AppError> {
        if idle_auto_logout_minutes > 1440 {
            return Err(AppError::InvalidInput("idle_auto_logout_minutes"));
        }
        {
            let mut state = self.state.write().await;
            state.idle_auto_logout_minutes = idle_auto_logout_minutes;
        }
        self.persist().await
    }

    pub async fn update_history_policy(
        &self,
        keep_days: u32,
        keep_count: u32,
    ) -> Result<(), AppError> {
        if keep_days > 3650 {
            return Err(AppError::InvalidInput("download_history_keep_days"));
        }
        if keep_count > 100_000 {
            return Err(AppError::InvalidInput("download_history_keep_count"));
        }
        {
            let mut state = self.state.write().await;
            state.download_history_keep_days = keep_days;
            state.download_history_keep_count = keep_count;
        }
        self.persist().await
    }

    async fn persist(&self) -> Result<(), AppError> {
        let payload = serde_json::to_vec_pretty(&*self.state.read().await)
            .map_err(|_| AppError::LocalData)?;
        let parent = self.path.parent().ok_or(AppError::LocalData)?;
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|_| AppError::LocalData)?;
        let temporary = self.path.with_extension("json.tmp");
        tokio::fs::write(&temporary, payload)
            .await
            .map_err(|_| AppError::LocalData)?;
        if tokio::fs::try_exists(&self.path).await.unwrap_or(false) {
            tokio::fs::remove_file(&self.path)
                .await
                .map_err(|_| AppError::LocalData)?;
        }
        tokio::fs::rename(temporary, &self.path)
            .await
            .map_err(|_| AppError::LocalData)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BandwidthMode, CacheMode, DownloadConflictStrategy, InterfaceLanguage, LinkDefaultKind,
        SettingsDefaults, SettingsStore, UiDensity,
    };

    fn defaults(directory: &std::path::Path) -> SettingsDefaults {
        SettingsDefaults {
            download_directory: directory.join("Downloads"),
            cache_directory: directory.join("Cache"),
            log_directory: directory.join("Logs"),
        }
    }

    #[tokio::test]
    async fn persists_non_secret_settings_without_credentials() {
        let directory =
            std::env::temp_dir().join(format!("koofr-settings-{}", uuid::Uuid::new_v4()));
        let path = directory.join("settings.json");
        let default_download_directory = directory.join("Downloads");
        std::fs::create_dir_all(&default_download_directory).expect("create downloads directory");
        std::fs::create_dir_all(directory.join("Cache")).expect("create cache directory");
        let store = SettingsStore::load(path.clone(), defaults(&directory));

        store
            .update_cache(CacheMode::Disk, 60)
            .await
            .expect("update cache settings");
        store
            .set_remembered_email(Some("person@example.com".to_owned()))
            .await
            .expect("store remembered email");

        let reloaded = SettingsStore::load(path.clone(), defaults(&directory));
        assert_eq!(reloaded.cache_policy().await, (CacheMode::Disk, 60));
        assert_eq!(
            reloaded.remembered_email().await.as_deref(),
            Some("person@example.com")
        );
        let payload = std::fs::read_to_string(path).expect("read settings file");
        assert!(!payload.contains("password"));

        std::fs::remove_dir_all(directory).expect("remove settings directory");
    }

    #[tokio::test]
    async fn uses_the_os_download_directory_and_prompts_by_default() {
        // Given
        let directory =
            std::env::temp_dir().join(format!("koofr-settings-{}", uuid::Uuid::new_v4()));
        let default_download_directory = directory.join("Downloads");

        // When
        let store = SettingsStore::load(directory.join("settings.json"), defaults(&directory));

        // Then
        assert_eq!(
            store.download_policy().await,
            (default_download_directory, true)
        );
    }

    #[tokio::test]
    async fn persists_validated_download_preferences() {
        let directory =
            std::env::temp_dir().join(format!("koofr-settings-{}", uuid::Uuid::new_v4()));
        let custom_download_directory = directory.join("Koofr downloads");
        std::fs::create_dir_all(&custom_download_directory)
            .expect("create custom downloads directory");
        let path = directory.join("settings.json");
        let store = SettingsStore::load(path.clone(), defaults(&directory));

        store
            .update_download(custom_download_directory.clone(), false)
            .await
            .expect("save download settings");
        let reloaded = SettingsStore::load(path, defaults(&directory));

        assert_eq!(
            reloaded.download_policy().await,
            (custom_download_directory, false)
        );
        std::fs::remove_dir_all(directory).expect("remove settings directory");
    }

    #[tokio::test]
    async fn rejects_a_download_destination_that_is_not_a_directory() {
        let directory =
            std::env::temp_dir().join(format!("koofr-settings-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).expect("create settings directory");
        let file_path = directory.join("not-a-directory.txt");
        std::fs::write(&file_path, b"not a directory").expect("create test file");
        let store = SettingsStore::load(directory.join("settings.json"), defaults(&directory));

        let result = store.update_download(file_path, true).await;

        assert!(result.is_err());
        std::fs::remove_dir_all(directory).expect("remove settings directory");
    }

    #[tokio::test]
    async fn persists_the_network_error_retry_switch() {
        let directory =
            std::env::temp_dir().join(format!("koofr-settings-{}", uuid::Uuid::new_v4()));
        let path = directory.join("settings.json");
        let store = SettingsStore::load(path.clone(), defaults(&directory));
        assert!(!store.network_retry_settings().await.enabled);

        store
            .update_transfer(true, None, 30)
            .await
            .expect("enable network retry");
        let reloaded = SettingsStore::load(path, defaults(&directory));
        let retry = reloaded.network_retry_settings().await;

        assert!(retry.enabled);
        assert_eq!(retry.max_retries, None);
        assert_eq!(retry.interval_seconds, 30);
        std::fs::remove_dir_all(directory).expect("remove settings directory");
    }

    #[tokio::test]
    async fn ignores_legacy_standalone_cache_and_log_directories() {
        let directory =
            std::env::temp_dir().join(format!("koofr-settings-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).expect("create settings directory");
        let path = directory.join("settings.json");
        std::fs::write(
            &path,
            br#"{
                "version": 1,
                "cacheMode": "memory",
                "cacheTtlMinutes": 15,
                "rememberedEmail": null,
                "cacheDirectory": "D:\\Legacy Cache",
                "logDirectory": "D:\\Legacy Logs"
            }"#,
        )
        .expect("write legacy settings");
        let store = SettingsStore::load(path, defaults(&directory));

        assert_eq!(store.initial_cache_directory(), &directory.join("Cache"));
        assert_eq!(store.log_policy().await.0, directory.join("Logs"));
        std::fs::remove_dir_all(directory).expect("remove settings directory");
    }

    #[tokio::test]
    async fn persists_appearance_general_and_transfer_advanced_settings() {
        let directory =
            std::env::temp_dir().join(format!("koofr-settings-{}", uuid::Uuid::new_v4()));
        let path = directory.join("settings.json");
        let store = SettingsStore::load(path.clone(), defaults(&directory));

        store
            .update_appearance(InterfaceLanguage::ZhCn, true, UiDensity::Compact)
            .await
            .expect("save appearance");
        store
            .update_general(true, false, true, false)
            .await
            .expect("save general");
        store
            .update_transfer_advanced(4, 6, 128, 1024)
            .await
            .expect("save transfer advanced");

        let reloaded = SettingsStore::load(path, defaults(&directory));
        let appearance = reloaded.appearance().await;
        assert_eq!(appearance.language, InterfaceLanguage::ZhCn);
        assert!(appearance.reduce_motion);
        assert_eq!(appearance.density, UiDensity::Compact);
        let general = reloaded.general().await;
        assert!(general.launch_at_login);
        assert!(!general.close_to_tray);
        assert!(general.minimize_to_tray);
        assert!(!general.confirm_quit_with_transfers);
        let advanced = reloaded.transfer_advanced().await;
        assert_eq!(advanced.upload_concurrency, 4);
        assert_eq!(advanced.download_concurrency, 6);
        assert_eq!(advanced.default_split_part_mib, 128);
        assert_eq!(advanced.auto_split_threshold_mib, 1024);

        std::fs::remove_dir_all(directory).expect("remove settings directory");
    }

    #[tokio::test]
    async fn rejects_out_of_range_transfer_advanced_settings() {
        let directory =
            std::env::temp_dir().join(format!("koofr-settings-{}", uuid::Uuid::new_v4()));
        let store = SettingsStore::load(directory.join("settings.json"), defaults(&directory));

        assert!(store.update_transfer_advanced(0, 1, 64, 512).await.is_err());
        assert!(
            store
                .update_transfer_advanced(1, 100, 64, 512)
                .await
                .is_err()
        );
        assert!(store.update_transfer_advanced(1, 1, 0, 512).await.is_err());
        assert!(store.update_transfer_advanced(1, 1, 64, 8).await.is_err());
    }

    #[tokio::test]
    async fn persists_all_new_categories() {
        let directory =
            std::env::temp_dir().join(format!("koofr-settings-{}", uuid::Uuid::new_v4()));
        let path = directory.join("settings.json");
        let store = SettingsStore::load(path.clone(), defaults(&directory));

        store
            .update_download_behavior(DownloadConflictStrategy::Rename, true, false, 7)
            .await
            .expect("save download behavior");
        store
            .update_bandwidth(2048, 512, BandwidthMode::Limited)
            .await
            .expect("save bandwidth");
        store
            .update_notifications(false, true, false, true, true, false, false, false)
            .await
            .expect("save notifications");
        store
            .update_link_defaults(LinkDefaultKind::Upload, 30, true)
            .await
            .expect("save link defaults");
        store.update_security(15).await.expect("save security");
        store
            .update_history_policy(60, 200)
            .await
            .expect("save history policy");

        let reloaded = SettingsStore::load(path, defaults(&directory));
        let behavior = reloaded.download_behavior().await;
        assert_eq!(behavior.conflict_strategy, DownloadConflictStrategy::Rename);
        assert!(behavior.open_folder_after_download);
        assert!(!behavior.verify_hash_after_download);
        assert_eq!(behavior.keep_failed_part_days, 7);
        let bandwidth = reloaded.bandwidth().await;
        assert_eq!(bandwidth.global_kbps, 2048);
        assert_eq!(bandwidth.per_task_kbps, 512);
        assert_eq!(bandwidth.mode, BandwidthMode::Limited);
        let notifications = reloaded.notifications().await;
        assert!(!notifications.notify_on_complete);
        assert!(notifications.notify_sound);
        assert!(!notifications.tray_badge);
        assert!(notifications.floating_window_enabled);
        assert!(notifications.floating_window_prompted);
        assert!(!notifications.floating_window_compact_enabled);
        assert!(!notifications.floating_window_click_toggle_enabled);
        assert!(!notifications.floating_window_double_click_open_enabled);
        reloaded
            .disable_floating_window()
            .await
            .expect("disable floating window");
        let notifications = reloaded.notifications().await;
        assert!(!notifications.floating_window_enabled);
        assert!(notifications.floating_window_prompted);
        let links = reloaded.link_defaults().await;
        assert_eq!(links.kind, LinkDefaultKind::Upload);
        assert_eq!(links.expire_days, 30);
        assert!(links.require_password);
        let security = reloaded.security().await;
        assert_eq!(security.idle_auto_logout_minutes, 15);
        let history = reloaded.history_policy().await;
        assert_eq!(history.keep_days, 60);
        assert_eq!(history.keep_count, 200);

        std::fs::remove_dir_all(directory).expect("remove settings directory");
    }

    #[tokio::test]
    async fn migrates_legacy_overwrite_and_drops_removed_diagnostic_setting() {
        let directory =
            std::env::temp_dir().join(format!("koofr-settings-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).expect("create settings directory");
        let path = directory.join("settings.json");
        std::fs::write(
            &path,
            br#"{
                "version": 1,
                "cacheMode": "memory",
                "cacheTtlMinutes": 15,
                "rememberedEmail": null,
                "downloadConflictStrategy": "overwrite",
                "clipboardStripPaths": true
            }"#,
        )
        .expect("write legacy settings");
        let store = SettingsStore::load(path.clone(), defaults(&directory));

        assert_eq!(
            store.download_behavior().await.conflict_strategy,
            DownloadConflictStrategy::Rename
        );
        store
            .update_security(15)
            .await
            .expect("save migrated settings");
        let payload = std::fs::read_to_string(path).expect("read migrated settings");
        assert!(!payload.contains("overwrite"));
        assert!(!payload.contains("clipboardStripPaths"));
        std::fs::remove_dir_all(directory).expect("remove settings directory");
    }
}
