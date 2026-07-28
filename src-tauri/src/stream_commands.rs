use std::sync::Arc;

use chardetng::EncodingDetector;
use encoding_rs::Encoding;
use futures_util::{AsyncReadExt, StreamExt, io::Cursor};
use serde::Serialize;
use tauri::State;
use zeroize::Zeroizing;

use crate::{
    AppState,
    error::{AppError, CommandError},
    file_ops::{MountId, RemotePath},
    koofr_api::FileInfo,
    preview_decode,
    stream_session::{self, PreviewKind, PreviewSource, StreamSession, StreamSessionManager},
};

type CommandResult<T> = Result<T, CommandError>;

const MATERIALIZED_PREVIEW_LIMIT: u64 = 100 * 1024 * 1024;
const VAULT_PREVIEW_LIMIT: u64 = 50 * 1024 * 1024;
const TEXT_PREVIEW_LIMIT: usize = 5 * 1024 * 1024;

const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "m4v", "mov", "avi", "wmv", "flv", "mkv", "webm", "mpeg", "mpg", "mpe", "mts", "m2ts",
    "3gp", "3g2", "vob", "ogv", "rm", "rmvb", "asf", "f4v",
];
const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "wav", "flac", "aac", "m4a", "m4b", "ogg", "oga", "opus", "wma", "aiff", "aif", "ape",
    "alac", "amr", "mid", "midi", "dsd", "dsf", "dff",
];
const NATIVE_IMAGE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "jfif", "gif", "webp", "bmp", "ico", "avif",
];
const RAW_EXTENSIONS: &[&str] = &[
    "raw", "cr2", "cr3", "nef", "nrw", "arw", "srf", "sr2", "raf", "orf", "rw2", "dng", "pef",
    "3fr", "erf", "mef", "mos", "mrw", "x3f",
];
const TEXT_EXTENSIONS: &[&str] = &[
    "txt",
    "md",
    "markdown",
    "rst",
    "rtf",
    "log",
    "csv",
    "tsv",
    "tex",
    "bib",
    "org",
    "adoc",
    "asciidoc",
    "js",
    "jsx",
    "mjs",
    "cjs",
    "ts",
    "tsx",
    "json",
    "jsonc",
    "json5",
    "html",
    "htm",
    "xhtml",
    "xml",
    "xsl",
    "xslt",
    "css",
    "scss",
    "sass",
    "less",
    "py",
    "pyw",
    "rb",
    "php",
    "go",
    "rs",
    "toml",
    "java",
    "kt",
    "kts",
    "scala",
    "groovy",
    "gradle",
    "c",
    "h",
    "cc",
    "cpp",
    "cxx",
    "hh",
    "hpp",
    "hxx",
    "m",
    "mm",
    "cs",
    "fs",
    "fsx",
    "vb",
    "swift",
    "dart",
    "sh",
    "bash",
    "zsh",
    "fish",
    "sql",
    "lua",
    "pl",
    "pm",
    "r",
    "jl",
    "ex",
    "exs",
    "erl",
    "elm",
    "clj",
    "cljs",
    "hs",
    "yaml",
    "yml",
    "ini",
    "cfg",
    "conf",
    "env",
    "properties",
    "vue",
    "svelte",
    "astro",
];
const DOWNLOAD_ONLY_EXTENSIONS: &[&str] = &[
    "doc", "docx", "docm", "dot", "dotx", "dotm", "xls", "xlsx", "xlsm", "xlsb", "xlt", "xltx",
    "xltm", "ppt", "pptx", "pptm", "pot", "potx", "potm", "pps", "ppsx", "odt", "ods", "odp",
    "pages", "numbers", "key", "wps", "psd", "psb", "ai", "eps", "epub", "mobi", "azw", "azw3",
    "kfx", "fb2", "lit", "lrf", "ibooks",
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePreviewInfo {
    session_id: String,
    file_size: u64,
    content_type: String,
    kind: String,
    decoder: String,
    limits: PreviewLimits,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewLimits {
    max_bytes: Option<u64>,
    text_bytes: Option<u64>,
    max_pixels: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewTextResult {
    text: String,
    encoding: String,
    truncated: bool,
    had_decode_errors: bool,
}

struct ResolvedPreview {
    kind: PreviewKind,
    content_type: String,
    decoder: String,
    materialize: bool,
}

#[tauri::command]
pub async fn prepare_file_preview(
    state: State<'_, AppState>,
    mount_id: String,
    remote_path: String,
) -> CommandResult<FilePreviewInfo> {
    let mount_id = MountId::parse(mount_id).map_err(CommandError::from)?;
    let remote_path = RemotePath::parse(remote_path).map_err(CommandError::from)?;
    remote_path.require_non_root().map_err(CommandError::from)?;
    let info = state
        .api
        .file_info(&mount_id, &remote_path)
        .await
        .map_err(CommandError::from)?;
    validate_file(&info).map_err(CommandError::from)?;
    let resolved = resolve_preview(&info.name, &info.content_type).map_err(CommandError::from)?;
    let original_size =
        u64::try_from(info.size).map_err(|_| CommandError::from(AppError::PreviewDecode))?;

    let (source, file_size, content_type, decoder) = if resolved.materialize {
        if original_size > MATERIALIZED_PREVIEW_LIMIT {
            return Err(CommandError::from(AppError::PreviewTooLarge));
        }
        let bytes = Zeroizing::new(
            download_limited(
                &state.api,
                &mount_id,
                &remote_path,
                MATERIALIZED_PREVIEW_LIMIT,
            )
            .await
            .map_err(CommandError::from)?,
        );
        let (bytes, content_type) = convert_materialized(bytes, &resolved.decoder)
            .await
            .map_err(CommandError::from)?;
        let size = bytes.len() as u64;
        (
            PreviewSource::Memory {
                bytes: Arc::new(Zeroizing::new(bytes)),
            },
            size,
            content_type,
            "native".to_owned(),
        )
    } else {
        if resolved.kind == PreviewKind::Image && original_size > MATERIALIZED_PREVIEW_LIMIT {
            return Err(CommandError::from(AppError::PreviewTooLarge));
        }
        (
            PreviewSource::Remote {
                mount_id,
                remote_path,
            },
            original_size,
            resolved.content_type.clone(),
            resolved.decoder.clone(),
        )
    };
    create_preview_info(
        state.inner(),
        StreamSession {
            source,
            file_size,
            content_type,
            kind: resolved.kind,
            decoder,
            vault_repo_id: None,
        },
    )
    .map_err(CommandError::from)
}

#[tauri::command]
pub async fn prepare_vault_preview(
    state: State<'_, AppState>,
    repo_id: String,
    entry_id: String,
) -> CommandResult<FilePreviewInfo> {
    let (repo, cipher, remote_path) = state
        .vault
        .operation_target(&repo_id, &entry_id, Some("file"))
        .await
        .map_err(CommandError::from)?;
    let mount_id = MountId::parse(repo.mount_id).map_err(CommandError::from)?;
    let info = state
        .api
        .file_info(&mount_id, &remote_path)
        .await
        .map_err(CommandError::from)?;
    validate_file(&info).map_err(CommandError::from)?;
    let plain_size = crate::crypto::VaultCipher::decrypted_size(info.size)
        .and_then(|size| u64::try_from(size).map_err(|_| AppError::PreviewDecode))
        .map_err(CommandError::from)?;
    let name = cipher
        .decrypt_name(&info.name)
        .map_err(CommandError::from)?;
    let resolved = resolve_preview(&name, &info.content_type).map_err(CommandError::from)?;
    if plain_size > VAULT_PREVIEW_LIMIT {
        return Err(CommandError::from(AppError::PreviewTooLarge));
    }
    if plain_size == 0 && resolved.kind != PreviewKind::Text {
        return Err(CommandError::from(AppError::InvalidInput(
            "preview target is empty",
        )));
    }
    let encrypted_limit =
        u64::try_from(info.size).map_err(|_| CommandError::from(AppError::PreviewDecode))?;
    let encrypted = Zeroizing::new(
        download_limited(&state.api, &mount_id, &remote_path, encrypted_limit)
            .await
            .map_err(CommandError::from)?,
    );
    let mut reader = cipher.decrypt_reader(Cursor::new(encrypted.as_slice()));
    let mut plain = Zeroizing::new(Vec::with_capacity(plain_size as usize));
    reader
        .read_to_end(&mut plain)
        .await
        .map_err(|_| CommandError::from(AppError::PreviewDecode))?;
    if plain.len() as u64 != plain_size {
        return Err(CommandError::from(AppError::PreviewDecode));
    }
    let (plain, content_type, decoder) = if resolved.materialize {
        let (converted, content_type) = convert_materialized(plain, &resolved.decoder)
            .await
            .map_err(CommandError::from)?;
        (Zeroizing::new(converted), content_type, "native".to_owned())
    } else {
        (
            plain,
            resolved.content_type.clone(),
            resolved.decoder.clone(),
        )
    };
    state
        .vault
        .touch_preview(&repo_id)
        .await
        .map_err(CommandError::from)?;
    let file_size = plain.len() as u64;
    create_preview_info(
        state.inner(),
        StreamSession {
            source: PreviewSource::Memory {
                bytes: Arc::new(plain),
            },
            file_size,
            content_type,
            kind: resolved.kind,
            decoder,
            vault_repo_id: Some(repo_id),
        },
    )
    .map_err(CommandError::from)
}

#[tauri::command]
pub async fn read_preview_text(
    state: State<'_, AppState>,
    session_id: String,
    encoding: Option<String>,
) -> CommandResult<PreviewTextResult> {
    let sessions = stream_session::STREAM_SESSIONS.get().unwrap();
    let session = sessions
        .get_session(&session_id)
        .ok_or_else(|| CommandError::from(AppError::NotFound))?;
    if session.kind != PreviewKind::Text {
        return Err(CommandError::from(AppError::InvalidInput(
            "preview text session",
        )));
    }
    let limit = TEXT_PREVIEW_LIMIT.min(session.file_size as usize);
    let bytes = match session.source {
        PreviewSource::Memory { bytes } => bytes[..limit].to_vec(),
        PreviewSource::Remote {
            mount_id,
            remote_path,
        } => {
            if limit == 0 {
                Vec::new()
            } else {
                download_prefix(&state.api, &mount_id, &remote_path, limit)
                    .await
                    .map_err(CommandError::from)?
            }
        }
    };
    let (text, selected, had_decode_errors) =
        decode_preview_text(&bytes, encoding.as_deref()).map_err(CommandError::from)?;
    Ok(PreviewTextResult {
        text,
        encoding: selected.name().to_owned(),
        truncated: session.file_size > TEXT_PREVIEW_LIMIT as u64,
        had_decode_errors,
    })
}

#[tauri::command]
pub async fn close_file_preview(
    state: State<'_, AppState>,
    session_id: String,
) -> CommandResult<()> {
    if let Some(sessions) = stream_session::STREAM_SESSIONS.get() {
        if let Some(session) = sessions.get_session(&session_id) {
            let mut fields = serde_json::Map::new();
            fields.insert("sessionId".to_owned(), session_id.clone().into());
            fields.insert("previewKind".to_owned(), session.kind.as_str().into());
            fields.insert("bytes".to_owned(), session.file_size.into());
            fields.insert("decoder".to_owned(), session.decoder.into());
            fields.insert("terminalCode".to_owned(), "closed".into());
            state.logger.info("preview", "preview_closed", None, fields);
        }
        sessions.close_session(&session_id);
    }
    Ok(())
}

fn create_preview_info(
    state: &AppState,
    session: StreamSession,
) -> Result<FilePreviewInfo, AppError> {
    let sessions: &StreamSessionManager = stream_session::STREAM_SESSIONS.get().unwrap();
    let is_vault = session.vault_repo_id.is_some();
    let session_id = sessions.create_session(session.clone())?;
    let mut fields = serde_json::Map::new();
    fields.insert("sessionId".to_owned(), session_id.clone().into());
    fields.insert("previewKind".to_owned(), session.kind.as_str().into());
    fields.insert("bytes".to_owned(), session.file_size.into());
    fields.insert("decoder".to_owned(), session.decoder.clone().into());
    state
        .logger
        .info("preview", "preview_prepared", None, fields);
    let info = FilePreviewInfo {
        session_id,
        file_size: session.file_size,
        content_type: session.content_type,
        kind: session.kind.as_str().to_owned(),
        decoder: session.decoder,
        limits: PreviewLimits {
            max_bytes: if is_vault {
                Some(VAULT_PREVIEW_LIMIT)
            } else if session.kind == PreviewKind::Image {
                Some(MATERIALIZED_PREVIEW_LIMIT)
            } else {
                None
            },
            text_bytes: (session.kind == PreviewKind::Text).then_some(TEXT_PREVIEW_LIMIT as u64),
            max_pixels: (session.kind == PreviewKind::Image).then_some(80_000_000),
        },
    };
    Ok(info)
}

fn validate_file(info: &FileInfo) -> Result<(), AppError> {
    if info.entry_type != "file" {
        return Err(AppError::InvalidInput("preview target is not a file"));
    }
    if info.size < 0 {
        return Err(AppError::PreviewDecode);
    }
    Ok(())
}

fn resolve_preview(name: &str, content_type: &str) -> Result<ResolvedPreview, AppError> {
    let extension = file_extension(name);
    let mime = content_type.trim().to_ascii_lowercase();
    if DOWNLOAD_ONLY_EXTENSIONS.contains(&extension.as_str()) {
        return Err(AppError::PreviewUnsupported);
    }
    let native = |kind, content_type: String, decoder: &str| ResolvedPreview {
        kind,
        content_type,
        decoder: decoder.to_owned(),
        materialize: false,
    };
    if mime.starts_with("video/") || VIDEO_EXTENSIONS.contains(&extension.as_str()) {
        return Ok(native(
            PreviewKind::Video,
            guess_content_type(&extension).unwrap_or_else(|| {
                if mime.is_empty() {
                    "application/octet-stream".to_owned()
                } else {
                    mime.clone()
                }
            }),
            "native",
        ));
    }
    if mime.starts_with("audio/") || AUDIO_EXTENSIONS.contains(&extension.as_str()) {
        return Ok(native(
            PreviewKind::Audio,
            guess_content_type(&extension).unwrap_or_else(|| {
                if mime.is_empty() {
                    "application/octet-stream".to_owned()
                } else {
                    mime.clone()
                }
            }),
            "native",
        ));
    }
    if mime == "application/pdf" || extension == "pdf" {
        return Ok(native(
            PreviewKind::Pdf,
            "application/pdf".to_owned(),
            "pdfjs",
        ));
    }
    if mime.starts_with("text/")
        || matches!(
            mime.as_str(),
            "application/json" | "application/xml" | "application/yaml"
        )
        || TEXT_EXTENSIONS.contains(&extension.as_str())
    {
        return Ok(native(
            PreviewKind::Text,
            "text/plain; charset=utf-8".to_owned(),
            "text",
        ));
    }
    if RAW_EXTENSIONS.contains(&extension.as_str()) {
        return Ok(ResolvedPreview {
            kind: PreviewKind::Image,
            content_type: "image/jpeg".to_owned(),
            decoder: "raw".to_owned(),
            materialize: true,
        });
    }
    if matches!(extension.as_str(), "tif" | "tiff") || mime == "image/tiff" {
        return Ok(ResolvedPreview {
            kind: PreviewKind::Image,
            content_type: "image/png".to_owned(),
            decoder: "tiff".to_owned(),
            materialize: true,
        });
    }
    if matches!(extension.as_str(), "heic" | "heif")
        || matches!(mime.as_str(), "image/heic" | "image/heif")
    {
        return Ok(native(PreviewKind::Image, "image/heic".to_owned(), "heic"));
    }
    if extension == "svg" || mime == "image/svg+xml" {
        return Ok(native(
            PreviewKind::Image,
            "image/svg+xml".to_owned(),
            "svg",
        ));
    }
    if mime.starts_with("image/") || NATIVE_IMAGE_EXTENSIONS.contains(&extension.as_str()) {
        return Ok(native(
            PreviewKind::Image,
            guess_content_type(&extension)
                .or_else(|| (!mime.is_empty()).then_some(mime))
                .unwrap_or_else(|| "application/octet-stream".to_owned()),
            "native",
        ));
    }
    Err(AppError::PreviewUnsupported)
}

async fn convert_materialized(
    bytes: Zeroizing<Vec<u8>>,
    decoder: &str,
) -> Result<(Vec<u8>, String), AppError> {
    let decoder = decoder.to_owned();
    tokio::task::spawn_blocking(move || match decoder.as_str() {
        "tiff" => preview_decode::decode_tiff(&bytes).map(|bytes| (bytes, "image/png".to_owned())),
        "raw" => preview_decode::decode_raw(&bytes),
        _ => Err(AppError::PreviewDecode),
    })
    .await
    .map_err(|_| AppError::PreviewDecode)?
}

async fn download_limited(
    api: &crate::koofr_api::KoofrApi,
    mount_id: &MountId,
    remote_path: &RemotePath,
    limit: u64,
) -> Result<Vec<u8>, AppError> {
    let response = api.download_response(mount_id, remote_path).await?;
    let mut stream = response.bytes_stream();
    let mut output = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if output.len().saturating_add(chunk.len()) as u64 > limit {
            return Err(AppError::PreviewTooLarge);
        }
        output.extend_from_slice(&chunk);
    }
    Ok(output)
}

async fn download_prefix(
    api: &crate::koofr_api::KoofrApi,
    mount_id: &MountId,
    remote_path: &RemotePath,
    limit: usize,
) -> Result<Vec<u8>, AppError> {
    let response = api.download_response_from(mount_id, remote_path, 0).await?;
    let mut stream = response.bytes_stream();
    let mut output = Vec::with_capacity(limit);
    while output.len() < limit {
        let Some(chunk) = stream.next().await else {
            break;
        };
        let chunk = chunk?;
        let take = (limit - output.len()).min(chunk.len());
        output.extend_from_slice(&chunk[..take]);
    }
    Ok(output)
}

fn resolve_encoding(bytes: &[u8], requested: Option<&str>) -> Result<&'static Encoding, AppError> {
    if let Some(requested) = requested.filter(|value| !value.eq_ignore_ascii_case("auto")) {
        return Encoding::for_label(requested.as_bytes())
            .ok_or(AppError::InvalidInput("preview text encoding"));
    }
    if let Some((encoding, _)) = Encoding::for_bom(bytes) {
        return Ok(encoding);
    }
    let mut detector = EncodingDetector::new();
    detector.feed(bytes, true);
    Ok(detector.guess(None, true))
}

fn decode_preview_text(
    bytes: &[u8],
    requested: Option<&str>,
) -> Result<(String, &'static Encoding, bool), AppError> {
    let encoding = resolve_encoding(bytes, requested)?;
    let bom_length = Encoding::for_bom(bytes)
        .filter(|(bom_encoding, _)| *bom_encoding == encoding)
        .map_or(0, |(_, length)| length);
    let (text, had_decode_errors) = encoding.decode_without_bom_handling(&bytes[bom_length..]);
    Ok((text.into_owned(), encoding, had_decode_errors))
}

fn file_extension(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default()
}

fn guess_content_type(extension: &str) -> Option<String> {
    let mime = match extension {
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "ogv" => "video/ogg",
        "mov" => "video/quicktime",
        "mkv" => "video/x-matroska",
        "avi" => "video/x-msvideo",
        "wmv" => "video/x-ms-wmv",
        "mpeg" | "mpg" | "mpe" => "video/mpeg",
        "ts" | "mts" | "m2ts" => "video/mp2t",
        "3gp" => "video/3gpp",
        "3g2" => "video/3gpp2",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "aac" => "audio/aac",
        "m4a" | "m4b" => "audio/mp4",
        "ogg" | "oga" | "opus" => "audio/ogg",
        "png" => "image/png",
        "jpg" | "jpeg" | "jfif" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "avif" => "image/avif",
        _ => return None,
    };
    Some(mime.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_supported_preview_families() {
        assert_eq!(
            resolve_preview("report.pdf", "").unwrap().kind,
            PreviewKind::Pdf
        );
        assert_eq!(
            resolve_preview("notes.txt", "").unwrap().kind,
            PreviewKind::Text
        );
        assert_eq!(
            resolve_preview("source.ts", "").unwrap().kind,
            PreviewKind::Text
        );
        assert_eq!(
            resolve_preview("capture.ts", "video/mp2t").unwrap().kind,
            PreviewKind::Video
        );
        assert_eq!(resolve_preview("photo.heic", "").unwrap().decoder, "heic");
        assert_eq!(resolve_preview("camera.cr3", "").unwrap().decoder, "raw");
        assert!(matches!(
            resolve_preview("document.docx", "application/octet-stream"),
            Err(AppError::PreviewUnsupported)
        ));
        assert!(matches!(
            resolve_preview("design.psd", "image/vnd.adobe.photoshop"),
            Err(AppError::PreviewUnsupported)
        ));
    }

    #[test]
    fn detects_utf8_and_honors_big5_override() {
        assert_eq!(
            resolve_encoding("你好".as_bytes(), None).unwrap().name(),
            "UTF-8"
        );
        assert_eq!(
            resolve_encoding(b"hello", Some("big5")).unwrap().name(),
            "Big5"
        );
        let (big5, _, _) = encoding_rs::BIG5.encode("中文");
        assert_eq!(decode_preview_text(&big5, Some("big5")).unwrap().0, "中文");
        let utf16 = [0xff, 0xfe, 0x60, 0x4f, 0x7d, 0x59];
        assert_eq!(decode_preview_text(&utf16, None).unwrap().0, "你好");
    }
}
