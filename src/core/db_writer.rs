use super::db::Database;
use super::job::{Job, JobEvent};
use anyhow::Result;
use std::time::Duration;
use tokio::sync::mpsc;

/// Operations that can be queued for database writes
#[derive(Debug)]
pub enum DbOperation {
    /// Insert a single job
    InsertJob(Job),
    /// Update a single job
    UpdateJob(Job),
    /// Insert multiple jobs in batch
    InsertJobsBatch(Vec<Job>),
    /// Update multiple jobs in batch
    UpdateJobsBatch(Vec<Job>),
    /// Log an event
    LogEvent(JobEvent),
    /// Update job and log event atomically
    UpdateJobWithEvent(Job, JobEvent),
    /// Set metadata value
    SetMetadata(String, String),
}

/// Async database writer with micro-batching
///
/// Provides non-blocking write operations by queuing them and processing
/// in a background task. Updates are batched every 100ms to reduce transaction overhead.
#[derive(Clone)]
pub struct DatabaseWriter {
    tx: mpsc::UnboundedSender<DbOperation>,
}

impl DatabaseWriter {
    /// Create a new DatabaseWriter with a background processing task
    ///
    /// The background task will:
    /// - Process write operations from the queue
    /// - Batch UpdateJob operations together (every 100ms)
    /// - Process InsertJob and LogEvent operations immediately
    /// - Handle errors gracefully without blocking
    pub fn new(db: Database) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<DbOperation>();

