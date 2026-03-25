use super::{
    deserialize_group_id, serialize_group_id, DependencyIds, DependencyMode, GpuIds,
    GpuSharingMode, JobError, JobState, JobStateReason, Parameters,
};
use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use uuid::Uuid;

/// JobSpec contains immutable submission-time configuration (cold data).
/// This data is rarely accessed during scheduling, only at execution time.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(default)]
pub struct JobSpec {
    // Execution config (cold - accessed only at execution time)
    pub script: Option<Box<PathBuf>>,
    pub command: Option<CompactString>,
    pub conda_env: Option<CompactString>,
    pub run_dir: PathBuf,
    #[serde(default)]
    pub parameters: Parameters,

    // Metadata (cold - rarely accessed)
    pub submitted_by: CompactString,
    pub submitted_at: Option<SystemTime>,
    pub task_id: Option<u32>,
    pub redone_from: Option<u32>,
    pub max_retry: Option<u32>,
    pub auto_close_tmux: bool,
    pub run_name: Option<CompactString>,

    // Project tracking (optional, immutable after submission)
    // Normalized and validated at submission time (whitespace trimmed, length checked)
    #[serde(default)]
    pub project: Option<CompactString>,

    #[serde(default)]
    #[serde(skip_serializing_if = "JobNotifications::is_empty")]
    pub notifications: JobNotifications,

    // Dependency config (cold - accessed once during submission)
    pub depends_on: Option<u32>,
    #[serde(default)]
    pub depends_on_ids: DependencyIds,
    #[serde(default)]
    pub dependency_mode: Option<DependencyMode>,
    #[serde(default)]
    pub auto_cancel_on_dependency_failure: bool,
}

impl Default for JobSpec {
    fn default() -> Self {
        Self {
            script: None,
            command: None,
            conda_env: None,
            run_dir: PathBuf::from("."),
            parameters: Parameters::new(),
            submitted_by: CompactString::const_new("unknown"),
            submitted_at: None,
            task_id: None,
            redone_from: None,
            max_retry: None,
            auto_close_tmux: false,
            run_name: None,
            project: None,
            notifications: JobNotifications::default(),
            depends_on: None,
            depends_on_ids: DependencyIds::new(),
            dependency_mode: None,
            auto_cancel_on_dependency_failure: true,
        }
    }
}

/// JobRuntime contains mutable runtime state (hot data).
/// This data is frequently accessed during scheduling and should fit in CPU cache.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct JobRuntime {
    // Identity (hot - always accessed)
    pub id: u32,

    // Scheduling state (hot - accessed every scheduling cycle)
    pub state: JobState,
    pub priority: u8,

    // Resource requirements (hot - checked during scheduling)
    pub gpus: u32,
    #[serde(default)]
    pub gpu_sharing_mode: GpuSharingMode,
    #[serde(default)]
    pub gpu_memory_limit_mb: Option<u64>,
    pub time_limit: Option<Duration>,
    pub memory_limit_mb: Option<u64>,

    // Resource allocation (hot - modified during scheduling)
    #[serde(default)]
    pub gpu_ids: Option<GpuIds>,

    // Group concurrency (hot - checked during scheduling)
    #[serde(
        default,
        serialize_with = "serialize_group_id",
        deserialize_with = "deserialize_group_id"
    )]
    pub group_id: Option<Uuid>,
    pub max_concurrent: Option<usize>,

    // Timing (warm - accessed for timeout checks)
    pub started_at: Option<SystemTime>,
    pub finished_at: Option<SystemTime>,

    // Failure reason (cold - only set on failure)
    #[serde(default)]
    pub reason: Option<Box<JobStateReason>>,
    #[serde(default)]
    pub retry_attempt: u32,
}

impl Default for JobRuntime {
    fn default() -> Self {
        Self {
            id: 0,
            state: JobState::Queued,
            priority: 10,
            gpus: 0,
            gpu_sharing_mode: GpuSharingMode::Exclusive,
            gpu_memory_limit_mb: None,
            time_limit: None,
            memory_limit_mb: None,
            gpu_ids: None,
            group_id: None,
            max_concurrent: None,
            started_at: None,
            finished_at: None,
            reason: None,
            retry_attempt: 0,
        }
    }
}

