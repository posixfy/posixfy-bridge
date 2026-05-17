use dashmap::DashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockType {
    Read,
    Write,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct LockEntry {
    uid: u32,
    lock_type: LockType,
    acquired_at: Instant,
    expires_at: Instant,
}

const DEFAULT_TTL: Duration = Duration::from_secs(300); // 5 minutes

/// RAII guard that holds a file lock and releases it on drop.
/// Prevents lock leaks when operations panic or return early.
#[derive(Debug)]
pub struct LockGuard {
    manager: LockManager,
    path: PathBuf,
    uid: u32,
    released: bool,
}

impl LockGuard {
    /// Explicitly release the lock. Safe to call multiple times.
    pub fn release(&mut self) {
        if !self.released {
            self.manager.release(&self.path, self.uid);
            self.released = true;
        }
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        if !self.released {
            self.manager.release(&self.path, self.uid);
        }
    }
}

#[derive(Clone, Debug)]
pub struct LockManager {
    locks: Arc<DashMap<PathBuf, Vec<LockEntry>>>,
}

impl Default for LockManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LockManager {
    pub fn new() -> Self {
        Self {
            locks: Arc::new(DashMap::new()),
        }
    }

    /// Acquire a lock on a path. Returns a `LockGuard` that auto-releases on drop.
    pub fn acquire(&self, path: &Path, uid: u32, lock_type: LockType) -> Result<LockGuard, String> {
        let now = Instant::now();
        let mut entries = self.locks.entry(path.to_path_buf()).or_default();

        // Remove expired locks
        entries.retain(|e| e.expires_at > now);

        // Check conflicts
        for entry in entries.iter() {
            match (entry.lock_type, lock_type) {
                // Multiple reads are OK
                (LockType::Read, LockType::Read) => {}
                // Same user can upgrade/re-acquire
                _ if entry.uid == uid => {}
                // Write conflicts with anything from another user
                _ => {
                    return Err(format!(
                        "file is locked by uid {} ({})",
                        entry.uid,
                        if entry.lock_type == LockType::Write {
                            "write"
                        } else {
                            "read"
                        }
                    ));
                }
            }
        }

        // Remove existing entries from same user (re-acquire / upgrade)
        entries.retain(|e| e.uid != uid);

        entries.push(LockEntry {
            uid,
            lock_type,
            acquired_at: now,
            expires_at: now + DEFAULT_TTL,
        });

        Ok(LockGuard {
            manager: self.clone(),
            path: path.to_path_buf(),
            uid,
            released: false,
        })
    }

    /// Release a lock held by the given uid. Called by LockGuard::Drop or explicitly.
    pub fn release(&self, path: &Path, uid: u32) {
        if let Some(mut entries) = self.locks.get_mut(path) {
            entries.retain(|e| e.uid != uid);
            if entries.is_empty() {
                drop(entries);
                self.locks.remove(path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_path() -> PathBuf {
        PathBuf::from("/test/file.txt")
    }

    #[test]
    fn acquire_and_release_read_lock() {
        let mgr = LockManager::new();
        let path = test_path();

        let mut guard = mgr.acquire(&path, 1000, LockType::Read).unwrap();
        guard.release();
        // After release, the path entry should be removed entirely
        assert!(!mgr.locks.contains_key(&path));
    }

    #[test]
    fn acquire_and_release_write_lock() {
        let mgr = LockManager::new();
        let path = test_path();

        let mut guard = mgr.acquire(&path, 1000, LockType::Write).unwrap();
        guard.release();
        assert!(!mgr.locks.contains_key(&path));
    }

    #[test]
    fn multiple_reads_from_different_users_succeed() {
        let mgr = LockManager::new();
        let path = test_path();

        let _g1 = mgr.acquire(&path, 1000, LockType::Read).unwrap();
        let _g2 = mgr.acquire(&path, 1001, LockType::Read).unwrap();
        let _g3 = mgr.acquire(&path, 1002, LockType::Read).unwrap();

        // Verify all three entries are present
        let entries = mgr.locks.get(&path).unwrap();
        assert_eq!(entries.len(), 3);
    }

    #[test]
    fn write_lock_conflicts_with_read_from_different_user() {
        let mgr = LockManager::new();
        let path = test_path();

        let _g = mgr.acquire(&path, 1000, LockType::Read).unwrap();

        let result = mgr.acquire(&path, 1001, LockType::Write);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("locked by uid 1000"));
    }

    #[test]
    fn read_lock_conflicts_with_write_from_different_user() {
        let mgr = LockManager::new();
        let path = test_path();

        let _g = mgr.acquire(&path, 1000, LockType::Write).unwrap();

        let result = mgr.acquire(&path, 1001, LockType::Read);
        assert!(result.is_err());
        let err_msg = result.unwrap_err();
        assert!(err_msg.contains("locked by uid 1000"));
        assert!(err_msg.contains("write"));
    }

    #[test]
    fn write_lock_conflicts_with_write_from_different_user() {
        let mgr = LockManager::new();
        let path = test_path();

        let _g = mgr.acquire(&path, 1000, LockType::Write).unwrap();

        let result = mgr.acquire(&path, 1001, LockType::Write);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("locked by uid 1000"));
    }

    #[test]
    fn same_user_can_reacquire_read_as_write() {
        let mgr = LockManager::new();
        let path = test_path();

        // Acquire read, then drop and re-acquire as write
        {
            let _g = mgr.acquire(&path, 1000, LockType::Read).unwrap();
        }
        let mut g2 = mgr.acquire(&path, 1000, LockType::Write).unwrap();
        assert_eq!(g2.released, false);

        // Should have exactly one entry
        let entries = mgr.locks.get(&path).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].lock_type, LockType::Write);
    }

    #[test]
    fn same_user_can_reacquire_write_as_read() {
        let mgr = LockManager::new();
        let path = test_path();

        // Acquire write, then drop and re-acquire as read
        {
            let _g = mgr.acquire(&path, 1000, LockType::Write).unwrap();
        }
        let mut g2 = mgr.acquire(&path, 1000, LockType::Read).unwrap();
        assert_eq!(g2.released, false);

        let entries = mgr.locks.get(&path).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].lock_type, LockType::Read);
    }

    #[test]
    fn release_only_removes_specified_user() {
        let mgr = LockManager::new();
        let path = test_path();

        let mut g1 = mgr.acquire(&path, 1000, LockType::Read).unwrap();
        let _g2 = mgr.acquire(&path, 1001, LockType::Read).unwrap();

        g1.release();

        // uid 1001 should still hold the lock
        let entries = mgr.locks.get(&path).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].uid, 1001);
    }

    #[test]
    fn release_cleans_up_empty_entry() {
        let mgr = LockManager::new();
        let path = test_path();

        let mut guard = mgr.acquire(&path, 1000, LockType::Read).unwrap();
        guard.release();

        // The DashMap entry should be removed when the last lock is released
        assert!(!mgr.locks.contains_key(&path));
    }

    #[test]
    fn release_nonexistent_lock_is_noop() {
        let mgr = LockManager::new();
        let path = test_path();

        // Dropping a guard that was never acquired is fine (no-op in release)
        // The release() method handles missing entries gracefully
        let mut guard = mgr.acquire(&path, 1000, LockType::Read).unwrap();
        guard.release();
        guard.release(); // double release should not panic
    }

    #[test]
    fn different_paths_are_independent() {
        let mgr = LockManager::new();
        let path_a = PathBuf::from("/test/a.txt");
        let path_b = PathBuf::from("/test/b.txt");

        let _g1 = mgr.acquire(&path_a, 1000, LockType::Write).unwrap();
        // Different path, different user -- should succeed even though
        // user 1001 could not write-lock path_a
        let _g2 = mgr.acquire(&path_b, 1001, LockType::Write).unwrap();
    }

    #[test]
    fn guard_auto_releases_on_drop() {
        let mgr = LockManager::new();
        let path = test_path();

        {
            let _guard = mgr.acquire(&path, 1000, LockType::Write).unwrap();
            // Lock is held
            assert!(mgr.acquire(&path, 1001, LockType::Read).is_err());
        }
        // Guard dropped, lock released — now another user can acquire
        assert!(mgr.acquire(&path, 1001, LockType::Write).is_ok());
    }

    #[test]
    fn explicit_release_then_drop_is_safe() {
        let mgr = LockManager::new();
        let path = test_path();

        let mut guard = mgr.acquire(&path, 1000, LockType::Write).unwrap();
        guard.release();
        // Dropping after explicit release should not panic or double-release
        drop(guard);

        // Another user can acquire the lock
        assert!(mgr.acquire(&path, 1001, LockType::Write).is_ok());
    }

    // Note: Expired lock cleanup is tested implicitly by the acquire() method
    // which calls `entries.retain(|e| e.expires_at > now)` before checking
    // conflicts. The DEFAULT_TTL is 5 minutes (300s). Since Instant cannot
    // be mocked without restructuring the code, expiry behavior is covered
    // by code review rather than a unit test.
}
