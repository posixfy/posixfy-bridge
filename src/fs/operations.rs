use serde::Serialize;
use std::path::Path;

use crate::fs::guard::FsUidGuard;

#[derive(Debug, Serialize)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: i64,
}

pub async fn list_dir(
    path: &Path,
    uid: u32,
    gid: u32,
    groups: &[u32],
) -> Result<Vec<DirEntry>, std::io::Error> {
    let path = path.to_path_buf();
    let groups = groups.to_vec();
    tokio::task::spawn_blocking(move || {
        let _guard = FsUidGuard::new(uid, gid, &groups);
        let res = (|| -> Result<Vec<DirEntry>, std::io::Error> {
            let mut entries = Vec::new();
            for entry in std::fs::read_dir(&path)? {
                let entry = entry?;
                let metadata = entry.metadata()?;
                let modified = metadata
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                entries.push(DirEntry {
                    name: entry.file_name().to_string_lossy().to_string(),
                    is_dir: metadata.is_dir(),
                    size: metadata.len(),
                    modified,
                });
            }
            entries.sort_by(|a, b| a.name.cmp(&b.name));
            Ok(entries)
        })();
        match &res {
            Ok(entries) => {
                tracing::debug!(path = %path.display(), uid, count = entries.len(), "listed directory")
            }
            Err(e) => tracing::error!(path = %path.display(), uid, error = %e, "list failed"),
        }
        res
    })
    .await
    .map_err(std::io::Error::other)?
}

pub async fn read_file(
    path: &Path,
    uid: u32,
    gid: u32,
    groups: &[u32],
) -> Result<Vec<u8>, std::io::Error> {
    let path = path.to_path_buf();
    let groups = groups.to_vec();
    tokio::task::spawn_blocking(move || {
        let _guard = FsUidGuard::new(uid, gid, &groups);
        let res = std::fs::read(&path);
        match &res {
            Ok(data) => {
                tracing::debug!(path = %path.display(), uid, bytes = data.len(), "read file")
            }
            Err(e) => tracing::error!(path = %path.display(), uid, error = %e, "read failed"),
        }
        res
    })
    .await
    .map_err(std::io::Error::other)?
}

pub async fn write_file(
    path: &Path,
    data: Vec<u8>,
    uid: u32,
    gid: u32,
    groups: &[u32],
) -> Result<(), std::io::Error> {
    let path = path.to_path_buf();
    let groups = groups.to_vec();
    tokio::task::spawn_blocking(move || {
        let _guard = FsUidGuard::new(uid, gid, &groups);
        let res = std::fs::write(&path, &data);
        match &res {
            Ok(()) => {
                tracing::info!(path = %path.display(), uid, bytes = data.len(), "wrote file")
            }
            Err(e) => tracing::error!(path = %path.display(), uid, error = %e, "write failed"),
        }
        res
    })
    .await
    .map_err(std::io::Error::other)?
}

pub async fn delete_path(
    path: &Path,
    uid: u32,
    gid: u32,
    groups: &[u32],
) -> Result<(), std::io::Error> {
    let path = path.to_path_buf();
    let groups = groups.to_vec();
    tokio::task::spawn_blocking(move || {
        let _guard = FsUidGuard::new(uid, gid, &groups);
        match std::fs::metadata(&path) {
            Ok(metadata) => {
                let res = if metadata.is_dir() {
                    std::fs::remove_dir_all(&path)
                } else {
                    std::fs::remove_file(&path)
                };
                match &res {
                    Ok(()) => tracing::info!(path = %path.display(), uid, "deleted path"),
                    Err(e) => {
                        tracing::error!(path = %path.display(), uid, error = %e, "delete failed")
                    }
                }
                res
            }
            // Idempotent: deleting an already-absent path is a success. External
            // clients (e.g. a Mac over a network share) may recreate/remove files
            // like .DS_Store concurrently; we should not report a spurious error.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                tracing::info!(path = %path.display(), uid, "delete: already absent");
                Ok(())
            }
            Err(e) => {
                tracing::error!(path = %path.display(), uid, error = %e, "delete failed (stat)");
                Err(e)
            }
        }
    })
    .await
    .map_err(std::io::Error::other)?
}

pub async fn create_dir(
    path: &Path,
    uid: u32,
    gid: u32,
    groups: &[u32],
) -> Result<(), std::io::Error> {
    let path = path.to_path_buf();
    let groups = groups.to_vec();
    tokio::task::spawn_blocking(move || {
        let _guard = FsUidGuard::new(uid, gid, &groups);
        let res = std::fs::create_dir_all(&path);
        match &res {
            Ok(()) => tracing::info!(path = %path.display(), uid, "created directory"),
            Err(e) => tracing::error!(path = %path.display(), uid, error = %e, "mkdir failed"),
        }
        res
    })
    .await
    .map_err(std::io::Error::other)?
}

/// Returns (size, mtime_millis) for a file. Returns None if the file does not exist.
pub async fn stat_file(
    path: &Path,
    uid: u32,
    gid: u32,
    groups: &[u32],
) -> Result<Option<(u64, i64)>, std::io::Error> {
    let path = path.to_path_buf();
    let groups = groups.to_vec();
    tokio::task::spawn_blocking(move || {
        let _guard = FsUidGuard::new(uid, gid, &groups);
        match std::fs::metadata(&path) {
            Ok(meta) => {
                let mtime = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                Ok(Some((meta.len(), mtime)))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    })
    .await
    .map_err(std::io::Error::other)?
}

/// Rename or move a file/directory from one path to another.
pub async fn rename_path(
    from: &Path,
    to: &Path,
    uid: u32,
    gid: u32,
    groups: &[u32],
) -> Result<(), std::io::Error> {
    let from = from.to_path_buf();
    let to = to.to_path_buf();
    let groups = groups.to_vec();
    tokio::task::spawn_blocking(move || {
        let _guard = FsUidGuard::new(uid, gid, &groups);
        let res = std::fs::rename(&from, &to);
        match &res {
            Ok(()) => {
                tracing::info!(from = %from.display(), to = %to.display(), uid, "renamed path")
            }
            Err(e) => {
                tracing::error!(from = %from.display(), to = %to.display(), uid, error = %e, "rename failed")
            }
        }
        res
    })
    .await
    .map_err(std::io::Error::other)?
}
