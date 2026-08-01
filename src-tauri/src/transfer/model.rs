use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tauri::{AppHandle, Emitter, Manager};
use tokio_util::sync::CancellationToken;

use crate::{AppState, error::AppError, koofr_api::FileInfo, settings::NetworkRetrySettings};

pub const TRANSFER_EVENT: &str = "koofr://transfer-progress";

static LAST_EMIT: LazyLock<Mutex<HashMap<String, Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
const MIN_PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(80);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferDirection {
    Upload,
    Download,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferState {
    Queued,
    Running,
    Retrying,
    Paused,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitProgressPhase {
    Parts,
    Finalizing,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitProgress {
    pub completed_parts: u32,
    pub current_part: Option<u32>,
    pub total_parts: u32,
    pub phase: SplitProgressPhase,
}

impl SplitProgress {
    pub fn new(
        total_bytes: u64,
        part_bytes: u64,
        completed_parts: usize,
        phase: SplitProgressPhase,
    ) -> Result<Self, AppError> {
        if total_bytes == 0 || part_bytes == 0 {
            return Err(AppError::InvalidInput("split progress"));
        }
        let total_parts = u32::try_from(total_bytes.div_ceil(part_bytes))
            .map_err(|_| AppError::InvalidInput("split part count"))?;
        let completed_parts = u32::try_from(completed_parts)
            .map_err(|_| AppError::InvalidInput("split part count"))?
            .min(total_parts);
        let current_part = match phase {
            SplitProgressPhase::Parts if completed_parts < total_parts => Some(completed_parts + 1),
            _ => Some(total_parts),
        };
        Ok(Self {
            completed_parts,
            current_part,
            total_parts,
            phase,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IntegrityStatus {
    #[default]
    NotRequested,
    Unavailable,
    Verified,
}

#[derive(Clone, Copy, Debug)]
pub struct NetworkRetryPolicy {
    enabled: bool,
    max_retries: Option<u32>,
    interval_seconds: u32,
}

impl NetworkRetryPolicy {
    pub const fn new(enabled: bool, max_retries: Option<u32>, interval_seconds: u32) -> Self {
        Self {
            enabled,
            max_retries,
            interval_seconds,
        }
    }

    pub const fn enabled(self) -> bool {
        self.enabled
    }

    pub const fn interval_seconds(self) -> u32 {
        self.interval_seconds
    }
}

impl Default for NetworkRetryPolicy {
    fn default() -> Self {
        Self::new(false, Some(8), 5)
    }
}

impl From<NetworkRetrySettings> for NetworkRetryPolicy {
    fn from(settings: NetworkRetrySettings) -> Self {
        Self::new(
            settings.enabled,
            settings.max_retries,
            settings.interval_seconds,
        )
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferProgress {
    pub transfer_id: String,
    pub direction: TransferDirection,
    pub state: TransferState,
    pub bytes_transferred: u64,
    pub total_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_progress: Option<SplitProgress>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferResult {
    pub transfer_id: String,
    pub bytes_transferred: u64,
    pub file: Option<FileInfo>,
    pub integrity_status: IntegrityStatus,
}

pub fn emit_progress(
    app: &AppHandle,
    transfer_id: &str,
    direction: TransferDirection,
    state: TransferState,
    bytes_transferred: u64,
    total_bytes: Option<u64>,
) {
    emit_progress_with_split(
        app,
        transfer_id,
        direction,
        state,
        bytes_transferred,
        total_bytes,
        None,
    );
}

pub fn emit_progress_with_split(
    app: &AppHandle,
    transfer_id: &str,
    direction: TransferDirection,
    state: TransferState,
    bytes_transferred: u64,
    total_bytes: Option<u64>,
    split_progress: Option<SplitProgress>,
) {
    app.state::<AppState>().download_history.record_progress(
        transfer_id,
        state,
        bytes_transferred,
        total_bytes,
    );

    let is_running = state == TransferState::Running;
    let is_boundary =
        bytes_transferred == 0 || total_bytes.is_some_and(|total| bytes_transferred >= total);

    if is_running && !is_boundary {
        let now = Instant::now();
        if let Ok(mut map) = LAST_EMIT.lock() {
            let should_suppress = map.get(transfer_id).is_some_and(|last_time| {
                now.duration_since(*last_time) < MIN_PROGRESS_EMIT_INTERVAL
            });
            if should_suppress {
                return;
            }
            map.insert(transfer_id.to_owned(), now);
        }
    } else if !is_running {
        let _ = LAST_EMIT.lock().map(|mut map| map.remove(transfer_id));
    }

    let _ = app.emit(
        TRANSFER_EVENT,
        TransferProgress {
            transfer_id: transfer_id.to_owned(),
            direction,
            state,
            bytes_transferred,
            total_bytes,
            split_progress,
        },
    );
}

pub fn emit_terminal(
    app: &AppHandle,
    transfer_id: &str,
    direction: TransferDirection,
    bytes_transferred: u64,
    result: &Result<TransferResult, AppError>,
) {
    let (state, bytes) = match result {
        Ok(result) => (TransferState::Completed, result.bytes_transferred),
        Err(AppError::TransferPaused) => (TransferState::Paused, bytes_transferred),
        Err(AppError::Cancelled) => (TransferState::Cancelled, bytes_transferred),
        Err(_) => (TransferState::Failed, bytes_transferred),
    };
    let mut fields = Map::new();
    fields.insert(
        "direction".to_owned(),
        Value::String(
            match direction {
                TransferDirection::Upload => "upload",
                TransferDirection::Download => "download",
            }
            .to_owned(),
        ),
    );
    fields.insert("bytesTransferred".to_owned(), Value::from(bytes));
    let logger = &app.state::<AppState>().logger;
    if let Ok(completed) = result {
        app.state::<AppState>()
            .download_history
            .record_integrity(transfer_id, completed.integrity_status);
    }
    match result {
        Ok(_) => logger.info("transfer", "transfer_completed", Some(transfer_id), fields),
        Err(AppError::TransferPaused) => {
            logger.info("transfer", "transfer_paused", Some(transfer_id), fields)
        }
        Err(AppError::Cancelled) => {
            logger.info("transfer", "transfer_cancelled", Some(transfer_id), fields)
        }
        Err(error) => logger.error(
            "transfer",
            "transfer_failed",
            Some(transfer_id),
            error,
            fields,
        ),
    }
    emit_progress(app, transfer_id, direction, state, bytes, None);
}

pub fn normalize_interruption<T>(
    result: Result<T, AppError>,
    pause_requested: bool,
) -> Result<T, AppError> {
    match result {
        Err(AppError::Cancelled) if pause_requested => Err(AppError::TransferPaused),
        other => other,
    }
}

pub fn should_retry_network<T>(
    result: &Result<T, AppError>,
    policy: NetworkRetryPolicy,
    retries_completed: u32,
) -> bool {
    let within_limit = match policy.max_retries {
        Some(limit) => retries_completed < limit,
        None => true,
    };
    policy.enabled() && within_limit && matches!(result, Err(AppError::Network(_)))
}

pub struct NetworkRetryRequest<'a> {
    pub app: &'a AppHandle,
    pub cancel: &'a CancellationToken,
    pub transfer_id: &'a str,
    pub direction: TransferDirection,
    pub retry_attempt: u32,
    pub bytes_transferred: u64,
    pub total_bytes: Option<u64>,
    pub split_progress: Option<SplitProgress>,
    pub policy: NetworkRetryPolicy,
}

pub async fn wait_for_network_retry(request: NetworkRetryRequest<'_>) -> Result<(), AppError> {
    let NetworkRetryRequest {
        app,
        cancel,
        transfer_id,
        direction,
        retry_attempt,
        bytes_transferred,
        total_bytes,
        split_progress,
        policy,
    } = request;
    let delay_seconds = policy.interval_seconds();
    let mut fields = Map::new();
    fields.insert("retryAttempt".to_owned(), Value::from(retry_attempt));
    fields.insert("delaySeconds".to_owned(), Value::from(delay_seconds));
    fields.insert(
        "direction".to_owned(),
        Value::String(
            match direction {
                TransferDirection::Upload => "upload",
                TransferDirection::Download => "download",
            }
            .to_owned(),
        ),
    );
    app.state::<AppState>().logger.warn(
        "transfer",
        "network_retry_scheduled",
        Some(transfer_id),
        fields,
    );
    emit_progress_with_split(
        app,
        transfer_id,
        direction,
        TransferState::Retrying,
        bytes_transferred,
        total_bytes,
        split_progress,
    );
    tokio::select! {
        () = tokio::time::sleep(std::time::Duration::from_secs(u64::from(delay_seconds))) => Ok(()),
        () = cancel.cancelled() => Err(AppError::Cancelled),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        NetworkRetryPolicy, SplitProgress, SplitProgressPhase, normalize_interruption,
        should_retry_network,
    };
    use crate::error::AppError;

    #[test]
    fn preserves_real_failures_instead_of_reporting_an_automatic_pause() {
        let result = normalize_interruption::<()>(
            Err(AppError::Io(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "sensitive path omitted",
            ))),
            false,
        );

        assert!(matches!(result, Err(AppError::Io(_))));
    }

    #[test]
    fn only_an_explicit_pause_converts_cancellation_to_paused() {
        assert!(matches!(
            normalize_interruption::<()>(Err(AppError::Cancelled), true),
            Err(AppError::TransferPaused)
        ));
        assert!(matches!(
            normalize_interruption::<()>(Err(AppError::Cancelled), false),
            Err(AppError::Cancelled)
        ));
    }

    #[test]
    fn network_retry_policy_supports_finite_and_unlimited_retries() {
        assert!(!NetworkRetryPolicy::default().enabled());
        assert!(NetworkRetryPolicy::new(true, None, 30).enabled());
    }

    #[tokio::test]
    async fn retries_only_network_errors_and_stops_at_the_limit() {
        let network_error = reqwest::Client::new()
            .get("http://127.0.0.1:0")
            .send()
            .await
            .expect_err("closed local endpoint should fail");
        let result = Err::<(), _>(AppError::Network(network_error));
        let enabled = NetworkRetryPolicy::new(true, Some(8), 5);

        assert!(should_retry_network(&result, enabled, 0));
        assert!(!should_retry_network(&result, enabled, 8));
        assert!(!should_retry_network(
            &result,
            NetworkRetryPolicy::default(),
            0
        ));
        assert!(!should_retry_network(
            &Err::<(), _>(AppError::Forbidden),
            enabled,
            0
        ));
        assert!(should_retry_network(
            &result,
            NetworkRetryPolicy::new(true, None, 5),
            u32::MAX
        ));
    }

    #[test]
    fn split_progress_reports_one_based_parts_and_finalization() {
        let first =
            SplitProgress::new(10, 4, 0, SplitProgressPhase::Parts).expect("first split progress");
        assert_eq!(first.current_part, Some(1));
        assert_eq!(first.completed_parts, 0);
        assert_eq!(first.total_parts, 3);

        let last =
            SplitProgress::new(10, 4, 2, SplitProgressPhase::Parts).expect("last split progress");
        assert_eq!(last.current_part, Some(3));

        let finalizing = SplitProgress::new(10, 4, 3, SplitProgressPhase::Finalizing)
            .expect("finalizing split progress");
        assert_eq!(finalizing.completed_parts, 3);
        assert_eq!(finalizing.current_part, Some(3));
    }

    #[test]
    fn split_progress_handles_exact_part_boundaries() {
        let progress =
            SplitProgress::new(8, 4, 1, SplitProgressPhase::Parts).expect("split progress");
        assert_eq!(progress.total_parts, 2);
        assert_eq!(progress.current_part, Some(2));
    }
}
