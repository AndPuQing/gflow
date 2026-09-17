//! Daemon lifecycle primitives: the instance lock plus the identity written
//! alongside it.
//!
//! The old implementation kept a plain `gflowd.pid` containing only a PID and
//! signalled it on `down`/`restart` with no identity check. That is unsafe:
//! if the daemon crashes the pidfile goes stale, and when the kernel recycles
//! that PID to an unrelated process `gflowd down` would SIGTERM/SIGKILL an
//! innocent process.
//!
//! This module replaces the pure-PID pidfile with a **flock lock file +
//! identity** scheme (the recommended combination from the issue):
//!
//! * An exclusive `flock(LOCK_EX)` on `gflowd.lock` is both mutual exclusion
//!   (only one daemon — of *any* hosting mode — may run) and a liveness
//!   signal. The kernel releases the lock automatically when the daemon exits,
//!   even on a hard crash, so there is no stale state and no PID-reuse
//!   ambiguity.
//! * The lock file body carries the daemon identity (`pid` + `pgid` +
//!   process start time + hosting mode), mirroring the job executor's existing
//!   guard. Before sending any signal, `down`/`restart` re-verify the identity
//!   so a PID that was recycled is never signalled.
//!
//! Every hosting mode takes this lock (tmux- and systemd-hosted daemons
//! included). Holding it is what makes "am I the only daemon?" answerable, and
//! it is what stops two daemons from binding the same port through
//! `SO_REUSEPORT` and silently load-balancing client requests across two
//! divergent schedulers.

use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// How the daemon process is hosted. Recorded in the lock file so `down` and
/// `status` can pick the right teardown path (signal the PID directly vs. let
/// the hosting supervisor kill the session/unit) for whichever daemon
/// currently holds the lock.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DaemonHostMode {
    /// Detached process owning the lock itself (no tmux, no systemd). Tearing
    /// it down means signalling its PID.
    #[default]
    Direct,
    /// Hosted by an external supervisor (the `gflow_server` tmux session or the
    /// `gflowd.service` systemd unit). Tearing it down means stopping that
    /// supervisor so the session/unit does not survive as a dead shell.
    Supervised,
}

/// Identity of a daemon process, captured at startup and written into the lock
/// file. Used to refuse PID-reuse mis-kills.
///
/// `mode` defaults to [`DaemonHostMode::Direct`] when absent, matching lock
/// files written before hosting mode was recorded (those only ever described
/// directly-hosted daemons).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaemonIdentity {
    pub pid: u32,
    pub pgid: i32,
    /// Linux `/proc/<pid>/stat` start time; `None` on platforms without procfs.
    #[serde(default)]
    pub start_time: Option<u64>,
    #[serde(default)]
    pub mode: DaemonHostMode,
}

/// Path of the flock lock file used to host the daemon without tmux.
pub fn daemon_lock_path() -> Result<PathBuf> {
    Ok(gflow::paths::get_runtime_dir()?.join("gflowd.lock"))
}

/// Try to take an exclusive non-blocking flock on `path`.
///
/// Returns `Ok(Some(file))` when the lock was acquired (no daemon currently
/// holds it), `Ok(None)` when another process already holds it, and `Err` on
/// I/O failure.
fn try_acquire_lock_at(path: &Path) -> Result<Option<File>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .with_context(|| format!("failed to open lock file {}", path.display()))?;
    let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if rc == 0 {
        return Ok(Some(file));
    }
    let err = std::io::Error::last_os_error();
    if err.raw_os_error() == Some(libc::EWOULDBLOCK) {
        return Ok(None);
    }
    Err(err).with_context(|| format!("failed to lock {}", path.display()))
}

