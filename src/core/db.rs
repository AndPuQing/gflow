use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::job::{Job, JobState};

const SCHEMA_SQL: &str = r#"
-- Core jobs table
CREATE TABLE IF NOT EXISTS jobs (
    id INTEGER PRIMARY KEY,
    script TEXT,
    command TEXT,
    gpus INTEGER NOT NULL DEFAULT 0,
    conda_env TEXT,
    run_dir TEXT NOT NULL,
    priority INTEGER NOT NULL DEFAULT 10,
    depends_on INTEGER,
    task_id INTEGER,
    time_limit_secs INTEGER,
    memory_limit_mb INTEGER,
    submitted_by TEXT NOT NULL,
    redone_from INTEGER,
    auto_close_tmux INTEGER NOT NULL DEFAULT 0,
    group_id TEXT,
    max_concurrent INTEGER,
    run_name TEXT,
    state TEXT NOT NULL DEFAULT 'Queued',
    started_at INTEGER,
    finished_at INTEGER,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at INTEGER NOT NULL DEFAULT (unixepoch()),
    FOREIGN KEY (depends_on) REFERENCES jobs(id),
    FOREIGN KEY (redone_from) REFERENCES jobs(id),
    CHECK (state IN ('Queued', 'Hold', 'Running', 'Finished', 'Failed', 'Cancelled', 'Timeout'))
);

-- GPU assignments (Vec<u32> → table)
CREATE TABLE IF NOT EXISTS job_gpu_assignments (
    job_id INTEGER NOT NULL,
    gpu_index INTEGER NOT NULL,
    assigned_at INTEGER NOT NULL DEFAULT (unixepoch()),
    PRIMARY KEY (job_id, gpu_index),
    FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE CASCADE
);

-- Job parameters (HashMap<String, String> → table)
CREATE TABLE IF NOT EXISTS job_parameters (
    job_id INTEGER NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY (job_id, key),
    FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE CASCADE
);

-- Metadata (next_job_id, allowed_gpu_indices, version)
CREATE TABLE IF NOT EXISTS scheduler_metadata (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at INTEGER NOT NULL DEFAULT (unixepoch())
);

-- Indexes for common queries
CREATE INDEX IF NOT EXISTS idx_jobs_state ON jobs(state);
CREATE INDEX IF NOT EXISTS idx_jobs_group_id ON jobs(group_id) WHERE group_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_jobs_depends_on ON jobs(depends_on) WHERE depends_on IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_jobs_created_at ON jobs(created_at);
"#;

/// Database handle for managing gflow state
#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
    #[allow(dead_code)]
    db_path: PathBuf,
}

impl Database {
    /// Create a new database connection
    pub fn new(db_path: PathBuf) -> Result<Self> {
        let conn = Connection::open(&db_path)
            .with_context(|| format!("Failed to open database at {:?}", db_path))?;

        // Enable foreign keys
        conn.execute("PRAGMA foreign_keys = ON", [])
            .context("Failed to enable foreign keys")?;

        // Enable WAL mode for better concurrency and durability
        conn.pragma_update(None, "journal_mode", "WAL")
            .context("Failed to enable WAL mode")?;

        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
            db_path,
        };

