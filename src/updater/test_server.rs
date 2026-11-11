//! Test-only local HTTP server that mimics a GitHub release directory:
//! a tarball asset, `sha256sums.txt`, and `sha256sums.txt.minisig`.

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

pub(crate) struct ReleaseFiles {
    /// Request path of the tarball asset, e.g. `/maestro-v99.0.0-<target>.tar.gz`.
    pub archive_path: String,
    pub archive_body: Vec<u8>,
    pub sums: String,
    /// `None` serves a 404 for the signature asset (fail-closed test).
    pub signature: Option<String>,
}

pub(crate) struct ReleaseServer {
    pub base_url: String,
    handle: tokio::task::JoinHandle<()>,
}

impl ReleaseServer {
    pub(crate) async fn spawn(files: ReleaseFiles) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind local test server");
        let addr = listener.local_addr().expect("local addr");
        let handle = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                serve_one(stream, &files).await;
            }
        });
        Self {
            base_url: format!("http://{addr}"),
            handle,
        }
    }

    pub(crate) fn shutdown(self) {
        self.handle.abort();
    }
}

async fn serve_one(mut stream: TcpStream, files: &ReleaseFiles) {
    let mut buf = vec![0u8; 8192];
    let n = stream.read(&mut buf).await.unwrap_or(0);
    let head = String::from_utf8_lossy(&buf[..n]);
    let path = head.split_whitespace().nth(1).unwrap_or("").to_string();

    let (status, body): (String, Vec<u8>) = if path == files.archive_path {
        ("200 OK".into(), files.archive_body.clone())
    } else if path == "/sha256sums.txt" {
        ("200 OK".into(), files.sums.as_bytes().to_vec())
    } else if path == "/sha256sums.txt.minisig" {
        match &files.signature {
            Some(sig) => ("200 OK".into(), sig.as_bytes().to_vec()),
            None => ("404 Not Found".into(), b"not found".to_vec()),
        }
    } else {
        ("404 Not Found".into(), b"not found".to_vec())
    };

    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(header.as_bytes()).await;
    let _ = stream.write_all(&body).await;
    let _ = stream.flush().await;
    let _ = stream.shutdown().await;
}

/// Build an in-memory tar.gz archive containing a single `maestro` binary.
pub(crate) fn build_tar_gz(binary: &[u8]) -> Vec<u8> {
    use flate2::Compression;
    use flate2::write::GzEncoder;

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    {
        let mut builder = tar::Builder::new(&mut encoder);
        let mut header = tar::Header::new_gnu();
        header.set_size(binary.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder
            .append_data(&mut header, "maestro", binary)
            .expect("append maestro binary");
        builder.finish().expect("finish tar builder");
    }
    encoder.finish().expect("finish gzip encoder")
}
