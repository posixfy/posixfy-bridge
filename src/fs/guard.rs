/// RAII guard for setfsuid/setfsgid/setgroups on Linux.
/// On non-Linux platforms, this is a no-op.
#[cfg(target_os = "linux")]
pub struct FsUidGuard {
    prev_uid: u32,
    prev_gid: u32,
    prev_groups: Vec<nix::unistd::Gid>,
}

#[cfg(target_os = "linux")]
impl FsUidGuard {
    pub fn new(uid: u32, gid: u32, groups: &[u32]) -> Self {
        use nix::unistd::{getgroups, setfsgid, setfsuid, setgroups, Gid, Uid};

        let prev_uid = setfsuid(Uid::from_raw(uid)).as_raw();
        let prev_gid = setfsgid(Gid::from_raw(gid)).as_raw();
        let prev_groups = getgroups().unwrap_or_default();

        if !groups.is_empty() {
            let new_groups: Vec<Gid> = groups.iter().map(|&g| Gid::from_raw(g)).collect();
            let _ = setgroups(&new_groups);
        }

        Self {
            prev_uid,
            prev_gid,
            prev_groups,
        }
    }
}

#[cfg(target_os = "linux")]
impl Drop for FsUidGuard {
    fn drop(&mut self) {
        use nix::unistd::{setfsgid, setfsuid, setgroups, Gid, Uid};
        setfsuid(Uid::from_raw(self.prev_uid));
        setfsgid(Gid::from_raw(self.prev_gid));
        let _ = setgroups(&self.prev_groups);
    }
}

#[cfg(not(target_os = "linux"))]
pub struct FsUidGuard;

#[cfg(not(target_os = "linux"))]
impl FsUidGuard {
    pub fn new(_uid: u32, _gid: u32, _groups: &[u32]) -> Self {
        tracing::warn!("FsUidGuard is a no-op on non-Linux platforms");
        Self
    }
}
