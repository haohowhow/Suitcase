use std::sync::OnceLock;
use std::{collections::HashMap, sync::Mutex, time::Instant};

use crate::{error::AppError, file_ops::MountId, file_ops::RemotePath};

const MAX_SESSIONS: usize = 32;
const SESSION_TTL_SECS: u64 = 1800;

pub static STREAM_SESSIONS: OnceLock<StreamSessionManager> = OnceLock::new();

#[derive(Clone, Debug)]
pub struct StreamSession {
    pub mount_id: MountId,
    pub remote_path: RemotePath,
    pub file_size: u64,
    pub content_type: String,
    created_at: Instant,
}

impl StreamSession {
    fn new(
        mount_id: MountId,
        remote_path: RemotePath,
        file_size: u64,
        content_type: String,
    ) -> Self {
        Self {
            mount_id,
            remote_path,
            file_size,
            content_type,
            created_at: Instant::now(),
        }
    }

    fn is_expired(&self) -> bool {
        self.created_at.elapsed().as_secs() >= SESSION_TTL_SECS
    }
}

#[derive(Default)]
pub struct StreamSessionManager {
    sessions: Mutex<HashMap<String, StreamSession>>,
}

impl StreamSessionManager {
    pub fn create_session(
        &self,
        mount_id: MountId,
        remote_path: RemotePath,
        file_size: u64,
        content_type: String,
    ) -> Result<String, AppError> {
        let session_id = uuid::Uuid::new_v4().to_string();
        let session = StreamSession::new(mount_id, remote_path, file_size, content_type);
        let mut sessions = self.sessions.lock().expect("stream session store poisoned");
        if sessions.len() >= MAX_SESSIONS {
            sessions.retain(|_, s| !s.is_expired());
        }
        if sessions.len() >= MAX_SESSIONS {
            sessions.clear();
        }
        sessions.insert(session_id.clone(), session);
        Ok(session_id)
    }

    pub fn get_session(&self, session_id: &str) -> Option<StreamSession> {
        let sessions = self.sessions.lock().expect("stream session store poisoned");
        let session = sessions.get(session_id)?.clone();
        if session.is_expired() {
            drop(sessions);
            let mut sessions = self.sessions.lock().expect("stream session store poisoned");
            sessions.remove(session_id);
            return None;
        }
        Some(session)
    }

    pub fn close_session(&self, session_id: &str) {
        self.sessions
            .lock()
            .expect("stream session store poisoned")
            .remove(session_id);
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
    let session_id = trimmed.split('/').next()?;
    uuid::Uuid::parse_str(session_id).ok()?;
    Some(session_id)
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

    fn make_session() -> (MountId, RemotePath) {
        (
            MountId::parse("mount-1".to_owned()).unwrap(),
            RemotePath::parse("/video.mp4".to_owned()).unwrap(),
        )
    }

    #[test]
    fn create_and_get_session() {
        let manager = StreamSessionManager::default();
        let (mount_id, remote_path) = make_session();
        let id = manager
            .create_session(
                mount_id.clone(),
                remote_path.clone(),
                1024,
                "video/mp4".to_owned(),
            )
            .unwrap();
        let session = manager.get_session(&id).expect("session exists");
        assert_eq!(session.mount_id, mount_id);
        assert_eq!(session.remote_path, remote_path);
        assert_eq!(session.file_size, 1024);
        assert_eq!(session.content_type, "video/mp4");
    }

    #[test]
    fn close_session_removes_it() {
        let manager = StreamSessionManager::default();
        let (mount_id, remote_path) = make_session();
        let id = manager
            .create_session(mount_id, remote_path, 1024, "video/mp4".to_owned())
            .unwrap();
        assert!(manager.get_session(&id).is_some());
        manager.close_session(&id);
        assert!(manager.get_session(&id).is_none());
    }

    #[test]
    fn close_all_clears_everything() {
        let manager = StreamSessionManager::default();
        let (mount_id, remote_path) = make_session();
        let id1 = manager
            .create_session(
                mount_id.clone(),
                remote_path.clone(),
                100,
                "video/mp4".to_owned(),
            )
            .unwrap();
        let id2 = manager
            .create_session(mount_id, remote_path, 200, "audio/mpeg".to_owned())
            .unwrap();
        manager.close_all();
        assert!(manager.get_session(&id1).is_none());
        assert!(manager.get_session(&id2).is_none());
    }

    #[test]
    fn get_nonexistent_session_returns_none() {
        let manager = StreamSessionManager::default();
        assert!(manager.get_session("does-not-exist").is_none());
    }

    #[test]
    fn parse_session_id_from_valid_path() {
        let id = uuid::Uuid::new_v4().to_string();
        let path = format!("/{id}");
        assert_eq!(parse_session_id_from_path(&path), Some(id.as_str()));
    }

    #[test]
    fn parse_session_id_ignores_trailing_segments() {
        let id = uuid::Uuid::new_v4().to_string();
        let path = format!("/{id}/extra");
        assert_eq!(parse_session_id_from_path(&path), Some(id.as_str()));
    }

    #[test]
    fn parse_session_id_rejects_non_uuid() {
        assert!(parse_session_id_from_path("/not-a-uuid").is_none());
        assert!(parse_session_id_from_path("no-leading-slash").is_none());
    }

    #[test]
    fn parse_range_open_ended() {
        let result = parse_range_header(Some("bytes=0-"), 10_000);
        assert_eq!(result, Some((0, 9999)));
    }

    #[test]
    fn parse_range_bounded() {
        let result = parse_range_header(Some("bytes=100-200"), 10_000);
        assert_eq!(result, Some((100, 200)));
    }

    #[test]
    fn parse_range_clamps_end_to_file_size() {
        let result = parse_range_header(Some("bytes=9990-20000"), 10_000);
        assert_eq!(result, Some((9990, 9999)));
    }

    #[test]
    fn parse_range_rejects_start_beyond_file() {
        assert!(parse_range_header(Some("bytes=10000-"), 10_000).is_none());
        assert!(parse_range_header(Some("bytes=50000-60000"), 10_000).is_none());
    }

    #[test]
    fn parse_range_rejects_malformed() {
        assert!(parse_range_header(None, 1000).is_none());
        assert!(parse_range_header(Some("items=0-100"), 1000).is_none());
        assert!(parse_range_header(Some("bytes=abc-100"), 1000).is_none());
        assert!(parse_range_header(Some("bytes=100-50"), 1000).is_none());
    }

    #[test]
    fn cap_range_limits_chunk_size() {
        let (start, end) = cap_range(0, 9_999_999, 10_000_000);
        assert_eq!(start, 0);
        assert_eq!(end, STREAM_CHUNK_SIZE - 1);
    }

    #[test]
    fn cap_range_preserves_small_ranges() {
        let (start, end) = cap_range(100, 200, 10_000);
        assert_eq!(start, 100);
        assert_eq!(end, 200);
    }

    #[test]
    fn cap_range_clamps_to_file_end() {
        let (start, end) = cap_range(9_999_000, 9_999_999, 10_000_000);
        assert_eq!(start, 9_999_000);
        assert_eq!(end, 9_999_999);
    }
}
