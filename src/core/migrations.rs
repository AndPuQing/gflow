use super::db::Database;
use super::scheduler::Scheduler;
use anyhow::{anyhow, Context, Result};
use std::path::Path;

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

/// Check if migration from JSON to SQLite is needed
pub fn needs_migration(json_path: &Path, db_path: &Path) -> bool {
    json_path.exists() && !db_path.exists()
}

/// Migrate from state.json to SQLite database
pub fn migrate_json_to_sqlite(json_path: &Path, db: &Database) -> Result<()> {
    tracing::info!("Starting migration from state.json to SQLite database");

    // Read state.json
    let state_json = std::fs::read_to_string(json_path)
        .with_context(|| format!("Failed to read state.json from {:?}", json_path))?;

    // Deserialize to Scheduler
    let mut scheduler: Scheduler =
        serde_json::from_str(&state_json).context("Failed to deserialize state.json")?;

    tracing::info!(
        "Loaded {} jobs from state.json (version {})",
        scheduler.jobs.len(),
        scheduler.version
    );

    // Apply any necessary migrations (v0 -> v1 -> v2)
    if scheduler.version < CURRENT_VERSION {
        scheduler = migrate_state(scheduler)
            .context("Failed to apply migrations during JSON to SQLite migration")?;
        tracing::info!("Applied migrations to version {}", scheduler.version);
    }

    // Insert all jobs into SQLite
    let jobs: Vec<_> = scheduler.jobs.values().cloned().collect();
    if !jobs.is_empty() {
        db.insert_jobs_batch(&jobs)
            .context("Failed to insert jobs into database")?;
        tracing::info!("Inserted {} jobs into database", jobs.len());
    }

    // Set metadata
    db.set_metadata("next_job_id", &scheduler.next_job_id.to_string())
        .context("Failed to set next_job_id metadata")?;

    db.set_metadata("version", &CURRENT_VERSION.to_string())
        .context("Failed to set version metadata")?;

    if let Some(ref allowed_gpu_indices) = scheduler.allowed_gpu_indices {
        let json = serde_json::to_string(allowed_gpu_indices)
            .context("Failed to serialize allowed_gpu_indices")?;
        db.set_metadata("allowed_gpu_indices", &json)
            .context("Failed to set allowed_gpu_indices metadata")?;
    }

    // Backup state.json
    let backup_path = json_path.with_extension("json.backup");
    std::fs::rename(json_path, &backup_path)
        .with_context(|| format!("Failed to backup state.json to {:?}", backup_path))?;

    tracing::info!("Migration complete. Backup saved to {:?}", backup_path);

    Ok(())
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
