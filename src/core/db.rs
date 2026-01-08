use anyhow::{Context, Result};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, OptionalExtension, Row};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::job::{EventType, Job, JobEvent, JobState};

/// Filter criteria for querying jobs
#[derive(Debug, Clone, Default)]
pub struct JobFilter {
    pub states: Option<Vec<JobState>>,
    pub users: Option<Vec<String>>,
    pub include_inactive: bool, // include jobs where is_active = 0
}

impl JobFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_states(mut self, states: Vec<JobState>) -> Self {
        self.states = Some(states);
        self
    }

    pub fn with_users(mut self, users: Vec<String>) -> Self {
        self.users = Some(users);
        self
    }

    pub fn include_inactive(mut self) -> Self {
        self.include_inactive = true;
        self
    }
}

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
    is_active INTEGER NOT NULL DEFAULT 1,
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

-- Job events table (append-only audit trail)
CREATE TABLE IF NOT EXISTS job_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    job_id INTEGER NOT NULL,
    event_type TEXT NOT NULL,
    timestamp INTEGER NOT NULL DEFAULT (unixepoch()),
    old_state TEXT,
    new_state TEXT,
    reason TEXT,
    gpu_ids TEXT,
    parameters TEXT,
    FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE CASCADE,
    CHECK (event_type IN ('Created', 'StateTransition', 'GPUAssignment', 'GPURelease', 'ParameterUpdate', 'Hold', 'Release'))
);

-- Archive tables (for old completed jobs)
CREATE TABLE IF NOT EXISTS jobs_archive (
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
    state TEXT NOT NULL,
    started_at INTEGER,
    finished_at INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    archived_at INTEGER NOT NULL DEFAULT (unixepoch()),
    CHECK (state IN ('Finished', 'Failed', 'Cancelled', 'Timeout'))
);

