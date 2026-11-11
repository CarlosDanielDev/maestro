use crate::updater::error::UpdateError;
use minisign_verify::{PublicKey, Signature};

/// Verify a minisign signature over `data` against the trusted release keys.
///
/// Fails closed: no trusted keys, an undecodable signature, or a signature
/// that no trusted key produced all map to `UpdateError::SignatureInvalid`.
/// The trusted keys travel inside the binary (compile-time constants), never
/// through the release channel being verified.
pub(crate) fn verify_sums_signature(
    data: &[u8],
    signature_text: &str,
    trusted_keys: &[PublicKey],
) -> Result<(), UpdateError> {
    if trusted_keys.is_empty() {
        return Err(UpdateError::SignatureInvalid {
            reason: "no trusted release keys configured".to_string(),
        });
    }
    let signature =
        Signature::decode(signature_text).map_err(|e| UpdateError::SignatureInvalid {
            reason: format!("decoding minisign signature: {e}"),
        })?;
    for key in trusted_keys {
        // allow_legacy=false: only current pre-hashed minisign signatures
        // (the format `rsign sign`/`minisign -S` produce by default).
        if key.verify(data, &signature, false).is_ok() {
            return Ok(());
        }
    }
    Err(UpdateError::SignatureInvalid {
        reason: "signature does not match any trusted release key".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    // Fixture generated once with the CI signing tool (rsign2 0.6.7,
    // `rsign generate -W` + `rsign sign -W`) in a temp dir and discarded.
    // It pins the wire format the release workflow will produce to the
    // format the runtime verifier accepts.
    const RSIGN_PUB_B64: &str = "RWRCmeRNkUlMFMZHYoszF6GPwPl3NV7qqA2b+40qEyJSmKx86XsW8bC4";
    const RSIGN_SUMS: &str = "abc123\nhash2  file2.tar.gz\n";
    const RSIGN_MINISIG: &str = "untrusted comment: signature from rsign secret key\nRURCmeRNkUlMFFmEdWcJGkAj+hA4ANBxfa47mLgMquDzE8xBDmIuR3QvSf+MYBxAAGJR8CubylpF73W35VPnjeW42xm5/UvAfQ0=\ntrusted comment: timestamp:auto file:sha256sums.txt\nYKcCXo5OBxKUY/DvK/AZcAd7FpfoGPZELn587fUrKLR2tAdBJhjDMOqnJqJD+nCHF/9xY9aweudTBsW5/0XxCA==";

    fn rsign_trusted_key() -> PublicKey {
        PublicKey::from_base64(RSIGN_PUB_B64).expect("decode rsign fixture public key")
    }

    fn generated_keypair() -> minisign::KeyPair {
        minisign::KeyPair::generate_unencrypted_keypair().expect("generate test keypair")
    }

    fn sign_sums(kp: &minisign::KeyPair, sums: &str) -> String {
        minisign::sign(
            Some(&kp.pk),
            &kp.sk,
            Cursor::new(sums.as_bytes()),
            Some("timestamp:0 file:sha256sums.txt"),
            None,
        )
        .expect("sign sums")
        .to_string()
    }

    #[test]
    fn rsign_cli_signature_verifies() {
        verify_sums_signature(RSIGN_SUMS.as_bytes(), RSIGN_MINISIG, &[rsign_trusted_key()])
            .expect("rsign-generated signature must verify");
    }

    #[test]
    fn keypair_signature_verifies() {
        let kp = generated_keypair();
        let pk = PublicKey::from_base64(&kp.pk.to_base64()).expect("decode public key");
        verify_sums_signature(RSIGN_SUMS.as_bytes(), &sign_sums(&kp, RSIGN_SUMS), &[pk])
            .expect("signature from trusted key must verify");
    }

    #[test]
    fn tampered_sums_with_valid_signature_from_real_key_rejected() {
        let kp = generated_keypair();
        let pk = PublicKey::from_base64(&kp.pk.to_base64()).expect("decode public key");
        let tampered = RSIGN_SUMS.replace("abc123", "deadbeef");
        let sig = sign_sums(&kp, RSIGN_SUMS); // valid over the ORIGINAL sums
        let result = verify_sums_signature(tampered.as_bytes(), &sig, &[pk]);
        assert!(
            matches!(result, Err(UpdateError::SignatureInvalid { .. })),
            "tampered sums must be rejected, got: {result:?}"
        );
    }

    #[test]
    fn rsign_signature_over_tampered_sums_rejected() {
        let tampered = format!("{RSIGN_SUMS}extra line\n");
        let result =
            verify_sums_signature(tampered.as_bytes(), RSIGN_MINISIG, &[rsign_trusted_key()]);
        assert!(
            matches!(result, Err(UpdateError::SignatureInvalid { .. })),
            "tampered sums with valid CLI signature must be rejected, got: {result:?}"
        );
    }

    #[test]
    fn signature_from_untrusted_key_rejected() {
        let signer = generated_keypair();
        let trusted = generated_keypair();
        let trusted_pk =
            PublicKey::from_base64(&trusted.pk.to_base64()).expect("decode public key");
        let result = verify_sums_signature(
            RSIGN_SUMS.as_bytes(),
            &sign_sums(&signer, RSIGN_SUMS),
            &[trusted_pk],
        );
        assert!(
            matches!(result, Err(UpdateError::SignatureInvalid { .. })),
            "signature from an untrusted key must be rejected, got: {result:?}"
        );
    }

    #[test]
    fn garbage_signature_text_rejected() {
        let result = verify_sums_signature(
            RSIGN_SUMS.as_bytes(),
            "not a minisign signature",
            &[rsign_trusted_key()],
        );
        assert!(
            matches!(result, Err(UpdateError::SignatureInvalid { .. })),
            "garbage signature text must be rejected, got: {result:?}"
        );
    }

    #[test]
    fn empty_trusted_keys_fails_closed() {
        let result = verify_sums_signature(RSIGN_SUMS.as_bytes(), RSIGN_MINISIG, &[]);
        assert!(
            matches!(result, Err(UpdateError::SignatureInvalid { .. })),
            "no trusted keys must fail closed, got: {result:?}"
        );
    }
}
