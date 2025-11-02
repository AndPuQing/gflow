use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use strum::{Display, EnumIter, EnumString, FromRepr};

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
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Job {
    /// Required fields at submission time
    pub id: u32,
    pub script: Option<PathBuf>,
    pub command: Option<String>,
    pub gpus: u32,
    pub conda_env: Option<String>,
    pub run_dir: PathBuf,
    pub priority: u8,
    pub depends_on: Option<u32>,
    pub task_id: Option<u32>,
    pub time_limit: Option<Duration>, // Maximum runtime in seconds (None = no limit)
    pub submitted_by: String,

    /// Optional fields that get populated by gflowd
    pub run_name: Option<String>, // tmux session name
    pub state: JobState,
    pub gpu_ids: Option<Vec<u32>>,       // GPU IDs assigned to this job
    pub started_at: Option<SystemTime>,  // When the job started running
    pub finished_at: Option<SystemTime>, // When the job finished or failed
}

#[derive(Default)]
pub struct JobBuilder {
    script: Option<PathBuf>,
    command: Option<String>,
    gpus: u32,
    conda_env: Option<String>,
    run_dir: PathBuf,
    priority: u8,
    depends_on: Option<u32>,
    task_id: Option<u32>,
    time_limit: Option<Duration>,
    submitted_by: String,
}

impl JobBuilder {
    pub fn new() -> Self {
        Self {
            priority: 10, // Default priority
            depends_on: None,
            task_id: None,
            ..Default::default()
        }
    }

    pub fn script(mut self, script: PathBuf) -> Self {
        self.script = Some(script);
        self
    }

    pub fn command(mut self, command: String) -> Self {
        self.command = Some(command);
        self
    }

    pub fn gpus(mut self, gpus: u32) -> Self {
        self.gpus = gpus;
        self
    }

    pub fn conda_env(mut self, conda_env: &Option<String>) -> Self {
        self.conda_env = conda_env.clone();
        self
    }

    pub fn run_dir(mut self, run_dir: PathBuf) -> Self {
        self.run_dir = run_dir;
        self
    }

    pub fn priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }

    pub fn depends_on(mut self, depends_on: Option<u32>) -> Self {
        self.depends_on = depends_on;
        self
    }

    pub fn task_id(mut self, task_id: Option<u32>) -> Self {
        self.task_id = task_id;
        self
    }

    pub fn time_limit(mut self, time_limit: Option<Duration>) -> Self {
        self.time_limit = time_limit;
        self
    }

    pub fn submitted_by(mut self, submitted_by: String) -> Self {
        self.submitted_by = submitted_by;
        self
    }

    pub fn build(self) -> Job {
        Job {
            id: 0,
            script: self.script,
            command: self.command,
            gpus: self.gpus,
            conda_env: self.conda_env,
            priority: self.priority,
            depends_on: self.depends_on,
            task_id: self.task_id,
            time_limit: self.time_limit,
            submitted_by: self.submitted_by,
            run_name: None,
            state: JobState::Queued,
            gpu_ids: None,
            run_dir: self.run_dir,
            started_at: None,
            finished_at: None,
        }
    }
}

impl Job {
    pub fn builder() -> JobBuilder {
        JobBuilder::new()
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

        if !self.state.clone().can_transition_to(next.clone()) {
            return Err(JobError::InvalidTransition {
                from: self.state.clone(),
                to: next,
            });
        }
        self.update_timestamps(&next);
        self.state = next;
        Ok(())
    }

    pub fn try_transition(&mut self, job_id: u32, next: JobState) -> bool {
        match self.transition_to(next.clone()) {
            Ok(_) => {
                log::debug!("Job {} transitioned to {}", job_id, next);
                true
            }
            Err(JobError::AlreadyInState(state)) => {
                log::warn!(
                    "Job {} already in state {}, ignoring transition",
                    job_id,
                    state
                );
                false
            }
            Err(JobError::InvalidTransition { from, to }) => {
                log::error!("Job {} invalid transition: {} → {}", job_id, from, to);
                false
            }
            Err(e) => {
                log::error!("Unexpected error transitioning job {}: {}", job_id, e);
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
}