        db.initialize_schema()?;
        Ok(db)
    }

    /// Initialize database schema
    pub fn initialize_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(SCHEMA_SQL)
            .context("Failed to initialize database schema")?;
        Ok(())
    }

    /// Health check - verify database connectivity
    pub fn health_check(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.query_row("SELECT 1", [], |_| Ok(()))
            .context("Database health check failed")?;
        Ok(())
    }

    /// Get metadata value by key
    pub fn get_metadata(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let result: Option<String> = conn
            .query_row(
                "SELECT value FROM scheduler_metadata WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .context("Failed to get metadata")?;
        Ok(result)
    }

    /// Set metadata value
    pub fn set_metadata(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO scheduler_metadata (key, value, updated_at)
             VALUES (?1, ?2, unixepoch())
             ON CONFLICT(key) DO UPDATE SET value = ?2, updated_at = unixepoch()",
            params![key, value],
        )
        .context("Failed to set metadata")?;
        Ok(())
    }

    /// Insert a new job
    pub fn insert_job(&self, job: &Job) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO jobs (
                id, script, command, gpus, conda_env, run_dir, priority,
                depends_on, task_id, time_limit_secs, memory_limit_mb,
                submitted_by, redone_from, auto_close_tmux, group_id,
                max_concurrent, run_name, state, started_at, finished_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
            params![
                job.id,
                job.script.as_ref().map(|p| p.to_string_lossy().to_string()),
                job.command,
                job.gpus,
                job.conda_env,
                job.run_dir.to_string_lossy().to_string(),
                job.priority,
                job.depends_on,
                job.task_id,
                job.time_limit.as_ref().map(|d| d.as_secs() as i64),
                job.memory_limit_mb.map(|m| m as i64),
                job.submitted_by,
                job.redone_from,
                if job.auto_close_tmux { 1 } else { 0 },
                job.group_id,
                job.max_concurrent.map(|m| m as i64),
                job.run_name,
                job.state.to_string(),
                job.started_at.as_ref().map(system_time_to_unix),
                job.finished_at.as_ref().map(system_time_to_unix),
            ],
        )
        .context("Failed to insert job")?;

        // Insert GPU assignments
        if let Some(ref gpu_ids) = job.gpu_ids {
            for &gpu_index in gpu_ids {
                conn.execute(
                    "INSERT INTO job_gpu_assignments (job_id, gpu_index) VALUES (?1, ?2)",
                    params![job.id, gpu_index],
                )
                .context("Failed to insert GPU assignment")?;
            }
        }

        // Insert job parameters
        for (key, value) in &job.parameters {
            conn.execute(
                "INSERT INTO job_parameters (job_id, key, value) VALUES (?1, ?2, ?3)",
                params![job.id, key, value],
            )
            .context("Failed to insert job parameter")?;
        }

        Ok(())
    }

    /// Update an existing job
    pub fn update_job(&self, job: &Job) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().context("Failed to begin transaction")?;

        Self::update_job_tx(&tx, job)?;

        tx.commit().context("Failed to commit job update")?;
        Ok(())
    }

    /// Get a single job by ID
    pub fn get_job(&self, id: u32) -> Result<Option<Job>> {
        let conn = self.conn.lock().unwrap();

        let job_opt: Option<Job> = conn
            .query_row("SELECT * FROM jobs WHERE id = ?1", params![id], |row| {
                row_to_job(row)
            })
            .optional()
            .context("Failed to get job")?;

        if let Some(mut job) = job_opt {
            // Load GPU assignments
            let mut stmt = conn
                .prepare("SELECT gpu_index FROM job_gpu_assignments WHERE job_id = ?1 ORDER BY gpu_index")
                .context("Failed to prepare GPU query")?;
            let gpu_ids: Vec<u32> = stmt
                .query_map(params![id], |row| row.get(0))
                .context("Failed to query GPU assignments")?
                .collect::<Result<Vec<_>, _>>()
                .context("Failed to collect GPU assignments")?;

            if !gpu_ids.is_empty() {
                job.gpu_ids = Some(gpu_ids);
            }

            // Load parameters
            let mut stmt = conn
                .prepare("SELECT key, value FROM job_parameters WHERE job_id = ?1")
                .context("Failed to prepare parameters query")?;
            let params: HashMap<String, String> = stmt
                .query_map(params![id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .context("Failed to query job parameters")?
                .collect::<Result<HashMap<_, _>, _>>()
                .context("Failed to collect job parameters")?;

            job.parameters = params;

            Ok(Some(job))
        } else {
            Ok(None)
        }
    }

    /// Get all jobs
    pub fn get_all_jobs(&self) -> Result<HashMap<u32, Job>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn
            .prepare("SELECT * FROM jobs")
            .context("Failed to prepare jobs query")?;

        let jobs: Vec<Job> = stmt
            .query_map([], row_to_job)
            .context("Failed to query jobs")?
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to collect jobs")?;

        // Build HashMap
        let mut job_map: HashMap<u32, Job> = jobs.into_iter().map(|j| (j.id, j)).collect();

        // Load GPU assignments for all jobs
        let mut stmt = conn
            .prepare("SELECT job_id, gpu_index FROM job_gpu_assignments ORDER BY job_id, gpu_index")
            .context("Failed to prepare GPU assignments query")?;

        let gpu_assignments: Vec<(u32, u32)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .context("Failed to query GPU assignments")?
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to collect GPU assignments")?;

        // Group GPU assignments by job_id
        let mut gpu_map: HashMap<u32, Vec<u32>> = HashMap::new();
        for (job_id, gpu_index) in gpu_assignments {
            gpu_map.entry(job_id).or_default().push(gpu_index);
        }

        // Load parameters for all jobs
        let mut stmt = conn
            .prepare("SELECT job_id, key, value FROM job_parameters")
            .context("Failed to prepare job parameters query")?;

        let job_params: Vec<(u32, String, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .context("Failed to query job parameters")?
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to collect job parameters")?;

        // Group parameters by job_id
        let mut param_map: HashMap<u32, HashMap<String, String>> = HashMap::new();
        for (job_id, key, value) in job_params {
            param_map.entry(job_id).or_default().insert(key, value);
        }

        // Attach GPU assignments and parameters to jobs
        for (job_id, job) in job_map.iter_mut() {
            if let Some(gpu_ids) = gpu_map.remove(job_id) {
                job.gpu_ids = Some(gpu_ids);
            }
            if let Some(params) = param_map.remove(job_id) {
                job.parameters = params;
            }
        }

        Ok(job_map)
    }

    /// Insert multiple jobs in a single transaction
    pub fn insert_jobs_batch(&self, jobs: &[Job]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let tx = conn
            .unchecked_transaction()
            .context("Failed to begin transaction")?;

        // Defer foreign key checks until commit to allow jobs to reference each other
        // This is necessary during migration when jobs may have depends_on or redone_from
        // references to other jobs that haven't been inserted yet
        tx.execute("PRAGMA defer_foreign_keys = ON", [])
            .context("Failed to defer foreign key checks")?;

        for job in jobs {
            tx.execute(
                "INSERT INTO jobs (
                    id, script, command, gpus, conda_env, run_dir, priority,
                    depends_on, task_id, time_limit_secs, memory_limit_mb,
                    submitted_by, redone_from, auto_close_tmux, group_id,
                    max_concurrent, run_name, state, started_at, finished_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
                params![
                    job.id,
                    job.script.as_ref().map(|p| p.to_string_lossy().to_string()),
                    job.command,
                    job.gpus,
                    job.conda_env,
                    job.run_dir.to_string_lossy().to_string(),
                    job.priority,
                    job.depends_on,
                    job.task_id,
                    job.time_limit.as_ref().map(|d| d.as_secs() as i64),
                    job.memory_limit_mb.map(|m| m as i64),
                    job.submitted_by,
                    job.redone_from,
                    if job.auto_close_tmux { 1 } else { 0 },
                    job.group_id,
                    job.max_concurrent.map(|m| m as i64),
                    job.run_name,
                    job.state.to_string(),
                    job.started_at.as_ref().map(system_time_to_unix),
                    job.finished_at.as_ref().map(system_time_to_unix),
                ],
            )
            .context("Failed to insert job in batch")?;

            // Insert GPU assignments
            if let Some(ref gpu_ids) = job.gpu_ids {
                for &gpu_index in gpu_ids {
                    tx.execute(
                        "INSERT INTO job_gpu_assignments (job_id, gpu_index) VALUES (?1, ?2)",
                        params![job.id, gpu_index],
                    )
                    .context("Failed to insert GPU assignment in batch")?;
                }
            }

            // Insert job parameters
            for (key, value) in &job.parameters {
                tx.execute(
                    "INSERT INTO job_parameters (job_id, key, value) VALUES (?1, ?2, ?3)",
                    params![job.id, key, value],
                )
                .context("Failed to insert job parameter in batch")?;
            }
        }

        tx.commit().context("Failed to commit batch insert")?;
        Ok(())
    }

    fn update_job_tx(tx: &rusqlite::Transaction<'_>, job: &Job) -> Result<()> {
        tx.execute(
            "UPDATE jobs SET
            script = ?2, command = ?3, gpus = ?4, conda_env = ?5, run_dir = ?6,
            priority = ?7, depends_on = ?8, task_id = ?9, time_limit_secs = ?10,
            memory_limit_mb = ?11, submitted_by = ?12, redone_from = ?13,
            auto_close_tmux = ?14, group_id = ?15, max_concurrent = ?16,
            run_name = ?17, state = ?18, started_at = ?19, finished_at = ?20,
            updated_at = unixepoch()
         WHERE id = ?1",
            params![
                job.id,
                job.script.as_ref().map(|p| p.to_string_lossy().to_string()),
                job.command,
                job.gpus,
                job.conda_env,
                job.run_dir.to_string_lossy().to_string(),
                job.priority,
                job.depends_on,
                job.task_id,
                job.time_limit.as_ref().map(|d| d.as_secs() as i64),
                job.memory_limit_mb.map(|m| m as i64),
                job.submitted_by,
                job.redone_from,
                if job.auto_close_tmux { 1 } else { 0 },
                job.group_id,
                job.max_concurrent.map(|m| m as i64),
                job.run_name,
                job.state.to_string(),
                job.started_at.as_ref().map(system_time_to_unix),
                job.finished_at.as_ref().map(system_time_to_unix),
            ],
        )
        .context("Failed to update job")?;

        // GPU assignments
        tx.execute(
            "DELETE FROM job_gpu_assignments WHERE job_id = ?1",
            params![job.id],
        )
        .context("Failed to delete old GPU assignments")?;

        if let Some(ref gpu_ids) = job.gpu_ids {
            for &gpu_index in gpu_ids {
                tx.execute(
                    "INSERT INTO job_gpu_assignments (job_id, gpu_index)
                 VALUES (?1, ?2)",
                    params![job.id, gpu_index],
                )
                .context("Failed to insert GPU assignment")?;
            }
        }

        // Parameters
        tx.execute(
            "DELETE FROM job_parameters WHERE job_id = ?1",
            params![job.id],
        )
        .context("Failed to delete old job parameters")?;

        for (key, value) in &job.parameters {
            tx.execute(
                "INSERT INTO job_parameters (job_id, key, value)
             VALUES (?1, ?2, ?3)",
                params![job.id, key, value],
            )
            .context("Failed to insert job parameter")?;
        }

        Ok(())
    }

    /// Update multiple jobs in a single transaction
    pub fn update_jobs_batch(&self, jobs: &[Job]) -> Result<()> {
        if jobs.is_empty() {
            return Ok(());
        }

        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().context("Failed to begin transaction")?;

        for job in jobs {
            Self::update_job_tx(&tx, job)
                .with_context(|| format!("Failed to update job {}", job.id))?;
        }

        tx.commit().context("Failed to commit batch update")?;
        Ok(())
    }

    /// Delete a job
    #[allow(dead_code)]
    pub fn delete_job(&self, id: u32) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        // Foreign key CASCADE will handle job_gpu_assignments and job_parameters
        conn.execute("DELETE FROM jobs WHERE id = ?1", params![id])
            .context("Failed to delete job")?;
        Ok(())
    }
}

