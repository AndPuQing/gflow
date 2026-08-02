//! Quota enforcement: effective-limit resolution, O(1) running-usage gating in
//! the scheduling loop, and queue-depth checks at submission time.

use super::*;
use crate::config::{QuotaConfig, QuotaLimits};
use crate::core::quota::{QuotaScope, QuotaStatusEntry, QuotaUsage};

impl Scheduler {
    /// Effective quotas: `gflow.toml` baseline merged with persisted runtime
    /// overrides (overrides win field-wise).
    pub fn effective_quotas(&self) -> QuotaConfig {
        self.quota_baseline.merged_with(&self.quota_overrides)
    }

    /// Replace the file-based baseline (called when the daemon starts).
    pub fn set_quota_baseline(&mut self, quota: QuotaConfig) {
        self.quota_baseline = quota;
    }

    /// Runtime overrides currently persisted in state.
    pub fn quota_overrides(&self) -> &QuotaConfig {
        &self.quota_overrides
    }

    /// Merge `limits` into the persisted override for one subject. Only `Some`
    /// fields are written; existing fields are left untouched. Returns whether
    /// anything changed.
    pub fn merge_quota_override(
        &mut self,
        scope: QuotaScope,
        name: Option<&str>,
        limits: &QuotaLimits,
    ) -> bool {
        if limits.is_empty() {
            return false;
        }
        let target = match scope {
            QuotaScope::DefaultUser => &mut self.quota_overrides.default_user,
            QuotaScope::DefaultProject => &mut self.quota_overrides.default_project,
            QuotaScope::User => {
                let Some(name) = name else { return false };
                self.quota_overrides
                    .users
                    .entry(name.to_string())
                    .or_default()
            }
            QuotaScope::Project => {
                let Some(name) = name else { return false };
                self.quota_overrides
                    .projects
                    .entry(name.to_string())
                    .or_default()
            }
        };
        let merged = target.merged_with(limits);
        let changed = &merged != target;
        *target = merged;
        changed
    }

    /// Remove a persisted override entry (or reset a default). Returns whether
    /// anything changed.
    pub fn remove_quota_override(&mut self, scope: QuotaScope, name: Option<&str>) -> bool {
        match scope {
            QuotaScope::DefaultUser => {
                let changed = !self.quota_overrides.default_user.is_empty();
                self.quota_overrides.default_user = QuotaLimits::default();
                changed
            }
            QuotaScope::DefaultProject => {
                let changed = !self.quota_overrides.default_project.is_empty();
                self.quota_overrides.default_project = QuotaLimits::default();
                changed
            }
            QuotaScope::User => name
                .and_then(|name| self.quota_overrides.users.remove(name))
                .is_some(),
            QuotaScope::Project => name
                .and_then(|name| self.quota_overrides.projects.remove(name))
                .is_some(),
        }
    }

    /// O(1) quota gate used by the scheduling loop: would starting a job that
    /// requests `gpus` for `user` / `project` exceed any running quota?
    pub(crate) fn within_quota(&self, user: &str, project: Option<&str>, gpus: u32) -> bool {
        let effective = self.effective_quotas();

        let user_limits = effective.user_limits(user);
        let user_usage = self.quota_usage.user(user);
        if let Some(max_jobs) = user_limits.max_running_jobs {
            if user_usage.jobs >= max_jobs {
                return false;
            }
        }
        if let Some(max_gpus) = user_limits.max_running_gpus {
            if user_usage.gpus + gpus > max_gpus {
                return false;
            }
        }

        if let Some(project_limits) = effective.project_limits(project) {
            // `project` is `Some` whenever `project_limits` is `Some`.
            let project_usage = project
                .map(|p| self.quota_usage.project(p))
                .unwrap_or_default();
            if let Some(max_jobs) = project_limits.max_running_jobs {
                if project_usage.jobs >= max_jobs {
                    return false;
                }
            }
            if let Some(max_gpus) = project_limits.max_running_gpus {
                if project_usage.gpus + gpus > max_gpus {
                    return false;
                }
            }
        }

        true
    }

    /// Count pending (Queued or Hold) jobs for a user via the user index.
    pub fn count_pending_jobs_for_user(&self, user: &str) -> usize {
        self.count_pending(self.user_jobs_index.get(user))
    }

    /// Count pending (Queued or Hold) jobs for a project via the project index.
    pub fn count_pending_jobs_for_project(&self, project: &str) -> usize {
        self.count_pending(self.project_jobs_index.get(project))
    }

    fn count_pending(&self, job_ids: Option<&Vec<u32>>) -> usize {
        let Some(job_ids) = job_ids else {
            return 0;
        };
        job_ids
            .iter()
            .filter_map(|&id| self.get_job_runtime(id))
            .filter(|rt| matches!(rt.state, JobState::Queued | JobState::Hold))
            .count()
    }

