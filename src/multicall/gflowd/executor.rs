use anyhow::{Context, Result};
use gflow::core::executor::{ExecutionResult, ExecutionStatus, Executor};
use gflow::core::job::{Job, JobState};
use gflow::utils::substitute_parameters;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long to wait for a SIGTERM'd process group to exit before escalating
/// to SIGKILL.
const TERMINATE_GRACE: Duration = Duration::from_secs(5);
const RUNNER_METADATA_VERSION: u32 = 1;

/// A spawned runner tracked locally for child reaping. The durable metadata is
/// the source of truth used by a fresh daemon instance.
struct TrackedProcess {
    /// Runner pid, which equals the process group id because the runner calls
    /// `setsid()` before exec.
    pid: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RunnerMetadata {
    version: u32,
    job_id: u32,
    pid: i32,
    pgid: i32,
    /// Linux `/proc/<pid>/stat` start time. `None` on platforms without procfs.
    #[serde(default)]
    start_time: Option<u64>,
    result_path: std::path::PathBuf,
}

#[derive(Debug, Deserialize)]
struct RunnerResultFile {
    job_id: u32,
    exit_code: i32,
    #[serde(default)]
    signal: Option<i32>,
}

/// Process-backed executor. Each job is owned by an independent shell runner
/// in its own session. The runner writes an atomic result file after the
/// payload exits, so a new daemon can adopt the process or collect its result.
pub struct ProcessExecutor {
    /// Kept for child reaping while this daemon is alive. Re-adoption never
    /// relies on this registry; it reads the durable runner metadata instead.
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
        let rc = unsafe { libc::kill(pid, 0) };
        rc == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }

    fn group_is_alive(pgid: i32) -> bool {
        let rc = unsafe { libc::kill(-pgid, 0) };
        rc == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }

    fn process_start_time(pid: i32) -> Option<u64> {
        let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let end = stat.rfind(')')?;
        let mut fields = stat.get(end + 1..)?.split_whitespace();
        let _state = fields.next()?;
        let _ppid = fields.next()?;
        let fields: Vec<_> = fields.collect();
        fields.get(17)?.parse().ok()
    }

    fn process_identity_matches(metadata: &RunnerMetadata) -> bool {
        if !Self::is_alive(metadata.pid) {
            return false;
        }

        let current_pgid = unsafe { libc::getpgid(metadata.pid) };
        if current_pgid != metadata.pgid {
            return false;
        }

        metadata
            .start_time
            .map(|expected| Self::process_start_time(metadata.pid) == Some(expected))
            .unwrap_or(true)
    }

