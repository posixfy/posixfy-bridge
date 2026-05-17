use dashmap::DashMap;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

#[derive(Debug, Clone, Serialize)]
pub struct FsEvent {
    pub event_type: String, // "created", "modified", "deleted"
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: i64,
}

#[derive(Debug, Clone)]
struct FileSnapshot {
    is_dir: bool,
    size: u64,
    modified: i64,
}

struct WatchState {
    tx: broadcast::Sender<FsEvent>,
    subscriber_count: Arc<AtomicUsize>,
}

#[derive(Clone)]
pub struct FileWatcher {
    watches: Arc<DashMap<PathBuf, WatchState>>,
}

impl Default for FileWatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl FileWatcher {
    pub fn new() -> Self {
        Self {
            watches: Arc::new(DashMap::new()),
        }
    }

    /// Subscribe to file changes in a directory. Returns a broadcast receiver.
    /// The first subscriber for a directory starts the polling task.
    pub fn subscribe(&self, dir: PathBuf) -> broadcast::Receiver<FsEvent> {
        let entry = self.watches.entry(dir.clone());
        let state = entry.or_insert_with(|| {
            let (tx, _) = broadcast::channel(64);
            let subscriber_count = Arc::new(AtomicUsize::new(0));

            // Spawn polling task
            let watches = Arc::clone(&self.watches);
            let poll_tx = tx.clone();
            let poll_count = Arc::clone(&subscriber_count);
            let poll_dir = dir.clone();

            tokio::spawn(async move {
                poll_directory(poll_dir, poll_tx, poll_count, watches).await;
            });

            WatchState {
                tx,
                subscriber_count,
            }
        });

        state.subscriber_count.fetch_add(1, Ordering::SeqCst);
        state.tx.subscribe()
    }

    /// Unsubscribe from a directory. When subscriber_count reaches 0, the poll task exits.
    pub fn unsubscribe(&self, dir: &PathBuf) {
        if let Some(state) = self.watches.get(dir) {
            let prev = state.subscriber_count.fetch_sub(1, Ordering::SeqCst);
            if prev <= 1 {
                drop(state);
                self.watches.remove(dir);
            }
        }
    }
}

async fn poll_directory(
    dir: PathBuf,
    tx: broadcast::Sender<FsEvent>,
    subscriber_count: Arc<AtomicUsize>,
    watches: Arc<DashMap<PathBuf, WatchState>>,
) {
    let mut snapshot = take_snapshot(&dir);

    loop {
        tokio::time::sleep(Duration::from_secs(3)).await;

        // Stop if no more subscribers
        if subscriber_count.load(Ordering::SeqCst) == 0 {
            watches.remove(&dir);
            break;
        }

        let new_snapshot = take_snapshot(&dir);

        // Detect created entries
        for (name, new_entry) in &new_snapshot {
            if !snapshot.contains_key(name) {
                let _ = tx.send(FsEvent {
                    event_type: "created".to_string(),
                    name: name.clone(),
                    is_dir: new_entry.is_dir,
                    size: new_entry.size,
                    modified: new_entry.modified,
                });
            }
        }

        // Detect modified entries
        for (name, new_entry) in &new_snapshot {
            if let Some(old_entry) = snapshot.get(name) {
                if old_entry.size != new_entry.size || old_entry.modified != new_entry.modified {
                    let _ = tx.send(FsEvent {
                        event_type: "modified".to_string(),
                        name: name.clone(),
                        is_dir: new_entry.is_dir,
                        size: new_entry.size,
                        modified: new_entry.modified,
                    });
                }
            }
        }

        // Detect deleted entries
        for (name, old_entry) in &snapshot {
            if !new_snapshot.contains_key(name) {
                let _ = tx.send(FsEvent {
                    event_type: "deleted".to_string(),
                    name: name.clone(),
                    is_dir: old_entry.is_dir,
                    size: 0,
                    modified: 0,
                });
            }
        }

        snapshot = new_snapshot;
    }
}

fn take_snapshot(dir: &PathBuf) -> HashMap<String, FileSnapshot> {
    let mut map = HashMap::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return map,
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if let Ok(meta) = entry.metadata() {
            let modified = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            map.insert(
                name,
                FileSnapshot {
                    is_dir: meta.is_dir(),
                    size: meta.len(),
                    modified,
                },
            );
        }
    }
    map
}
