use super::*;
use crate::updater::error::UpdateError;
use crate::updater::replace::{FakeReplacer, ReplacerBehavior};
use std::io;
use std::sync::Arc;
use tempfile::tempdir;

#[tokio::test]
async fn installer_writes_downloaded_bytes_to_staging() {
    let dir = tempdir().expect("tempdir");
    let dest = dir.path().join("maestro");
    fs::write(&dest, b"old binary").await.expect("write");

    let installer = Installer::new(dest);
    let fake_bytes = b"new binary content";
    let staging = installer
        .write_to_staging(fake_bytes)
        .await
        .expect("staging");

    assert!(staging.exists());
    let written = fs::read(&staging).await.expect("read");
    assert_eq!(written, fake_bytes);
}

#[tokio::test]
async fn installer_returns_error_when_dest_does_not_exist() {
    let dir = tempdir().expect("tempdir");
    let dest = dir.path().join("nonexistent_maestro");
    let installer = Installer::new(dest);
    let result = installer.install_with_backup(b"new bytes".to_vec()).await;
    assert!(result.is_err(), "expected Err, got: {result:?}");
}

#[test]
fn compute_sha256_hex_matches_known_vector() {
    let data = b"hello world";
    let expected = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
    assert_eq!(Installer::compute_sha256_hex(data), expected);
}

#[test]
fn checksum_url_uses_sha256sums_txt_filename() {
    let download_url = "https://github.com/CarlosDanielDev/maestro/releases/download/v0.10.0/maestro-v0.10.0-aarch64-apple-darwin.tar.gz";
    let expected_base = "https://github.com/CarlosDanielDev/maestro/releases/download/v0.10.0";
    let sums_url = download_url
        .rsplit_once('/')
        .map(|(base, _)| format!("{}/sha256sums.txt", base))
        .expect("rsplit");
    assert_eq!(sums_url, format!("{}/sha256sums.txt", expected_base));
}

#[test]
fn extract_binary_from_tar_gz_finds_maestro_binary() {
    use flate2::Compression;
    use flate2::write::GzEncoder;

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    {
        let mut builder = tar::Builder::new(&mut encoder);
        let binary_content = b"fake maestro binary";
        let mut header = tar::Header::new_gnu();
        header.set_size(binary_content.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder
            .append_data(&mut header, "maestro", &binary_content[..])
            .expect("append");
        builder.finish().expect("finish");
    }
    let archive_bytes = encoder.finish().expect("encoder");

    let result = extract_binary_from_tar_gz(&archive_bytes);
    assert!(result.is_ok(), "Should extract binary: {:?}", result.err());
    assert_eq!(result.expect("ok"), b"fake maestro binary");
}

#[test]
fn extract_binary_from_tar_gz_fails_when_no_maestro_binary() {
    use flate2::Compression;
    use flate2::write::GzEncoder;

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    {
        let mut builder = tar::Builder::new(&mut encoder);
        let content = b"some other file";
        let mut header = tar::Header::new_gnu();
        header.set_size(content.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, "not-maestro", &content[..])
            .expect("append");
        builder.finish().expect("finish");
    }
    let archive_bytes = encoder.finish().expect("encoder");

    let result = extract_binary_from_tar_gz(&archive_bytes);
    assert!(
        result.is_err(),
        "Should fail when no maestro binary in archive"
    );
}

#[tokio::test]
async fn installer_success_returns_backup_path() {
    let dir = tempdir().expect("tempdir");
    let dest = dir.path().join("maestro");
    let backup = dir.path().join("maestro.bak");
    let fake = Arc::new(FakeReplacer::new([ReplacerBehavior::Succeed {
        backup_path: backup.clone(),
    }]));
    let installer = Installer::with_replacer(dest, fake.clone());

    let result = installer.install_with_backup(b"new bytes".to_vec()).await;

    assert!(result.is_ok(), "expected Ok, got: {result:?}");
    assert_eq!(result.expect("ok"), backup);
    assert_eq!(fake.call_count(), 1);
}

#[tokio::test]
async fn installer_permission_denied_surfaces_typed_error() {
    let dir = tempdir().expect("tempdir");
    let dest = dir.path().join("maestro");
    let fake = Arc::new(FakeReplacer::new([ReplacerBehavior::Fail(
        UpdateError::PermissionDenied {
            path: dest.clone(),
            source: io::Error::from(io::ErrorKind::PermissionDenied),
        },
    )]));
    let installer = Installer::with_replacer(dest, fake);

    let result = installer.install_with_backup(b"bytes".to_vec()).await;

    assert!(
        matches!(result, Err(UpdateError::PermissionDenied { .. })),
        "expected PermissionDenied, got: {result:?}"
    );
}

#[tokio::test]
async fn installer_replace_fails_rollback_ok_returns_rolled_back_error() {
    let dir = tempdir().expect("tempdir");
    let dest = dir.path().join("maestro");
    let fake = Arc::new(FakeReplacer::new([ReplacerBehavior::Fail(
        UpdateError::ReplaceFailedRolledBack {
            source: io::Error::from(io::ErrorKind::Other),
        },
    )]));
    let installer = Installer::with_replacer(dest, fake);

    let result = installer.install_with_backup(b"bytes".to_vec()).await;

    let err = result.expect_err("expected ReplaceFailedRolledBack");
    assert!(
        matches!(err, UpdateError::ReplaceFailedRolledBack { .. }),
        "expected ReplaceFailedRolledBack, got: {err:?}"
    );
    assert_eq!(
        format!("{err}"),
        "update failed — original version restored"
    );
}

#[tokio::test]
async fn installer_rollback_failed_no_panic() {
    let dir = tempdir().expect("tempdir");
    let dest = dir.path().join("maestro");
    let fake = Arc::new(FakeReplacer::new([ReplacerBehavior::Fail(
        UpdateError::RollbackFailed {
            replace_source: io::Error::from(io::ErrorKind::Other),
            rollback_source: io::Error::from(io::ErrorKind::BrokenPipe),
        },
    )]));
    let installer = Installer::with_replacer(dest, fake);

    let result = installer.install_with_backup(b"bytes".to_vec()).await;

    let err = result.expect_err("expected RollbackFailed");
    assert!(
        matches!(err, UpdateError::RollbackFailed { .. }),
        "expected RollbackFailed, got: {err:?}"
    );
    assert_eq!(
        format!("{err}"),
        "update failed and rollback could not complete — please reinstall maestro manually"
    );
}

#[tokio::test]
async fn download_and_install_rejects_untrusted_url() {
    let dir = tempdir().expect("tempdir");
    let dest = dir.path().join("maestro");
    let fake = Arc::new(FakeReplacer::new([]));
    let installer = Installer::with_replacer(dest, fake.clone());

    let result = installer
        .download_and_install("http://evil.com/maestro")
        .await;

    assert!(result.is_err(), "expected Err for untrusted URL");
    assert_eq!(
        fake.call_count(),
        0,
        "replace() must not be called for untrusted URL"
    );
}
