use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

use zeroize::Zeroizing;

use crate::{
    error::AppError,
    file_ops::{MountId, RemotePath},
};

const MAX_SESSIONS: usize = 32;
const SESSION_IDLE_TTL: Duration = Duration::from_secs(30 * 60);

pub static STREAM_SESSIONS: OnceLock<StreamSessionManager> = OnceLock::new();

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PreviewKind {
    Video,
    Audio,
    Image,
    Pdf,
    Text,
}

impl PreviewKind {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Video => "video",
            Self::Audio => "audio",
            Self::Image => "image",
            Self::Pdf => "pdf",
            Self::Text => "text",
        }
    }
}

#[derive(Clone)]
pub enum PreviewSource {
    Remote {
        mount_id: MountId,
        remote_path: RemotePath,
    },
    Memory {
        bytes: Arc<Zeroizing<Vec<u8>>>,
    },
}

#[derive(Clone)]
pub struct StreamSession {
    pub source: PreviewSource,
    pub file_size: u64,
    pub content_type: String,
    pub kind: PreviewKind,
    pub decoder: String,
    pub vault_repo_id: Option<String>,
}

struct SessionRecord {
    session: StreamSession,
    last_accessed: Instant,
}

#[derive(Default)]
pub struct StreamSessionManager {
    sessions: Mutex<HashMap<String, SessionRecord>>,
}

impl StreamSessionManager {
    pub fn create_session(&self, session: StreamSession) -> Result<String, AppError> {
        if session.file_size == 0 && session.kind != PreviewKind::Text {
            return Err(AppError::InvalidInput("preview target is empty"));
        }
        let session_id = uuid::Uuid::new_v4().to_string();
        let mut sessions = self.sessions.lock().expect("stream session store poisoned");
        sessions.retain(|_, record| record.last_accessed.elapsed() < SESSION_IDLE_TTL);
        if sessions.len() >= MAX_SESSIONS
            && let Some(oldest) = sessions
                .iter()
                .min_by_key(|(_, record)| record.last_accessed)
                .map(|(id, _)| id.clone())
        {
            sessions.remove(&oldest);
        }
        sessions.insert(
            session_id.clone(),
            SessionRecord {
                session,
                last_accessed: Instant::now(),
            },
        );
        Ok(session_id)
    }

    pub fn get_session(&self, session_id: &str) -> Option<StreamSession> {
        let mut sessions = self.sessions.lock().expect("stream session store poisoned");
        let expired = sessions
            .get(session_id)
            .is_some_and(|record| record.last_accessed.elapsed() >= SESSION_IDLE_TTL);
        if expired {
            sessions.remove(session_id);
            return None;
        }
        let record = sessions.get_mut(session_id)?;
        record.last_accessed = Instant::now();
        Some(record.session.clone())
    }

    pub fn close_session(&self, session_id: &str) {
        self.sessions
            .lock()
            .expect("stream session store poisoned")
            .remove(session_id);
    }

    pub fn close_vault(&self, repo_id: &str) {
        self.sessions
            .lock()
            .expect("stream session store poisoned")
            .retain(|_, record| record.session.vault_repo_id.as_deref() != Some(repo_id));
    }

    pub fn close_all(&self) {
        self.sessions
            .lock()
            .expect("stream session store poisoned")
            .clear();
    }
}

pub fn parse_session_id_from_path(path: &str) -> Option<&str> {
    let trimmed = path.strip_prefix('/')?;
    if trimmed.is_empty() || trimmed.contains('/') {
        return None;
    }
    uuid::Uuid::parse_str(trimmed).ok()?;
    Some(trimmed)
}

pub fn parse_range_header(header: Option<&str>, file_size: u64) -> Option<(u64, u64)> {
    let header = header?;
    let value = header.strip_prefix("bytes=")?;
    let (start_str, end_str) = value.split_once('-')?;
    let start: u64 = start_str.parse().ok()?;
    if start >= file_size {
        return None;
    }
    let end = if end_str.is_empty() {
        file_size - 1
    } else {
        let parsed: u64 = end_str.parse().ok()?;
        parsed.min(file_size - 1)
    };
    if end < start {
        return None;
    }
    Some((start, end))
}

pub const STREAM_CHUNK_SIZE: u64 = 2 * 1024 * 1024;

pub fn cap_range(start: u64, end: u64, file_size: u64) -> (u64, u64) {
    let capped_end = end.min(start + STREAM_CHUNK_SIZE - 1).min(file_size - 1);
    (start, capped_end)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_session(repo_id: Option<&str>) -> StreamSession {
        StreamSession {
            source: PreviewSource::Remote {
                mount_id: MountId::parse("mount-1".to_owned()).unwrap(),
                remote_path: RemotePath::parse("/video.mp4".to_owned()).unwrap(),
            },
            file_size: 1024,
            content_type: "video/mp4".to_owned(),
            kind: PreviewKind::Video,
            decoder: "native".to_owned(),
            vault_repo_id: repo_id.map(str::to_owned),
        }
    }

    #[test]
    fn create_get_and_close_session() {
        let manager = StreamSessionManager::default();
        let id = manager.create_session(make_session(None)).unwrap();
        assert_eq!(manager.get_session(&id).unwrap().file_size, 1024);
        manager.close_session(&id);
        assert!(manager.get_session(&id).is_none());
    }

    #[test]
    fn closing_a_vault_preserves_other_sessions() {
        let manager = StreamSessionManager::default();
        let vault = manager
            .create_session(make_session(Some("repo-1")))
            .unwrap();
        let plain = manager.create_session(make_session(None)).unwrap();
        manager.close_vault("repo-1");
        assert!(manager.get_session(&vault).is_none());
        assert!(manager.get_session(&plain).is_some());
    }

    #[test]
    fn parse_and_cap_ranges() {
        assert_eq!(
            parse_range_header(Some("bytes=100-200"), 10_000),
            Some((100, 200))
        );
        assert_eq!(
            parse_range_header(Some("bytes=9990-20000"), 10_000),
            Some((9990, 9999))
        );
        assert!(parse_range_header(Some("bytes=10000-"), 10_000).is_none());
        let (start, end) = cap_range(0, 9_999_999, 10_000_000);
        assert_eq!(start, 0);
        assert_eq!(end, STREAM_CHUNK_SIZE - 1);
    }

    #[test]
    fn session_path_rejects_extra_segments() {
        let id = uuid::Uuid::new_v4().to_string();
        assert_eq!(
            parse_session_id_from_path(&format!("/{id}")),
            Some(id.as_str())
        );
        assert!(parse_session_id_from_path(&format!("/{id}/extra")).is_none());
    }

    #[test]
    fn creating_more_than_the_limit_evicts_the_oldest_session() {
        let manager = StreamSessionManager::default();
        let oldest = manager.create_session(make_session(None)).unwrap();
        std::thread::sleep(Duration::from_millis(1));
        for _ in 0..MAX_SESSIONS {
            manager.create_session(make_session(None)).unwrap();
        }
        assert!(manager.get_session(&oldest).is_none());
    }

    #[test]
    fn preview_sessions_use_a_thirty_minute_idle_timeout() {
        assert_eq!(SESSION_IDLE_TTL, Duration::from_secs(30 * 60));
    }
}
