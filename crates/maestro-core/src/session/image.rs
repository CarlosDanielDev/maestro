use anyhow::{Result, bail};
use std::path::{Path, PathBuf};

pub const VALID_IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "svg"];

/// Validate that `path` exists on disk and has an allowed image extension.
pub fn validate_image_path(path: &Path) -> Result<()> {
    if !path.exists() {
        bail!("image path does not exist: {}", path.display());
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    if !VALID_IMAGE_EXTENSIONS.contains(&ext.as_str()) {
        bail!(
            "unsupported image extension '{}' for path: {}",
            ext,
            path.display()
        );
    }
    Ok(())
}

/// Copy each image into `worktree_dir` and return the relative filenames.
pub fn copy_images_to_worktree(
    image_paths: &[PathBuf],
    worktree_dir: &Path,
) -> Result<Vec<PathBuf>> {
    let mut relative = Vec::with_capacity(image_paths.len());
    for src in image_paths {
        let filename = src
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("image path has no filename: {}", src.display()))?;
        let dest = worktree_dir.join(filename);
        std::fs::copy(src, &dest)?;
        relative.push(PathBuf::from(filename));
    }
    Ok(relative)
}

/// Build the image section for a prompt. Returns `None` when `paths` is empty.
pub fn image_section_for_prompt(paths: &[PathBuf]) -> Option<String> {
    if paths.is_empty() {
        return None;
    }
    let list = paths
        .iter()
        .map(|p| format!("- {}", p.display()))
        .collect::<Vec<_>>()
        .join("\n");
    Some(format!("## Attached Images\n\n{list}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    // --- validate_image_path ---

    #[test]
    fn validate_image_path_accepts_valid_extensions() {
        let dir = tempdir().unwrap();
        for ext in VALID_IMAGE_EXTENSIONS {
            let path = dir.path().join(format!("test.{ext}"));
            std::fs::write(&path, b"").unwrap();
            assert!(
                validate_image_path(&path).is_ok(),
                "expected Ok for extension: {ext}",
            );
        }
    }

    #[test]
    fn validate_image_path_rejects_invalid_extension() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("document.txt");
        std::fs::write(&path, b"").unwrap();
        let result = validate_image_path(&path);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("txt") || msg.contains("extension"),
            "msg: {msg}"
        );
    }

    #[test]
    fn validate_image_path_rejects_nonexistent_file() {
        let path = PathBuf::from("/nonexistent/ghost.png");
        let result = validate_image_path(&path);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("does not exist") || msg.contains("ghost.png"),
            "msg: {msg}"
        );
    }

    #[test]
    fn validate_image_path_rejects_path_with_no_extension() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("Makefile");
        std::fs::write(&path, b"").unwrap();
        assert!(validate_image_path(&path).is_err());
    }

    // --- copy_images_to_worktree ---

    #[test]
    fn copy_images_to_worktree_copies_files_to_destination() {
        let src_dir = tempdir().unwrap();
        let wt_dir = tempdir().unwrap();
        let img1 = src_dir.path().join("photo.png");
        let img2 = src_dir.path().join("diagram.jpg");
        std::fs::write(&img1, b"png-data").unwrap();
        std::fs::write(&img2, b"jpg-data").unwrap();

        copy_images_to_worktree(&[img1, img2], wt_dir.path()).unwrap();

        assert!(wt_dir.path().join("photo.png").exists());
        assert!(wt_dir.path().join("diagram.jpg").exists());
    }

    #[test]
    fn copy_images_to_worktree_returns_relative_paths() {
        let src_dir = tempdir().unwrap();
        let wt_dir = tempdir().unwrap();
        let img = src_dir.path().join("photo.png");
        std::fs::write(&img, b"data").unwrap();

        let relative = copy_images_to_worktree(&[img], wt_dir.path()).unwrap();

        assert_eq!(relative.len(), 1);
        assert_eq!(relative[0], PathBuf::from("photo.png"));
        assert!(!relative[0].is_absolute());
    }

    #[test]
    fn copy_images_to_worktree_with_empty_slice_returns_empty_vec() {
        let wt_dir = tempdir().unwrap();
        let result = copy_images_to_worktree(&[], wt_dir.path()).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn copy_images_to_worktree_handles_duplicate_filenames_by_overwriting() {
        let src1 = tempdir().unwrap();
        let src2 = tempdir().unwrap();
        let wt_dir = tempdir().unwrap();
        let p1 = src1.path().join("logo.png");
        let p2 = src2.path().join("logo.png");
        std::fs::write(&p1, b"version-1").unwrap();
        std::fs::write(&p2, b"version-2").unwrap();

        copy_images_to_worktree(&[p1, p2], wt_dir.path()).unwrap();

        let content = std::fs::read(wt_dir.path().join("logo.png")).unwrap();
        assert_eq!(content, b"version-2");
    }

    // --- image_section_for_prompt ---

    #[test]
    fn image_section_for_prompt_returns_none_for_empty_slice() {
        assert!(image_section_for_prompt(&[]).is_none());
    }

    #[test]
    fn image_section_for_prompt_returns_some_with_single_image() {
        let result = image_section_for_prompt(&[PathBuf::from("diagram.svg")]);
        let section = result.unwrap();
        assert!(section.contains("diagram.svg"));
        assert!(section.contains("Attached Images"));
    }

    #[test]
    fn image_section_for_prompt_formats_multiple_images_as_list() {
        let paths = vec![
            PathBuf::from("a.png"),
            PathBuf::from("b.jpg"),
            PathBuf::from("c.gif"),
        ];
        let section = image_section_for_prompt(&paths).unwrap();
        assert!(section.contains("a.png"));
        assert!(section.contains("b.jpg"));
        assert!(section.contains("c.gif"));
    }
}
