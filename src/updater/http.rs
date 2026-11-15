//! Hardened HTTP plumbing for the updater download path (#1044).

use crate::updater::error::UpdateError;

/// Most redirect hops the updater follows (reqwest's own default is 10).
const MAX_REDIRECTS: usize = 10;

/// HTTP client whose redirects are re-checked against `trust_check`, so the
/// host allowlist covers every hop, not only the first URL (#1044, SUP-9).
pub(crate) fn build_client(trust_check: fn(&str) -> bool) -> Result<reqwest::Client, UpdateError> {
    let policy = reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() >= MAX_REDIRECTS {
            attempt.error("too many redirects")
        } else if trust_check(attempt.url().as_str()) {
            attempt.follow()
        } else {
            attempt.error("redirect to untrusted host refused")
        }
    });
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .redirect(policy)
        .build()
        .map_err(|e| UpdateError::Internal(format!("building reqwest client: {e}")))
}

/// Read the body chunk by chunk and stop once more than `max` bytes arrive;
/// `Content-Length` is only a hint and chunked bodies have none (SUP-8).
pub(crate) async fn read_body_capped(
    mut resp: reqwest::Response,
    max: u64,
) -> Result<Vec<u8>, UpdateError> {
    let mut body = Vec::new();
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| UpdateError::NetworkInterrupted {
            message: format!("download interrupted: {e}"),
        })?
    {
        body.extend_from_slice(&chunk);
        if body.len() as u64 > max {
            return Err(UpdateError::NetworkInterrupted {
                message: format!("binary too large: more than {max} bytes"),
            });
        }
    }
    Ok(body)
}
