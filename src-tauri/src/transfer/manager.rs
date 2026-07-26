use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
};

use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use crate::error::AppError;

use super::{BandwidthLimiter, TransferDirection};
use crate::settings::BandwidthMode;

const MAX_CONCURRENCY: usize = 16;

pub struct TransferManager {
    active: Mutex<HashMap<String, TransferControl>>,
    upload_gate: Arc<DynamicGate>,
    download_gate: Arc<DynamicGate>,
    bandwidth: BandwidthLimiter,
}

impl Default for TransferManager {
    fn default() -> Self {
        Self {
            active: Mutex::new(HashMap::new()),
            upload_gate: Arc::new(DynamicGate::new(2)),
            download_gate: Arc::new(DynamicGate::new(3)),
            bandwidth: BandwidthLimiter::default(),
        }
    }
}

struct TransferControl {
    token: CancellationToken,
    pause_requested: Arc<AtomicBool>,
}

#[derive(Default)]
struct GateState {
    active: usize,
    limit: usize,
}

struct DynamicGate {
    state: Mutex<GateState>,
    changed: Notify,
}

impl DynamicGate {
    fn new(limit: usize) -> Self {
        Self {
            state: Mutex::new(GateState { active: 0, limit }),
            changed: Notify::new(),
        }
    }

    async fn acquire(
        self: &Arc<Self>,
        cancel: &CancellationToken,
    ) -> Result<DynamicPermit, AppError> {
        loop {
            let notified = self.changed.notified();
            {
                let mut state = lock(&self.state);
                if state.active < state.limit {
                    state.active += 1;
                    return Ok(DynamicPermit { gate: self.clone() });
                }
            }
            tokio::select! {
                () = notified => {}
                () = cancel.cancelled() => return Err(AppError::Cancelled),
            }
        }
    }

    fn set_limit(&self, limit: usize) {
        lock(&self.state).limit = limit.clamp(1, MAX_CONCURRENCY);
        self.changed.notify_waiters();
    }
}

pub struct DynamicPermit {
    gate: Arc<DynamicGate>,
}

impl Drop for DynamicPermit {
    fn drop(&mut self) {
        let mut state = lock(&self.gate.state);
        state.active = state.active.saturating_sub(1);
        drop(state);
        self.gate.changed.notify_waiters();
    }
}

impl TransferManager {
    pub fn register(&self, transfer_id: &str) -> Result<CancellationToken, AppError> {
        validate_transfer_id(transfer_id)?;
        let mut active = self.active();
        if active.contains_key(transfer_id) {
            return Err(AppError::DuplicateTransfer);
        }
        let token = CancellationToken::new();
        active.insert(
            transfer_id.to_owned(),
            TransferControl {
                token: token.clone(),
                pause_requested: Arc::new(AtomicBool::new(false)),
            },
        );
        Ok(token)
    }

    pub async fn acquire(
        &self,
        direction: TransferDirection,
        cancel: &CancellationToken,
    ) -> Result<DynamicPermit, AppError> {
        match direction {
            TransferDirection::Upload => self.upload_gate.acquire(cancel).await,
            TransferDirection::Download => self.download_gate.acquire(cancel).await,
        }
    }

    pub fn finish(&self, transfer_id: &str) {
        self.active().remove(transfer_id);
        self.bandwidth.finish(transfer_id);
    }

    #[allow(dead_code)]
    pub fn active_count(&self) -> usize {
        self.active().len()
    }

    pub fn cancel(&self, transfer_id: &str) -> Result<bool, AppError> {
        validate_transfer_id(transfer_id)?;
        let active = self.active();
        let control = active.get(transfer_id);
        if let Some(control) = control {
            control.pause_requested.store(false, Ordering::Release);
            control.token.cancel();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn pause(&self, transfer_id: &str) -> Result<bool, AppError> {
        validate_transfer_id(transfer_id)?;
        let active = self.active();
        let control = active.get(transfer_id);
        if let Some(control) = control {
            control.pause_requested.store(true, Ordering::Release);
            control.token.cancel();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn was_paused(&self, transfer_id: &str) -> bool {
        self.active()
            .get(transfer_id)
            .is_some_and(|control| control.pause_requested.load(Ordering::Acquire))
    }

    pub fn cancel_all(&self) {
        for control in self.active().values() {
            control.token.cancel();
        }
    }

    pub fn set_concurrency(&self, upload: usize, download: usize) {
        self.upload_gate.set_limit(upload);
        self.download_gate.set_limit(download);
    }

    pub fn set_bandwidth(&self, mode: BandwidthMode, global_kib: u32, per_task_kib: u32) {
        self.bandwidth.configure(mode, global_kib, per_task_kib);
    }

    pub async fn throttle(
        &self,
        transfer_id: &str,
        byte_count: usize,
        cancel: &CancellationToken,
    ) -> Result<(), AppError> {
        self.bandwidth
            .acquire(transfer_id, byte_count, cancel)
            .await
    }

    fn active(&self) -> MutexGuard<'_, HashMap<String, TransferControl>> {
        lock(&self.active)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(value) => value,
        Err(poisoned) => poisoned.into_inner(),
    }
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
mod tests {
    use super::TransferManager;
    use crate::transfer::TransferDirection;

    #[test]
    fn transfer_ids_are_unique_and_cancellable() {
        let manager = TransferManager::default();
        let id = uuid::Uuid::new_v4().to_string();
        let token = manager.register(&id).expect("register transfer");
        assert!(manager.register(&id).is_err());
        assert!(manager.cancel(&id).expect("cancel transfer"));
        assert!(token.is_cancelled());
        assert!(!manager.was_paused(&id));
        manager.finish(&id);
        assert!(!manager.cancel(&id).expect("cancel missing transfer"));
    }

    #[tokio::test]
    async fn lowering_limit_does_not_interrupt_running_work() {
        let manager = TransferManager::default();
        let first_cancel = tokio_util::sync::CancellationToken::new();
        let second_cancel = tokio_util::sync::CancellationToken::new();
        let first = manager
            .acquire(TransferDirection::Upload, &first_cancel)
            .await
            .expect("first permit");
        let second = manager
            .acquire(TransferDirection::Upload, &second_cancel)
            .await
            .expect("second permit");
        manager.set_concurrency(1, 3);
        drop(first);

        let third_cancel = tokio_util::sync::CancellationToken::new();
        let pending = manager.acquire(TransferDirection::Upload, &third_cancel);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(25), pending)
                .await
                .is_err()
        );
        drop(second);
        manager
            .acquire(TransferDirection::Upload, &third_cancel)
            .await
            .expect("permit after active work drains");
    }

    #[tokio::test]
    async fn queued_work_can_be_cancelled() {
        let manager = TransferManager::default();
        manager.set_concurrency(1, 1);
        let running_cancel = tokio_util::sync::CancellationToken::new();
        let _running = manager
            .acquire(TransferDirection::Download, &running_cancel)
            .await
            .expect("running permit");
        let queued_cancel = tokio_util::sync::CancellationToken::new();
        queued_cancel.cancel();
        assert!(
            manager
                .acquire(TransferDirection::Download, &queued_cancel)
                .await
                .is_err()
        );
    }
}
