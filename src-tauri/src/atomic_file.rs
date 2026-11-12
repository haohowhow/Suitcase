//! Crash-safe replacement of the small JSON files that hold local application state.
//!
//! Every store used to write a shared `<name>.json.tmp`, delete the real file and
//! then rename the temporary one into place. Two writers could interleave on the
//! shared temporary file, and a crash between the delete and the rename left no
//! file at all. This module gives each write its own temporary file and replaces
//! the destination in a single rename.

use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::de::DeserializeOwned;

const RENAME_ATTEMPTS: u32 = 5;
const RENAME_RETRY_DELAY: Duration = Duration::from_millis(25);

/// Labels of the stores whose file was set aside during this launch. The stores
/// load before the logger exists, so start-up collects the labels afterwards.
static QUARANTINED: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

/// Replaces `path` with `bytes`. A reader, or the next launch after a crash, sees
/// either the previous complete file or the new complete file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "no parent"))?;
    std::fs::create_dir_all(parent)?;
    let temporary = temporary_path(path);
    let result = write_and_replace(&temporary, path, bytes);
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

/// Async wrapper that keeps the file system work off the async runtime threads.
pub async fn write_atomic_async(path: PathBuf, bytes: Vec<u8>) -> std::io::Result<()> {
    tokio::task::spawn_blocking(move || write_atomic(&path, &bytes))
        .await
        .map_err(|_| std::io::Error::other("state file write task failed"))?
}

/// Reads a JSON state file.
///
/// A missing file yields `None`. A file that exists but cannot be parsed, or that
/// `accept` rejects (an unknown format version), is renamed to
/// `<name>.corrupt-<unix millis>` before `None` is returned, so the next save does
/// not silently overwrite the only copy of the user's data.
pub fn read_json_or_quarantine<T, F>(path: &Path, label: &'static str, accept: F) -> Option<T>
where
    T: DeserializeOwned,
    F: FnOnce(&T) -> bool,
{
    let bytes = std::fs::read(path).ok()?;
    match serde_json::from_slice::<T>(&bytes) {
        Ok(value) if accept(&value) => Some(value),
        _ => {
            quarantine(path, label);
            None
        }
    }
}

/// Returns and clears the labels recorded by [`read_json_or_quarantine`].
pub fn take_quarantine_reports() -> Vec<&'static str> {
    match QUARANTINED.lock() {
        Ok(mut reports) => std::mem::take(&mut *reports),
        Err(poisoned) => std::mem::take(&mut *poisoned.into_inner()),
    }
}

fn write_and_replace(temporary: &Path, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    // Antivirus and the search indexer briefly hold freshly written files open on
    // Windows, which makes the rename fail with a sharing violation.
    let mut attempt = 1;
    loop {
        match std::fs::rename(temporary, path) {
            Ok(()) => return Ok(()),
            Err(error)
                if attempt < RENAME_ATTEMPTS
                    && error.kind() == std::io::ErrorKind::PermissionDenied =>
            {
                attempt += 1;
                std::thread::sleep(RENAME_RETRY_DELAY);
            }
            Err(error) => return Err(error),
        }
    }
}

fn temporary_path(path: &Path) -> PathBuf {
    sibling_with_suffix(path, &format!(".{}.tmp", uuid::Uuid::new_v4().simple()))
}

fn quarantine(path: &Path, label: &'static str) {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    if std::fs::rename(
        path,
        sibling_with_suffix(path, &format!(".corrupt-{millis}")),
    )
    .is_ok()
        && let Ok(mut reports) = QUARANTINED.lock()
    {
        reports.push(label);
    }
}

