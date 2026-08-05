use crate::core::job::Job;
use anyhow::Result;

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

    /// Terminate a running job (used by cancel / timeout / shutdown paths).
    ///
    /// `run_name` is the job's session/run name (only meaningful for tmux).
    /// Executors that cannot actively terminate should leave this as a no-op.
    fn terminate(&self, _job_id: u32, _run_name: Option<&str>) -> Result<()> {
        Ok(())
    }

    /// Whether the job's underlying execution is still alive (zombie monitor).
    ///
    /// Executors that cannot probe liveness should return `true` so they never
    /// false-positive a running job as a zombie.
    fn is_running(&self, _job_id: u32, _run_name: Option<&str>) -> bool {
        true
    }

    /// Best-effort cleanup after a job reaches a terminal state.
    fn cleanup(&self, _job: &Job) {}

    /// Terminate everything this executor manages (daemon shutdown).
    fn shutdown(&self) {}
}