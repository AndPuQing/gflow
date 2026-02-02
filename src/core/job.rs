use compact_str::CompactString;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use smallvec::SmallVec;
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use strum::{Display, EnumIter, EnumString, FromRepr};
use uuid::Uuid;

/// Type alias for dependency IDs - uses SmallVec to avoid heap allocation for small lists
/// Most jobs have 0-2 dependencies, so inline storage of 2 elements keeps same size as Vec
pub type DependencyIds = SmallVec<[u32; 2]>;

/// Custom serializer for group_id that outputs string format for compatibility
fn serialize_group_id<S>(group_id: &Option<Uuid>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match group_id {
        Some(uuid) => serializer.serialize_some(&uuid.to_string()),
        None => serializer.serialize_none(),
    }
}

/// Custom deserializer for group_id that accepts both string and binary UUID formats
fn deserialize_group_id<'de, D>(deserializer: D) -> Result<Option<Uuid>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;

    let opt: Option<String> = Option::deserialize(deserializer)?;
    match opt {
        Some(s) => {
            // Try to parse as UUID string
            Uuid::parse_str(&s)
                .map(Some)
                .map_err(|e| D::Error::custom(format!("Invalid UUID string: {}", e)))
        }
        None => Ok(None),
    }
}

#[derive(Debug)]
pub enum JobError {
    NotFound(u32),
    InvalidTransition { from: JobState, to: JobState },
    AlreadyInState(JobState),
}

impl std::error::Error for JobError {}
impl fmt::Display for JobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobError::NotFound(id) => write!(f, "Job {} not found", id),
            JobError::InvalidTransition { from, to } => {
                write!(f, "Invalid transition from {} to {}", from, to)
            }
            JobError::AlreadyInState(state) => write!(f, "Job already in state {}", state),
        }
    }
}

#[derive(
    Debug,
    Deserialize,
    Serialize,
    PartialEq,
    Eq,
    Clone,
    Copy,
    Display,
    EnumIter,
    FromRepr,
    EnumString,
    Hash,
    Ord,
    PartialOrd,
)]
pub enum JobState {
    #[strum(to_string = "Queued", serialize = "PD", serialize = "pd")]
    Queued,
    #[strum(to_string = "Hold", serialize = "H", serialize = "h")]
    Hold,
    #[strum(to_string = "Running", serialize = "R", serialize = "r")]
    Running,
    #[strum(to_string = "Finished", serialize = "CD", serialize = "cd")]
    Finished,
    #[strum(to_string = "Failed", serialize = "F", serialize = "f")]
    Failed,
    #[strum(to_string = "Cancelled", serialize = "CA", serialize = "ca")]
    Cancelled,
    #[strum(to_string = "Timeout", serialize = "TO", serialize = "to")]
    Timeout,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
pub enum DependencyMode {
    /// All dependencies must finish successfully (AND logic)
    All,
    /// Any one dependency must finish successfully (OR logic)
    Any,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub enum JobStateReason {
    /// Job is on hold by user request
    JobHeldUser,
    /// Job is waiting for dependencies to complete
    WaitingForDependency,
    /// Job is waiting for available resources (GPUs, memory, etc.)
    WaitingForResources,
    /// Job was cancelled by user request
    CancelledByUser,
    /// Job was cancelled because a dependency failed
    DependencyFailed(u32),
    /// Job was cancelled due to system error
    SystemError(String),
}

impl fmt::Display for JobStateReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobStateReason::JobHeldUser => write!(f, "JobHeldUser"),
            JobStateReason::WaitingForDependency => write!(f, "Dependency"),
            JobStateReason::WaitingForResources => write!(f, "Resources"),
            JobStateReason::CancelledByUser => write!(f, "CancelledByUser"),
            JobStateReason::DependencyFailed(job_id) => {
                write!(f, "DependencyFailed:{}", job_id)
            }
            JobStateReason::SystemError(msg) => write!(f, "SystemError:{}", msg),
        }
    }
}