fn sibling_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path
        .file_name()
        .map(std::ffi::OsStr::to_os_string)
        .unwrap_or_default();
    name.push(suffix);
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use super::{read_json_or_quarantine, take_quarantine_reports, write_atomic};

    #[derive(Debug, Deserialize, PartialEq, Serialize)]
    struct Sample {
        version: u8,
        value: String,
    }

    fn directory(name: &str) -> std::path::PathBuf {
        let directory =
            std::env::temp_dir().join(format!("koofr-atomic-{name}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).expect("create test directory");
        directory
    }

    fn entries(directory: &std::path::Path) -> Vec<String> {
        let mut names = std::fs::read_dir(directory)
            .expect("read test directory")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect::<Vec<_>>();
        names.sort();
        names
    }

    #[test]
    fn replaces_an_existing_file_without_leaving_temporaries() {
        let directory = directory("replace");
        let path = directory.join("state.json");

        write_atomic(&path, b"first").expect("first write");
        write_atomic(&path, b"second").expect("second write");

        assert_eq!(std::fs::read(&path).expect("read state"), b"second");
        assert_eq!(entries(&directory), ["state.json"]);
        std::fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[test]
    fn concurrent_writers_always_leave_one_complete_file() {
        let directory = directory("concurrent");
        let path = directory.join("state.json");
        let payloads = (0..16)
            .map(|index| vec![b'a' + index as u8; 64 * 1024])
            .collect::<Vec<_>>();

        // The stores serialise their own writes. This only checks that unsynchronised
        // writers can lose a race but can never mix their bytes or leak a temporary.
        let succeeded = std::thread::scope(|scope| {
            let writers = payloads
                .iter()
                .map(|payload| {
                    let path = path.clone();
                    scope.spawn(move || write_atomic(&path, payload).is_ok())
                })
                .collect::<Vec<_>>();
            writers
                .into_iter()
                .map(|writer| writer.join())
                .filter(|outcome| matches!(outcome, Ok(true)))
                .count()
        });

        assert!(succeeded > 0);
        let written = std::fs::read(&path).expect("read state");
        assert!(payloads.contains(&written), "file holds a mixed payload");
        assert_eq!(entries(&directory), ["state.json"]);
        std::fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[test]
    fn sets_an_unreadable_file_aside_instead_of_discarding_it() {
        let directory = directory("quarantine");
        let path = directory.join("state.json");
        std::fs::write(&path, b"{ not json").expect("write broken state");
        take_quarantine_reports();

        let loaded = read_json_or_quarantine::<Sample, _>(&path, "sample", |_| true);

        assert!(loaded.is_none());
        let names = entries(&directory);
        assert_eq!(names.len(), 1);
        assert!(names[0].starts_with("state.json.corrupt-"));
        assert_eq!(
            std::fs::read(directory.join(&names[0])).expect("read quarantined file"),
            b"{ not json"
        );
        assert!(take_quarantine_reports().contains(&"sample"));
        std::fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[test]
    fn sets_an_unknown_version_aside_and_keeps_a_valid_file() {
        let directory = directory("version");
        let path = directory.join("state.json");
        let sample = Sample {
            version: 2,
            value: "kept".to_owned(),
        };
        write_atomic(&path, &serde_json::to_vec(&sample).expect("encode")).expect("write");

        let accepted =
            read_json_or_quarantine::<Sample, _>(&path, "sample", |value| value.version == 2);
        assert_eq!(accepted, Some(sample));
        assert_eq!(entries(&directory), ["state.json"]);

        let rejected =
            read_json_or_quarantine::<Sample, _>(&path, "sample", |value| value.version == 3);
        assert!(rejected.is_none());
        assert!(entries(&directory)[0].starts_with("state.json.corrupt-"));
        std::fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[test]
    fn a_missing_file_is_not_an_error() {
        let directory = directory("missing");
        let loaded =
            read_json_or_quarantine::<Sample, _>(&directory.join("absent.json"), "sample", |_| {
                true
            });
        assert!(loaded.is_none());
        assert!(entries(&directory).is_empty());
        std::fs::remove_dir_all(directory).expect("remove test directory");
    }
}
