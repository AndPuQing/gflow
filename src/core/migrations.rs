use super::scheduler::Scheduler;
use anyhow::{anyhow, Result};

pub const CURRENT_VERSION: u32 = 3;

/// Migrate state from any version to the current version
pub fn migrate_state(mut scheduler: Scheduler) -> Result<Scheduler> {
    let from_version = scheduler.version;

    if from_version > CURRENT_VERSION {
        return Err(anyhow!(
            "State file version {} is newer than supported version {}. Please upgrade gflowd.",
            from_version,
            CURRENT_VERSION
        ));
    }

    if from_version == CURRENT_VERSION {
        return Ok(scheduler); // No migration needed
    }

    tracing::info!(
        "Migrating state from version {} to {}",
        from_version,
        CURRENT_VERSION
    );

    // Chain migrations
    if from_version < 1 {
        scheduler = migrate_v0_to_v1(scheduler)?;
    }
    if from_version < 2 {
        scheduler = migrate_v1_to_v2(scheduler)?;
    }
    if from_version < 3 {
        scheduler = migrate_v2_to_v3(scheduler)?;
    }

    scheduler.version = CURRENT_VERSION;
    Ok(scheduler)
}

/// Migrate from version 0 (no version field) to version 1
fn migrate_v0_to_v1(mut scheduler: Scheduler) -> Result<Scheduler> {
    tracing::info!("Migrating from v0 to v1: adding version field");
    scheduler.version = 1;
    Ok(scheduler)
}

/// Migrate from version 1 to version 2 (HashMap<u32, Job> to Vec<Job>)
fn migrate_v1_to_v2(mut scheduler: Scheduler) -> Result<Scheduler> {
    tracing::info!("Migrating from v1 to v2: converting jobs HashMap to Vec");
    // The jobs field is already a Vec in the current Scheduler struct,
    // so if we successfully deserialized it, the migration is already done.
    // This function exists for completeness and future-proofing.
    scheduler.version = 2;
    Ok(scheduler)
}

