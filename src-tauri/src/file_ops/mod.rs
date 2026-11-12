use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::error::AppError;

const MAX_REMOTE_PATH_CHARS: usize = 1023;
const MAX_NAME_CHARS: usize = 255;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct MountId(String);

impl MountId {
    pub fn parse(value: String) -> Result<Self, AppError> {
        let valid = !value.is_empty()
            && value.len() <= 128
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
        if !valid {
            return Err(AppError::InvalidInput("mount id"));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct RemotePath(String);

impl RemotePath {
    pub fn parse(value: String) -> Result<Self, AppError> {
        let char_count = value.encode_utf16().count();
        if value.is_empty()
            || !value.starts_with('/')
            || char_count > MAX_REMOTE_PATH_CHARS
            || value.contains('\0')
            || (value.len() > 1 && value.ends_with('/'))
        {
            return Err(AppError::InvalidInput("remote path"));
        }

        if value != "/"
            && value[1..]
                .split('/')
                .any(|segment| segment.is_empty() || matches!(segment, "." | ".."))
        {
            return Err(AppError::InvalidInput("remote path segment"));
        }

        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn require_non_root(&self) -> Result<(), AppError> {
        if self.0 == "/" {
            Err(AppError::InvalidInput("root path cannot be changed"))
        } else {
            Ok(())
        }
    }

    pub fn file_name(&self) -> Result<&str, AppError> {
        self.require_non_root()?;
        self.0
            .rsplit('/')
            .next()
            .filter(|name| !name.is_empty())
            .ok_or(AppError::InvalidInput("remote file name"))
    }

    pub fn parent(&self) -> Result<Self, AppError> {
        self.require_non_root()?;
        let parent = self
            .0
            .rsplit_once('/')
            .map(|(parent, _)| if parent.is_empty() { "/" } else { parent })
            .ok_or(AppError::InvalidInput("remote parent"))?;
        Ok(Self(parent.to_owned()))
    }

    pub fn join(&self, name: &RemoteName) -> Result<Self, AppError> {
        let joined = if self.0 == "/" {
            format!("/{}", name.as_str())
        } else {
            format!("{}/{}", self.0, name.as_str())
        };
        Self::parse(joined)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteName(String);

impl RemoteName {
    pub fn parse(value: String) -> Result<Self, AppError> {
        let char_count = value.encode_utf16().count();
        if value.is_empty()
            || char_count > MAX_NAME_CHARS
            || value.contains('/')
            || value.contains('\0')
            || matches!(value.as_str(), "." | "..")
        {
            return Err(AppError::InvalidInput("remote name"));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug)]
pub struct LocalUploadPath(PathBuf);

impl LocalUploadPath {
    pub async fn from_selected(path: PathBuf) -> Result<Self, AppError> {
        if !path.is_absolute() {
            return Err(AppError::InvalidInput("local upload path"));
        }
        let metadata = tokio::fs::symlink_metadata(&path).await?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(AppError::InvalidInput("local upload file"));
        }
        Ok(Self(path))
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub fn file_name(&self) -> Result<String, AppError> {
        self.0
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
            .ok_or(AppError::InvalidInput("local file name"))
    }
}

#[derive(Clone, Debug)]
pub struct LocalDownloadPath(PathBuf);

impl LocalDownloadPath {
    /// Picks the first free name in `parent`, numbering it when the name is taken
    /// on disk or reserved by an unfinished download or a not yet used grant.
    /// `reserved` holds [`local_path_key`]s.
    pub async fn from_parent_avoiding(
        parent: PathBuf,
        name: &RemoteName,
        reserved: &HashSet<String>,
    ) -> Result<Self, AppError> {
        if !parent.is_absolute() {
            return Err(AppError::InvalidInput("local download parent"));
        }
        let metadata = tokio::fs::symlink_metadata(&parent).await?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(AppError::InvalidInput("local download parent"));
        }
        let preferred = safe_suggested_file_name(name);
        for index in 1_u64.. {
            let candidate = if index == 1 {
                preferred.clone()
            } else {
                numbered_file_name(&preferred, index)
            };
            let target = parent.join(candidate);
            if !reserved.contains(&local_path_key(&target))
                && !tokio::fs::try_exists(&target).await?
            {
                return Self::from_selected(target).await;
            }
        }
        unreachable!()
    }

    pub async fn from_selected(path: PathBuf) -> Result<Self, AppError> {
        if !path.is_absolute() || path.file_name().is_none() {
            return Err(AppError::InvalidInput("local download path"));
        }
        if tokio::fs::try_exists(&path).await? {
            return Err(AppError::Conflict);
        }
        let parent = path
            .parent()
            .ok_or(AppError::InvalidInput("local download parent"))?;
        let metadata = tokio::fs::metadata(parent).await?;
        if !metadata.is_dir() {
            return Err(AppError::InvalidInput("local download parent"));
        }
        Ok(Self(path))
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub fn resumable_temporary_path(&self, transfer_id: &str) -> Result<PathBuf, AppError> {
        let transfer_id = uuid::Uuid::parse_str(transfer_id)
            .map_err(|_| AppError::InvalidInput("transfer id"))?;
        if transfer_id.is_nil() {
            return Err(AppError::InvalidInput("transfer id"));
        }
        let file_name = self
            .0
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or(AppError::InvalidInput("local download file name"))?;
        Ok(self
            .0
            .with_file_name(format!(".{file_name}.koofr-part-{transfer_id}")))
    }
}

/// A comparison key for a local path. Windows file names are case-insensitive,
/// so two downloads into `Report.pdf` and `report.pdf` would collide.
pub fn local_path_key(path: &Path) -> String {
    path.to_string_lossy().replace('/', "\\").to_lowercase()
}

const MAX_PUBLISH_ATTEMPTS: u64 = 1000;

/// Moves a finished temporary file to `target` without ever replacing an existing
/// file. When something took the name while the download ran, the next free
/// numbered name is used instead. Returns the path the file now has.
pub async fn publish_without_overwrite(
    source: PathBuf,
    target: PathBuf,
) -> Result<PathBuf, AppError> {
    tokio::task::spawn_blocking(move || publish_without_overwrite_blocking(&source, &target))
        .await
        .map_err(|_| AppError::LocalData)?
}

fn publish_without_overwrite_blocking(source: &Path, target: &Path) -> Result<PathBuf, AppError> {
    let parent = target
        .parent()
        .ok_or(AppError::InvalidInput("local download parent"))?;
    let preferred = target
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(AppError::InvalidInput("local download file name"))?;
    for index in 1..=MAX_PUBLISH_ATTEMPTS {
        let candidate = if index == 1 {
            target.to_path_buf()
        } else {
            parent.join(numbered_file_name(preferred, index))
        };
        match rename_without_replace(source, &candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err(AppError::Conflict)
}

#[cfg(windows)]
fn rename_without_replace(source: &Path, target: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{MOVEFILE_WRITE_THROUGH, MoveFileExW};

    let wide = |path: &Path| {
        path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<u16>>()
    };
    let source = wide(source);
    let target = wide(target);
    // Without MOVEFILE_REPLACE_EXISTING the call fails with ERROR_ALREADY_EXISTS
    // instead of replacing a file that appeared after the name was chosen.
    // SAFETY: both buffers are NUL-terminated UTF-16 strings that outlive the call.
    let moved = unsafe { MoveFileExW(source.as_ptr(), target.as_ptr(), MOVEFILE_WRITE_THROUGH) };
    if moved == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn rename_without_replace(source: &Path, target: &Path) -> std::io::Result<()> {
    // A hard link fails with AlreadyExists instead of replacing the target.
    std::fs::hard_link(source, target)?;
    std::fs::remove_file(source)
}

pub fn numbered_file_name(preferred: &str, index: u64) -> String {
    let path = Path::new(preferred);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(preferred);
    match path.extension().and_then(|value| value.to_str()) {
        Some(extension) => format!("{stem} ({index}).{extension}"),
        None => format!("{stem} ({index})"),
    }
}

pub fn safe_suggested_file_name(name: &RemoteName) -> String {
    let mut sanitized: String = name
        .as_str()
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
                )
            {
                '_'
            } else {
                character
            }
        })
        .collect();
    sanitized = sanitized.trim_end_matches(['.', ' ']).to_owned();
    if sanitized.is_empty() {
        return "download".to_owned();
    }

    let stem = sanitized
        .split_once('.')
        .map_or(sanitized.as_str(), |(stem, _)| stem)
        .to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || stem.strip_prefix("COM").is_some_and(|suffix| {
            matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
        })
        || stem.strip_prefix("LPT").is_some_and(|suffix| {
            matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
        });
    if reserved {
        if sanitized.encode_utf16().count() == MAX_NAME_CHARS {
            sanitized.pop();
        }
        sanitized.insert(0, '_');
    }
    sanitized
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{
        LocalDownloadPath, MountId, RemoteName, RemotePath, local_path_key,
        publish_without_overwrite, safe_suggested_file_name,
    };

    #[test]
    fn validates_mount_identifiers() {
        assert!(MountId::parse("abc_DEF-123".to_owned()).is_ok());
        assert!(MountId::parse("../mount".to_owned()).is_err());
        assert!(MountId::parse(String::new()).is_err());
    }

    #[test]
    fn rejects_ambiguous_remote_paths() {
        for path in ["", "relative", "/a/../b", "/a//b", "/a/"] {
            assert!(
                RemotePath::parse(path.to_owned()).is_err(),
                "accepted {path}"
            );
        }
        assert!(RemotePath::parse("/资料/计划.txt".to_owned()).is_ok());
    }

    #[test]
    fn joins_remote_paths_without_platform_path_rules() {
        let parent = RemotePath::parse("/资料".to_owned()).expect("valid parent");
        let name = RemoteName::parse("预算.xlsx".to_owned()).expect("valid name");
        assert_eq!(
            parent.join(&name).expect("joined").as_str(),
            "/资料/预算.xlsx"
        );
    }

    #[test]
    fn root_cannot_be_mutated() {
        let root = RemotePath::parse("/".to_owned()).expect("valid root");
        assert!(root.require_non_root().is_err());
    }

    #[test]
    fn sanitizes_windows_download_suggestions() {
        let reserved = RemoteName::parse("CON.txt".to_owned()).expect("valid remote name");
        let invalid = RemoteName::parse("report:final?.pdf".to_owned()).expect("valid remote name");
        assert_eq!(safe_suggested_file_name(&reserved), "_CON.txt");
        assert_eq!(safe_suggested_file_name(&invalid), "report_final_.pdf");
    }

    #[test]
    fn counts_non_bmp_names_as_two_utf16_units() {
        let too_long = "😀".repeat(128);
        assert!(RemoteName::parse(too_long).is_err());
    }

    #[tokio::test]
    async fn chooses_a_numbered_file_name_instead_of_overwriting() {
        // Given
        let directory =
            std::env::temp_dir().join(format!("koofr-download-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).expect("create downloads directory");
        std::fs::write(directory.join("report.pdf"), b"existing").expect("create existing file");
        let name = RemoteName::parse("report.pdf".to_owned()).expect("valid remote name");

        // When
        let target =
            LocalDownloadPath::from_parent_avoiding(directory.clone(), &name, &HashSet::new())
                .await
                .expect("choose available target");

        // Then
        assert_eq!(target.as_path(), directory.join("report (2).pdf"));
        std::fs::remove_dir_all(directory).expect("remove downloads directory");
    }

    #[tokio::test]
    async fn skips_names_reserved_by_unfinished_downloads() {
        // Given
        let directory =
            std::env::temp_dir().join(format!("koofr-reserved-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).expect("create downloads directory");
        let name = RemoteName::parse("Report.pdf".to_owned()).expect("valid remote name");
        let reserved = HashSet::from([local_path_key(&directory.join("report.pdf"))]);

        // When
        let target = LocalDownloadPath::from_parent_avoiding(directory.clone(), &name, &reserved)
            .await
            .expect("choose unreserved target");

        // Then
        assert_eq!(target.as_path(), directory.join("Report (2).pdf"));
        std::fs::remove_dir_all(directory).expect("remove downloads directory");
    }

    #[tokio::test]
    async fn publishing_never_replaces_a_file_that_appeared_meanwhile() {
        // Given
        let directory =
            std::env::temp_dir().join(format!("koofr-publish-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).expect("create downloads directory");
        let target = directory.join("photo.jpg");
        std::fs::write(&target, b"first download").expect("write first file");
        let partial = directory.join(".photo.jpg.koofr-part");
        std::fs::write(&partial, b"second download").expect("write partial file");

        // When
        let published = publish_without_overwrite(partial.clone(), target.clone())
            .await
            .expect("publish second file");

        // Then
        assert_eq!(published, directory.join("photo (2).jpg"));
        assert_eq!(
            std::fs::read(&target).expect("read first"),
            b"first download"
        );
        assert_eq!(
            std::fs::read(&published).expect("read second"),
            b"second download"
        );
        assert!(!partial.exists());
        std::fs::remove_dir_all(directory).expect("remove downloads directory");
    }
}