CREATE TABLE IF NOT EXISTS job_gpu_assignments_archive (
    job_id INTEGER NOT NULL,
    gpu_index INTEGER NOT NULL,
    assigned_at INTEGER NOT NULL,
    PRIMARY KEY (job_id, gpu_index),
    FOREIGN KEY (job_id) REFERENCES jobs_archive(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS job_parameters_archive (
    job_id INTEGER NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY (job_id, key),
    FOREIGN KEY (job_id) REFERENCES jobs_archive(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS job_events_archive (
    id INTEGER PRIMARY KEY,
    job_id INTEGER NOT NULL,
    event_type TEXT NOT NULL,
    timestamp INTEGER NOT NULL,
    old_state TEXT,
    new_state TEXT,
    reason TEXT,
    gpu_ids TEXT,
    parameters TEXT,
    FOREIGN KEY (job_id) REFERENCES jobs_archive(id) ON DELETE CASCADE
);

-- Indexes for common queries
CREATE INDEX IF NOT EXISTS idx_jobs_state ON jobs(state) WHERE is_active = 1;
CREATE INDEX IF NOT EXISTS idx_jobs_group_id ON jobs(group_id) WHERE group_id IS NOT NULL AND is_active = 1;
CREATE INDEX IF NOT EXISTS idx_jobs_depends_on ON jobs(depends_on) WHERE depends_on IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_jobs_created_at ON jobs(created_at DESC) WHERE is_active = 1;
CREATE INDEX IF NOT EXISTS idx_jobs_submitted_by ON jobs(submitted_by) WHERE is_active = 1;
CREATE INDEX IF NOT EXISTS idx_jobs_state_created_at ON jobs(state, created_at DESC) WHERE is_active = 1;
CREATE INDEX IF NOT EXISTS idx_jobs_user_state ON jobs(submitted_by, state) WHERE is_active = 1;
CREATE INDEX IF NOT EXISTS idx_jobs_active_running ON jobs(id) WHERE state = 'Running' AND is_active = 1;

-- Indexes for job events
CREATE INDEX IF NOT EXISTS idx_events_job_id ON job_events(job_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_events_timestamp ON job_events(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_events_type ON job_events(event_type, timestamp DESC);

-- Indexes for archive tables
CREATE INDEX IF NOT EXISTS idx_archive_created_at ON jobs_archive(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_archive_user ON jobs_archive(submitted_by, finished_at DESC);
CREATE INDEX IF NOT EXISTS idx_archive_archived_at ON jobs_archive(archived_at DESC);
CREATE INDEX IF NOT EXISTS idx_archive_events_job_id ON job_events_archive(job_id, timestamp DESC);
"#;

/// Database handle for managing gflow state with connection pooling
#[derive(Clone)]
pub struct Database {
    pool: Arc<Pool<SqliteConnectionManager>>,
    #[allow(dead_code)]
    db_path: PathBuf,
}

impl Database {
    /// Create a new database connection pool
    pub fn new(db_path: PathBuf) -> Result<Self> {
        // Create connection manager
        let manager = SqliteConnectionManager::file(&db_path).with_init(|conn| {
            // Enable foreign keys
            conn.execute("PRAGMA foreign_keys = ON", [])?;
            // Enable WAL mode for better read concurrency (returns result, so use execute_batch)
            conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA busy_timeout = 5000;")?;
            Ok(())
        });

        // Create connection pool with configuration
        let pool = Pool::builder()
            .max_size(10) // Max 10 connections
            .min_idle(Some(2)) // Keep at least 2 connections ready
            .build(manager)
            .context("Failed to create connection pool")?;

        let db = Self {
            pool: Arc::new(pool),
            db_path,
        };

        db.initialize_schema()?;
        Ok(db)
    }

    /// Initialize database schema
    pub fn initialize_schema(&self) -> Result<()> {
        let conn = self
            .pool
            .get()
            .context("Failed to get connection from pool")?;
        conn.execute_batch(SCHEMA_SQL)
            .context("Failed to initialize database schema")?;

        // Set schema version to 3 for new databases
        // Note: For existing databases, data loss is acceptable so we don't migrate
        let version = self.get_metadata("schema_version")?;
        if version.is_none() {
            self.set_metadata("schema_version", "3")?;
        }

        Ok(())
    }

    /// Health check - verify database connectivity
    pub fn health_check(&self) -> Result<()> {
        let conn = self
            .pool
            .get()
            .context("Failed to get connection from pool")?;
        conn.query_row("SELECT 1", [], |_| Ok(()))
            .context("Database health check failed")?;
        Ok(())
    }

    /// Get metadata value by key
    pub fn get_metadata(&self, key: &str) -> Result<Option<String>> {
        let conn = self
            .pool
            .get()
            .context("Failed to get connection from pool")?;
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
        let conn = self
            .pool
            .get()
            .context("Failed to get connection from pool")?;
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
        let conn = self
            .pool
            .get()
            .context("Failed to get connection from pool")?;
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
        let mut conn = self
            .pool
            .get()
            .context("Failed to get connection from pool")?;
        let tx = conn.transaction().context("Failed to begin transaction")?;

        Self::update_job_tx(&tx, job)?;

        tx.commit().context("Failed to commit job update")?;
        Ok(())
    }

    /// Get a single job by ID
    pub fn get_job(&self, id: u32) -> Result<Option<Job>> {
        let conn = self
            .pool
            .get()
            .context("Failed to get connection from pool")?;

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
        let conn = self
            .pool
            .get()
            .context("Failed to get connection from pool")?;

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

    /// Get only active jobs (Queued, Hold, Running) - optimized for startup
    /// This excludes completed, failed, and canceled jobs to reduce memory usage
    pub fn get_active_jobs(&self) -> Result<HashMap<u32, Job>> {
        let conn = self
            .pool
            .get()
            .context("Failed to get connection from pool")?;

        let mut stmt = conn
            .prepare(
                "SELECT * FROM jobs WHERE state IN ('Queued', 'Hold', 'Running') AND is_active = 1",
            )
            .context("Failed to prepare active jobs query")?;

        let jobs: Vec<Job> = stmt
            .query_map([], row_to_job)
            .context("Failed to query active jobs")?
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to collect active jobs")?;

        // Build HashMap
        let mut job_map: HashMap<u32, Job> = jobs.into_iter().map(|j| (j.id, j)).collect();

        // Early return if no active jobs
        if job_map.is_empty() {
            return Ok(job_map);
        }

        // Load GPU assignments for active jobs only
        let job_ids: Vec<u32> = job_map.keys().copied().collect();
        let placeholders = job_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query = format!(
            "SELECT job_id, gpu_index FROM job_gpu_assignments WHERE job_id IN ({}) ORDER BY job_id, gpu_index",
            placeholders
        );

        let mut stmt = conn
            .prepare(&query)
            .context("Failed to prepare GPU assignments query")?;

        let params: Vec<&dyn rusqlite::ToSql> = job_ids
            .iter()
            .map(|id| id as &dyn rusqlite::ToSql)
            .collect();
        let gpu_assignments: Vec<(u32, u32)> = stmt
            .query_map(params.as_slice(), |row| Ok((row.get(0)?, row.get(1)?)))
            .context("Failed to query GPU assignments")?
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to collect GPU assignments")?;

        // Group GPU assignments by job_id
        let mut gpu_map: HashMap<u32, Vec<u32>> = HashMap::new();
        for (job_id, gpu_index) in gpu_assignments {
            gpu_map.entry(job_id).or_default().push(gpu_index);
        }

        // Load parameters for active jobs only
        let job_ids: Vec<u32> = job_map.keys().copied().collect();
        let placeholders = job_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query = format!(
            "SELECT job_id, key, value FROM job_parameters WHERE job_id IN ({})",
            placeholders
        );

        let mut stmt = conn
            .prepare(&query)
            .context("Failed to prepare job parameters query")?;

        let params: Vec<&dyn rusqlite::ToSql> = job_ids
            .iter()
            .map(|id| id as &dyn rusqlite::ToSql)
            .collect();
        let job_params: Vec<(u32, String, String)> = stmt
            .query_map(params.as_slice(), |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
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

    /// Query jobs with filtering and pagination
    /// Returns (jobs, total_count) where total_count is total jobs matching filter
    pub fn query_jobs_paginated(
        &self,
        filter: &JobFilter,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<Job>, usize)> {
        let conn = self
            .pool
            .get()
            .context("Failed to get connection from pool")?;

        // Build WHERE clause
        let mut where_clauses = Vec::new();
        let mut where_strings = Vec::new(); // Store owned strings
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        // Filter by is_active
        if !filter.include_inactive {
            where_clauses.push("is_active = 1");
        }

        // Filter by states
        if let Some(ref states) = filter.states {
            let placeholders = states.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let clause = format!("state IN ({})", placeholders);
            where_strings.push(clause);
            for state in states {
                params.push(Box::new(state.to_string()));
            }
        }

        // Filter by users
        if let Some(ref users) = filter.users {
            let placeholders = users.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let clause = format!("submitted_by IN ({})", placeholders);
            where_strings.push(clause);
            for user in users {
                params.push(Box::new(user.clone()));
            }
        }

        // Add owned strings to where_clauses as borrowed refs
        for s in &where_strings {
            where_clauses.push(s.as_str());
        }

        let where_clause = if where_clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_clauses.join(" AND "))
        };

        // Get total count
        let count_query = format!("SELECT COUNT(*) FROM jobs {}", where_clause);
        let mut stmt = conn.prepare(&count_query)?;
        let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let total_count: i64 = stmt.query_row(param_refs.as_slice(), |row| row.get(0))?;

        // Get paginated jobs
        let jobs_query = format!(
            "SELECT * FROM jobs {} ORDER BY id DESC LIMIT ? OFFSET ?",
            where_clause
        );

        let mut stmt = conn.prepare(&jobs_query)?;

        // Rebuild params with limit and offset
        let mut query_params: Vec<Box<dyn rusqlite::ToSql>> = params;
        query_params.push(Box::new(limit as i64));
        query_params.push(Box::new(offset as i64));

        let param_refs: Vec<&dyn rusqlite::ToSql> =
            query_params.iter().map(|p| p.as_ref()).collect();

        let jobs: Vec<Job> = stmt
            .query_map(param_refs.as_slice(), row_to_job)?
            .collect::<Result<Vec<_>, _>>()?;

        // Load GPU assignments and parameters for these jobs
        let job_ids: Vec<u32> = jobs.iter().map(|j| j.id).collect();

        if job_ids.is_empty() {
            return Ok((jobs, total_count as usize));
        }

        let mut job_map: HashMap<u32, Job> = jobs.into_iter().map(|j| (j.id, j)).collect();

        // Load GPU assignments
        let placeholders = job_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query = format!(
            "SELECT job_id, gpu_index FROM job_gpu_assignments WHERE job_id IN ({}) ORDER BY job_id, gpu_index",
            placeholders
        );
        let mut stmt = conn.prepare(&query)?;
        let params: Vec<&dyn rusqlite::ToSql> = job_ids
            .iter()
            .map(|id| id as &dyn rusqlite::ToSql)
            .collect();
        let gpu_assignments: Vec<(u32, u32)> = stmt
            .query_map(params.as_slice(), |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;

        let mut gpu_map: HashMap<u32, Vec<u32>> = HashMap::new();
        for (job_id, gpu_index) in gpu_assignments {
            gpu_map.entry(job_id).or_default().push(gpu_index);
        }

        // Load parameters
        let placeholders = job_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query = format!(
            "SELECT job_id, key, value FROM job_parameters WHERE job_id IN ({})",
            placeholders
        );
        let mut stmt = conn.prepare(&query)?;
        let params: Vec<&dyn rusqlite::ToSql> = job_ids
            .iter()
            .map(|id| id as &dyn rusqlite::ToSql)
            .collect();
        let job_params: Vec<(u32, String, String)> = stmt
            .query_map(params.as_slice(), |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut param_map: HashMap<u32, HashMap<String, String>> = HashMap::new();
        for (job_id, key, value) in job_params {
            param_map.entry(job_id).or_default().insert(key, value);
        }

        // Attach GPU assignments and parameters
        for (job_id, job) in job_map.iter_mut() {
            if let Some(gpu_ids) = gpu_map.remove(job_id) {
                job.gpu_ids = Some(gpu_ids);
            }
            if let Some(params) = param_map.remove(job_id) {
                job.parameters = params;
            }
        }

        // Return jobs in DESC order (most recent first)
        let mut result_jobs: Vec<Job> = job_map.into_values().collect();
        result_jobs.sort_by(|a, b| b.id.cmp(&a.id));

        Ok((result_jobs, total_count as usize))
    }

    /// Insert multiple jobs in a single transaction
    pub fn insert_jobs_batch(&self, jobs: &[Job]) -> Result<()> {
        let conn = self
            .pool
            .get()
            .context("Failed to get connection from pool")?;
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

        let mut conn = self
            .pool
            .get()
            .context("Failed to get connection from pool")?;
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
        let conn = self
            .pool
            .get()
            .context("Failed to get connection from pool")?;
        // Foreign key CASCADE will handle job_gpu_assignments and job_parameters
        conn.execute("DELETE FROM jobs WHERE id = ?1", params![id])
            .context("Failed to delete job")?;
        Ok(())
    }

    /// Log an event to the job_events table
    pub fn log_event(&self, event: &JobEvent) -> Result<()> {
        let conn = self
            .pool
            .get()
            .context("Failed to get connection from pool")?;

        conn.execute(
            "INSERT INTO job_events (job_id, event_type, timestamp, old_state, new_state, reason, gpu_ids, parameters)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                event.job_id,
                event.event_type.to_string(),
                system_time_to_unix(&event.timestamp),
                event.old_state.as_ref().map(|s| s.to_string()),
                event.new_state.as_ref().map(|s| s.to_string()),
                event.reason,
                event.gpu_ids.as_ref().map(|ids| serde_json::to_string(ids).unwrap()),
                event.parameters.as_ref().map(|p| serde_json::to_string(p).unwrap()),
            ],
        )
        .context("Failed to insert event")?;

        Ok(())
    }

    /// Get all events for a specific job
    pub fn get_job_events(&self, job_id: u32) -> Result<Vec<JobEvent>> {
        let conn = self
            .pool
            .get()
            .context("Failed to get connection from pool")?;

        let mut stmt = conn
            .prepare(
                "SELECT id, job_id, event_type, timestamp, old_state, new_state, reason, gpu_ids, parameters
                 FROM job_events
                 WHERE job_id = ?1
                 ORDER BY timestamp ASC, id ASC",
            )
            .context("Failed to prepare events query")?;

        let events = stmt
            .query_map(params![job_id], row_to_event)
            .context("Failed to query events")?
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to collect events")?;

        Ok(events)
    }

    /// Update a job and log an event atomically in a single transaction
    pub fn update_job_with_event(&self, job: &Job, event: &JobEvent) -> Result<()> {
        let mut conn = self
            .pool
            .get()
            .context("Failed to get connection from pool")?;
        let tx = conn.transaction().context("Failed to begin transaction")?;

        // Update job
        Self::update_job_tx(&tx, job)?;

        // Log event
        tx.execute(
            "INSERT INTO job_events (job_id, event_type, timestamp, old_state, new_state, reason, gpu_ids, parameters)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                event.job_id,
                event.event_type.to_string(),
                system_time_to_unix(&event.timestamp),
                event.old_state.as_ref().map(|s| s.to_string()),
                event.new_state.as_ref().map(|s| s.to_string()),
                event.reason,
                event.gpu_ids.as_ref().map(|ids| serde_json::to_string(ids).unwrap()),
                event.parameters.as_ref().map(|p| serde_json::to_string(p).unwrap()),
            ],
        )
        .context("Failed to insert event")?;

        tx.commit().context("Failed to commit transaction")?;
        Ok(())
    }

    /// Archive old completed jobs (Finished, Failed, Cancelled) older than retention_days
    /// Moves jobs and related data to archive tables and marks them as inactive
    pub fn archive_old_jobs(&self, retention_days: u32) -> Result<usize> {
        let mut conn = self
            .pool
            .get()
            .context("Failed to get connection from pool")?;
        let tx = conn.transaction().context("Failed to begin transaction")?;

        let cutoff_timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64
            - (retention_days as i64 * 86400);

        // Find jobs to archive
        let mut stmt = tx.prepare(
            "SELECT id FROM jobs
             WHERE state IN ('Finished', 'Failed', 'Cancelled')
             AND is_active = 1
             AND finished_at < ?1",
        )?;

        let job_ids: Vec<u32> = stmt
            .query_map([cutoff_timestamp], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;

        drop(stmt); // Explicitly drop stmt before continuing

        if job_ids.is_empty() {
            return Ok(0);
        }

        let archived_count = job_ids.len();
        tracing::info!("Archiving {} old jobs", archived_count);

        // Copy jobs to archive
        tx.execute(
            "INSERT INTO jobs_archive SELECT * FROM jobs WHERE id IN (SELECT value FROM json_each(?))",
            [serde_json::to_string(&job_ids)?],
        )?;

        // Copy GPU assignments to archive
        tx.execute(
            "INSERT INTO job_gpu_assignments_archive SELECT * FROM job_gpu_assignments WHERE job_id IN (SELECT value FROM json_each(?))",
            [serde_json::to_string(&job_ids)?],
        )?;

        // Copy parameters to archive
        tx.execute(
            "INSERT INTO job_parameters_archive SELECT * FROM job_parameters WHERE job_id IN (SELECT value FROM json_each(?))",
            [serde_json::to_string(&job_ids)?],
        )?;

        // Copy events to archive
        tx.execute(
            "INSERT INTO job_events_archive SELECT * FROM job_events WHERE job_id IN (SELECT value FROM json_each(?))",
            [serde_json::to_string(&job_ids)?],
        )?;

        // Mark jobs as inactive (don't delete to preserve referential integrity)
        tx.execute(
            "UPDATE jobs SET is_active = 0 WHERE id IN (SELECT value FROM json_each(?))",
            [serde_json::to_string(&job_ids)?],
        )?;

        tx.commit()
            .context("Failed to commit archival transaction")?;

        Ok(archived_count)
    }

    /// Get a job by ID, searching both active and archive tables
    pub fn get_job_including_archive(&self, id: u32) -> Result<Option<Job>> {
        // Try active jobs first
        if let Some(job) = self.get_job(id)? {
            return Ok(Some(job));
        }

        // Search archive
        let conn = self
            .pool
            .get()
            .context("Failed to get connection from pool")?;
        let mut stmt = conn.prepare("SELECT * FROM jobs_archive WHERE id = ?1")?;
        let job = stmt
            .query_row([id], row_to_job)
            .optional()
            .context("Failed to query archived job")?;

        if let Some(mut job) = job {
            // Load GPU assignments from archive
            let mut stmt = conn.prepare(
                "SELECT gpu_index FROM job_gpu_assignments_archive WHERE job_id = ?1 ORDER BY gpu_index",
            )?;
            let gpu_ids: Vec<u32> = stmt
                .query_map([id], |row| row.get(0))?
                .collect::<Result<Vec<_>, _>>()?;
            if !gpu_ids.is_empty() {
                job.gpu_ids = Some(gpu_ids);
            }

            // Load parameters from archive
            let mut stmt =
                conn.prepare("SELECT key, value FROM job_parameters_archive WHERE job_id = ?1")?;
            let params: Vec<(String, String)> = stmt
                .query_map([id], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            for (key, value) in params {
                job.parameters.insert(key, value);
            }

            return Ok(Some(job));
        }

        Ok(None)
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

/// Convert a database row to a JobEvent struct
fn row_to_event(row: &Row) -> rusqlite::Result<JobEvent> {
    let event_type_str: String = row.get("event_type")?;
    let timestamp_unix: i64 = row.get("timestamp")?;
    let old_state_str: Option<String> = row.get("old_state")?;
    let new_state_str: Option<String> = row.get("new_state")?;
    let gpu_ids_json: Option<String> = row.get("gpu_ids")?;
    let parameters_json: Option<String> = row.get("parameters")?;

    let event_type = event_type_str.parse::<EventType>().map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })?;

    let old_state = old_state_str
        .map(|s| s.parse::<JobState>())
        .transpose()
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?;

    let new_state = new_state_str
        .map(|s| s.parse::<JobState>())
        .transpose()
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?;

    let gpu_ids = gpu_ids_json
        .map(|json| serde_json::from_str(&json))
        .transpose()
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?;

    let parameters = parameters_json
        .map(|json| serde_json::from_str(&json))
        .transpose()
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?;

    Ok(JobEvent {
        id: Some(row.get("id")?),
        job_id: row.get("job_id")?,
        event_type,
        timestamp: unix_to_system_time(timestamp_unix),
        old_state,
        new_state,
        reason: row.get("reason")?,
        gpu_ids,
        parameters,
    })
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

    #[test]
    fn test_log_and_get_events() {
        use crate::core::job::{EventType, JobEvent};

        let (db, _temp) = create_test_db();

        // Insert a job first
        let job = Job {
            id: 1,
            submitted_by: "alice".to_string(),
            ..Default::default()
        };
        db.insert_job(&job).unwrap();

        // Log a Created event
        let created_event = JobEvent::created(1, JobState::Queued);
        db.log_event(&created_event).unwrap();

        // Log a StateTransition event
        let transition_event =
            JobEvent::state_transition(1, JobState::Queued, JobState::Running, None);
        db.log_event(&transition_event).unwrap();

        // Log a GPUAssignment event
        let gpu_event = JobEvent::gpu_assignment(1, vec![0, 1]);
        db.log_event(&gpu_event).unwrap();

        // Retrieve all events for this job
        let events = db.get_job_events(1).unwrap();
        assert_eq!(events.len(), 3);

        // Verify event types
        assert_eq!(events[0].event_type, EventType::Created);
        assert_eq!(events[0].job_id, 1);
        assert_eq!(events[0].new_state, Some(JobState::Queued));

        assert_eq!(events[1].event_type, EventType::StateTransition);
        assert_eq!(events[1].old_state, Some(JobState::Queued));
        assert_eq!(events[1].new_state, Some(JobState::Running));

        assert_eq!(events[2].event_type, EventType::GPUAssignment);
        assert_eq!(events[2].gpu_ids, Some(vec![0, 1]));
    }

    #[test]
    fn test_update_job_with_event() {
        use crate::core::job::JobEvent;

        let (db, _temp) = create_test_db();

        // Insert a job
        let mut job = Job {
            id: 1,
            state: JobState::Queued,
            submitted_by: "alice".to_string(),
            ..Default::default()
        };
        db.insert_job(&job).unwrap();

        // Update job state and log event atomically
        job.state = JobState::Running;
        job.started_at = Some(SystemTime::now());
        let event = JobEvent::state_transition(1, JobState::Queued, JobState::Running, None);

        db.update_job_with_event(&job, &event).unwrap();

        // Verify job was updated
        let updated_job = db.get_job(1).unwrap().unwrap();
        assert_eq!(updated_job.state, JobState::Running);
        assert!(updated_job.started_at.is_some());

        // Verify event was logged
        let events = db.get_job_events(1).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::StateTransition);
        assert_eq!(events[0].old_state, Some(JobState::Queued));
        assert_eq!(events[0].new_state, Some(JobState::Running));
    }

    #[test]
    fn test_event_ordering() {
        use crate::core::job::JobEvent;
        use std::thread;

        let (db, _temp) = create_test_db();

        let job = Job {
            id: 1,
            submitted_by: "alice".to_string(),
            ..Default::default()
        };
        db.insert_job(&job).unwrap();

        // Log multiple events with slight delays to ensure ordering
        for i in 0..5 {
            let event = JobEvent::hold(1, Some(format!("Reason {}", i)));
            db.log_event(&event).unwrap();
            thread::sleep(std::time::Duration::from_millis(10));
        }

        // Retrieve events
        let events = db.get_job_events(1).unwrap();
        assert_eq!(events.len(), 5);

        // Verify events are ordered by timestamp (and then by id)
        for i in 0..4 {
            assert!(events[i].timestamp <= events[i + 1].timestamp);
        }
    }

    #[test]
    fn test_get_active_jobs() {
        let (db, _temp) = create_test_db();

        // Insert jobs with different states
        let jobs = vec![
            Job {
                id: 1,
                state: JobState::Queued,
                submitted_by: "alice".to_string(),
                ..Default::default()
            },
            Job {
                id: 2,
                state: JobState::Running,
                submitted_by: "bob".to_string(),
                ..Default::default()
            },
            Job {
                id: 3,
                state: JobState::Hold,
                submitted_by: "charlie".to_string(),
                ..Default::default()
            },
            Job {
                id: 4,
                state: JobState::Finished,
                submitted_by: "dave".to_string(),
                ..Default::default()
            },
            Job {
                id: 5,
                state: JobState::Failed,
                submitted_by: "eve".to_string(),
                ..Default::default()
            },
            Job {
                id: 6,
                state: JobState::Cancelled,
                submitted_by: "frank".to_string(),
                ..Default::default()
            },
        ];

        for job in &jobs {
            db.insert_job(job).unwrap();
        }

        // Get all jobs
        let all_jobs = db.get_all_jobs().unwrap();
        assert_eq!(all_jobs.len(), 6);

        // Get only active jobs
        let active_jobs = db.get_active_jobs().unwrap();
        assert_eq!(active_jobs.len(), 3);

        // Verify only Queued, Running, and Hold jobs are loaded
        assert!(active_jobs.contains_key(&1)); // Queued
        assert!(active_jobs.contains_key(&2)); // Running
        assert!(active_jobs.contains_key(&3)); // Hold
        assert!(!active_jobs.contains_key(&4)); // Finished - should NOT be loaded
        assert!(!active_jobs.contains_key(&5)); // Failed - should NOT be loaded
        assert!(!active_jobs.contains_key(&6)); // Canceled - should NOT be loaded
    }

    #[test]
    fn test_query_jobs_paginated() {
        let (db, _temp) = create_test_db();

        // Insert 10 jobs with different states and users
        for i in 1..=10 {
            let state = match i % 4 {
                0 => JobState::Finished,
                1 => JobState::Queued,
                2 => JobState::Running,
                _ => JobState::Failed,
            };
            let user = if i <= 5 { "alice" } else { "bob" };

            let job = Job {
                id: i,
                state,
                submitted_by: user.to_string(),
                ..Default::default()
            };
            db.insert_job(&job).unwrap();
        }

        // Test 1: Query all jobs with pagination
        let filter = JobFilter::new();
        let (jobs, total) = db.query_jobs_paginated(&filter, 5, 0).unwrap();
        assert_eq!(total, 10);
        assert_eq!(jobs.len(), 5);
        // Jobs should be in DESC order (most recent first)
        assert_eq!(jobs[0].id, 10);
        assert_eq!(jobs[4].id, 6);

        // Test 2: Second page
        let (jobs, total) = db.query_jobs_paginated(&filter, 5, 5).unwrap();
        assert_eq!(total, 10);
        assert_eq!(jobs.len(), 5);
        assert_eq!(jobs[0].id, 5);
        assert_eq!(jobs[4].id, 1);

        // Test 3: Filter by state
        let filter = JobFilter::new().with_states(vec![JobState::Running]);
        let (jobs, total) = db.query_jobs_paginated(&filter, 10, 0).unwrap();
        assert_eq!(total, 3); // jobs 2, 6, 10 are Running (i % 4 = 2)
        assert!(jobs.iter().all(|j| j.state == JobState::Running));

        // Test 4: Filter by user
        let filter = JobFilter::new().with_users(vec!["alice".to_string()]);
        let (jobs, total) = db.query_jobs_paginated(&filter, 10, 0).unwrap();
        assert_eq!(total, 5);
        assert!(jobs.iter().all(|j| j.submitted_by == "alice"));

        // Test 5: Combined filters
        let filter = JobFilter::new()
            .with_states(vec![JobState::Queued, JobState::Running])
            .with_users(vec!["alice".to_string()]);
        let (jobs, total) = db.query_jobs_paginated(&filter, 10, 0).unwrap();
        // Alice has jobs 1,2,3,4,5
        // States: 1=Queued, 2=Running, 3=Failed, 4=Finished, 5=Queued
        // So Queued or Running: 1,2,5
        assert_eq!(total, 3);
        assert!(jobs.iter().all(|j| j.submitted_by == "alice"));
        assert!(jobs
            .iter()
            .all(|j| j.state == JobState::Queued || j.state == JobState::Running));
    }
}
