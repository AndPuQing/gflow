use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use std::fmt;
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
    All,
    Any,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Default)]
pub enum GpuSharingMode {
    #[default]
    Exclusive,
    Shared,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub enum JobStateReason {
    JobHeldUser,
    WaitingForDependency,
    WaitingForResources,
    WaitingForGpu,
    WaitingForMemory,
    CancelledByUser,
    DependencyFailed(u32),
    SystemError(CompactString),
}

impl fmt::Display for JobStateReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobStateReason::JobHeldUser => write!(f, "JobHeldUser"),
            JobStateReason::WaitingForDependency => write!(f, "Dependency"),
            JobStateReason::WaitingForResources => write!(f, "Resources"),
            JobStateReason::WaitingForGpu => write!(f, "Resources(GPU)"),
            JobStateReason::WaitingForMemory => write!(f, "Resources(Memory)"),
            JobStateReason::CancelledByUser => write!(f, "CancelledByUser"),
            JobStateReason::DependencyFailed(job_id) => {
                write!(f, "DependencyFailed:{}", job_id)
            }
            JobStateReason::SystemError(msg) => write!(f, "SystemError:{}", msg),
        }
    }
}

impl JobState {
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

    pub fn dependency_outcome(self) -> Option<bool> {
        self.is_final().then_some(self == Self::Finished)
    }

    pub const ACTIVE: &'static [JobState] = &[JobState::Queued, JobState::Hold, JobState::Running];

    pub const COMPLETED: &'static [JobState] = &[
        JobState::Finished,
        JobState::Failed,
        JobState::Cancelled,
        JobState::Timeout,
    ];

    pub fn active_states() -> &'static [JobState] {
        Self::ACTIVE
    }

    pub fn completed_states() -> &'static [JobState] {
        Self::COMPLETED
    }
}