/// Migrate from version 2 to version 3 (adding GPU reservations with gpu_spec)
fn migrate_v2_to_v3(mut scheduler: Scheduler) -> Result<Scheduler> {
    tracing::info!("Migrating from v2 to v3: adding GPU reservations");
    scheduler.reservations = Vec::new();
    scheduler.next_reservation_id = 1;
    scheduler.version = 3;
    Ok(scheduler)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::scheduler::Scheduler;

    #[test]
    fn test_current_version_no_migration() {
        let scheduler = Scheduler {
            version: CURRENT_VERSION,
            ..Default::default()
        };
        let next_id = scheduler.next_job_id();

        let result = migrate_state(scheduler).unwrap();
        assert_eq!(result.version, CURRENT_VERSION);
        assert_eq!(result.next_job_id(), next_id);
    }

    #[test]
    fn test_future_version_fails() {
        let scheduler = Scheduler {
            version: 999,
            ..Default::default()
        };

        let result = migrate_state(scheduler);
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("newer than supported"));
        }
    }

    #[test]
    fn test_v0_to_v1_migration() {
        let scheduler = Scheduler {
            version: 0,
            ..Default::default()
        };
        let original_next_id = scheduler.next_job_id();

        let result = migrate_state(scheduler).unwrap();
        assert_eq!(result.version, CURRENT_VERSION); // Migrates to current version
        assert_eq!(result.next_job_id(), original_next_id); // Data preserved
    }

    #[test]
    fn test_data_preservation_through_migration() {
        use crate::core::job::{Job, JobState};

        // Create test job
        let job = Job {
            id: 1,
            state: JobState::Finished,
            ..Default::default()
        };
        let jobs = vec![job];

        let scheduler = Scheduler {
            version: 0,
            next_job_id: 42,
            jobs,
            ..Default::default()
        };

        let result = migrate_state(scheduler).unwrap();
        assert_eq!(result.version, CURRENT_VERSION); // Migrates to current version
        assert_eq!(result.next_job_id(), 42);
        assert_eq!(result.jobs.len(), 1);
        assert_eq!(result.get_job(1).unwrap().state, JobState::Finished);
    }

    #[test]
    fn test_deserialize_old_hashmap_format() {
        use crate::core::job::JobState;

        // Old format: jobs as a HashMap (JSON object)
        // Using minimal fields to avoid complex serialization issues
        let old_format_json = r#"{
            "version": 1,
            "jobs": {
                "1": {
                    "id": 1,
                    "state": "Finished",
                    "script": null,
                    "command": null,
                    "gpus": 0,
                    "conda_env": null,
                    "run_dir": ".",
                    "priority": 0,
                    "depends_on": null,
                    "depends_on_ids": [],
                    "dependency_mode": null,
                    "auto_cancel_on_dependency_failure": true,
                    "task_id": null,
                    "time_limit": null,
                    "memory_limit_mb": null,
                    "submitted_by": "",
                    "redone_from": null,
                    "auto_close_tmux": false,
                    "parameters": {},
                    "group_id": null,
                    "max_concurrent": null,
                    "run_name": null,
                    "gpu_ids": null,
                    "submitted_at": null,
                    "started_at": null,
                    "finished_at": null
                },
                "2": {
                    "id": 2,
                    "state": "Queued",
                    "script": null,
                    "command": null,
                    "gpus": 0,
                    "conda_env": null,
                    "run_dir": ".",
                    "priority": 0,
                    "depends_on": null,
                    "depends_on_ids": [],
                    "dependency_mode": null,
                    "auto_cancel_on_dependency_failure": true,
                    "task_id": null,
                    "time_limit": null,
                    "memory_limit_mb": null,
                    "submitted_by": "",
                    "redone_from": null,
                    "auto_close_tmux": false,
                    "parameters": {},
                    "group_id": null,
                    "max_concurrent": null,
                    "run_name": null,
                    "gpu_ids": null,
                    "submitted_at": null,
                    "started_at": null,
                    "finished_at": null
                }
            },
            "state_path": "state.json",
            "next_job_id": 3,
            "allowed_gpu_indices": null
        }"#;

        let scheduler: Scheduler = serde_json::from_str(old_format_json).unwrap();
        assert_eq!(scheduler.version, 1);
        assert_eq!(scheduler.jobs.len(), 2);
        assert_eq!(scheduler.get_job(1).unwrap().state, JobState::Finished);
        assert_eq!(scheduler.get_job(2).unwrap().state, JobState::Queued);
        assert_eq!(scheduler.next_job_id(), 3);
    }

    #[test]
    fn test_deserialize_new_vec_format() {
        use crate::core::job::JobState;

        // New format: jobs as a Vec (JSON array)
        let new_format_json = r#"{
            "version": 2,
            "jobs": [
                {
                    "id": 1,
                    "state": "Finished",
                    "script": null,
                    "command": null,
                    "gpus": 0,
                    "conda_env": null,
                    "run_dir": ".",
                    "priority": 0,
                    "depends_on": null,
                    "depends_on_ids": [],
                    "dependency_mode": null,
                    "auto_cancel_on_dependency_failure": true,
                    "task_id": null,
                    "time_limit": null,
                    "memory_limit_mb": null,
                    "submitted_by": "",
                    "redone_from": null,
                    "auto_close_tmux": false,
                    "parameters": {},
                    "group_id": null,
                    "max_concurrent": null,
                    "run_name": null,
                    "gpu_ids": null,
                    "submitted_at": null,
                    "started_at": null,
                    "finished_at": null
                },
                {
                    "id": 2,
                    "state": "Queued",
                    "script": null,
                    "command": null,
                    "gpus": 0,
                    "conda_env": null,
                    "run_dir": ".",
                    "priority": 0,
                    "depends_on": null,
                    "depends_on_ids": [],
                    "dependency_mode": null,
                    "auto_cancel_on_dependency_failure": true,
                    "task_id": null,
                    "time_limit": null,
                    "memory_limit_mb": null,
                    "submitted_by": "",
                    "redone_from": null,
                    "auto_close_tmux": false,
                    "parameters": {},
                    "group_id": null,
                    "max_concurrent": null,
                    "run_name": null,
                    "gpu_ids": null,
                    "submitted_at": null,
                    "started_at": null,
                    "finished_at": null
                }
            ],
            "state_path": "state.json",
            "next_job_id": 3,
            "allowed_gpu_indices": null
        }"#;

        let scheduler: Scheduler = serde_json::from_str(new_format_json).unwrap();
        assert_eq!(scheduler.version, 2);
        assert_eq!(scheduler.jobs.len(), 2);
        assert_eq!(scheduler.get_job(1).unwrap().state, JobState::Finished);
        assert_eq!(scheduler.get_job(2).unwrap().state, JobState::Queued);
        assert_eq!(scheduler.next_job_id(), 3);
    }
}