    fn write_json_atomic<T: Serialize>(path: &std::path::Path, value: &T) -> Result<()> {
        let parent = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("JSON path has no parent: {}", path.display()))?;
        fs::create_dir_all(parent)?;
        let tmp_path = parent.join(format!(
            ".{}.tmp.{}",
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("runner"),
            std::process::id()
        ));
        let bytes = serde_json::to_vec(value)?;
        let mut file = fs::File::create(&tmp_path)?;
        file.write_all(&bytes)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::rename(&tmp_path, path)?;
        Ok(())
    }

    fn metadata(job_id: u32) -> Result<RunnerMetadata> {
        let path = gflow::paths::get_runner_metadata_path(job_id)?;
        let bytes = fs::read(path)?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    fn result(job_id: u32) -> Option<ExecutionResult> {
        let path = gflow::paths::get_runner_result_path(job_id).ok()?;
        let bytes = fs::read(path).ok()?;
        let result = match serde_json::from_slice::<RunnerResultFile>(&bytes) {
            Ok(result) => result,
            Err(error) => {
                tracing::warn!(job_id, %error, "Ignoring incomplete runner result file");
                return None;
            }
        };
        if result.job_id != job_id {
            tracing::warn!(
                expected_job_id = job_id,
                result_job_id = result.job_id,
                "Ignoring runner result for a different job"
            );
            return None;
        }
        Some(ExecutionResult {
            job_id,
            exit_code: Some(result.exit_code),
            signal: result.signal,
        })
    }

    fn remove_runner_files(job_id: u32) {
        for path in [
            gflow::paths::get_runner_metadata_path(job_id).ok(),
            gflow::paths::get_runner_result_path(job_id).ok(),
        ]
        .into_iter()
        .flatten()
        {
            let _ = fs::remove_file(path);
        }
    }

    fn status_from_metadata(&self, job_id: u32) -> ExecutionStatus {
        if let Some(result) = Self::result(job_id) {
            return ExecutionStatus::Finished(result);
        }

        let Ok(metadata) = Self::metadata(job_id) else {
            return self
                .processes
                .lock()
                .unwrap()
                .get(&job_id)
                .filter(|process| Self::is_alive(process.pid))
                .map(|_| ExecutionStatus::Running)
                .unwrap_or(ExecutionStatus::Missing);
        };

        if metadata.version != RUNNER_METADATA_VERSION || metadata.job_id != job_id {
            tracing::warn!(
                job_id,
                "Ignoring runner metadata with an incompatible identity"
            );
            return ExecutionStatus::Missing;
        }

        if Self::process_identity_matches(&metadata) {
            ExecutionStatus::Running
        } else {
            ExecutionStatus::Missing
        }
    }

    fn build_user_command(job: &Job) -> Result<String> {
        let mut user_command = String::new();

        if let Some(script) = &job.script {
            if let Some(script_str) = script.to_str() {
                user_command.push_str(&format!("bash {}", shell_escape::escape(script_str.into())));
            }
        } else if let Some(cmd) = &job.command {
            let substituted = substitute_parameters(cmd, &job.parameters)?;
            user_command.push_str(&substituted);
        } else {
            anyhow::bail!("Job {} has neither a script nor a command", job.id);
        }

        if let Some(conda_env) = &job.conda_env {
            user_command = format!(
                "{} && {}",
                conda_activation_command(job, conda_env)?,
                user_command
            );
        }

        Ok(user_command)
    }

    /// Build the detached runner shell. The payload is executed by a nested
    /// bash so `exit` in user input cannot skip the result write performed by
    /// the outer runner.
    fn build_runner_command(job: &Job, result_path: &std::path::Path) -> Result<String> {
        let user_command = Self::build_user_command(job)?;
        let payload = shell_escape::escape(user_command.into());
        let result = shell_escape::escape(result_path.to_string_lossy());

        Ok(format!(
            "bash -c {payload}\n\
             status=$?\n\
             finished_at=$(date +%s 2>/dev/null || printf 0)\n\
             tmp={result}.tmp.$$\n\
             printf '{{\"version\":1,\"job_id\":{job_id},\"exit_code\":%s,\"signal\":null,\"finished_at_unix_secs\":%s}}\\n' \"$status\" \"$finished_at\" > \"$tmp\" && mv -f \"$tmp\" {result}\n\
             exit \"$status\"",
            job_id = job.id,
        ))
    }
}

/// Build the shell fragment that activates `conda_env` in the job shell.
///
/// The process runner is a non-interactive, non-login `bash -c`, so it never
/// loads the user's rc files. The tmux pane may not have conda initialized
/// either. Source conda's init script explicitly so `conda activate` is
/// available in the shell that runs the payload.
fn conda_activation_command(job: &Job, conda_env: &str) -> Result<String> {
    let root = locate_conda_root().with_context(|| {
        format!(
            "Job {} needs conda environment '{}' but no conda installation was found. \
             gflowd checked $CONDA_EXE, $PATH, $CONDA_PREFIX, and common install locations \
             ($HOME/miniconda3, $HOME/anaconda3, /opt/conda, ...)",
            job.id, conda_env
        )
    })?;
    let init_script = root.join("etc/profile.d/conda.sh");
    Ok(format!(
        "source {} && conda activate {}",
        shell_escape::escape(init_script.to_string_lossy().into_owned().into()),
        shell_escape::escape(conda_env.into())
    ))
}

/// Locate the root of a conda installation, i.e. the directory containing
/// `etc/profile.d/conda.sh`. Detection uses the daemon's inherited
/// environment because the job may start after the submitting shell exits.
/// The order keeps explicit runtime hints ahead of filesystem fallbacks.
#[cfg(not(windows))]
fn locate_conda_root() -> Option<PathBuf> {
    locate_conda_root_impl(":")
}