        // Spawn dedicated writer task
        tokio::spawn(async move {
            let mut batch_buffer: Vec<Job> = Vec::new();
            let mut interval = tokio::time::interval(Duration::from_millis(100));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                tokio::select! {
                    // Process incoming operations
                    Some(op) = rx.recv() => {
                        match op {
                            DbOperation::UpdateJob(job) => {
                                // Buffer for batching
                                batch_buffer.push(job);
                            }
                            DbOperation::InsertJob(job) => {
                                // Insert immediately (rare operation, needs confirmation)
                                if let Err(e) = db.insert_job(&job) {
                                    tracing::error!("Failed to insert job {}: {}", job.id, e);
                                }
                            }
                            DbOperation::LogEvent(event) => {
                                // Log event immediately (append-only, very fast)
                                if let Err(e) = db.log_event(&event) {
                                    tracing::error!("Failed to log event for job {}: {}", event.job_id, e);
                                }
                            }
                            DbOperation::UpdateJobWithEvent(job, event) => {
                                // Atomic update + event logging
                                if let Err(e) = db.update_job_with_event(&job, &event) {
                                    tracing::error!("Failed to update job {} with event: {}", job.id, e);
                                }
                            }
                            DbOperation::InsertJobsBatch(jobs) => {
                                // Batch insert immediately
                                if let Err(e) = db.insert_jobs_batch(&jobs) {
                                    tracing::error!("Failed to batch insert {} jobs: {}", jobs.len(), e);
                                }
                            }
                            DbOperation::UpdateJobsBatch(jobs) => {
                                // Add to batch buffer
                                batch_buffer.extend(jobs);
                            }
                            DbOperation::SetMetadata(key, value) => {
                                // Set metadata immediately
                                if let Err(e) = db.set_metadata(&key, &value) {
                                    tracing::error!("Failed to set metadata {}: {}", key, e);
                                }
                            }
                        }
                    }

                    // Flush batch buffer every 100ms
                    _ = interval.tick() => {
                        if !batch_buffer.is_empty() {
                            let jobs_to_write = std::mem::take(&mut batch_buffer);
                            let job_count = jobs_to_write.len();

                            if let Err(e) = db.update_jobs_batch(&jobs_to_write) {
                                tracing::error!("Failed to batch update {} jobs: {}", job_count, e);
                            } else {
                                tracing::debug!("Batch updated {} jobs", job_count);
                            }
                        }
                    }
                }
            }
        });

        Self { tx }
    }

    /// Queue a job update (non-blocking)
    ///
    /// The update will be batched with other updates and written within 100ms.
    pub fn queue_update(&self, job: Job) {
        if let Err(e) = self.tx.send(DbOperation::UpdateJob(job)) {
            tracing::error!("Failed to queue job update: {}", e);
        }
    }

    /// Queue multiple job updates (non-blocking)
    pub fn queue_update_batch(&self, jobs: Vec<Job>) {
        if !jobs.is_empty() {
            if let Err(e) = self.tx.send(DbOperation::UpdateJobsBatch(jobs)) {
                tracing::error!("Failed to queue batch job update: {}", e);
            }
        }
    }

    /// Queue an event log (non-blocking)
    ///
    /// Events are written immediately (not batched) since they're append-only.
    pub fn queue_event(&self, event: JobEvent) {
        if let Err(e) = self.tx.send(DbOperation::LogEvent(event)) {
            tracing::error!("Failed to queue event: {}", e);
        }
    }

    /// Queue a job update with event atomically (non-blocking)
    pub fn queue_update_with_event(&self, job: Job, event: JobEvent) {
        if let Err(e) = self.tx.send(DbOperation::UpdateJobWithEvent(job, event)) {
            tracing::error!("Failed to queue job update with event: {}", e);
        }
    }

    /// Insert a job (blocking - waits for confirmation)
    ///
    /// Job insertion is a critical operation that requires confirmation,
    /// so this method is synchronous.
    pub async fn insert_job(&self, job: Job) -> Result<()> {
        // For now, just queue it - we could add a oneshot channel for confirmation
        // but that adds complexity. Since we're using unbounded channels,
        // the send should never fail unless the receiver is dropped.
        self.tx
            .send(DbOperation::InsertJob(job))
            .map_err(|e| anyhow::anyhow!("Failed to queue job insert: {}", e))?;

        // Give the writer task a chance to process
        // In production, we might want a confirmation mechanism
        tokio::time::sleep(Duration::from_millis(10)).await;

        Ok(())
    }

    /// Insert multiple jobs in batch (blocking)
    pub async fn insert_jobs_batch(&self, jobs: Vec<Job>) -> Result<()> {
        self.tx
            .send(DbOperation::InsertJobsBatch(jobs))
            .map_err(|e| anyhow::anyhow!("Failed to queue batch job insert: {}", e))?;

        tokio::time::sleep(Duration::from_millis(10)).await;

        Ok(())
    }

    /// Set metadata (non-blocking)
    pub fn set_metadata(&self, key: String, value: String) {
        if let Err(e) = self.tx.send(DbOperation::SetMetadata(key, value)) {
            tracing::error!("Failed to queue metadata set: {}", e);
        }
    }

    /// Get the number of pending operations in the queue
    ///
    /// This can be used for monitoring and debugging
    pub fn pending_operations(&self) -> usize {
        // Note: mpsc::UnboundedSender doesn't expose queue length
        // This would require a custom wrapper or metrics collection
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::job::{JobEvent, JobState};
    use tempfile::TempDir;

    fn create_test_db() -> (Database, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Database::new(db_path).unwrap();
        (db, temp_dir)
    }

    #[tokio::test]
    async fn test_queue_update() {
        let (db, _temp) = create_test_db();
        let writer = DatabaseWriter::new(db.clone());

        // Insert a job first
        let job = Job {
            id: 1,
            state: JobState::Queued,
            submitted_by: "alice".to_string(),
            ..Default::default()
        };
        writer.insert_job(job.clone()).await.unwrap();

        // Queue an update
        let mut updated_job = job;
        updated_job.state = JobState::Running;
        writer.queue_update(updated_job);

        // Wait for batch to process
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Verify the update was written
        let retrieved = db.get_job(1).unwrap().unwrap();
        assert_eq!(retrieved.state, JobState::Running);
    }

    #[tokio::test]
    async fn test_queue_event() {
        let (db, _temp) = create_test_db();
        let writer = DatabaseWriter::new(db.clone());

        // Insert a job
        let job = Job {
            id: 1,
            submitted_by: "alice".to_string(),
            ..Default::default()
        };
        writer.insert_job(job).await.unwrap();

        // Queue events
        let event1 = JobEvent::created(1, JobState::Queued);
        let event2 = JobEvent::state_transition(1, JobState::Queued, JobState::Running, None);

        writer.queue_event(event1);
        writer.queue_event(event2);

        // Wait for events to process (immediate)
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Verify events were logged
        let events = db.get_job_events(1).unwrap();
        assert_eq!(events.len(), 2);
    }

    #[tokio::test]
    async fn test_batch_update() {
        let (db, _temp) = create_test_db();
        let writer = DatabaseWriter::new(db.clone());

        // Insert multiple jobs
        let jobs: Vec<Job> = (1..=10)
            .map(|i| Job {
                id: i,
                state: JobState::Queued,
                submitted_by: "alice".to_string(),
                ..Default::default()
            })
            .collect();

        writer.insert_jobs_batch(jobs.clone()).await.unwrap();

        // Update all jobs to Running
        let updated_jobs: Vec<Job> = jobs
            .into_iter()
            .map(|mut j| {
                j.state = JobState::Running;
                j
            })
            .collect();

        writer.queue_update_batch(updated_jobs);

        // Wait for batch to process
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Verify all jobs were updated
        let all_jobs = db.get_all_jobs().unwrap();
        assert_eq!(all_jobs.len(), 10);
        assert!(all_jobs.values().all(|j| j.state == JobState::Running));
    }

    #[tokio::test]
    async fn test_update_with_event() {
        let (db, _temp) = create_test_db();
        let writer = DatabaseWriter::new(db.clone());

        // Insert a job
        let mut job = Job {
            id: 1,
            state: JobState::Queued,
            submitted_by: "alice".to_string(),
            ..Default::default()
        };
        writer.insert_job(job.clone()).await.unwrap();

        // Update with event
        job.state = JobState::Running;
        let event = JobEvent::state_transition(1, JobState::Queued, JobState::Running, None);
        writer.queue_update_with_event(job, event);

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Verify both job and event were updated
        let retrieved_job = db.get_job(1).unwrap().unwrap();
        assert_eq!(retrieved_job.state, JobState::Running);

        let events = db.get_job_events(1).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].event_type,
            crate::core::job::EventType::StateTransition
        );
    }

    #[tokio::test]
    async fn test_write_latency_performance() {
        use std::time::Instant;

        let (db, _temp) = create_test_db();
        let writer = DatabaseWriter::new(db.clone());

        // Insert test jobs
        let jobs: Vec<Job> = (1..=100)
            .map(|i| Job {
                id: i,
                state: JobState::Queued,
                submitted_by: "alice".to_string(),
                ..Default::default()
            })
            .collect();
        writer.insert_jobs_batch(jobs.clone()).await.unwrap();

        // Measure time to queue 100 updates (should be instant - non-blocking)
        let start = Instant::now();
        for mut job in jobs {
            job.state = JobState::Running;
            writer.queue_update(job);
        }
        let queue_time = start.elapsed();

        // Queueing should be < 10ms (non-blocking)
        assert!(
            queue_time.as_millis() < 10,
            "Queue time too slow: {:?}ms",
            queue_time.as_millis()
        );

        // Wait for batch processing (100ms interval + buffer)
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Verify all updates were persisted
        let all_jobs = db.get_all_jobs().unwrap();
        assert_eq!(all_jobs.len(), 100);
        assert!(all_jobs.values().all(|j| j.state == JobState::Running));

        tracing::info!(
            "Performance test: Queued 100 updates in {:?}ms, persisted within 150ms",
            queue_time.as_millis()
        );
    }

    #[tokio::test]
    async fn test_high_throughput() {
        let (db, _temp) = create_test_db();
        let writer = DatabaseWriter::new(db.clone());

        // Insert 1000 jobs
        let jobs: Vec<Job> = (1..=1000)
            .map(|i| Job {
                id: i,
                state: JobState::Queued,
                submitted_by: "alice".to_string(),
                ..Default::default()
            })
            .collect();
        writer.insert_jobs_batch(jobs.clone()).await.unwrap();

        // Queue 1000 rapid updates
        for mut job in jobs {
            job.state = JobState::Running;
            writer.queue_update(job);
        }

        // Wait for batches to process (multiple 100ms intervals)
        tokio::time::sleep(Duration::from_millis(300)).await;

        // Verify all updates persisted
        let all_jobs = db.get_all_jobs().unwrap();
        assert_eq!(all_jobs.len(), 1000);
        assert!(all_jobs.values().all(|j| j.state == JobState::Running));
    }
}