/// Convert a database row to a Job struct
fn row_to_job(row: &Row) -> rusqlite::Result<Job> {
    let script_str: Option<String> = row.get("script")?;
    let time_limit_secs: Option<i64> = row.get("time_limit_secs")?;
    let started_at_unix: Option<i64> = row.get("started_at")?;
    let finished_at_unix: Option<i64> = row.get("finished_at")?;
    let memory_limit_mb_i64: Option<i64> = row.get("memory_limit_mb")?;
    let max_concurrent_i64: Option<i64> = row.get("max_concurrent")?;
    let state_str: String = row.get("state")?;

    Ok(Job {
        id: row.get("id")?,
        script: script_str.map(PathBuf::from),
        command: row.get("command")?,
        gpus: row.get("gpus")?,
        conda_env: row.get("conda_env")?,
        run_dir: PathBuf::from(row.get::<_, String>("run_dir")?),
        priority: row.get("priority")?,
        depends_on: row.get("depends_on")?,
        task_id: row.get("task_id")?,
        time_limit: time_limit_secs.map(|secs| Duration::from_secs(secs as u64)),
        memory_limit_mb: memory_limit_mb_i64.map(|m| m as u64),
        submitted_by: row.get("submitted_by")?,
        redone_from: row.get("redone_from")?,
        auto_close_tmux: row.get::<_, i32>("auto_close_tmux")? != 0,
        parameters: HashMap::new(), // Loaded separately
        group_id: row.get("group_id")?,
        max_concurrent: max_concurrent_i64.map(|m| m as usize),
        run_name: row.get("run_name")?,
        state: state_str.parse::<JobState>().map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?,
        gpu_ids: None, // Loaded separately
        started_at: started_at_unix.map(unix_to_system_time),
        finished_at: finished_at_unix.map(unix_to_system_time),
    })
}

