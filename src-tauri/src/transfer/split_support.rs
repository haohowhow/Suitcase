use serde_json::Map;

use crate::{
    error::AppError,
    file_ops::{MountId, RemoteName, RemotePath},
    koofr_api::{BODY_STALL_TIMEOUT, FileInfo, KoofrApi},
    logging::AppLogger,
};

use super::split_package::{
    MANIFEST_NAME, README_NAME, RESTORE_CMD_NAME, RESTORE_SH_NAME, SHA256SUMS_NAME, SplitManifest,
    restore_cmd, restore_sh,
};

pub(super) async fn upload_support_files(
    api: &KoofrApi,
    mount_id: &MountId,
    package_path: &RemotePath,
    manifest: &SplitManifest,
    diagnostics: Option<(&AppLogger, &str)>,
    report: impl Fn(usize, usize),
) -> Result<(), AppError> {
    let support_file_count = 5;
    report(0, support_file_count);
    let list = api.list_files(mount_id, package_path);
    let existing = if let Some((logger, transfer_id)) = diagnostics {
        logger
            .trace(
                "transfer",
                "split_list_support_files",
                Some(transfer_id),
                list,
            )
            .await?
    } else {
        list.await?
    };
    let target = RemoteTarget {
        api,
        mount_id,
        directory: package_path,
        existing: &existing,
    };
    let payload = serde_json::to_vec_pretty(manifest).map_err(|_| AppError::LocalData)?;
    let support_files = [
        (
            "windows_restore",
            RESTORE_CMD_NAME,
            restore_cmd().as_bytes().to_vec(),
        ),
        (
            "posix_restore",
            RESTORE_SH_NAME,
            restore_sh().as_bytes().to_vec(),
        ),
        ("instructions", README_NAME, manifest.readme().into_bytes()),
        (
            "checksum_list",
            SHA256SUMS_NAME,
            manifest.sha256sums().into_bytes(),
        ),
        ("manifest", MANIFEST_NAME, payload),
    ];
    for (index, (kind, name, bytes)) in support_files.into_iter().enumerate() {
        let span = diagnostics.and_then(|(logger, id)| {
            let operation = if existing.iter().any(|file| file.name == name) {
                "split_verify_support_file"
            } else {
                "split_upload_support_file"
            };
            logger.span(
                "transfer",
                operation,
                Some(id),
                Map::from_iter([
                    ("supportKind".to_owned(), kind.into()),
                    ("bytes".to_owned(), bytes.len().into()),
                ]),
            )
        });
        // These files are small, so both verification and upload can have a total deadline.
        let result =
            tokio::time::timeout(BODY_STALL_TIMEOUT, target.upload_if_missing(name, bytes))
                .await
                .unwrap_or(Err(AppError::NetworkStalled));
        if let Some(span) = span {
            span.finish(result.as_ref().err(), Map::new());
        }
        result?;
        report(index + 1, support_file_count);
    }
    Ok(())
}

struct RemoteTarget<'a> {
    api: &'a KoofrApi,
    mount_id: &'a MountId,
    directory: &'a RemotePath,
    existing: &'a [FileInfo],
}

impl RemoteTarget<'_> {
    async fn upload_if_missing(&self, name: &str, bytes: Vec<u8>) -> Result<(), AppError> {
        let length = u64::try_from(bytes.len()).map_err(|_| AppError::LocalData)?;
        let remote_name = RemoteName::parse(name.to_owned())?;
        if let Some(file) = self.existing.iter().find(|file| file.name == name) {
            if file.entry_type == "file" && u64::try_from(file.size).ok() == Some(length) {
                let path = self.directory.join(&remote_name)?;
                let existing = self.api.download_response(self.mount_id, &path).await?;
                if existing.bytes().await?.as_ref() == bytes.as_slice() {
                    return Ok(());
                }
            }
            return Err(AppError::Conflict);
        }
        self.api
            .upload(
                self.mount_id,
                self.directory,
                &remote_name,
                reqwest::Body::from(bytes),
                length,
            )
            .await?;
        Ok(())
    }
}