/// JobView combines JobSpec and JobRuntime for API compatibility.
/// This provides a unified view of job data for external interfaces.
#[derive(Debug, Serialize, Clone)]
pub struct JobView {
    #[serde(flatten)]
    pub spec: JobSpec,
    #[serde(flatten)]
    pub runtime: JobRuntime,
}

impl JobView {
    /// Create a JobView from separate spec and runtime components
    pub fn from_parts(spec: JobSpec, runtime: JobRuntime) -> Self {
        Self { spec, runtime }
    }

    /// Convert JobView into a legacy Job struct
    pub fn into_job(self) -> Job {
        Job::from_parts(self.spec, self.runtime)
    }

    /// Create a JobView by borrowing spec and runtime (requires cloning)
    pub fn from_refs(spec: &JobSpec, runtime: &JobRuntime) -> Self {
        Self {
            spec: spec.clone(),
            runtime: runtime.clone(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct Job {
    /// Required fields at submission time
    pub id: u32,
    pub script: Option<Box<PathBuf>>,
    pub command: Option<CompactString>,
    pub gpus: u32,
    pub conda_env: Option<CompactString>,
    pub run_dir: PathBuf,
    pub priority: u8,
    pub depends_on: Option<u32>, // Legacy single dependency (for backward compatibility)
    #[serde(default)]
    pub depends_on_ids: DependencyIds, // New multi-dependency field
    #[serde(default)]
    pub dependency_mode: Option<DependencyMode>, // AND or OR logic
    #[serde(default)]
    pub auto_cancel_on_dependency_failure: bool, // Auto-cancel when dependency fails
    pub task_id: Option<u32>,
    #[serde(default)]
    pub gpu_sharing_mode: GpuSharingMode,
    #[serde(default)]
    pub gpu_memory_limit_mb: Option<u64>, // Per-GPU memory limit in MB (None = no limit)
    pub time_limit: Option<Duration>, // Maximum runtime in seconds (None = no limit)
    pub memory_limit_mb: Option<u64>, // Maximum memory in MB (None = no limit)
    pub submitted_by: CompactString,
    pub redone_from: Option<u32>, // The job ID this job was redone from
    pub max_retry: Option<u32>,   // Maximum automatic retries after a failed attempt
    pub auto_close_tmux: bool,    // Whether to automatically close tmux on successful completion
    #[serde(default)]
    pub parameters: Parameters, // Parameter values for template substitution
    #[serde(
        default,
        serialize_with = "serialize_group_id",
        deserialize_with = "deserialize_group_id"
    )]
    pub group_id: Option<Uuid>, // UUID for job group (for batch submissions)
    pub max_concurrent: Option<usize>, // Max concurrent jobs in this group

    /// Optional fields that get populated by gflowd
    pub run_name: Option<CompactString>, // tmux session name
    #[serde(default)]
    pub project: Option<CompactString>, // Project code for tracking (normalized, immutable)
    pub state: JobState,
    pub gpu_ids: Option<GpuIds>,          // GPU IDs assigned to this job
    pub submitted_at: Option<SystemTime>, // When the job was submitted
    pub started_at: Option<SystemTime>,   // When the job started running
    pub finished_at: Option<SystemTime>,  // When the job finished or failed
    #[serde(default)]
    pub reason: Option<Box<JobStateReason>>, // Reason for cancellation/failure
    #[serde(default)]
    pub retry_attempt: u32, // Number of retry attempts already consumed
    // Append-only for backward compatibility with legacy msgpack array layout.
    #[serde(default)]
    #[serde(skip_serializing_if = "JobNotifications::is_empty")]
    pub notifications: JobNotifications,
}

#[derive(Default)]
pub struct JobBuilder {
    script: Option<PathBuf>,
    command: Option<CompactString>,
    gpus: Option<u32>,
    conda_env: Option<CompactString>,
    run_dir: Option<PathBuf>,
    priority: Option<u8>,
    depends_on: Option<u32>,
    depends_on_ids: Option<DependencyIds>,
    dependency_mode: Option<Option<DependencyMode>>,
    auto_cancel_on_dependency_failure: Option<bool>,
    task_id: Option<u32>,
    time_limit: Option<Duration>,
    gpu_memory_limit_mb: Option<u64>,
    memory_limit_mb: Option<u64>,
    submitted_by: Option<CompactString>,
    run_name: Option<CompactString>,
    redone_from: Option<u32>,
    max_retry: Option<u32>,
    auto_close_tmux: Option<bool>,
    parameters: Option<Parameters>,
    group_id: Option<Uuid>,
    max_concurrent: Option<usize>,
    project: Option<CompactString>,
    notifications: Option<JobNotifications>,
    gpu_sharing_mode: Option<GpuSharingMode>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, Default)]
#[serde(default)]
pub struct JobNotifications {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub emails: Vec<CompactString>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<CompactString>,
}

impl JobNotifications {
    pub fn is_empty(&self) -> bool {
        self.emails.is_empty()
    }

