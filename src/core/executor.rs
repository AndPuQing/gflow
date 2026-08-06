use crate::core::job::Job;
use anyhow::Result;

/// The result reported by an execution runner after the payload exits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutionResult {
    pub job_id: u32,
    pub exit_code: Option<i32>,
    pub signal: Option<i32>,
}

impl ExecutionResult {
    pub fn succeeded(self) -> bool {
        self.exit_code == Some(0) && self.signal.is_none()
    }
}

/// The state of the external execution entity for a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionStatus {
    Running,
    Finished(ExecutionResult),
    Missing,
}

/// Abstraction over how a job's payload is actually executed.
///
/// The default implementation (`ProcessExecutor`) spawns a detached child
/// process group (`setsid`) and redirects its stdio to the job log file.
/// The legacy `TmuxExecutor` keeps the terminal-injection behaviour and is
/// available as an opt-in via `[executor] type = "tmux"`.
///
/// All methods other than `execute` have default no-op implementations so
/// that mock executors used in tests only need to implement `execute`.
pub trait Executor: Send + Sync {
    /// Start executing a job. Must return quickly (spawn, not wait).
    fn execute(&self, job: &Job) -> Result<()>;

    /// Backend identifier, e.g. `"process"` or `"tmux"`. Exposed via the
    /// daemon's `/info` endpoint so clients can render mode-appropriate
    /// liveness indicators.
    fn kind(&self) -> &'static str {
        "unknown"
    }

    /// Terminate a running job (used by cancel / timeout / shutdown paths).
    ///
    /// `run_name` is the job's session/run name (only meaningful for tmux).
    /// Executors that cannot actively terminate should leave this as a no-op.
    fn terminate(&self, _job_id: u32, _run_name: Option<&str>) -> Result<()> {
        Ok(())
    }

    /// Return the durable state of the job's external execution entity.
    ///
    /// The default implementation preserves the legacy executor contract: an
    /// executor that cannot distinguish an exited process from an unavailable
    /// probe is treated as alive and is therefore never falsely failed.
    fn execution_status(&self, job_id: u32, run_name: Option<&str>) -> ExecutionStatus {
        if self.is_running(job_id, run_name) {
            ExecutionStatus::Running
        } else {
            ExecutionStatus::Missing
        }
    }

    /// Whether the job's underlying execution is still alive (zombie monitor).
    ///
    /// Executors that cannot probe liveness should return `true` so they never
    /// false-positive a running job as a zombie.
    fn is_running(&self, _job_id: u32, _run_name: Option<&str>) -> bool {
        true
    }

    /// Return execution results that were recorded while the daemon was
    /// running or offline. The default executor has no durable result store.
    fn collect_finished(&self) -> Vec<ExecutionResult> {
        Vec::new()
    }

    /// Best-effort cleanup after a job reaches a terminal state.
    fn cleanup(&self, _job: &Job) {}

    /// Terminate everything this executor manages (daemon shutdown).
    fn shutdown(&self) {}
}