/// Convert SystemTime to Unix timestamp (seconds since epoch)
fn system_time_to_unix(time: &SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs() as i64
}

/// Convert Unix timestamp to SystemTime
fn unix_to_system_time(timestamp: i64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(timestamp as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_db() -> (Database, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Database::new(db_path).unwrap();
        (db, temp_dir)
    }

    #[test]
    fn test_database_creation() {
        let (db, _temp) = create_test_db();
        assert!(db.health_check().is_ok());
    }

    #[test]
    fn test_metadata() {
        let (db, _temp) = create_test_db();

        // Set and get metadata
        db.set_metadata("next_job_id", "42").unwrap();
        let value = db.get_metadata("next_job_id").unwrap();
        assert_eq!(value, Some("42".to_string()));

        // Update metadata
        db.set_metadata("next_job_id", "43").unwrap();
        let value = db.get_metadata("next_job_id").unwrap();
        assert_eq!(value, Some("43".to_string()));

        // Non-existent key
        let value = db.get_metadata("nonexistent").unwrap();
        assert_eq!(value, None);
    }

    #[test]
    fn test_insert_and_get_job() {
        let (db, _temp) = create_test_db();

        let job = Job {
            id: 1,
            script: Some(PathBuf::from("/tmp/test.sh")),
            command: None,
            gpus: 2,
            conda_env: Some("myenv".to_string()),
            run_dir: PathBuf::from("/tmp"),
            priority: 10,
            depends_on: None,
            task_id: None,
            time_limit: Some(Duration::from_secs(3600)),
            memory_limit_mb: Some(4096),
            submitted_by: "alice".to_string(),
            redone_from: None,
            auto_close_tmux: true,
            parameters: {
                let mut map = HashMap::new();
                map.insert("key1".to_string(), "value1".to_string());
                map.insert("key2".to_string(), "value2".to_string());
                map
            },
            group_id: Some("group-uuid".to_string()),
            max_concurrent: Some(5),
            run_name: Some("test-job-1".to_string()),
            state: JobState::Queued,
            gpu_ids: Some(vec![0, 1]),
            started_at: None,
            finished_at: None,
        };

        db.insert_job(&job).unwrap();

        let retrieved = db.get_job(1).unwrap().unwrap();
        assert_eq!(retrieved.id, 1);
        assert_eq!(retrieved.gpus, 2);
        assert_eq!(retrieved.submitted_by, "alice");
        assert_eq!(retrieved.gpu_ids, Some(vec![0, 1]));
        assert_eq!(retrieved.parameters.len(), 2);
        assert_eq!(
            retrieved.parameters.get("key1"),
            Some(&"value1".to_string())
        );
    }

    #[test]
    fn test_update_job() {
        let (db, _temp) = create_test_db();

        let mut job = Job {
            id: 1,
            state: JobState::Queued,
            submitted_by: "alice".to_string(),
            ..Default::default()
        };

        db.insert_job(&job).unwrap();

        // Update job state
        job.state = JobState::Running;
        job.started_at = Some(SystemTime::now());
        job.gpu_ids = Some(vec![0]);

        db.update_job(&job).unwrap();

        let retrieved = db.get_job(1).unwrap().unwrap();
        assert_eq!(retrieved.state, JobState::Running);
        assert!(retrieved.started_at.is_some());
        assert_eq!(retrieved.gpu_ids, Some(vec![0]));
    }

    #[test]
    fn test_get_all_jobs() {
        let (db, _temp) = create_test_db();

        let job1 = Job {
            id: 1,
            submitted_by: "alice".to_string(),
            ..Default::default()
        };
        let job2 = Job {
            id: 2,
            submitted_by: "bob".to_string(),
            ..Default::default()
        };

        db.insert_job(&job1).unwrap();
        db.insert_job(&job2).unwrap();

        let jobs = db.get_all_jobs().unwrap();
        assert_eq!(jobs.len(), 2);
        assert!(jobs.contains_key(&1));
        assert!(jobs.contains_key(&2));
    }

    #[test]
    fn test_batch_insert() {
        let (db, _temp) = create_test_db();

        let jobs = vec![
            Job {
                id: 1,
                submitted_by: "alice".to_string(),
                ..Default::default()
            },
            Job {
                id: 2,
                submitted_by: "bob".to_string(),
                ..Default::default()
            },
            Job {
                id: 3,
                submitted_by: "charlie".to_string(),
                ..Default::default()
            },
        ];

        db.insert_jobs_batch(&jobs).unwrap();

        let all_jobs = db.get_all_jobs().unwrap();
        assert_eq!(all_jobs.len(), 3);
    }

    #[test]
    fn test_batch_update() {
        let (db, _temp) = create_test_db();

        let jobs = vec![
            Job {
                id: 1,
                state: JobState::Queued,
                submitted_by: "alice".to_string(),
                ..Default::default()
            },
            Job {
                id: 2,
                state: JobState::Queued,
                submitted_by: "bob".to_string(),
                ..Default::default()
            },
        ];

        db.insert_jobs_batch(&jobs).unwrap();

        // Update all jobs to Running
        let updated_jobs: Vec<Job> = jobs
            .into_iter()
            .map(|mut j| {
                j.state = JobState::Running;
                j
            })
            .collect();

        db.update_jobs_batch(&updated_jobs).unwrap();

        let all_jobs = db.get_all_jobs().unwrap();
        assert!(all_jobs.values().all(|j| j.state == JobState::Running));
    }
}
