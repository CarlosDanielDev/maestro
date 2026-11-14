use super::*;
use crate::updater::checker::GitHubReleaseChecker;
use crate::updater::error::UpdateError;
use crate::updater::replace::{FakeReplacer, ReplacerBehavior};
use crate::updater::test_server::{ReleaseFiles, ReleaseServer, build_tar_gz};
use minisign_verify::PublicKey;
use std::sync::Arc;
use tempfile::tempdir;

/// Test-only trust check: accept plain-HTTP loopback URLs served by
/// `test_server::ReleaseServer` (production HTTPS+GitHub checks stay covered
/// by the `is_trusted_download_url` unit tests).
fn is_local_test_url(url: &str) -> bool {
    url.strip_prefix("http://")
        .and_then(|rest| rest.split('/').next())
        .map(|authority| {
            let host = authority.split(':').next().unwrap_or("");
            host == "127.0.0.1" || host == "localhost"
        })
        .unwrap_or(false)
}

fn test_archive_name(version: &str) -> String {
    format!(
        "maestro-v{version}-{}.tar.gz",
        GitHubReleaseChecker::platform_asset_suffix()
    )
}

/// Sums line in the exact format `release.yml` writes (`sha256sum ./*.tar.gz`,
/// which prefixes the filename with `./`).
fn sums_line_for(archive_name: &str, archive: &[u8]) -> String {
    format!(
        "{}  ./{}\n",
        Installer::compute_sha256_hex(archive),
        archive_name
    )
}

fn sign_sums(kp: &minisign::KeyPair, sums: &str) -> String {
    minisign::sign(
        Some(&kp.pk),
        &kp.sk,
        std::io::Cursor::new(sums.as_bytes()),
        Some("timestamp:0 file:sha256sums.txt"),
        None,
    )
    .expect("sign sums")
    .to_string()
}

fn trusted_key_of(kp: &minisign::KeyPair) -> PublicKey {
    PublicKey::from_base64(&kp.pk.to_base64()).expect("decode public key")
}

fn test_installer(
    dest: std::path::PathBuf,
    fake: Arc<FakeReplacer>,
    keys: Vec<PublicKey>,
) -> Installer {
    Installer::with_test_config(dest, fake, keys, is_local_test_url)
}

async fn spawn_release_server(
    sums: &str,
    signature: Option<&str>,
    archive_body: &[u8],
    archive_name: &str,
) -> ReleaseServer {
    ReleaseServer::spawn(ReleaseFiles {
        archive_path: format!("/{archive_name}"),
        archive_body: archive_body.to_vec(),
        sums: sums.to_string(),
        signature: signature.map(|s| s.to_string()),
    })
    .await
}

#[tokio::test]
async fn signed_release_downloads_and_installs() {
    let dir = tempdir().expect("tempdir");
    let kp = minisign::KeyPair::generate_unencrypted_keypair().expect("keypair");
    let archive = build_tar_gz(b"new maestro binary 1046");
    let name = test_archive_name("99.0.0");
    let sums = sums_line_for(&name, &archive);
    let sig = sign_sums(&kp, &sums);
    let server = spawn_release_server(&sums, Some(&sig), &archive, &name).await;

    let fake = Arc::new(FakeReplacer::new([ReplacerBehavior::Succeed {
        backup_path: dir.path().join("maestro.bak"),
    }]));
    let installer = test_installer(
        dir.path().join("maestro"),
        fake.clone(),
        vec![trusted_key_of(&kp)],
    );

    let url = format!("{}/{}", server.base_url, name);
    let result = installer.download_and_install(&url).await;

    assert!(
        result.is_ok(),
        "fully signed release must install, got: {result:?}"
    );
    assert_eq!(fake.call_count(), 1, "replace must run exactly once");
    server.shutdown();
}

/// Done when #5: matching sums but no signature asset → Err, no replace.
#[tokio::test]
async fn missing_signature_fails_closed_and_never_replaces() {
    let dir = tempdir().expect("tempdir");
    let kp = minisign::KeyPair::generate_unencrypted_keypair().expect("keypair");
    let archive = build_tar_gz(b"matching binary");
    let name = test_archive_name("99.0.0");
    let sums = sums_line_for(&name, &archive);
    let server = spawn_release_server(&sums, None, &archive, &name).await;

    let fake = Arc::new(FakeReplacer::new([]));
    let installer = test_installer(
        dir.path().join("maestro"),
        fake.clone(),
        vec![trusted_key_of(&kp)],
    );

    let url = format!("{}/{}", server.base_url, name);
    let result = installer.download_and_install(&url).await;

    let err = result.expect_err("missing signature must fail closed");
    assert!(
        matches!(err, UpdateError::SignatureInvalid { .. }),
        "expected SignatureInvalid, got: {err:?}"
    );
    assert_eq!(
        fake.call_count(),
        0,
        "replace() must not run without a verified signature"
    );
    server.shutdown();
}

#[tokio::test]
async fn signature_from_untrusted_key_fails_closed_and_never_replaces() {
    let dir = tempdir().expect("tempdir");
    let signer = minisign::KeyPair::generate_unencrypted_keypair().expect("keypair");
    let trusted = minisign::KeyPair::generate_unencrypted_keypair().expect("keypair");
    let archive = build_tar_gz(b"binary");
    let name = test_archive_name("99.0.0");
    let sums = sums_line_for(&name, &archive);
    let sig = sign_sums(&signer, &sums);
    let server = spawn_release_server(&sums, Some(&sig), &archive, &name).await;

    let fake = Arc::new(FakeReplacer::new([]));
    let installer = test_installer(
        dir.path().join("maestro"),
        fake.clone(),
        vec![trusted_key_of(&trusted)],
    );

    let url = format!("{}/{}", server.base_url, name);
    let result = installer.download_and_install(&url).await;

    let err = result.expect_err("signature from untrusted key must fail closed");
    assert!(
        matches!(err, UpdateError::SignatureInvalid { .. }),
        "expected SignatureInvalid, got: {err:?}"
    );
    assert_eq!(fake.call_count(), 0);
    server.shutdown();
}