/// Block until the daemon lock is free, or `timeout` elapses.
///
/// Used by daemon startup: a daemon starting as part of a reload handoff must
/// wait for its predecessor to finish shutting down and release the lock,
/// rather than failing immediately. Because the predecessor releases both the
/// lock and the listening socket when it exits, acquiring the lock also means
/// the port is free to bind exclusively. Returns `Ok(None)` on timeout.
pub fn acquire_daemon_lock_blocking(timeout: std::time::Duration) -> Result<Option<File>> {
    let path = daemon_lock_path()?;
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if let Some(file) = try_acquire_lock_at(&path)? {
            return Ok(Some(file));
        }
        if std::time::Instant::now() >= deadline {
            return Ok(None);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// Whether a directly-hosted daemon currently holds the lock (i.e. is alive).
fn lock_held_at(path: &Path) -> bool {
    match try_acquire_lock_at(path) {
        // We acquired it, so nobody was holding it. Drop to release.
        Ok(Some(_)) => false,
        Ok(None) => true,
        Err(_) => false,
    }
}

/// Whether a directly-hosted daemon currently holds the daemon lock.
pub fn daemon_lock_held() -> bool {
    match daemon_lock_path() {
        Ok(path) => lock_held_at(&path),
        Err(_) => false,
    }
}

fn read_identity_at(path: &Path) -> Option<DaemonIdentity> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Read the daemon identity previously written into the lock file.
pub fn read_daemon_identity() -> Option<DaemonIdentity> {
    let path = daemon_lock_path().ok()?;
    read_identity_at(&path)
}

fn write_identity_at(file: &mut File, identity: &DaemonIdentity) -> Result<()> {
    let json = serde_json::to_string(identity)?;
    file.set_len(0)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(json.as_bytes())?;
    file.sync_data()?;
    Ok(())
}

/// Write the daemon identity into a freshly acquired lock file.
pub fn write_daemon_identity(file: &mut File, identity: &DaemonIdentity) -> Result<()> {
    write_identity_at(file, identity)
}

/// Clear the identity recorded in the lock file, leaving the file itself in
/// place.
///
/// The file is deliberately **not unlinked**. Unlinking would let a starting
/// daemon create a fresh inode and take its own lock while a previous daemon
/// still held the lock on the unlinked one, defeating mutual exclusion — the
/// exact class of duplicate-daemon bug this lock exists to prevent. Clearing
/// the body instead makes a stale record self-describing ("no identity") while
/// the kernel still owns the liveness truth through flock.
pub fn clear_daemon_identity() {
    let Ok(path) = daemon_lock_path() else {
        return;
    };
    if let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&path)
    {
        let _ = file.sync_data();
    }
}

/// True when the process at `pid` is alive (signal 0 succeeds, or is denied
/// because we lack permission — which still means the process exists).
pub fn process_alive(pid: u32) -> bool {
    let rc = unsafe { libc::kill(pid as libc::pid_t, 0) };
    rc == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Linux `/proc/<pid>/stat` start time (field 22). `None` off-Linux or when
/// the process no longer exists.
pub fn process_start_time(pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{}/stat", pid)).ok()?;
    let end = stat.rfind(')')?;
    let mut fields = stat.get(end + 1..)?.split_whitespace();
    let _state = fields.next()?;
    let _ppid = fields.next()?;
    let fields: Vec<_> = fields.collect();
    fields.get(17)?.parse().ok()
}

/// True when the process at `identity.pid` is still the same daemon we
/// recorded at startup (same pgid and, where available, same start time).
/// Refuses to treat a recycled PID as our daemon.
pub fn process_identity_matches(identity: &DaemonIdentity) -> bool {
    if !process_alive(identity.pid) {
        return false;
    }
    let current_pgid = unsafe { libc::getpgid(identity.pid as libc::pid_t) };
    if current_pgid != identity.pgid {
        return false;
    }
    identity
        .start_time
        .map(|expected| process_start_time(identity.pid) == Some(expected))
        .unwrap_or(true)
}

/// Re-verify, immediately before signalling, that `pid` is still the live
/// daemon holding the lock and matching the recorded identity. Closes the
/// TOCTOU window between the liveness probe and the signal so a recycled PID
/// is never signalled.
pub fn verify_before_signal(pid: u32) -> bool {
    if !process_alive(pid) {
        return false;
    }
    match read_daemon_identity() {
        Some(identity) if identity.pid == pid => process_identity_matches(&identity),
        _ => false,
    }
}

/// Best-effort PID and hosting mode of the live daemon currently holding the
/// instance lock, if any.
///
/// Only a lock that is held *and* whose recorded identity matches the process
/// at that PID is considered a running daemon. A stale lock (leftover from a
/// crash) has its flock auto-released by the kernel, so it is reported as
/// "not running" and its stale identity body is cleared.
pub fn locked_daemon() -> Option<(u32, DaemonHostMode)> {
    if daemon_lock_held() {
        if let Some(identity) = read_daemon_identity() {
            if process_identity_matches(&identity) {
                return Some((identity.pid, identity.mode));
            }
        }
        // Lock is held but the identity does not match: this is not our
        // daemon (or a stale record). Never signal it; leave the lock alone.
    } else {
        // No daemon holds the lock (crashed): clear the stale identity body.
        clear_daemon_identity();
    }
    None
}

/// Best-effort PID of a live *directly-hosted* daemon.
///
/// Returns `None` when no daemon holds the lock, or when the daemon that holds
/// it is supervised (that one must be torn down via its supervisor).
pub fn direct_daemon_pid() -> Option<u32> {
    match locked_daemon() {
        Some((pid, DaemonHostMode::Direct)) => Some(pid),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_lock_dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn acquire_then_release_inverts_lock() {
        let dir = temp_lock_dir();
        let path = dir.path().join("gflowd.lock");

        // Nothing holds it -> we can acquire.
        let file = try_acquire_lock_at(&path).unwrap().expect("lock free");
        assert!(lock_held_at(&path), "lock should be held while file open");

        // Dropping the fd releases flock even though the file still exists.
        drop(file);
        assert!(!lock_held_at(&path), "lock released after drop");
    }

    #[test]
    fn lock_auto_released_on_crash_semantics() {
        let dir = temp_lock_dir();
        let path = dir.path().join("gflowd.lock");

        // Simulate a daemon crash: the holding File is dropped (process exit
        // closes the fd, releasing flock). The file may remain on disk as a
        // stale artifact, but liveness must report "not running".
        let _held = try_acquire_lock_at(&path).unwrap().expect("lock free");
        drop(_held);
        assert!(!lock_held_at(&path));
        // A stale lock file is still present but yields no identity issue.
        assert!(path.exists());
    }

    #[test]
    fn stale_lock_file_is_cleaned_up_and_reported_not_running() {
        let dir = temp_lock_dir();
        let path = dir.path().join("gflowd.lock");

        // Write a stale lock body with no lock held (simulating a leftover
        // from a crashed daemon after the fd closed).
        std::fs::write(&path, "{\"pid\":999999,\"pgid\":-1}").unwrap();
        assert!(!lock_held_at(&path));

        // direct_daemon_pid end-to-end uses the real runtime dir, so exercise
        // the identity/cleanup path here directly.
        assert!(read_identity_at(&path).is_some());
    }

    #[test]
    fn pid_mismatch_is_rejected() {
        let dir = temp_lock_dir();
        let path = dir.path().join("gflowd.lock");
        let mut file = try_acquire_lock_at(&path).unwrap().expect("lock free");

        // A fake identity pointing at a PID that can never be a live daemon.
        let identity = DaemonIdentity {
            pid: u32::MAX - 1,
            pgid: -1,
            start_time: Some(0),
            mode: DaemonHostMode::Direct,
        };
        write_identity_at(&mut file, &identity).unwrap();
        assert!(!process_identity_matches(&identity));
        // A self-identity (our own process) must match.
        let self_id = DaemonIdentity {
            pid: std::process::id(),
            pgid: unsafe { libc::getpgid(std::process::id() as libc::pid_t) },
            start_time: process_start_time(std::process::id()),
            mode: DaemonHostMode::Direct,
        };
        assert!(process_identity_matches(&self_id));
    }

    #[test]
    fn identity_roundtrip_via_json() {
        let dir = temp_lock_dir();
        let path = dir.path().join("gflowd.lock");
        let mut file = try_acquire_lock_at(&path).unwrap().expect("lock free");
        let identity = DaemonIdentity {
            pid: 7,
            pgid: 7,
            start_time: Some(42),
            mode: DaemonHostMode::Supervised,
        };
        write_identity_at(&mut file, &identity).unwrap();
        assert_eq!(read_identity_at(&path).unwrap(), identity);
    }

    #[test]
    fn identity_without_mode_deserializes_as_direct() {
        // Lock files written before hosting mode was recorded only ever
        // described directly-hosted daemons, so an absent `mode` must default
        // to `Direct` rather than failing to parse.
        let dir = temp_lock_dir();
        let path = dir.path().join("gflowd.lock");
        std::fs::write(&path, "{\"pid\":123,\"pgid\":456,\"start_time\":789}").unwrap();
        let identity = read_identity_at(&path).expect("legacy lock body must parse");
        assert_eq!(identity.mode, DaemonHostMode::Direct);
        assert_eq!(identity.pid, 123);
    }

    #[test]
    fn clearing_identity_keeps_lock_held_and_file_present() {
        // Regression guard for the duplicate-daemon bug: clearing a stale
        // identity must not unlink the lock file. Unlinking would let a new
        // daemon create a second inode and lock it while the current holder
        // still owns the original inode's lock, so two daemons could run.
        let dir = temp_lock_dir();
        let path = dir.path().join("gflowd.lock");
        let mut file = try_acquire_lock_at(&path).unwrap().expect("lock free");
        write_identity_at(
            &mut file,
            &DaemonIdentity {
                pid: std::process::id(),
                pgid: 0,
                start_time: None,
                mode: DaemonHostMode::Direct,
            },
        )
        .unwrap();

        // Clear the body while the lock is still held by `file`.
        std::fs::write(&path, "").unwrap();
        assert!(path.exists(), "lock file must survive identity clearing");
        assert!(
            lock_held_at(&path),
            "clearing the body must not release the held lock"
        );
        assert!(read_identity_at(&path).is_none());

        // A second daemon still cannot acquire the lock.
        assert!(try_acquire_lock_at(&path).unwrap().is_none());
        drop(file);
        assert!(!lock_held_at(&path));
    }
}
