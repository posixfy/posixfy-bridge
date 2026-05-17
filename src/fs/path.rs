use crate::models::mount::MountPoint;
use std::path::{Component, Path, PathBuf};

/// Check if a path contains any traversal components (`..` or `.`).
/// Returns true if the path attempts to escape via `..`.
fn has_path_traversal(rel_path: &str) -> bool {
    Path::new(rel_path)
        .components()
        .any(|c| matches!(c, Component::ParentDir))
}

/// Resolve a user-provided path against mount points, with safety checks.
/// Returns the absolute filesystem path.
pub fn resolve_path(
    mount_points: &[MountPoint],
    mount_name: &str,
    rel_path: &str,
) -> Result<PathBuf, String> {
    let mount = mount_points
        .iter()
        .find(|m| m.name == mount_name)
        .ok_or_else(|| format!("Unknown mount point: {mount_name}"))?;

    // Reject path traversal attempts
    if has_path_traversal(rel_path) {
        return Err("Path traversal not allowed".to_string());
    }

    let rel = rel_path.trim_start_matches('/');
    let full_path = Path::new(&mount.path).join(rel);

    // Canonicalize and verify prefix
    let canonical = full_path
        .canonicalize()
        .map_err(|e| format!("Path resolution failed: {e}"))?;

    let mount_canonical = Path::new(&mount.path)
        .canonicalize()
        .map_err(|e| format!("Mount point resolution failed: {e}"))?;

    if !canonical.starts_with(&mount_canonical) {
        return Err("Path traversal not allowed".to_string());
    }

    Ok(canonical)
}