    pub fn normalized(
        emails: impl IntoIterator<Item = String>,
        events: impl IntoIterator<Item = String>,
    ) -> Self {
        let emails = dedupe_compact_strings(emails.into_iter().map(|s| s.trim().to_string()));
        let events = dedupe_compact_strings(events.into_iter().map(|s| s.trim().to_lowercase()));
        Self { emails, events }
    }
}

fn dedupe_compact_strings(values: impl IntoIterator<Item = String>) -> Vec<CompactString> {
    let mut out = Vec::new();
    for value in values {
        if value.is_empty() {
            continue;
        }
        let value = CompactString::from(value);
        if !out.contains(&value) {
            out.push(value);
        }
    }
    out
}

impl JobBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn script(mut self, script: impl Into<PathBuf>) -> Self {
        self.script = Some(script.into());
        self
    }

    pub fn command(mut self, command: impl Into<String>) -> Self {
        self.command = Some(CompactString::from(command.into()));
        self
    }

    pub fn gpus(mut self, gpus: u32) -> Self {
        self.gpus = Some(gpus);
        self
    }

    pub fn conda_env(mut self, conda_env: Option<String>) -> Self {
        self.conda_env = conda_env.map(CompactString::from);
        self
    }

    pub fn run_dir(mut self, run_dir: impl Into<PathBuf>) -> Self {
        self.run_dir = Some(run_dir.into());
        self
    }

    pub fn priority(mut self, priority: u8) -> Self {
        self.priority = Some(priority);
        self
    }

    pub fn depends_on(mut self, depends_on: impl Into<Option<u32>>) -> Self {
        self.depends_on = depends_on.into();
        self
    }

    pub fn depends_on_ids(mut self, depends_on_ids: impl Into<DependencyIds>) -> Self {
        self.depends_on_ids = Some(depends_on_ids.into());
        self
    }

    pub fn dependency_mode(mut self, dependency_mode: Option<DependencyMode>) -> Self {
        self.dependency_mode = Some(dependency_mode);
        self
    }

    pub fn auto_cancel_on_dependency_failure(mut self, auto_cancel: bool) -> Self {
        self.auto_cancel_on_dependency_failure = Some(auto_cancel);
        self
    }

    pub fn task_id(mut self, task_id: impl Into<Option<u32>>) -> Self {
        self.task_id = task_id.into();
        self
    }

    pub fn gpu_sharing_mode(mut self, gpu_sharing_mode: GpuSharingMode) -> Self {
        self.gpu_sharing_mode = Some(gpu_sharing_mode);
        self
    }

    pub fn shared(mut self, shared: bool) -> Self {
        self.gpu_sharing_mode = Some(if shared {
            GpuSharingMode::Shared
        } else {
            GpuSharingMode::Exclusive
        });
        self
    }