impl JobState {
    /// Returns the short form representation of the job state
    pub fn short_form(&self) -> &'static str {
        match self {
            JobState::Queued => "PD",
            JobState::Hold => "H",
            JobState::Running => "R",
            JobState::Finished => "CD",
            JobState::Failed => "F",
            JobState::Cancelled => "CA",
            JobState::Timeout => "TO",
        }
    }

    pub fn can_transition_to(self, next: JobState) -> bool {
        use JobState::*;
        // Queued → Running → Finished
        //   │       │
        //   ↓       ├──> Failed
        // Hold      ├──> Cancelled
        //   │       └──> Timeout
        //   └─────────> Cancelled
        matches!(
            (self, next),
            (Queued, Running)
                | (Queued, Hold)
                | (Hold, Queued)
                | (Hold, Cancelled)
                | (Running, Finished)
                | (Running, Failed)
                | (Queued, Cancelled)
                | (Running, Cancelled)
                | (Running, Timeout)
        )
    }

    pub fn is_final(&self) -> bool {
        Self::COMPLETED.contains(self)
    }

    pub const COMPLETED: &'static [JobState] = &[
        JobState::Finished,
        JobState::Failed,
        JobState::Cancelled,
        JobState::Timeout,
    ];

    pub fn completed_states() -> &'static [JobState] {
        Self::COMPLETED
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct Job {
    /// Required fields at submission time
    pub id: u32,
    pub script: Option<PathBuf>,
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
    pub time_limit: Option<Duration>, // Maximum runtime in seconds (None = no limit)
    pub memory_limit_mb: Option<u64>, // Maximum memory in MB (None = no limit)
    pub submitted_by: CompactString,
    pub redone_from: Option<u32>, // The job ID this job was redone from
    pub auto_close_tmux: bool,    // Whether to automatically close tmux on successful completion
    pub parameters: HashMap<CompactString, CompactString>, // Parameter values for template substitution
    #[serde(
        default,
        serialize_with = "serialize_group_id",
        deserialize_with = "deserialize_group_id"
    )]
    pub group_id: Option<Uuid>, // UUID for job group (for batch submissions)
    pub max_concurrent: Option<usize>,                     // Max concurrent jobs in this group

    /// Optional fields that get populated by gflowd
    pub run_name: Option<CompactString>, // tmux session name
    pub state: JobState,
    pub gpu_ids: Option<Vec<u32>>, // GPU IDs assigned to this job
    pub submitted_at: Option<SystemTime>, // When the job was submitted
    pub started_at: Option<SystemTime>, // When the job started running
    pub finished_at: Option<SystemTime>, // When the job finished or failed
    #[serde(default)]
    pub reason: Option<JobStateReason>, // Reason for cancellation/failure
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
    memory_limit_mb: Option<u64>,
    submitted_by: Option<CompactString>,
    run_name: Option<CompactString>,
    redone_from: Option<u32>,
    auto_close_tmux: Option<bool>,
    parameters: Option<HashMap<CompactString, CompactString>>,
    group_id: Option<Uuid>,
    max_concurrent: Option<usize>,
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

    pub fn time_limit(mut self, time_limit: impl Into<Option<Duration>>) -> Self {
        self.time_limit = time_limit.into();
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

    pub fn parameters_compact(mut self, parameters: HashMap<CompactString, CompactString>) -> Self {
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

    pub fn build(self) -> Job {
        Job {
            id: 0,
            script: self.script,
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
            time_limit: self.time_limit,
            memory_limit_mb: self.memory_limit_mb,
            submitted_by: self
                .submitted_by
                .unwrap_or_else(|| CompactString::const_new("unknown")),
            run_name: self.run_name,
            redone_from: self.redone_from,
            auto_close_tmux: self.auto_close_tmux.unwrap_or(false),
            parameters: self.parameters.unwrap_or_default(),
            group_id: self.group_id,
            max_concurrent: self.max_concurrent,
            state: JobState::Queued,
            gpu_ids: None,
            run_dir: self.run_dir.unwrap_or_else(|| ".".into()),
            submitted_at: None,
            started_at: None,
            finished_at: None,
            reason: None,
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
            time_limit: None,
            memory_limit_mb: None,
            submitted_by: CompactString::const_new("unknown"),
            redone_from: None,
            auto_close_tmux: false,
            parameters: HashMap::new(),
            group_id: None,
            max_concurrent: None,
            run_name: None,
            state: JobState::Queued,
            gpu_ids: None,
            submitted_at: None,
            started_at: None,
            finished_at: None,
            reason: None,
        }
    }
}

impl Job {
    pub fn builder() -> JobBuilder {
        JobBuilder::new()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backward_compatibility_missing_auto_close_tmux() {
        // Simulate an old state.json that doesn't have auto_close_tmux field
        let old_json = r#"{
            "id": 1,
            "script": "/tmp/test.sh",
            "command": null,
            "gpus": 0,
            "conda_env": null,
            "run_dir": "/tmp",
            "priority": 10,
            "depends_on": null,
            "task_id": null,
            "time_limit": null,
            "memory_limit_mb": null,
            "submitted_by": "test",
            "run_name": "test-job-1",
            "state": "Finished",
            "gpu_ids": [],
            "started_at": null,
            "finished_at": null
        }"#;

        let result: Result<Job, _> = serde_json::from_str(old_json);
        assert!(
            result.is_ok(),
            "Failed to deserialize old JSON format: {:?}",
            result.err()
        );

        let job = result.unwrap();
        assert_eq!(job.id, 1);
        assert!(!job.auto_close_tmux); // Should use default value
        assert_eq!(job.redone_from, None); // Should be None by default
    }

    #[test]
    fn test_backward_compatibility_missing_redone_from() {
        // Simulate an old state.json that doesn't have redone_from field
        let old_json = r#"{
            "id": 2,
            "script": null,
            "command": "echo test",
            "gpus": 1,
            "conda_env": "myenv",
            "run_dir": "/home/user",
            "priority": 5,
            "depends_on": null,
            "task_id": null,
            "time_limit": null,
            "memory_limit_mb": null,
            "submitted_by": "alice",
            "auto_close_tmux": true,
            "run_name": "test-job-2",
            "state": "Running",
            "gpu_ids": [0],
            "started_at": null,
            "finished_at": null
        }"#;

        let result: Result<Job, _> = serde_json::from_str(old_json);
        assert!(
            result.is_ok(),
            "Failed to deserialize JSON without redone_from: {:?}",
            result.err()
        );

        let job = result.unwrap();
        assert_eq!(job.id, 2);
        assert!(job.auto_close_tmux);
        assert_eq!(job.redone_from, None); // Should use default value
    }

    #[test]
    fn test_backward_compatibility_missing_memory_limit() {
        // Simulate an old state.json that doesn't have memory_limit_mb field
        let old_json = r#"{
            "id": 3,
            "script": "/tmp/script.sh",
            "command": null,
            "gpus": 2,
            "conda_env": null,
            "run_dir": "/workspace",
            "priority": 8,
            "depends_on": 1,
            "task_id": null,
            "time_limit": null,
            "submitted_by": "bob",
            "redone_from": null,
            "auto_close_tmux": false,
            "run_name": "test-job-3",
            "state": "Queued",
            "gpu_ids": null,
            "started_at": null,
            "finished_at": null
        }"#;

        let result: Result<Job, _> = serde_json::from_str(old_json);
        assert!(
            result.is_ok(),
            "Failed to deserialize JSON without memory_limit_mb: {:?}",
            result.err()
        );

        let job = result.unwrap();
        assert_eq!(job.id, 3);
        assert_eq!(job.memory_limit_mb, None); // Should use default value
    }

    #[test]
    fn test_backward_compatibility_minimal_json() {
        // Test with absolute minimal JSON - only required fields from old version
        let minimal_json = r#"{
            "id": 4,
            "gpus": 0,
            "run_dir": "/tmp",
            "priority": 10,
            "submitted_by": "minimal",
            "state": "Queued"
        }"#;

        let result: Result<Job, _> = serde_json::from_str(minimal_json);
        assert!(
            result.is_ok(),
            "Failed to deserialize minimal JSON: {:?}",
            result.err()
        );

        let job = result.unwrap();
        assert_eq!(job.id, 4);
        assert!(!job.auto_close_tmux);
        assert_eq!(job.redone_from, None);
        assert_eq!(job.memory_limit_mb, None);
        assert_eq!(job.script, None);
        assert_eq!(job.command, None);
    }

    #[test]
    fn test_backward_compatibility_string_to_compactstring() {
        // Test that old JSON with String fields can be deserialized to CompactString
        let old_json = r#"{
            "id": 5,
            "command": "python train.py --lr 0.001 --epochs 100",
            "gpus": 2,
            "conda_env": "pytorch",
            "run_dir": "/home/user/work",
            "priority": 10,
            "submitted_by": "alice",
            "run_name": "training-job-5",
            "state": "Queued",
            "parameters": {
                "lr": "0.001",
                "epochs": "100",
                "batch_size": "32"
            },
            "group_id": "550e8400-e29b-41d4-a716-446655440000"
        }"#;

        let result: Result<Job, _> = serde_json::from_str(old_json);
        assert!(
            result.is_ok(),
            "Failed to deserialize JSON with string fields: {:?}",
            result.err()
        );

        let job = result.unwrap();
        assert_eq!(job.id, 5);
        assert_eq!(
            job.command.as_ref().map(|s| s.as_str()),
            Some("python train.py --lr 0.001 --epochs 100")
        );
        assert_eq!(job.conda_env.as_ref().map(|s| s.as_str()), Some("pytorch"));
        assert_eq!(job.submitted_by.as_str(), "alice");
        assert_eq!(
            job.run_name.as_ref().map(|s| s.as_str()),
            Some("training-job-5")
        );

        // Verify parameters
        assert_eq!(job.parameters.len(), 3);
        assert_eq!(job.parameters.get("lr").map(|s| s.as_str()), Some("0.001"));
        assert_eq!(
            job.parameters.get("epochs").map(|s| s.as_str()),
            Some("100")
        );
        assert_eq!(
            job.parameters.get("batch_size").map(|s| s.as_str()),
            Some("32")
        );

        // Verify group_id (now deserialized as UUID)
        assert_eq!(
            job.group_id.as_ref().map(|u| u.to_string()),
            Some("550e8400-e29b-41d4-a716-446655440000".to_string())
        );
    }

    #[test]
    fn test_compactstring_serialization_roundtrip() {
        // Test that CompactString fields serialize and deserialize correctly
        let job = JobBuilder::new()
            .command("python script.py --arg value")
            .submitted_by("testuser")
            .run_dir("/tmp/test")
            .conda_env(Some("myenv".to_string()))
            .parameters(HashMap::from([
                ("key1".to_string(), "value1".to_string()),
                ("key2".to_string(), "value2".to_string()),
            ]))
            .group_id(Some("test-group-id".to_string()))
            .build();

        // Serialize to JSON
        let json = serde_json::to_string(&job).expect("Failed to serialize");

        // Deserialize back
        let deserialized: Job = serde_json::from_str(&json).expect("Failed to deserialize");

        assert_eq!(job.command, deserialized.command);
        assert_eq!(job.submitted_by, deserialized.submitted_by);
        assert_eq!(job.conda_env, deserialized.conda_env);
        assert_eq!(job.parameters, deserialized.parameters);
        assert_eq!(job.group_id, deserialized.group_id);
    }

    #[test]
    fn test_group_id_uuid_serialization() {
        // Test UUID serialization and deserialization
        let uuid_str = "550e8400-e29b-41d4-a716-446655440000";
        let uuid = Uuid::parse_str(uuid_str).unwrap();

        let job = JobBuilder::new()
            .command("test command")
            .submitted_by("testuser")
            .run_dir("/tmp/test")
            .group_id_uuid(Some(uuid))
            .build();

        // Serialize to JSON
        let json = serde_json::to_string(&job).expect("Failed to serialize");

        // Verify it serializes as a string
        assert!(json.contains(uuid_str));

        // Deserialize back
        let deserialized: Job = serde_json::from_str(&json).expect("Failed to deserialize");

        assert_eq!(job.group_id, deserialized.group_id);
        assert_eq!(deserialized.group_id, Some(uuid));
    }

    #[test]
    fn test_group_id_backward_compatibility() {
        // Test that old JSON with string group_id can be deserialized to UUID
        let old_json = r#"{
            "id": 6,
            "gpus": 1,
            "run_dir": "/tmp",
            "priority": 10,
            "submitted_by": "test",
            "state": "Queued",
            "group_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890"
        }"#;

        let result: Result<Job, _> = serde_json::from_str(old_json);
        assert!(
            result.is_ok(),
            "Failed to deserialize old JSON with string group_id: {:?}",
            result.err()
        );

        let job = result.unwrap();
        assert_eq!(
            job.group_id.map(|u| u.to_string()),
            Some("a1b2c3d4-e5f6-7890-abcd-ef1234567890".to_string())
        );
    }
}