/// Like resolve_path but allows the target to not exist yet (for mkdir/upload).
/// Validates the parent exists and is within the mount.
pub fn resolve_new_path(
    mount_points: &[MountPoint],
    mount_name: &str,
    rel_path: &str,
) -> Result<PathBuf, String> {
    let mount = mount_points
        .iter()
        .find(|m| m.name == mount_name)
        .ok_or_else(|| format!("Unknown mount point: {mount_name}"))?;

    if has_path_traversal(rel_path) {
        return Err("Path traversal not allowed".to_string());
    }

    let rel = rel_path.trim_start_matches('/');
    let full_path = Path::new(&mount.path).join(rel);

    // Check parent exists and is within mount
    let parent = full_path
        .parent()
        .ok_or_else(|| "Invalid path".to_string())?;

    let parent_canonical = parent
        .canonicalize()
        .map_err(|e| format!("Parent path resolution failed: {e}"))?;

    let mount_canonical = Path::new(&mount.path)
        .canonicalize()
        .map_err(|e| format!("Mount point resolution failed: {e}"))?;

    if !parent_canonical.starts_with(&mount_canonical) {
        return Err("Path traversal not allowed".to_string());
    }

    // Return the non-canonical path (target doesn't exist yet)
    Ok(parent_canonical.join(full_path.file_name().ok_or("Invalid filename")?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::mount::MountPoint;
    use std::fs;

    /// Helper: create a temp dir with a mount point vec pointing into it.
    fn setup_temp_mount() -> (tempfile::TempDir, Vec<MountPoint>) {
        let tmp = tempfile::tempdir().expect("failed to create temp dir");
        // Create a subdirectory inside the temp dir to act as the mount root
        let mount_root = tmp.path().join("data");
        fs::create_dir_all(&mount_root).unwrap();
        // Create a nested subdirectory so resolve_path has something real to resolve
        fs::create_dir_all(mount_root.join("subdir")).unwrap();
        // Create a file inside the mount root
        fs::write(mount_root.join("file.txt"), "hello").unwrap();

        let mounts = vec![MountPoint {
            name: "docs".to_string(),
            path: mount_root.to_string_lossy().to_string(),
        }];
        (tmp, mounts)
    }

    #[test]
    fn resolve_path_valid_file() {
        let (_tmp, mounts) = setup_temp_mount();
        let result = resolve_path(&mounts, "docs", "file.txt");
        assert!(result.is_ok());
        let resolved = result.unwrap();
        assert!(resolved.ends_with("file.txt"));
    }

    #[test]
    fn resolve_path_valid_subdir() {
        let (_tmp, mounts) = setup_temp_mount();
        let result = resolve_path(&mounts, "docs", "subdir");
        assert!(result.is_ok());
        let resolved = result.unwrap();
        assert!(resolved.ends_with("subdir"));
    }

    #[test]
    fn resolve_path_with_leading_slash() {
        let (_tmp, mounts) = setup_temp_mount();
        let result = resolve_path(&mounts, "docs", "/file.txt");
        assert!(result.is_ok());
    }

    #[test]
    fn resolve_path_dot_dot_rejected() {
        let (_tmp, mounts) = setup_temp_mount();
        let result = resolve_path(&mounts, "docs", "../etc/passwd");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Path traversal not allowed");
    }

    #[test]
    fn resolve_path_unknown_mount_returns_error() {
        let (_tmp, mounts) = setup_temp_mount();
        let result = resolve_path(&mounts, "unknown", "file.txt");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown mount point"));
    }

    #[test]
    fn resolve_path_nonexistent_file_returns_error() {
        let (_tmp, mounts) = setup_temp_mount();
        let result = resolve_path(&mounts, "docs", "nonexistent.txt");
        assert!(result.is_err());
        // canonicalize fails on non-existing paths
        assert!(result.unwrap_err().contains("Path resolution failed"));
    }

    #[test]
    fn resolve_path_mount_root_itself() {
        let (_tmp, mounts) = setup_temp_mount();
        // Empty rel_path resolves to the mount root
        let result = resolve_path(&mounts, "docs", "");
        assert!(result.is_ok());
    }

    #[test]
    fn resolve_new_path_non_existing_target() {
        let (_tmp, mounts) = setup_temp_mount();
        let result = resolve_new_path(&mounts, "docs", "new_file.txt");
        assert!(result.is_ok());
        let resolved = result.unwrap();
        assert!(resolved.ends_with("new_file.txt"));
    }

    #[test]
    fn resolve_new_path_in_existing_subdir() {
        let (_tmp, mounts) = setup_temp_mount();
        let result = resolve_new_path(&mounts, "docs", "subdir/new_file.txt");
        assert!(result.is_ok());
        let resolved = result.unwrap();
        assert!(resolved.ends_with("new_file.txt"));
    }

    #[test]
    fn resolve_new_path_dot_dot_rejected() {
        let (_tmp, mounts) = setup_temp_mount();
        let result = resolve_new_path(&mounts, "docs", "../outside.txt");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Path traversal not allowed");
    }

    #[test]
    fn resolve_new_path_unknown_mount() {
        let (_tmp, mounts) = setup_temp_mount();
        let result = resolve_new_path(&mounts, "unknown", "file.txt");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown mount point"));
    }

    #[test]
    fn resolve_new_path_parent_must_exist() {
        let (_tmp, mounts) = setup_temp_mount();
        let result = resolve_new_path(&mounts, "docs", "nonexistent_dir/file.txt");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Parent path resolution failed"));
    }

    #[test]
    fn resolve_path_triple_dot_directory_not_rejected() {
        // "..." is a valid directory name, not a traversal
        let (_tmp, mounts) = setup_temp_mount();
        let result = resolve_path(&mounts, "docs", ".../file.txt");
        // Should NOT be rejected by traversal check (will fail on canonicalize since dir doesn't exist,
        // but the traversal check itself should pass)
        assert!(result.is_err());
        assert!(!result.unwrap_err().contains("Path traversal"));
    }

    #[test]
    fn resolve_path_leading_dot_accepted() {
        // "./subdir" should resolve correctly
        let (_tmp, mounts) = setup_temp_mount();
        let result = resolve_path(&mounts, "docs", "./subdir");
        assert!(result.is_ok());
        assert!(result.unwrap().ends_with("subdir"));
    }
}
