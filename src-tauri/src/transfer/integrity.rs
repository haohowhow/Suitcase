use std::path::Path;

use md5::{Digest, Md5};
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;

use crate::error::AppError;

use super::IntegrityStatus;

pub async fn verify_file_integrity(
    path: &Path,
    expected_md5: &str,
    cancel: &CancellationToken,
    requested: bool,
) -> Result<IntegrityStatus, AppError> {
    if !requested {
        return Ok(IntegrityStatus::NotRequested);
    }
    let expected = expected_md5.trim();
    if expected.len() != 32 || !expected.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Ok(IntegrityStatus::Unavailable);
    }

    let mut file = tokio::fs::File::open(path).await?;
    let mut digest = Md5::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = tokio::select! {
            result = file.read(&mut buffer) => result?,
            () = cancel.cancelled() => return Err(AppError::Cancelled),
        };
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    let actual = format!("{:x}", digest.finalize());
    if !actual.eq_ignore_ascii_case(expected) {
        return Err(AppError::Integrity);
    }
    Ok(IntegrityStatus::Verified)
}

#[cfg(test)]
mod tests {
    use super::verify_file_integrity;
    use crate::{error::AppError, transfer::IntegrityStatus};

    #[tokio::test]
    async fn reports_verified_unavailable_and_mismatch_without_exposing_paths() {
        let path = std::env::temp_dir().join(format!("koofr-md5-{}", uuid::Uuid::new_v4()));
        tokio::fs::write(&path, b"hello")
            .await
            .expect("write fixture");
        let cancel = tokio_util::sync::CancellationToken::new();

        assert_eq!(
            verify_file_integrity(&path, "5d41402abc4b2a76b9719d911017c592", &cancel, true)
                .await
                .expect("valid digest"),
            IntegrityStatus::Verified
        );
        assert_eq!(
            verify_file_integrity(&path, "", &cancel, true)
                .await
                .expect("missing digest"),
            IntegrityStatus::Unavailable
        );
        assert!(matches!(
            verify_file_integrity(&path, "00000000000000000000000000000000", &cancel, true).await,
            Err(AppError::Integrity)
        ));
        tokio::fs::remove_file(path).await.expect("remove fixture");
    }
}
