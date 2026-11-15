//! Size caps and redirect/host trust for the updater download path (#1044:
//! SUP-8 / FS-6 / SUP-9).

use super::*;
use crate::updater::test_server::build_tar_gz;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Loopback trust check pinned to the IP literal, so `localhost` is an
/// "untrusted host" that still resolves — a followed redirect would succeed.
fn only_127(url: &str) -> bool {
    url.starts_with("http://127.0.0.1:")
}

/// Raw HTTP server: `respond(path, port)` returns the full response bytes.
async fn spawn_raw(respond: fn(&str, u16) -> Vec<u8>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let mut buf = vec![0u8; 8192];
            let n = stream.read(&mut buf).await.unwrap_or(0);
            let head = String::from_utf8_lossy(&buf[..n]).to_string();
            let path = head.split_whitespace().nth(1).unwrap_or("").to_string();
            let _ = stream.write_all(&respond(&path, port)).await;
            let _ = stream.shutdown().await;
        }
    });
    port
}

/// 4096-byte body with NO Content-Length: only bytes read can cap it.
fn unsized_body(_path: &str, _port: u16) -> Vec<u8> {
    let mut resp = b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n".to_vec();
    resp.extend(std::iter::repeat_n(b'x', 4096));
    resp
}

fn redirecting(path: &str, port: u16) -> Vec<u8> {
    let location = match path {
        "/to-untrusted" => format!("http://localhost:{port}/ok"),
        "/to-trusted" => format!("http://127.0.0.1:{port}/ok"),
        _ => {
            return b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok".to_vec();
        }
    };
    format!("HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
        .into_bytes()
}

#[test]
fn tar_entry_inflating_past_cap_is_rejected() {
    let archive = build_tar_gz(&vec![0u8; 4096]);
    let err = extract_binary_from_tar_gz_capped(&archive, 1024).expect_err("over cap");
    assert!(err.to_string().contains("exceeds"), "got: {err}");
    assert_eq!(
        extract_binary_from_tar_gz_capped(&archive, 4096)
            .expect("at cap")
            .len(),
        4096
    );
}

#[test]
fn tar_entry_declaring_more_than_max_download_size_is_rejected() {
    use flate2::Compression;
    use flate2::write::GzEncoder;

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    {
        let mut builder = tar::Builder::new(&mut encoder);
        let mut header = tar::Header::new_gnu();
        header.set_path("maestro").expect("path");
        header.set_size(crate::updater::MAX_DOWNLOAD_SIZE + 1);
        header.set_mode(0o755);
        header.set_cksum();
        builder.append(&header, &b"tiny"[..]).expect("append");
    }
    let archive = encoder.finish().expect("gzip");

    let err = extract_binary_from_tar_gz(&archive).expect_err("declared size over MAX");
    assert!(err.to_string().contains("exceeds"), "got: {err}");
}

#[tokio::test]
async fn body_without_content_length_is_capped_by_bytes_read() {
    let port = spawn_raw(unsized_body).await;
    let url = format!("http://127.0.0.1:{port}/asset");
    let client = build_client(only_127).expect("client");

    let resp = client.get(&url).send().await.expect("send");
    assert!(resp.content_length().is_none(), "fixture must be unsized");
    let err = read_body_capped(resp, 1024).await.expect_err("over cap");
    assert!(err.to_string().contains("too large"), "got: {err}");

    let resp = client.get(&url).send().await.expect("send");
    assert_eq!(
        read_body_capped(resp, 8192).await.expect("under cap").len(),
        4096
    );
}

#[tokio::test]
async fn redirect_to_untrusted_host_is_refused() {
    let port = spawn_raw(redirecting).await;
    let client = build_client(only_127).expect("client");

    let refused = client
        .get(format!("http://127.0.0.1:{port}/to-untrusted"))
        .send()
        .await;
    assert!(refused.is_err(), "redirect to untrusted host must fail");

    let followed = client
        .get(format!("http://127.0.0.1:{port}/to-trusted"))
        .send()
        .await
        .expect("trusted redirect is followed");
    assert!(followed.status().is_success());
}

#[tokio::test]
async fn derived_sums_url_is_rechecked_against_allowlist() {
    // `https://github.com` passes the allowlist, but its derived sums URL is
    // `https://sha256sums.txt` — a different host.
    assert!(crate::updater::is_trusted_download_url(
        "https://github.com"
    ));
    let client = build_client(crate::updater::is_trusted_download_url).expect("client");
    let err = Installer::fetch_sums_and_signature(
        &client,
        "https://github.com",
        crate::updater::is_trusted_download_url,
    )
    .await
    .expect_err("derived URL off the allowlist");
    assert!(err.to_string().contains("untrusted"), "got: {err}");
}

/// A stream longer than the cap: the bytes-read guard itself must stop it,
/// independent of any tar header (#1044 judge finding).
#[test]
fn capped_reader_stops_a_stream_longer_than_the_cap() {
    let stream = vec![7u8; 2048];
    let err = read_to_end_capped(&stream[..], 1024).expect_err("over cap");
    assert!(err.to_string().contains("exceeds"), "got: {err}");
    assert_eq!(
        read_to_end_capped(&stream[..], 2048).expect("at cap").len(),
        2048
    );
}

fn oversized_sums(_path: &str, _port: u16) -> Vec<u8> {
    let mut resp = b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n".to_vec();
    resp.extend(std::iter::repeat_n(b'a', MAX_SUMS_SIZE as usize + 1));
    resp
}

#[tokio::test]
async fn checksum_and_signature_downloads_are_capped() {
    let port = spawn_raw(oversized_sums).await;
    let client = build_client(only_127).expect("client");
    let err = Installer::fetch_text(&client, &format!("http://127.0.0.1:{port}/sha256sums.txt"))
        .await
        .expect_err("sums body over cap");
    assert!(err.to_string().contains("too large"), "got: {err}");
}
