use std::env;

pub fn get_current_username() -> String {
    env::var("USER")
        .or_else(|_| env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}

/// Linux `/proc/<pid>/stat` start time (field 22, in clock ticks since boot).
/// `None` off-Linux or when the process no longer exists.
pub fn process_start_ticks(pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{}/stat", pid)).ok()?;
    // The comm field is wrapped in parentheses, so split from the last ')'.
    let end = stat.rfind(')')?;
    let mut fields = stat.get(end + 1..)?.split_whitespace();
    let _state = fields.next()?;
    let _ppid = fields.next()?;
    let fields: Vec<_> = fields.collect();
    fields.get(17)?.parse().ok()
}

/// Seconds since the process at `pid` started, or `None` when unavailable.
///
/// Used to tell a long-lived unmanaged GPU process from one that just started.
pub fn process_age_secs(pid: u32) -> Option<u64> {
    let start_ticks = process_start_ticks(pid)?;
    let ticks_per_sec = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if ticks_per_sec <= 0 {
        return None;
    }
    let uptime_secs = system_uptime_secs()?;
    let start_secs = start_ticks / ticks_per_sec as u64;
    Some(uptime_secs.saturating_sub(start_secs))
}

/// System uptime in seconds from `/proc/uptime` (Linux only).
fn system_uptime_secs() -> Option<u64> {
    let content = std::fs::read_to_string("/proc/uptime").ok()?;
    let first = content.split_whitespace().next()?;
    first.parse::<f64>().ok().map(|secs| secs as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_age_is_available_for_the_current_process() {
        // The test process is definitely alive, so age must resolve.
        let age = process_age_secs(std::process::id());
        assert!(age.is_some(), "expected an age for the current process");
    }

    #[test]
    fn process_age_is_none_for_missing_process() {
        // PID 0 is never a normal userspace process we can stat.
        assert!(process_age_secs(u32::MAX).is_none());
    }

    #[test]
    fn process_start_ticks_resolves_for_alive_process() {
        assert!(process_start_ticks(std::process::id()).is_some());
        assert!(process_start_ticks(u32::MAX).is_none());
    }
}
