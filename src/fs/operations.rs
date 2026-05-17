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
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(&path)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            let modified = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
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
        std::fs::read(&path)
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
        std::fs::write(&path, data)
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
        let metadata = std::fs::metadata(&path)?;
        if metadata.is_dir() {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
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
        std::fs::create_dir_all(&path)
    })
    .await
    .map_err(std::io::Error::other)?
}

/// Returns (size, mtime_secs) for a file. Returns None if the file does not exist.
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
                    .map(|d| d.as_secs() as i64)
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
        std::fs::rename(&from, &to)
    })
    .await
    .map_err(std::io::Error::other)?
}
