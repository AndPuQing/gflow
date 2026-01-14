use super::scheduler::Scheduler;
use anyhow::{anyhow, Result};

pub const CURRENT_VERSION: u32 = 2;

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
    // Future migrations go here:
    // if from_version < 2 {
    //     scheduler = migrate_v1_to_v2(scheduler)?;
    // }

    scheduler.version = CURRENT_VERSION;
    Ok(scheduler)
}

/// Migrate from version 0 (no version field) to version 1
fn migrate_v0_to_v1(mut scheduler: Scheduler) -> Result<Scheduler> {
    tracing::info!("Migrating from v0 to v1: adding version field");
    scheduler.version = 1;
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
        assert_eq!(result.version, 2); // Now migrates to version 2
        assert_eq!(result.next_job_id(), original_next_id); // Data preserved
    }

    #[test]
    fn test_data_preservation_through_migration() {
        use crate::core::job::{Job, JobState};
        use std::collections::HashMap;

        // Create test job
        let mut jobs = HashMap::new();
        let job = Job {
            id: 1,
            state: JobState::Finished,
            ..Default::default()
        };
        jobs.insert(1, job);

        let scheduler = Scheduler {
            version: 0,
            next_job_id: 42,
            jobs,
            ..Default::default()
        };

        let result = migrate_state(scheduler).unwrap();
        assert_eq!(result.version, 2); // Now migrates to version 2
        assert_eq!(result.next_job_id(), 42);
        assert_eq!(result.jobs.len(), 1);
        assert_eq!(result.jobs.get(&1).unwrap().state, JobState::Finished);
    }
}