#[cfg(windows)]
fn locate_conda_root() -> Option<PathBuf> {
    locate_conda_root_impl(";")
}

fn locate_conda_root_impl(path_sep: &str) -> Option<PathBuf> {
    fn has_conda_init(root: &Path) -> bool {
        root.join("etc/profile.d/conda.sh").is_file()
    }

    /// Validate a candidate and return its canonical path. This also handles
    /// platform aliases such as macOS `/var` -> `/private/var`.
    fn accept(root: &Path) -> Option<PathBuf> {
        has_conda_init(root).then(|| fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf()))
    }

    /// Standard installs expose either `<root>/bin/conda` or the
    /// `<root>/condabin/conda` shell shim. Resolve symlinks before inspecting
    /// the parent so links such as `/usr/bin/conda` still work.
    fn root_of_exe(exe: &Path) -> Option<PathBuf> {
        let resolved = fs::canonicalize(exe).unwrap_or_else(|_| exe.to_path_buf());
        let bin = resolved.parent()?;
        let bin_name = bin.file_name()?.to_str()?;
        if bin_name != "bin" && bin_name != "condabin" {
            return None;
        }
        Some(bin.parent()?.to_path_buf())
    }

    if let Ok(exe) = env::var("CONDA_EXE") {
        if let Some(root) = root_of_exe(Path::new(&exe)) {
            if let Some(root) = accept(&root) {
                return Some(root);
            }
        }
    }

    if let Ok(paths) = env::var("PATH") {
        for dir in paths.split(path_sep) {
            if dir.is_empty() {
                continue;
            }
            let exe = Path::new(dir).join("conda");
            if let Some(root) = root_of_exe(&exe) {
                if let Some(root) = accept(&root) {
                    return Some(root);
                }
            }
        }
    }

    if let Ok(prefix) = env::var("CONDA_PREFIX") {
        let mut candidate = PathBuf::from(prefix);
        for _ in 0..3 {
            if let Some(root) = accept(&candidate) {
                return Some(root);
            }
            match candidate.parent() {
                Some(parent) => candidate = parent.to_path_buf(),
                None => break,
            }
        }
    }

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(home) = env::var("HOME") {
        for name in [
            "miniconda3",
            "miniforge3",
            "anaconda3",
            "mambaforge",
            "conda",
        ] {
            candidates.push(Path::new(&home).join(name));
        }
    }
    for base in ["/opt", "/usr/local", "/usr/share"] {
        for name in ["conda", "miniconda3", "miniforge3", "anaconda3"] {
            candidates.push(Path::new(base).join(name));
        }
    }
    candidates.iter().find_map(|candidate| accept(candidate))
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
        let result_path = gflow::paths::get_runner_result_path(job.id)?;
        let log_path = gflow::paths::prepare_log_file_path(job.id)?;
        if let Some(parent) = log_path.parent() {
            fs::create_dir_all(parent)?;
        }
        // Keep command-construction failures visible in the job log. In
        // particular, a missing conda installation is detected before a
        // runner can be spawned.
        let mut log_file = fs::File::create(&log_path)
            .with_context(|| format!("Failed to create log file {}", log_path.display()))?;

        let runner_command = match Self::build_runner_command(job, &result_path) {
            Ok(command) => command,
            Err(error) => {
                let _ = writeln!(
                    log_file,
                    "gflow: failed to prepare job {} for execution: {error:#}",
                    job.id
                );
                return Err(error);
            }
        };
        Self::remove_runner_files(job.id);

        let stderr_file = log_file
            .try_clone()
            .context("Failed to clone log file handle")?;

        let mut command = Command::new("bash");
        command
            .arg("-c")
            .arg(&runner_command)
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

        // Detach the runner into its own session/process group so it survives
        // daemon exit and the whole group can be signalled with kill(-pgid, ...).
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            unsafe {
                command.pre_exec(detach_with_setsid);
            }
        }

        let mut child = command.spawn().with_context(|| {
            format!(
                "Failed to spawn runner for job {}: bash -c {:?}",
                job.id, runner_command
            )
        })?;
        let pid = child.id() as i32;
        let pgid = pid;
        let metadata = RunnerMetadata {
            version: RUNNER_METADATA_VERSION,
            job_id: job.id,
            pid,
            pgid,
            start_time: Self::process_start_time(pid),
            result_path,
        };

        if let Err(error) =
            Self::write_json_atomic(&gflow::paths::get_runner_metadata_path(job.id)?, &metadata)
        {
            unsafe {
                libc::kill(-pgid, libc::SIGKILL);
            }
            let _ = child.wait();
            return Err(error).context("Failed to persist runner metadata");
        }

        self.processes
            .lock()
            .unwrap()
            .insert(job.id, TrackedProcess { pid: pgid });

        // Reap the runner while this daemon is alive. The result file remains
        // durable until the scheduler transitions the job and calls cleanup.
        let processes = Arc::clone(&self.processes);
        let job_id = job.id;
        std::thread::spawn(move || {
            let _wait_result = child.wait();
            let mut registry = processes.lock().unwrap();
            registry.remove(&job_id);
        });

        tracing::info!(job_id = job.id, pid, "Spawned durable job runner");
        Ok(())
    }

    fn execution_status(&self, job_id: u32, _run_name: Option<&str>) -> ExecutionStatus {
        self.status_from_metadata(job_id)
    }

    fn collect_finished(&self) -> Vec<ExecutionResult> {
        let Ok(dir) = gflow::paths::get_runner_dir() else {
            return Vec::new();
        };
        let Ok(entries) = fs::read_dir(dir) else {
            return Vec::new();
        };

        entries
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("json")
                    || path.file_name()?.to_str()?.contains(".result.")
                {
                    return None;
                }
                let metadata =
                    serde_json::from_slice::<RunnerMetadata>(&fs::read(path).ok()?).ok()?;
                let result = Self::result(metadata.job_id)?;
                Some(result)
            })
            .collect()
    }

    fn terminate(&self, job_id: u32, _run_name: Option<&str>) -> Result<()> {
        let metadata = Self::metadata(job_id).ok();
        let (pid, pgid) = metadata
            .as_ref()
            .map(|metadata| (metadata.pid, metadata.pgid))
            .or_else(|| {
                self.processes
                    .lock()
                    .unwrap()
                    .get(&job_id)
                    .map(|process| (process.pid, process.pid))
            })
            .unwrap_or((0, 0));
        if pid == 0 || pgid == 0 {
            return Ok(());
        }
        if let Some(metadata) = metadata.as_ref() {
            if !Self::process_identity_matches(metadata) {
                return Ok(());
            }
        }

        // SIGTERM to the whole process group, then escalate to SIGKILL after a
        // grace period in the background (never blocks the scheduler).
        let rc = unsafe { libc::kill(-pgid, libc::SIGTERM) };
        if rc != 0 {
            let err = std::io::Error::last_os_error();
            if matches!(err.raw_os_error(), Some(libc::ESRCH) | Some(libc::EPERM)) {
                return Ok(());
            }
            return Err(err.into());
        }

        std::thread::spawn(move || {
            let deadline = Instant::now() + TERMINATE_GRACE;
            while Instant::now() < deadline {
                if !Self::group_is_alive(pgid) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            tracing::warn!(pid, pgid, "Process group ignored SIGTERM, sending SIGKILL");
            unsafe {
                libc::kill(-pgid, libc::SIGKILL);
            }
        });
        Ok(())
    }

    fn is_running(&self, job_id: u32, run_name: Option<&str>) -> bool {
        matches!(
            self.execution_status(job_id, run_name),
            ExecutionStatus::Running
        )
    }

    fn cleanup(&self, job: &Job) {
        Self::remove_runner_files(job.id);
        if let Ok(mut processes) = self.processes.lock() {
            processes.remove(&job.id);
        }
    }

    fn shutdown(&self) {
        let mut groups: HashMap<i32, i32> = HashMap::new();
        if let Ok(dir) = gflow::paths::get_runner_dir() {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                        continue;
                    }
                    if let Ok(metadata) = serde_json::from_slice::<RunnerMetadata>(
                        &fs::read(path).unwrap_or_default(),
                    ) {
                        if Self::process_identity_matches(&metadata) {
                            groups.insert(metadata.pgid, metadata.pid);
                        }
                    }
                }
            }
        }
        if let Ok(processes) = self.processes.lock() {
            for process in processes.values() {
                groups.entry(process.pid).or_insert(process.pid);
            }
        }
        if groups.is_empty() {
            return;
        }

        tracing::info!(processes = groups.len(), "Terminating managed job runners");
        for pgid in groups.keys() {
            unsafe {
                libc::kill(-*pgid, libc::SIGTERM);
            }
        }

        let deadline = Instant::now() + TERMINATE_GRACE;
        while Instant::now() < deadline {
            if groups.keys().all(|pgid| !Self::group_is_alive(*pgid)) {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        for (pgid, pid) in groups {
            tracing::warn!(pid, pgid, "Process group survived SIGTERM, sending SIGKILL");
            unsafe {
                libc::kill(-pgid, libc::SIGKILL);
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
                user_command.push_str(&format!("bash {}", shell_escape::escape(script_str.into())));
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

            session.try_send_command(&format!(
                "cd {}",
                shell_escape::escape(job.run_dir.to_string_lossy())
            ))?;
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
                let activation = conda_activation_command(job, conda_env)?;
                session.try_send_command(&activation)?;
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
    fn test_runner_command_records_exit_code_without_http_reporting() {
        let job = job_with_command(123, "echo hello");
        let result_path = PathBuf::from("/tmp/runner.result.json");
        let command = ProcessExecutor::build_runner_command(&job, &result_path).unwrap();
        assert!(command.contains("bash -c"));
        assert!(command.contains("exit_code"));
        assert!(command.contains("runner.result.json"));
        assert!(!command.contains("gcancel"));
    }

    #[test]
    fn test_runner_command_isolates_payload_exit() {
        let job = job_with_command(456, "exit 7");
        let command =
            ProcessExecutor::build_runner_command(&job, PathBuf::from("/tmp/result").as_path())
                .unwrap();
        assert!(command.contains("bash -c 'exit 7'"));
        assert!(command.contains("exit \"$status\""));
    }

    #[test]
    fn test_runner_command_rejects_empty_job() {
        let job = Job {
            id: 1,
            state: JobState::Queued,
            run_dir: PathBuf::from("/tmp"),
            ..Default::default()
        };
        assert!(ProcessExecutor::build_runner_command(
            &job,
            PathBuf::from("/tmp/result").as_path()
        )
        .is_err());
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

    // ── conda environment tests ─────────────────────────────────────────────

    /// Serializes tests that mutate conda-related process environment
    /// variables, which are read when a runner command is built.
    static CONDA_ENV_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn with_conda_env_vars<T>(vars: &[(&str, Option<&str>)], f: impl FnOnce() -> T) -> T {
        let _guard = CONDA_ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let old: Vec<(String, Option<String>)> = vars
            .iter()
            .map(|(key, _)| (key.to_string(), std::env::var(key).ok()))
            .collect();
        for (key, value) in vars {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
        let result = f();
        for (key, value) in &old {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
        result
    }

    fn make_fake_conda_root(tempdir: &std::path::Path, init_extra: &str) -> PathBuf {
        let root = tempdir.join("fakeroot");
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::create_dir_all(root.join("etc/profile.d")).unwrap();
        fs::write(root.join("bin/conda"), "#!/bin/sh\n").unwrap();
        fs::write(
            root.join("etc/profile.d/conda.sh"),
            format!("# fake conda init\n{init_extra}\n"),
        )
        .unwrap();
        fs::canonicalize(&root).unwrap_or(root)
    }

    #[test]
    fn test_conda_root_located_from_conda_exe() {
        let tempdir = tempfile::tempdir().unwrap();
        let root = make_fake_conda_root(tempdir.path(), "");
        let found = with_conda_env_vars(
            &[("CONDA_EXE", Some(root.join("bin/conda").to_str().unwrap()))],
            locate_conda_root,
        )
        .unwrap();
        assert_eq!(found, root);
    }

    #[test]
    fn test_conda_root_located_from_path() {
        let tempdir = tempfile::tempdir().unwrap();
        let root = make_fake_conda_root(tempdir.path(), "");
        let found = with_conda_env_vars(
            &[
                ("CONDA_EXE", None),
                ("CONDA_PREFIX", None),
                ("PATH", Some(root.join("bin").to_str().unwrap())),
            ],
            locate_conda_root,
        )
        .unwrap();
        assert_eq!(found, root);
    }

    #[test]
    fn test_conda_root_located_from_condabin_on_path() {
        let tempdir = tempfile::tempdir().unwrap();
        let root = tempdir.path().join("fakeroot");
        fs::create_dir_all(root.join("condabin")).unwrap();
        fs::create_dir_all(root.join("etc/profile.d")).unwrap();
        fs::write(root.join("condabin/conda"), "#!/bin/sh\n").unwrap();
        fs::write(root.join("etc/profile.d/conda.sh"), "# fake\n").unwrap();
        let root = fs::canonicalize(root).unwrap();
        let found = with_conda_env_vars(
            &[
                ("CONDA_EXE", None),
                ("CONDA_PREFIX", None),
                ("PATH", Some(root.join("condabin").to_str().unwrap())),
            ],
            locate_conda_root,
        )
        .unwrap();
        assert_eq!(found, root);
    }

    #[test]
    fn test_conda_root_located_from_conda_prefix() {
        let tempdir = tempfile::tempdir().unwrap();
        let root = make_fake_conda_root(tempdir.path(), "");
        let env_dir = root.join("envs/nested");
        fs::create_dir_all(&env_dir).unwrap();
        let found = with_conda_env_vars(
            &[
                ("CONDA_EXE", None),
                ("CONDA_PREFIX", Some(env_dir.to_str().unwrap())),
                ("PATH", Some("/nonexistent-for-test")),
            ],
            locate_conda_root,
        )
        .unwrap();
        assert_eq!(found, root);
    }

    #[cfg(unix)]
    #[test]
    fn test_conda_root_located_through_symlink() {
        let tempdir = tempfile::tempdir().unwrap();
        let root = make_fake_conda_root(tempdir.path(), "");
        let link_dir = tempdir.path().join("links");
        fs::create_dir_all(&link_dir).unwrap();
        std::os::unix::fs::symlink(root.join("bin/conda"), link_dir.join("conda")).unwrap();
        let found = with_conda_env_vars(
            &[
                ("CONDA_EXE", None),
                ("CONDA_PREFIX", None),
                ("PATH", Some(link_dir.to_str().unwrap())),
            ],
            locate_conda_root,
        )
        .unwrap();
        assert_eq!(found, root);
    }

    #[test]
    fn test_runner_command_sources_conda_init_and_quotes_env() {
        let tempdir = tempfile::tempdir().unwrap();
        let root = make_fake_conda_root(tempdir.path(), "");
        let job = Job {
            conda_env: Some("myenv; printf injected".into()),
            ..job_with_command(777, "python --version")
        };
        let command = with_conda_env_vars(
            &[("CONDA_EXE", Some(root.join("bin/conda").to_str().unwrap()))],
            || ProcessExecutor::build_user_command(&job),
        )
        .unwrap();
        let expected_activation = format!(
            "source {} && conda activate {}",
            shell_escape::escape(
                root.join("etc/profile.d/conda.sh")
                    .to_string_lossy()
                    .into_owned()
                    .into()
            ),
            shell_escape::escape("myenv; printf injected".into())
        );
        assert!(command.starts_with(&expected_activation));
        assert!(command.ends_with("&& python --version"));
        assert!(!command.contains("conda activate myenv; printf injected"));
    }

    #[test]
    fn test_runner_command_quotes_script_path() {
        let job = Job {
            id: 42,
            script: Some(PathBuf::from("/tmp/my script.sh").into()),
            state: JobState::Queued,
            run_dir: PathBuf::from("/tmp"),
            ..Default::default()
        };
        let command = ProcessExecutor::build_user_command(&job).unwrap();
        assert_eq!(command, "bash '/tmp/my script.sh'");
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
    fn test_process_executor_re_adopts_runner_and_collects_exit_code() {
        with_isolated_data_dir(|| {
            let executor = ProcessExecutor::new();
            let job = job_with_command(9004, "sleep 1; exit 7");
            executor.execute(&job).unwrap();

            let metadata_path = gflow::paths::get_runner_metadata_path(job.id).unwrap();
            assert!(metadata_path.exists(), "runner metadata should be durable");
            assert!(executor.is_running(job.id, None));

            // A fresh executor has no in-memory registry, but it must still be
            // able to adopt the live runner from metadata.
            drop(executor);
            let adopted = ProcessExecutor::new();
            assert!(adopted.is_running(job.id, None));

            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            let result = loop {
                if let Some(result) = adopted
                    .collect_finished()
                    .into_iter()
                    .find(|result| result.job_id == job.id)
                {
                    break result;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "runner did not persist an exit result"
                );
                std::thread::sleep(Duration::from_millis(50));
            };
            assert_eq!(result.exit_code, Some(7));
            assert!(!result.succeeded());
            assert!(matches!(
                adopted.execution_status(job.id, None),
                ExecutionStatus::Finished(_)
            ));

            adopted.cleanup(&job);
            assert!(!metadata_path.exists());
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

    #[test]
    fn test_process_executor_conda_env_activation_end_to_end() {
        with_isolated_data_dir(|| {
            let tempdir = tempfile::tempdir().unwrap();
            let init_extra = r#"
conda() {
    if [ "${1:-}" = "activate" ]; then
        CONDA_DEFAULT_ENV="${2:-}"
        export CONDA_DEFAULT_ENV
        printf 'FAKE_CONDA_ACTIVATED %s\n' "${2:-}"
        return 0
    fi
    return 0
}
"#;
            let root = make_fake_conda_root(tempdir.path(), init_extra);
            let exe = root.join("bin/conda");
            let job = Job {
                id: 9201,
                command: Some("true".into()),
                conda_env: Some("myenv".into()),
                state: JobState::Queued,
                run_dir: PathBuf::from("/tmp"),
                ..Default::default()
            };

            let result = with_conda_env_vars(&[("CONDA_EXE", Some(exe.to_str().unwrap()))], || {
                let executor = ProcessExecutor::new();
                executor.execute(&job).unwrap();

                let deadline = std::time::Instant::now() + Duration::from_secs(15);
                let result = loop {
                    if let Some(result) = executor
                        .collect_finished()
                        .into_iter()
                        .find(|result| result.job_id == job.id)
                    {
                        break result;
                    }
                    assert!(
                        std::time::Instant::now() < deadline,
                        "runner did not finish"
                    );
                    std::thread::sleep(Duration::from_millis(50));
                };

                let log =
                    fs::read_to_string(gflow::paths::get_log_file_path(job.id).unwrap()).unwrap();
                assert!(
                    log.contains("FAKE_CONDA_ACTIVATED myenv"),
                    "conda env was not activated in the runner; log: {log}"
                );
                executor.cleanup(&job);
                result
            });

            assert_eq!(result.exit_code, Some(0));
            assert!(result.succeeded());
        })
    }

    #[test]
    fn test_process_executor_build_failure_is_written_to_job_log() {
        with_isolated_data_dir(|| {
            let executor = ProcessExecutor::new();
            let job = Job {
                id: 9202,
                state: JobState::Queued,
                run_dir: PathBuf::from("/tmp"),
                ..Default::default()
            };
            let error = executor.execute(&job).unwrap_err();

            let log = fs::read_to_string(gflow::paths::get_log_file_path(job.id).unwrap()).unwrap();
            assert!(log.contains("failed to prepare job 9202"), "log: {log}");
            assert!(log.contains("neither a script nor a command"), "log: {log}");
            assert!(error.to_string().contains("neither a script nor a command"));
            executor.cleanup(&job);
        })
    }
}
