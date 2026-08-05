use anyhow::{Context, Result};
use gflow::core::executor::Executor;
use gflow::core::job::{Job, JobState};
use gflow::utils::substitute_parameters;
use std::collections::HashMap;
use std::fs;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long to wait for a SIGTERM'd process group to exit before escalating
/// to SIGKILL.
const TERMINATE_GRACE: Duration = Duration::from_secs(5);

/// A spawned child process tracked by job id.
struct TrackedProcess {
    /// Child pid, which equals the process group id because the child calls
    /// `setsid()` before exec.
    pid: i32,
}

/// Process-backed executor: spawns `bash -c <wrapped>` in its own session
/// (`setsid`) with stdout/stderr redirected to `logs/<job_id>.log`.
///
/// Completion is reported by the wrapped command itself via
/// `gcancel --finish` / `gcancel --fail` (same channel as the tmux executor),
/// so the daemon's state machine is untouched. This executor additionally
/// tracks the child so the canceller can kill the whole process group and the
/// zombie monitor can probe real process liveness.
pub struct ProcessExecutor {
    processes: Arc<Mutex<HashMap<u32, TrackedProcess>>>,
}

impl Default for ProcessExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessExecutor {
    pub fn new() -> Self {
        Self {
            processes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn is_alive(pid: i32) -> bool {
        unsafe { libc::kill(pid, 0) == 0 }
    }

    /// Build the command line executed by the child shell.
    ///
    /// The user command is embedded raw (no key-escaping): it is passed as a
    /// single argv element to `bash -c`, so the shell parses it exactly as the
    /// user wrote it. `&&`/`||` chain the completion reporters onto the exit
    /// code of the user command.
    fn build_wrapped_command(job: &Job) -> Result<String> {
        let mut user_command = String::new();

        if let Some(script) = &job.script {
            if let Some(script_str) = script.to_str() {
                user_command.push_str(&format!("bash {script_str}"));
            }
        } else if let Some(cmd) = &job.command {
            // Apply parameter substitution
            let substituted = substitute_parameters(cmd, &job.parameters)?;
            user_command.push_str(&substituted);
        } else {
            anyhow::bail!("Job {} has neither a script nor a command", job.id);
        }

        if let Some(conda_env) = &job.conda_env {
            user_command = format!("conda activate {conda_env} && {user_command}");
        }

        Ok(format!(
            "{user_command} && gcancel --finish {job_id} || gcancel --fail {job_id}",
            job_id = job.id,
        ))
    }
}

#[cfg(unix)]
fn detach_with_setsid() -> Result<(), std::io::Error> {
    // SAFETY: setsid() is async-signal-safe and runs in the freshly forked
    // single-threaded child before exec.
    if unsafe { libc::setsid() } == -1 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

impl Executor for ProcessExecutor {
    fn kind(&self) -> &'static str {
        "process"
    }

    fn execute(&self, job: &Job) -> Result<()> {
        let wrapped_command = Self::build_wrapped_command(job)?;

        let log_path = gflow::paths::prepare_log_file_path(job.id)?;
        if let Some(parent) = log_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let log_file = fs::File::create(&log_path)
            .with_context(|| format!("Failed to create log file {}", log_path.display()))?;
        let stderr_file = log_file
            .try_clone()
            .context("Failed to clone log file handle")?;

        let mut command = Command::new("bash");
        command
            .arg("-c")
            .arg(&wrapped_command)
            .current_dir(&job.run_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::from(log_file))
            .stderr(Stdio::from(stderr_file))
            .env("GFLOW_ARRAY_TASK_ID", job.task_id.unwrap_or(0).to_string());

        if let Some(gpu_ids) = &job.gpu_ids {
            command.env(
                "CUDA_VISIBLE_DEVICES",
                gpu_ids
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(","),
            );
        }

        // Detach the child into its own session/process group so the whole
        // group can be signalled with kill(-pgid, ...).
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            unsafe {
                command.pre_exec(detach_with_setsid);
            }
        }

        let mut child = command.spawn().with_context(|| {
            format!(
                "Failed to spawn job {}: bash -c {:?}",
                job.id, wrapped_command
            )
        })?;
        let pid = child.id() as i32;
        let pgid = pid; // setsid() makes the child a session leader

        self.processes
            .lock()
            .unwrap()
            .insert(job.id, TrackedProcess { pid: pgid });

        // Reap the child in the background and drop the registry entry once it
        // exits. The wrapper's `gcancel --finish/--fail` reports the final job
        // state; the zombie monitor uses the registry to detect jobs whose
        // process died without reporting.
        let processes = Arc::clone(&self.processes);
        let job_id = job.id;
        std::thread::spawn(move || {
            let _wait_result = child.wait();
            let mut registry = processes.lock().unwrap();
            registry.remove(&job_id);
        });

        tracing::info!(job_id = job.id, pid, "Spawned job process group");
        Ok(())
    }

    fn terminate(&self, job_id: u32, _run_name: Option<&str>) -> Result<()> {
        let pid = match self.processes.lock().unwrap().get(&job_id) {
            Some(process) => process.pid,
            None => return Ok(()), // nothing tracked (already finished/reaped)
        };

        // SIGTERM to the whole process group, then escalate to SIGKILL after a
        // grace period in the background (never blocks the scheduler).
        let rc = unsafe { libc::kill(-pid, libc::SIGTERM) };
        if rc != 0 {
            let err = std::io::Error::last_os_error();
            // ESRCH: the group is already gone. EPERM: on some platforms
            // (macOS) signalling a dead or foreign process group returns EPERM
            // instead of ESRCH; either way there is nothing we own left to
            // terminate (and we must not touch a reused pid's group).
            if matches!(err.raw_os_error(), Some(libc::ESRCH) | Some(libc::EPERM)) {
                return Ok(());
            }
            return Err(err.into());
        }

        std::thread::spawn(move || {
            let deadline = Instant::now() + TERMINATE_GRACE;
            while Instant::now() < deadline {
                if !Self::is_alive(pid) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            tracing::warn!(pid, "Process group ignored SIGTERM, sending SIGKILL");
            unsafe {
                libc::kill(-pid, libc::SIGKILL);
            }
        });
        Ok(())
    }

    fn is_running(&self, job_id: u32, _run_name: Option<&str>) -> bool {
        let Some(pid) = self
            .processes
            .lock()
            .unwrap()
            .get(&job_id)
            .map(|process| process.pid)
        else {
            return false;
        };
        Self::is_alive(pid)
    }

    fn shutdown(&self) {
        let pids: Vec<i32> = self
            .processes
            .lock()
            .unwrap()
            .values()
            .map(|process| process.pid)
            .collect();
        if pids.is_empty() {
            return;
        }

        tracing::info!(processes = pids.len(), "Terminating managed job processes");
        for pid in &pids {
            unsafe {
                libc::kill(-*pid, libc::SIGTERM);
            }
        }

        let deadline = Instant::now() + TERMINATE_GRACE;
        while Instant::now() < deadline {
            if pids.iter().all(|pid| !Self::is_alive(*pid)) {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        for pid in pids {
            tracing::warn!(pid, "Process group survived SIGTERM, sending SIGKILL");
            unsafe {
                libc::kill(-pid, libc::SIGKILL);
            }
        }
    }
}

/// Legacy tmux-based executor: creates a detached tmux session per job and
/// injects the command via send-keys. Kept for `gjob attach` / interactive
/// inspection; opt-in via `[executor] type = "tmux"`.
pub struct TmuxExecutor;

impl TmuxExecutor {
    fn generate_wrapped_command(&self, job: &Job) -> Result<String> {
        let mut user_command = String::new();

        if let Some(script) = &job.script {
            if let Some(script_str) = script.to_str() {
                user_command.push_str(&format!("bash {script_str}"));
            }
        } else if let Some(cmd) = &job.command {
            // Apply parameter substitution
            let substituted = substitute_parameters(cmd, &job.parameters)?;
            user_command.push_str(&substituted);
        } else {
            anyhow::bail!("Job {} has neither a script nor a command", job.id);
        }

        // Wrap the command in bash -c to ensure && and || operators work
        // regardless of the user's default shell (fish, zsh, etc.).
        // The command is typed into a terminal via tmux send-keys, so it must
        // be escaped for the surrounding shell: backslash, double-quote,
        // dollar sign, backtick.
        let escaped_command = user_command
            .replace('\\', r"\\")
            .replace('"', r#"\""#)
            .replace('$', r"\$")
            .replace('`', r"\`");
        let wrapped_command = format!(
            r#"bash -c "{escaped_command} && gcancel --finish {job_id} || gcancel --fail {job_id}""#,
            job_id = job.id,
        );
        Ok(wrapped_command)
    }
}

impl Executor for TmuxExecutor {
    fn kind(&self) -> &'static str {
        "tmux"
    }

    fn execute(&self, job: &Job) -> Result<()> {
        if let Some(session_name) = job.run_name.as_ref() {
            let session = gflow::tmux::TmuxSession::create(session_name.to_string())?;

            // Enable pipe-pane to capture output to log file
            let log_path = gflow::paths::prepare_log_file_path(job.id)?;
            if let Some(parent) = log_path.parent() {
                fs::create_dir_all(parent)?;
            }
            session.enable_pipe_pane(&log_path)?;

            session.try_send_command(&format!("cd {}", job.run_dir.display()))?;
            session.try_send_command(&format!(
                "export GFLOW_ARRAY_TASK_ID={}",
                job.task_id.unwrap_or(0)
            ))?;
            if let Some(gpu_ids) = &job.gpu_ids {
                session.try_send_command(&format!(
                    "export CUDA_VISIBLE_DEVICES={}",
                    gpu_ids
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(",")
                ))?;
            }

            if let Some(conda_env) = &job.conda_env {
                session.try_send_command(&format!("conda activate {conda_env}"))?;
            }

            let wrapped_command = self.generate_wrapped_command(job)?;
            session.try_send_command(&wrapped_command)?;
        }
        Ok(())
    }

    fn terminate(&self, _job_id: u32, run_name: Option<&str>) -> Result<()> {
        if let Some(name) = run_name {
            gflow::tmux::send_ctrl_c(name)?;
        }
        Ok(())
    }

    fn is_running(&self, _job_id: u32, run_name: Option<&str>) -> bool {
        run_name.map(gflow::tmux::is_session_exist).unwrap_or(false)
    }

    fn cleanup(&self, job: &Job) {
        let Some(run_name) = job.run_name.as_ref() else {
            return;
        };
        if job.state == JobState::Finished {
            if job.auto_close_tmux {
                // Close the session (also disables pipe-pane).
                if let Err(e) = gflow::tmux::kill_session(run_name) {
                    tracing::warn!("Failed to auto-close tmux session '{}': {}", run_name, e);
                }
            } else {
                // Keep the session alive for user inspection, stop pipe-pane.
                gflow::tmux::disable_pipe_pane_for_job(job.id, run_name, false);
            }
        } else {
            // Failed / Cancelled / Timeout: keep the session, stop pipe-pane.
            gflow::tmux::disable_pipe_pane_for_job(job.id, run_name, false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gflow::core::job::JobState;
    use std::path::PathBuf;

    fn job_with_command(id: u32, command: &str) -> Job {
        Job {
            id,
            command: Some(command.into()),
            state: JobState::Queued,
            run_dir: PathBuf::from("/tmp"),
            ..Default::default()
        }
    }

    #[test]
    fn test_process_wrapped_command_basic() {
        let job = job_with_command(123, "echo hello");
        let wrapped = ProcessExecutor::build_wrapped_command(&job).unwrap();
        assert_eq!(
            wrapped,
            r#"echo hello && gcancel --finish 123 || gcancel --fail 123"#
        );
    }

    #[test]
    fn test_process_wrapped_command_keeps_quotes_unescaped() {
        // No key-escaping: quotes reach `bash -c` verbatim.
        let job = job_with_command(456, r#"echo "hello world""#);
        let wrapped = ProcessExecutor::build_wrapped_command(&job).unwrap();
        assert_eq!(
            wrapped,
            r#"echo "hello world" && gcancel --finish 456 || gcancel --fail 456"#
        );
    }

    #[test]
    fn test_process_wrapped_command_keeps_dollar_and_backtick_unescaped() {
        let job = job_with_command(200, "echo $HOME `date`");
        let wrapped = ProcessExecutor::build_wrapped_command(&job).unwrap();
        assert_eq!(
            wrapped,
            r#"echo $HOME `date` && gcancel --finish 200 || gcancel --fail 200"#
        );
    }

    #[test]
    fn test_process_wrapped_command_with_script() {
        let job = Job {
            id: 789,
            script: Some(Box::new(PathBuf::from("/tmp/script.sh"))),
            state: JobState::Queued,
            run_dir: PathBuf::from("/tmp"),
            ..Default::default()
        };
        let wrapped = ProcessExecutor::build_wrapped_command(&job).unwrap();
        assert_eq!(
            wrapped,
            r#"bash /tmp/script.sh && gcancel --finish 789 || gcancel --fail 789"#
        );
    }

    #[test]
    fn test_process_wrapped_command_with_conda_env() {
        let job = Job {
            id: 321,
            command: Some("python train.py".into()),
            conda_env: Some("ml".into()),
            state: JobState::Queued,
            run_dir: PathBuf::from("/tmp"),
            ..Default::default()
        };
        let wrapped = ProcessExecutor::build_wrapped_command(&job).unwrap();
        assert_eq!(
            wrapped,
            r#"conda activate ml && python train.py && gcancel --finish 321 || gcancel --fail 321"#
        );
    }

    #[test]
    fn test_process_wrapped_command_rejects_empty_job() {
        let job = Job {
            id: 1,
            state: JobState::Queued,
            run_dir: PathBuf::from("/tmp"),
            ..Default::default()
        };
        assert!(ProcessExecutor::build_wrapped_command(&job).is_err());
    }

    #[test]
    fn test_tmux_wrapped_command_still_escapes_for_terminal_injection() {
        let executor = TmuxExecutor;
        let job = job_with_command(100, r#"echo "hello world""#);
        let wrapped = executor.generate_wrapped_command(&job).unwrap();
        assert_eq!(
            wrapped,
            r#"bash -c "echo \"hello world\" && gcancel --finish 100 || gcancel --fail 100""#
        );
    }

    // ── live process tests (Linux) ─────────────────────────────────────────

    /// Serializes live-process tests and redirects the data dir to a temp dir
    /// so stray job logs don't pollute the real XDG data home.
    static LIVE_PROCESS_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn with_isolated_data_dir<T>(f: impl FnOnce() -> T) -> T {
        let _guard = LIVE_PROCESS_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let tempdir = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_DATA_HOME", tempdir.path());
        let result = f();
        std::env::remove_var("XDG_DATA_HOME");
        result
    }

    #[test]
    fn test_process_executor_tracks_and_terminates_process_group() {
        with_isolated_data_dir(|| {
            let executor = ProcessExecutor::new();
            let job = job_with_command(9001, "sleep 30");

            executor.execute(&job).unwrap();
            assert!(executor.is_running(9001, None));

            // The child must be a session leader so its pid == pgid.
            let pid = executor.processes.lock().unwrap().get(&9001).unwrap().pid;
            let pgid = unsafe { libc::getpgid(pid) };
            assert_eq!(pgid, pid, "child should be its own process group leader");

            executor.terminate(9001, None).unwrap();

            // Wait for the reaper to reap the child and drop the registry entry
            // (the definitive "process is gone" signal).
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            while std::time::Instant::now() < deadline {
                if !executor.processes.lock().unwrap().contains_key(&9001) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            assert!(
                !executor.processes.lock().unwrap().contains_key(&9001),
                "registry entry should be removed after the child exits"
            );
            assert!(!executor.is_running(9001, None));
        })
    }

    #[test]
    fn test_process_executor_sigkill_after_grace() {
        with_isolated_data_dir(|| {
            let executor = ProcessExecutor::new();
            // `trap '' TERM` makes the process ignore SIGTERM, forcing SIGKILL.
            // `echo READY` after the trap guarantees the trap is installed before
            // the test sends SIGTERM (otherwise the signal could arrive first).
            let job = job_with_command(9002, "trap '' TERM; echo READY; sleep 30");
            executor.execute(&job).unwrap();

            let log_path = gflow::paths::get_log_file_path(9002).unwrap();
            let ready_deadline = std::time::Instant::now() + Duration::from_secs(5);
            loop {
                if std::fs::read_to_string(&log_path)
                    .map(|content| content.contains("READY"))
                    .unwrap_or(false)
                {
                    break;
                }
                assert!(
                    std::time::Instant::now() < ready_deadline,
                    "job never became ready (trap not installed)"
                );
                std::thread::sleep(Duration::from_millis(50));
            }
            assert!(executor.is_running(9002, None));

            executor.terminate(9002, None).unwrap();

            let deadline = std::time::Instant::now() + Duration::from_secs(15);
            while std::time::Instant::now() < deadline {
                if !executor.is_running(9002, None) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            assert!(
                !executor.is_running(9002, None),
                "process ignoring SIGTERM should be SIGKILLed after the grace period"
            );
        })
    }

    #[test]
    fn test_process_executor_terminate_is_idempotent() {
        with_isolated_data_dir(|| {
            let executor = ProcessExecutor::new();
            let job = job_with_command(9003, "sleep 30");
            executor.execute(&job).unwrap();
            executor.terminate(9003, None).unwrap();

            // Second terminate on the same job: registry entry may be gone or the
            // group may already be dead; either way it must not error.
            executor.terminate(9003, None).unwrap();
            executor.terminate(9003, None).unwrap();
            executor.terminate(99999, None).unwrap(); // unknown job

            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            while std::time::Instant::now() < deadline {
                if !executor.is_running(9003, None) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            assert!(!executor.is_running(9003, None));
        })
    }

    #[test]
    fn test_process_executor_shutdown_kills_all() {
        with_isolated_data_dir(|| {
            let executor = ProcessExecutor::new();
            executor
                .execute(&job_with_command(9101, "sleep 30"))
                .unwrap();
            executor
                .execute(&job_with_command(9102, "sleep 30"))
                .unwrap();
            assert!(executor.is_running(9101, None));
            assert!(executor.is_running(9102, None));

            executor.shutdown();

            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            while std::time::Instant::now() < deadline {
                if !executor.is_running(9101, None) && !executor.is_running(9102, None) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            assert!(!executor.is_running(9101, None));
            assert!(!executor.is_running(9102, None));
        })
    }
}
