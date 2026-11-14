# Release Signing — Key Model and Rotation

How release checksums get signed and how the self-updater verifies them before
installing an update (issue #1046, CWE-494).

## Trust model

- The release workflow signs `sha256sums.txt` with a minisign secret key
  (`rsign` in `release.yml`), producing `sha256sums.txt.minisig` as a release
  asset. Tarball integrity chains from there: a tarball is trusted when its
  hash appears in a signature-verified sums file and matches the download.
- The updater verifies that signature against minisign public keys compiled
  into the binary (`TRUSTED_RELEASE_PUBLIC_KEYS` in `src/updater/mod.rs`).
  The verification key never travels through the release channel it
  authenticates: it ships inside the binary itself, not as a release asset,
  not in the updater's HTTP responses, and not in the repository's release
  pipeline artifacts.
- Verification fails closed. Missing signature asset, undecodable signature,
  signature from an untrusted key, tampered sums, or no trusted keys
  configured → `UpdateError::SignatureInvalid`, no binary replace. An empty
  `TRUSTED_RELEASE_PUBLIC_KEYS` list therefore disables installs entirely
  until a key is provisioned (the first key shipped in v0.31.6).
- Install-time rollback protection: the version recorded in the
  signature-verified sums file must be strictly newer than the running
  binary, or the install is refused (`UpdateError::DowngradeRejected`).

## Why minisign

Small verify-only runtime crate (`minisign-verify`, zero dependencies),
offline verification, no keyring state. Rejected: GPG (external binary,
keyring state, fragile on CI and end-user machines), Sigstore/cosign keyless
(network + Rekor dependency at update time), GitHub attestations (same
network dependency, heavier verification stack).

## Initial provisioning

Run locally — never commit the secret key.

```bash
cargo install rsign2 --version 0.6.7 --locked
rsign generate -p maestro-release.pub -s maestro-release.key -W \
  -c "maestro release signing key"
```

Then:

1. Copy the secret key file contents (the whole `untrusted comment:` block)
   into the GitHub secret `RELEASE_SIGNING_KEY`.
2. Copy **only the base64 line** of `maestro-release.pub` (the last line,
   starting with `RW`) into the GitHub secret `RELEASE_SIGNING_PUBLIC_KEY`
   (used by the workflow's self-check step). Do not paste the
   `untrusted comment:` line: the workflow runs `rsign verify -P`, which
   fails with `base64 conversion failed` on the full file.
3. Copy the base64 line from `maestro-release.pub` into
   `TRUSTED_RELEASE_PUBLIC_KEYS` in `src/updater/mod.rs` and cut a release.
   From that release on, the updater accepts installs.
4. Verify the secret key never lands in the repository, release assets, or
   updater responses. `-W` (passwordless) is used because CI cannot prompt;
   protect it with GitHub secret scoping and restrict workflow permissions.

## Key rotation (planned)

Keys are a list on purpose: during rotation two keys are trusted at once, so
older updaters keep accepting new releases while the new key spreads.

1. Generate a new pair as above (comment it, e.g. "maestro release signing
   key 2").
2. Append the new public key to `TRUSTED_RELEASE_PUBLIC_KEYS` (keep the old
   one) and cut a release **signed with the old key** — the CI secret still
   holds the old secret. This release ships both keys to users.
3. Switch the GitHub secrets to the new key pair and cut the next release,
   now signed with the new key. Updaters past step 2 accept it.
4. Once telemetry/logs show installs from releases ≥ step 2 have saturated
   (no definite deadline exists — pick a window based on the version-usage
   signal you have), remove the old key from the constant and cut a release
   signed with the new key.

## Emergency rotation (suspected key compromise)

1. Generate a replacement pair immediately.
2. Remove the compromised key from `TRUSTED_RELEASE_PUBLIC_KEYS` and append
   the replacement (a release signed with a remaining trusted key carries
   the change).
3. Rotate the GitHub secrets. Every release cut after this is signed with
   the new key.
4. Updaters built before the fix still list the compromised key; they refuse
   to install releases signed only with the new key and must be updated
   through the other distribution channels (Homebrew tap, `cargo install`).

## Revocation vs rotation

There is no online revocation (by design — verification is offline). A
compromised key is handled by removing it from the compile-time list, which
only reaches users through a release signed by a *remaining trusted* key.
This is the standard trade-off for updater trust anchors: no CRLs, no Rekor,
no network at verification time, in exchange for a slower compromise
response.
