//! Quota support types shared between the scheduler core and the daemon API.

use compact_str::CompactString;
use serde::{Deserialize, Serialize};

use crate::config::QuotaLimits;

/// Which quota bucket an override or status entry refers to.
#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq, Hash, strum::Display)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum QuotaScope {
    /// A named user (`submitted_by`).
    User,
    /// A named project.
    Project,
    /// The fallback limits applied to every user.
    DefaultUser,
    /// The fallback limits applied to every project.
    DefaultProject,
}

impl QuotaScope {
    /// Whether this scope addresses a named entry (vs. one of the defaults).
    pub fn is_named(self) -> bool {
        matches!(self, QuotaScope::User | QuotaScope::Project)
    }
}

/// Running resource consumption of one quota subject (O(1) index value).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct QuotaUsage {
    /// Number of currently running jobs.
    pub jobs: usize,
    /// Number of currently allocated GPUs.
    pub gpus: u32,
}

impl QuotaUsage {
    fn add(&mut self, gpus: u32) {
        self.jobs += 1;
        self.gpus += gpus;
    }

    fn remove(&mut self, gpus: u32) {
        self.jobs = self.jobs.saturating_sub(1);
        self.gpus = self.gpus.saturating_sub(gpus);
    }

    fn is_zero(&self) -> bool {
        self.jobs == 0 && self.gpus == 0
    }
}

/// Mutable per-subject running usage indexes used for O(1) quota checks in the
/// scheduling loop. Rebuilt from scratch on state load; maintained incrementally
/// on job state transitions.
#[derive(Debug, Default)]
pub struct QuotaUsageIndex {
    pub users: std::collections::HashMap<CompactString, QuotaUsage>,
    pub projects: std::collections::HashMap<CompactString, QuotaUsage>,
}

impl QuotaUsageIndex {
    pub fn clear(&mut self) {
        self.users.clear();
        self.projects.clear();
    }

    pub fn record_running(
        &mut self,
        user: &CompactString,
        project: Option<&CompactString>,
        gpus: u32,
    ) {
        self.users.entry(user.clone()).or_default().add(gpus);
        if let Some(project) = project {
            self.projects.entry(project.clone()).or_default().add(gpus);
        }
    }

    pub fn release_running(
        &mut self,
        user: &CompactString,
        project: Option<&CompactString>,
        gpus: u32,
    ) {
        if let Some(usage) = self.users.get_mut(user) {
            usage.remove(gpus);
            if usage.is_zero() {
                self.users.remove(user);
            }
        }
        if let Some(project) = project {
            if let Some(usage) = self.projects.get_mut(project) {
                usage.remove(gpus);
                if usage.is_zero() {
                    self.projects.remove(project);
                }
            }
        }
    }

    pub fn user(&self, user: &str) -> QuotaUsage {
        self.users.get(user).copied().unwrap_or_default()
    }

    pub fn project(&self, project: &str) -> QuotaUsage {
        self.projects.get(project).copied().unwrap_or_default()
    }
}

/// Snapshot of one quota subject: effective limits plus current usage.
/// Returned by `Scheduler::quota_status` and served by `GET /quotas`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct QuotaStatusEntry {
    pub scope: QuotaScope,
    /// Subject name; empty string for `default_user` / `default_project`.
    pub name: String,
    /// Effective limits (baseline merged with runtime overrides).
    pub limits: QuotaLimits,
    pub running_jobs: usize,
    pub running_gpus: u32,
    pub queued_jobs: usize,
}