    /// Submission-time queue-depth gate. `pending_bias` carries jobs accepted
    /// earlier in the same batch that are not indexed yet. Returns a
    /// human-readable rejection reason, or `None` if the job may be queued.
    pub fn check_queue_quota(
        &self,
        user: &str,
        project: Option<&str>,
        pending_bias: &HashMap<CompactString, usize>,
        project_pending_bias: &HashMap<CompactString, usize>,
    ) -> Option<String> {
        let effective = self.effective_quotas();
        let bias_key = CompactString::from(user);

        let user_limits = effective.user_limits(user);
        if let Some(max_queued) = user_limits.max_queued_jobs {
            let pending = self.count_pending_jobs_for_user(user)
                + pending_bias.get(&bias_key).copied().unwrap_or(0);
            if pending >= max_queued {
                return Some(format!(
                    "quota exceeded for user '{user}': {pending}/{max_queued} queued jobs"
                ));
            }
        }

        if let (Some(project), Some(project_limits)) = (project, effective.project_limits(project))
        {
            if let Some(max_queued) = project_limits.max_queued_jobs {
                let project_key = CompactString::from(project);
                let pending = self.count_pending_jobs_for_project(project)
                    + project_pending_bias.get(&project_key).copied().unwrap_or(0);
                if pending >= max_queued {
                    return Some(format!(
                        "quota exceeded for project '{project}': {pending}/{max_queued} queued jobs"
                    ));
                }
            }
        }

        None
    }

    /// Snapshot of every quota subject that has limits configured or non-zero
    /// usage, for `gctl quota list` / `GET /quotas`.
    pub fn quota_status(&self) -> Vec<QuotaStatusEntry> {
        let effective = self.effective_quotas();
        let mut entries: Vec<QuotaStatusEntry> = Vec::new();

        // Users: configured entries plus users with live usage or pending jobs.
        let mut users: std::collections::BTreeSet<String> = effective
            .users
            .keys()
            .cloned()
            .chain(self.quota_usage.users.keys().map(|s| s.to_string()))
            .chain(self.user_jobs_index.keys().map(|s| s.to_string()))
            .collect();
        // Keep output deterministic and compact: drop users that are neither
        // configured nor currently active.
        users.retain(|user| {
            effective.users.contains_key(user)
                || self.quota_usage.users.contains_key(user.as_str())
                || self.count_pending_jobs_for_user(user) > 0
        });
        for user in users {
            let usage = self.quota_usage.user(&user);
            entries.push(QuotaStatusEntry {
                scope: QuotaScope::User,
                name: user.clone(),
                limits: effective.user_limits(&user),
                running_jobs: usage.jobs,
                running_gpus: usage.gpus,
                queued_jobs: self.count_pending_jobs_for_user(&user),
            });
        }

        // Projects: same treatment.
        let mut projects: std::collections::BTreeSet<String> = effective
            .projects
            .keys()
            .cloned()
            .chain(self.quota_usage.projects.keys().map(|s| s.to_string()))
            .chain(self.project_jobs_index.keys().map(|s| s.to_string()))
            .collect();
        projects.retain(|project| {
            effective.projects.contains_key(project)
                || self.quota_usage.projects.contains_key(project.as_str())
                || self.count_pending_jobs_for_project(project) > 0
        });
        for project in projects {
            let usage = self.quota_usage.project(&project);
            entries.push(QuotaStatusEntry {
                scope: QuotaScope::Project,
                name: project.clone(),
                limits: effective.project_limits(Some(&project)).unwrap_or_default(),
                running_jobs: usage.jobs,
                running_gpus: usage.gpus,
                queued_jobs: self.count_pending_jobs_for_project(&project),
            });
        }

        // Defaults always reported so operators can see the fallback limits.
        entries.push(QuotaStatusEntry {
            scope: QuotaScope::DefaultUser,
            name: String::new(),
            limits: effective.default_user.clone(),
            running_jobs: 0,
            running_gpus: 0,
            queued_jobs: 0,
        });
        entries.push(QuotaStatusEntry {
            scope: QuotaScope::DefaultProject,
            name: String::new(),
            limits: effective.default_project.clone(),
            running_jobs: 0,
            running_gpus: 0,
            queued_jobs: 0,
        });

        entries
    }

    /// Current running usage for a user (for tests / diagnostics).
    pub fn quota_user_usage(&self, user: &str) -> QuotaUsage {
        self.quota_usage.user(user)
    }

    /// Current running usage for a project (for tests / diagnostics).
    pub fn quota_project_usage(&self, project: &str) -> QuotaUsage {
        self.quota_usage.project(project)
    }
}
