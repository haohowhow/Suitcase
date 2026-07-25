use serde::Serialize;
use tauri::State;

use crate::{
    AppState,
    error::{AppError, CommandError},
    file_ops::{MountId, RemotePath},
    koofr_api::FileInfo,
    stream_session,
};

type CommandResult<T> = Result<T, CommandError>;

const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "m4v", "mov", "avi", "wmv", "flv", "mkv", "webm", "mpeg", "mpg", "mpe", "mts", "m2ts",
    "ts", "3gp", "3g2", "vob", "ogv", "rm", "rmvb", "asf", "f4v",
];

const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "wav", "flac", "aac", "m4a", "m4b", "ogg", "oga", "opus", "wma", "aiff", "aif", "ape",
    "alac", "amr", "mid", "midi", "dsd", "dsf", "dff",
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoStreamInfo {
    session_id: String,
    file_size: u64,
    content_type: String,
    media_type: String,
}

#[tauri::command]
pub async fn prepare_video_stream(
    state: State<'_, AppState>,
    mount_id: String,
    remote_path: String,
) -> CommandResult<VideoStreamInfo> {
    let mount_id = MountId::parse(mount_id).map_err(CommandError::from)?;
    let remote_path = RemotePath::parse(remote_path).map_err(CommandError::from)?;
    remote_path.require_non_root().map_err(CommandError::from)?;

    let info = state
        .api
        .file_info(&mount_id, &remote_path)
        .await
        .map_err(CommandError::from)?;

    if info.entry_type != "file" {
        return Err(CommandError::from(AppError::InvalidInput(
            "stream target is not a file",
        )));
    }

    if info.size <= 0 {
        return Err(CommandError::from(AppError::InvalidInput(
            "stream target is empty",
        )));
    }

    let file_size = info.size as u64;
    let (content_type, media_type) = resolve_media_type(&info);
    let sessions = stream_session::STREAM_SESSIONS.get().unwrap();
    let session_id = sessions
        .create_session(mount_id, remote_path, file_size, content_type.clone())
        .map_err(CommandError::from)?;

    Ok(VideoStreamInfo {
        session_id,
        file_size,
        content_type,
        media_type,
    })
}

#[tauri::command]
pub async fn close_video_stream(session_id: String) -> CommandResult<()> {
    if let Some(sessions) = stream_session::STREAM_SESSIONS.get() {
        sessions.close_session(&session_id);
    }
    Ok(())
}

fn resolve_media_type(info: &FileInfo) -> (String, String) {
    let ct = info.content_type.trim().to_ascii_lowercase();
    if ct.starts_with("video/") {
        return (ct, "video".to_owned());
    }
    if ct.starts_with("audio/") {
        return (ct, "audio".to_owned());
    }
    let ext = file_extension(&info.name);
    if VIDEO_EXTENSIONS.contains(&ext.as_str()) {
        let mime =
            guess_content_type(&ext).unwrap_or_else(|| "application/octet-stream".to_owned());
        return (mime, "video".to_owned());
    }
    if AUDIO_EXTENSIONS.contains(&ext.as_str()) {
        let mime =
            guess_content_type(&ext).unwrap_or_else(|| "application/octet-stream".to_owned());
        return (mime, "audio".to_owned());
    }
    (
        if ct.is_empty() {
            "application/octet-stream".to_owned()
        } else {
            ct
        },
        "video".to_owned(),
    )
}

fn file_extension(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((_, ext)) => ext.to_ascii_lowercase(),
        None => String::new(),
    }
}

fn guess_content_type(ext: &str) -> Option<String> {
    let mime = match ext {
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "ogv" => "video/ogg",
        "mov" => "video/quicktime",
        "mkv" => "video/x-matroska",
        "avi" => "video/x-msvideo",
        "wmv" => "video/x-ms-wmv",
        "flv" => "video/x-flv",
        "mpeg" | "mpg" | "mpe" => "video/mpeg",
        "ts" | "mts" | "m2ts" => "video/mp2t",
        "3gp" => "video/3gpp",
        "3g2" => "video/3gpp2",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "aac" => "audio/aac",
        "m4a" | "m4b" => "audio/mp4",
        "ogg" | "oga" => "audio/ogg",
        "opus" => "audio/ogg",
        "wma" => "audio/x-ms-wma",
        "aiff" | "aif" => "audio/aiff",
        "mid" | "midi" => "audio/midi",
        _ => return None,
    };
    Some(mime.to_owned())
}
