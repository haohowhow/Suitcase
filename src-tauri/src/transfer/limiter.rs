use std::{
    collections::HashMap,
    sync::{Mutex, MutexGuard},
    time::{Duration, Instant},
};

use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use crate::{error::AppError, settings::BandwidthMode};

const IDLE_FULL_AFTER: Duration = Duration::from_secs(5 * 60);

#[derive(Clone, Copy)]
struct LimitConfig {
    mode: BandwidthMode,
    global_bytes_per_second: u64,
    per_task_bytes_per_second: u64,
}

struct Bucket {
    tokens: f64,
    last: Instant,
}

struct LimiterState {
    config: LimitConfig,
    global: Bucket,
    tasks: HashMap<String, Bucket>,
}

pub struct BandwidthLimiter {
    state: Mutex<LimiterState>,
    changed: Notify,
}

impl Default for BandwidthLimiter {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            state: Mutex::new(LimiterState {
                config: LimitConfig {
                    mode: BandwidthMode::Full,
                    global_bytes_per_second: 0,
                    per_task_bytes_per_second: 0,
                },
                global: Bucket {
                    tokens: 0.0,
                    last: now,
                },
                tasks: HashMap::new(),
            }),
            changed: Notify::new(),
        }
    }
}

impl BandwidthLimiter {
    pub fn configure(&self, mode: BandwidthMode, global_kib: u32, per_task_kib: u32) {
        let now = Instant::now();
        let mut state = lock(&self.state);
        state.config = LimitConfig {
            mode,
            global_bytes_per_second: u64::from(global_kib) * 1024,
            per_task_bytes_per_second: u64::from(per_task_kib) * 1024,
        };
        state.global = Bucket {
            tokens: state.config.global_bytes_per_second as f64,
            last: now,
        };
        state.tasks.clear();
        drop(state);
        self.changed.notify_waiters();
    }

    pub async fn acquire(
        &self,
        transfer_id: &str,
        byte_count: usize,
        cancel: &CancellationToken,
    ) -> Result<(), AppError> {
        let mut remaining = byte_count as u64;
        while remaining > 0 {
            let config = lock(&self.state).config;
            if config.mode == BandwidthMode::Full
                || (config.mode == BandwidthMode::IdleFull && system_idle_for(IDLE_FULL_AFTER))
            {
                return Ok(());
            }
            let slice_limit = [
                config.global_bytes_per_second,
                config.per_task_bytes_per_second,
            ]
            .into_iter()
            .filter(|rate| *rate > 0)
            .min()
            .unwrap_or(remaining);
            let slice = remaining.min(slice_limit.max(1));
            self.acquire_slice(transfer_id, slice, cancel).await?;
            remaining -= slice;
        }
        Ok(())
    }

    pub fn finish(&self, transfer_id: &str) {
        lock(&self.state).tasks.remove(transfer_id);
    }

    async fn acquire_slice(
        &self,
        transfer_id: &str,
        byte_count: u64,
        cancel: &CancellationToken,
    ) -> Result<(), AppError> {
        loop {
            let notified = self.changed.notified();
            let (wait, granted) = {
                let mut state = lock(&self.state);
                if state.config.mode == BandwidthMode::Full {
                    return Ok(());
                }
                let now = Instant::now();
                let global_rate = state.config.global_bytes_per_second;
                let task_rate = state.config.per_task_bytes_per_second;
                replenish(&mut state.global, global_rate, now);
                let mut task = state.tasks.remove(transfer_id).unwrap_or(Bucket {
                    tokens: task_rate as f64,
                    last: now,
                });
                replenish(&mut task, task_rate, now);
                let global_wait = required_wait(&state.global, global_rate, byte_count);
                let task_wait = required_wait(&task, task_rate, byte_count);
                let wait = global_wait.max(task_wait);
                if wait.is_zero() {
                    consume(&mut state.global, global_rate, byte_count);
                    consume(&mut task, task_rate, byte_count);
                }
                state.tasks.insert(transfer_id.to_owned(), task);
                (wait, wait.is_zero())
            };
            if granted {
                return Ok(());
            }
            tokio::select! {
                () = tokio::time::sleep(wait) => {}
                () = notified => {}
                () = cancel.cancelled() => return Err(AppError::Cancelled),
            }
        }
    }
}

fn replenish(bucket: &mut Bucket, rate: u64, now: Instant) {
    if rate == 0 {
        bucket.last = now;
        return;
    }
    let elapsed = now.saturating_duration_since(bucket.last).as_secs_f64();
    bucket.tokens = (bucket.tokens + elapsed * rate as f64).min(rate as f64);
    bucket.last = now;
}

fn required_wait(bucket: &Bucket, rate: u64, byte_count: u64) -> Duration {
    if rate == 0 || bucket.tokens >= byte_count as f64 {
        return Duration::ZERO;
    }
    Duration::from_secs_f64((byte_count as f64 - bucket.tokens) / rate as f64)
}

fn consume(bucket: &mut Bucket, rate: u64, byte_count: u64) {
    if rate > 0 {
        bucket.tokens = (bucket.tokens - byte_count as f64).max(0.0);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(value) => value,
        Err(poisoned) => poisoned.into_inner(),
    }
}

#[cfg(windows)]
fn system_idle_for(duration: Duration) -> bool {
    use windows_sys::Win32::{
        System::SystemInformation::GetTickCount,
        UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO},
    };

    let mut input = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    // SAFETY: Windows writes only to the initialized LASTINPUTINFO structure.
    let succeeded = unsafe { GetLastInputInfo(&mut input) } != 0;
    if !succeeded {
        return false;
    }
    // GetLastInputInfo and GetTickCount share the same wrapping 32-bit tick epoch.
    let now = unsafe { GetTickCount() };
    Duration::from_millis(u64::from(now.wrapping_sub(input.dwTime))) >= duration
}

#[cfg(not(windows))]
fn system_idle_for(_duration: Duration) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::BandwidthLimiter;
    use crate::settings::BandwidthMode;

    #[tokio::test]
    async fn applies_global_and_per_task_limits_with_one_second_burst() {
        let limiter = BandwidthLimiter::default();
        limiter.configure(BandwidthMode::Limited, 10, 10);
        let cancel = tokio_util::sync::CancellationToken::new();
        limiter
            .acquire("first", 10 * 1024, &cancel)
            .await
            .expect("initial burst");
        let pending = limiter.acquire("first", 1024, &cancel);
        tokio::pin!(pending);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(25), &mut pending)
                .await
                .is_err()
        );
        pending.await.expect("second bucket");
    }

    #[tokio::test]
    async fn zero_means_unlimited_at_each_layer() {
        let limiter = BandwidthLimiter::default();
        limiter.configure(BandwidthMode::Limited, 0, 0);
        limiter
            .acquire(
                "first",
                1024 * 1024,
                &tokio_util::sync::CancellationToken::new(),
            )
            .await
            .expect("unlimited");
    }
}