#[tokio::test]
async fn tampered_sums_fails_closed_and_never_replaces() {
    let dir = tempdir().expect("tempdir");
    let kp = minisign::KeyPair::generate_unencrypted_keypair().expect("keypair");
    let archive = build_tar_gz(b"binary");
    let name = test_archive_name("99.0.0");
    let sums = sums_line_for(&name, &archive);
    let sig = sign_sums(&kp, &sums);
    // Attacker swaps the hash AFTER the release was signed.
    let tampered = sums.replace(
        &Installer::compute_sha256_hex(&archive),
        "0000000000000000000000000000000000000000000000000000000000000000",
    );
    assert_ne!(tampered, sums, "tampering must change the sums file");
    let server = spawn_release_server(&tampered, Some(&sig), &archive, &name).await;

    let fake = Arc::new(FakeReplacer::new([]));
    let installer = test_installer(
        dir.path().join("maestro"),
        fake.clone(),
        vec![trusted_key_of(&kp)],
    );

    let url = format!("{}/{}", server.base_url, name);
    let result = installer.download_and_install(&url).await;

    let err = result.expect_err("tampered sums must fail closed");
    assert!(
        matches!(err, UpdateError::SignatureInvalid { .. }),
        "signature over tampered sums must be rejected, got: {err:?}"
    );
    assert_eq!(fake.call_count(), 0);
    server.shutdown();
}

#[tokio::test]
async fn checksum_mismatch_after_valid_signature_rejected() {
    let dir = tempdir().expect("tempdir");
    let kp = minisign::KeyPair::generate_unencrypted_keypair().expect("keypair");
    let archive = build_tar_gz(b"binary");
    let name = test_archive_name("99.0.0");
    // Sums honestly sign archive A; the server serves archive B.
    let other = build_tar_gz(b"different binary");
    let sums = sums_line_for(&name, &archive);
    let sig = sign_sums(&kp, &sums);
    let server = spawn_release_server(&sums, Some(&sig), &other, &name).await;

    let fake = Arc::new(FakeReplacer::new([]));
    let installer = test_installer(
        dir.path().join("maestro"),
        fake.clone(),
        vec![trusted_key_of(&kp)],
    );

    let url = format!("{}/{}", server.base_url, name);
    let result = installer.download_and_install(&url).await;

    let err = result.expect_err("checksum mismatch must be rejected");
    assert!(
        matches!(err, UpdateError::ChecksumMismatch { .. }),
        "expected ChecksumMismatch, got: {err:?}"
    );
    assert_eq!(fake.call_count(), 0);
    server.shutdown();
}

/// Done when #3: install-time rollback check — the version recorded in the
/// verified sums must be strictly newer than the running binary.
#[tokio::test]
async fn downgrade_blocked_at_install_time() {
    let dir = tempdir().expect("tempdir");
    let kp = minisign::KeyPair::generate_unencrypted_keypair().expect("keypair");
    let archive = build_tar_gz(b"old binary");
    let name = test_archive_name("0.0.1");
    let sums = sums_line_for(&name, &archive);
    let sig = sign_sums(&kp, &sums);
    let server = spawn_release_server(&sums, Some(&sig), &archive, &name).await;

    let fake = Arc::new(FakeReplacer::new([]));
    let installer = test_installer(
        dir.path().join("maestro"),
        fake.clone(),
        vec![trusted_key_of(&kp)],
    );

    let url = format!("{}/{}", server.base_url, name);
    let result = installer.download_and_install(&url).await;

    let err = result.expect_err("downgrade must be blocked at install time");
    assert!(
        matches!(err, UpdateError::DowngradeRejected { .. }),
        "expected DowngradeRejected, got: {err:?}"
    );
    assert_eq!(fake.call_count(), 0);
    server.shutdown();
}

#[test]
fn sums_entry_normalizes_dot_slash_prefix() {
    let name = test_archive_name("1.2.3");
    let sums = sums_line_for(&name, b"payload");
    let entry = Installer::find_sums_entry(&sums, &name).expect("sums entry");
    assert_eq!(
        entry.hash,
        Installer::compute_sha256_hex(b"payload"),
        "hash must be extracted despite the ./ prefix"
    );
}

#[test]
fn sums_entry_missing_asset_is_error() {
    let sums = Installer::compute_sha256_hex(b"x");
    let result = Installer::find_sums_entry(&format!("{sums}  ./other.tar.gz\n"), "nope.tar.gz");
    assert!(result.is_err(), "missing asset in sums must error");
}

#[test]
fn candidate_version_from_sums_line_extracts_tag() {
    let name = test_archive_name("10.2.3");
    let tag = Installer::candidate_version_from_raw_name(&format!("./{name}"));
    assert_eq!(tag.as_deref(), Some("10.2.3"));
}

#[test]
fn candidate_version_rejects_unrelated_names() {
    assert!(Installer::candidate_version_from_raw_name("random.txt").is_none());
    assert!(Installer::candidate_version_from_raw_name("").is_none());
}

#[test]
fn shipped_trusted_release_keys_all_decode() {
    // A malformed entry is skipped at runtime (fail closed), so pin here that
    // every shipped key decodes and at least one ships (first key: v0.31.6).
    let shipped = crate::updater::TRUSTED_RELEASE_PUBLIC_KEYS;
    assert!(!shipped.is_empty(), "no trusted release key shipped");
    assert_eq!(trusted_release_keys().len(), shipped.len());
}