    pub fn time_limit(mut self, time_limit: impl Into<Option<Duration>>) -> Self {
        self.time_limit = time_limit.into();
        self
    }

    pub fn gpu_memory_limit_mb(mut self, gpu_memory_limit_mb: impl Into<Option<u64>>) -> Self {
        self.gpu_memory_limit_mb = gpu_memory_limit_mb.into();
        self
    }

    pub fn memory_limit_mb(mut self, memory_limit_mb: impl Into<Option<u64>>) -> Self {
        self.memory_limit_mb = memory_limit_mb.into();
        self
    }

    pub fn submitted_by(mut self, submitted_by: impl Into<String>) -> Self {
        self.submitted_by = Some(CompactString::from(submitted_by.into()));
        self
    }

    pub fn run_name(mut self, run_name: Option<String>) -> Self {
        self.run_name = run_name.map(CompactString::from);
        self
    }

    pub fn redone_from(mut self, redone_from: impl Into<Option<u32>>) -> Self {
        self.redone_from = redone_from.into();
        self
    }

    pub fn max_retry(mut self, max_retry: Option<u32>) -> Self {
        self.max_retry = max_retry;
        self
    }

    pub fn auto_close_tmux(mut self, auto_close_tmux: bool) -> Self {
        self.auto_close_tmux = Some(auto_close_tmux);
        self
    }

    pub fn parameters(mut self, parameters: HashMap<String, String>) -> Self {
        self.parameters = Some(
            parameters
                .into_iter()
                .map(|(k, v)| (CompactString::from(k), CompactString::from(v)))
                .collect(),
        );
        self
    }

    pub fn parameters_compact(mut self, parameters: Parameters) -> Self {
        self.parameters = Some(parameters);
        self
    }

    pub fn group_id(mut self, group_id: Option<String>) -> Self {
        self.group_id = group_id.and_then(|s| Uuid::parse_str(&s).ok());
        self
    }

    pub fn group_id_uuid(mut self, group_id: Option<Uuid>) -> Self {
        self.group_id = group_id;
        self
    }

    pub fn max_concurrent(mut self, max_concurrent: Option<usize>) -> Self {
        self.max_concurrent = max_concurrent;
        self
    }

    pub fn project(mut self, project: Option<String>) -> Self {
        self.project = project.map(CompactString::from);
        self
    }

    pub fn notifications(mut self, notifications: JobNotifications) -> Self {
        self.notifications = Some(notifications);
        self
    }

    pub fn build(self) -> Job {
        Job {
            id: 0,
            script: self.script.map(Box::new),
            command: self.command,
            gpus: self.gpus.unwrap_or(0),
            conda_env: self.conda_env,
            priority: self.priority.unwrap_or(10),
            depends_on: self.depends_on,
            depends_on_ids: self.depends_on_ids.unwrap_or_default(),
            dependency_mode: self.dependency_mode.flatten(),
            auto_cancel_on_dependency_failure: self
                .auto_cancel_on_dependency_failure
                .unwrap_or(true),
            task_id: self.task_id,
            gpu_sharing_mode: self.gpu_sharing_mode.unwrap_or_default(),
            gpu_memory_limit_mb: self.gpu_memory_limit_mb,
            time_limit: self.time_limit,
            memory_limit_mb: self.memory_limit_mb,
            submitted_by: self
                .submitted_by
                .unwrap_or_else(|| CompactString::const_new("unknown")),
            redone_from: self.redone_from,
            max_retry: self.max_retry,
            auto_close_tmux: self.auto_close_tmux.unwrap_or(false),
            parameters: self.parameters.unwrap_or_default(),
            group_id: self.group_id,
            max_concurrent: self.max_concurrent,
            run_name: self.run_name,
            project: self.project,
            notifications: self.notifications.unwrap_or_default(),
            state: JobState::Queued,
            gpu_ids: None,
            run_dir: self.run_dir.unwrap_or_else(|| ".".into()),
            submitted_at: None,
            started_at: None,
            finished_at: None,
            reason: None,
            retry_attempt: 0,
        }
    }
}

