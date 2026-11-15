use std::path::Path;

/// Write `content` to `path` atomically. Uses `tempfile::NamedTempFile` to
/// create a unique-name temp file with `O_EXCL` semantics in the same
/// directory, then `persist` (rename) it over the destination. This closes
/// the TOCTOU window that a deterministic `<path>.tmp` filename would leave
/// open on shared filesystems.
pub(super) fn atomic_write(path: &Path, content: &str) -> std::io::Result<()> {
    use std::io::Write;
    let parent = path.parent().unwrap_or(Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    tmp.write_all(content.as_bytes())?;
    tmp.flush()?;
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}