impl Default for Job {
    fn default() -> Self {
        Job {
            id: 0,
            script: None,
            command: None,
            gpus: 0,
            conda_env: None,
            run_dir: PathBuf::from("."),
            priority: 10,
            depends_on: None,
            depends_on_ids: DependencyIds::new(),
            dependency_mode: None,
            auto_cancel_on_dependency_failure: true,
            task_id: None,
            gpu_sharing_mode: GpuSharingMode::Exclusive,
            gpu_memory_limit_mb: None,
            time_limit: None,
            memory_limit_mb: None,
            submitted_by: CompactString::const_new("unknown"),
            redone_from: None,
            max_retry: None,
            auto_close_tmux: false,
            parameters: Parameters::new(),
            group_id: None,
            max_concurrent: None,
            run_name: None,
            project: None,
            notifications: JobNotifications::default(),
            state: JobState::Queued,
            gpu_ids: None,
            submitted_at: None,
            started_at: None,
            finished_at: None,
            reason: None,
            retry_attempt: 0,
        }
    }
}

impl Job {
    pub fn builder() -> JobBuilder {
        JobBuilder::new()
    }

    /// Create a Job from separate JobSpec and JobRuntime components
    pub fn from_parts(spec: JobSpec, runtime: JobRuntime) -> Self {
        Self {
            id: runtime.id,
            script: spec.script,
            command: spec.command,
            gpus: runtime.gpus,
            conda_env: spec.conda_env,
            run_dir: spec.run_dir,
            priority: runtime.priority,
            depends_on: spec.depends_on,
            depends_on_ids: spec.depends_on_ids,
            dependency_mode: spec.dependency_mode,
            auto_cancel_on_dependency_failure: spec.auto_cancel_on_dependency_failure,
            task_id: spec.task_id,
            gpu_sharing_mode: runtime.gpu_sharing_mode,
            gpu_memory_limit_mb: runtime.gpu_memory_limit_mb,
            time_limit: runtime.time_limit,
            memory_limit_mb: runtime.memory_limit_mb,
            submitted_by: spec.submitted_by,
            redone_from: spec.redone_from,
            max_retry: spec.max_retry,
            auto_close_tmux: spec.auto_close_tmux,
            parameters: spec.parameters,
            group_id: runtime.group_id,
            max_concurrent: runtime.max_concurrent,
            run_name: spec.run_name,
            project: spec.project,
            notifications: spec.notifications,
            state: runtime.state,
            gpu_ids: runtime.gpu_ids,
            submitted_at: spec.submitted_at,
            started_at: runtime.started_at,
            finished_at: runtime.finished_at,
            reason: runtime.reason,
            retry_attempt: runtime.retry_attempt,
        }
    }

    /// Split a Job into separate JobSpec and JobRuntime components
    pub fn into_parts(self) -> (JobSpec, JobRuntime) {
        let spec = JobSpec {
            script: self.script,
            command: self.command,
            conda_env: self.conda_env,
            run_dir: self.run_dir,
            parameters: self.parameters,
            submitted_by: self.submitted_by,
            submitted_at: self.submitted_at,
            task_id: self.task_id,
            redone_from: self.redone_from,
            max_retry: self.max_retry,
            auto_close_tmux: self.auto_close_tmux,
            run_name: self.run_name,
            project: self.project,
            notifications: self.notifications,
            depends_on: self.depends_on,
            depends_on_ids: self.depends_on_ids,
            dependency_mode: self.dependency_mode,
            auto_cancel_on_dependency_failure: self.auto_cancel_on_dependency_failure,
        };

        let runtime = JobRuntime {
            id: self.id,
            state: self.state,
            priority: self.priority,
            gpus: self.gpus,
            gpu_sharing_mode: self.gpu_sharing_mode,
            gpu_memory_limit_mb: self.gpu_memory_limit_mb,
            time_limit: self.time_limit,
            memory_limit_mb: self.memory_limit_mb,
            gpu_ids: self.gpu_ids,
            group_id: self.group_id,
            max_concurrent: self.max_concurrent,
            started_at: self.started_at,
            finished_at: self.finished_at,
            reason: self.reason,
            retry_attempt: self.retry_attempt,
        };

        (spec, runtime)
    }

    /// Returns all dependency IDs (combining legacy single dependency and new multi-dependency)
    pub fn all_dependency_ids(&self) -> DependencyIds {
        let mut deps = self.depends_on_ids.clone();
        if let Some(dep) = self.depends_on {
            if !deps.contains(&dep) {
                deps.push(dep);
            }
        }
        deps
    }

    /// Returns an iterator over all dependency IDs without allocation.
    /// The legacy `depends_on` is yielded first (if present and not in `depends_on_ids`),
    /// followed by all IDs in `depends_on_ids`.
    pub fn dependency_ids_iter(&self) -> impl Iterator<Item = u32> + '_ {
        let legacy_dep = self
            .depends_on
            .filter(|dep| !self.depends_on_ids.contains(dep));
        legacy_dep
            .into_iter()
            .chain(self.depends_on_ids.iter().copied())
    }

    /// Returns true if this job has no dependencies.
    pub fn has_no_dependencies(&self) -> bool {
        self.depends_on.is_none() && self.depends_on_ids.is_empty()
    }

    fn update_timestamps(&mut self, next: &JobState) {
        match next {
            JobState::Running => self.started_at = Some(SystemTime::now()),
            JobState::Finished | JobState::Failed | JobState::Cancelled | JobState::Timeout => {
                self.finished_at = Some(SystemTime::now())
            }
            _ => {}
        }
    }

    pub fn transition_to(&mut self, next: JobState) -> Result<(), JobError> {
        if self.state == next {
            return Err(JobError::AlreadyInState(next));
        }

        if !self.state.can_transition_to(next) {
            return Err(JobError::InvalidTransition {
                from: self.state,
                to: next,
            });
        }
        self.update_timestamps(&next);
        self.state = next;
        Ok(())
    }

    pub fn try_transition(&mut self, job_id: u32, next: JobState) -> bool {
        match self.transition_to(next) {
            Ok(_) => {
                tracing::debug!("Job {} transitioned to {}", job_id, next);
                true
            }
            Err(JobError::AlreadyInState(state)) => {
                tracing::warn!(
                    "Job {} already in state {}, ignoring transition",
                    job_id,
                    state
                );
                false
            }
            Err(JobError::InvalidTransition { from, to }) => {
                tracing::error!("Job {} invalid transition: {} → {}", job_id, from, to);
                false
            }
            Err(e) => {
                tracing::error!("Unexpected error transitioning job {}: {}", job_id, e);
                false
            }
        }
    }

    /// Check if the job has exceeded its time limit
    pub fn has_exceeded_time_limit(&self) -> bool {
        if self.state != JobState::Running {
            return false;
        }

        if let (Some(time_limit), Some(started_at)) = (self.time_limit, self.started_at) {
            if let Ok(elapsed) = SystemTime::now().duration_since(started_at) {
                return elapsed > time_limit;
            }
        }

        false
    }

    /// Calculate wait time (time from submission to start)
    pub fn wait_time(&self) -> Option<Duration> {
        match (self.submitted_at, self.started_at) {
            (Some(submitted), Some(started)) => started.duration_since(submitted).ok(),
            _ => None,
        }
    }

    /// Calculate runtime (time from start to finish, or current elapsed time if still running)
    pub fn runtime(&self) -> Option<Duration> {
        match (self.started_at, self.finished_at) {
            (Some(started), Some(finished)) => finished.duration_since(started).ok(),
            (Some(started), None) if self.state == JobState::Running => {
                SystemTime::now().duration_since(started).ok()
            }
            _ => None,
        }
    }

    #[cfg(test)]
    pub fn with_id(mut self, id: u32) -> Self {
        self.id = id;
        self
    }

    #[cfg(test)]
    pub fn with_redone_from(mut self, redone_from: Option<u32>) -> Self {
        self.redone_from = redone_from;
        self
    }
}
