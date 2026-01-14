//! Prometheus metrics for gflow scheduler
//!
//! # Cardinality Warning
//! Per-user labels on counters can lead to high cardinality in environments with many users.
//! In high-scale deployments, consider:
//! - Using unlabelled totals for aggregate metrics
//! - Implementing optional per-user breakdown via configuration
//! - Setting up metric relabeling in your Prometheus scraper
//! - Monitoring cardinality with Prometheus queries like `count({__name__=~"gflow_.*"})`

#[cfg(feature = "metrics")]
use lazy_static::lazy_static;
#[cfg(feature = "metrics")]
use prometheus::{
    register_counter_vec, register_gauge_vec, register_histogram_vec, CounterVec, Encoder,
    GaugeVec, HistogramVec, TextEncoder,
};

#[cfg(feature = "metrics")]
lazy_static! {
    // Job lifecycle counters (labeled by user - watch for high cardinality)
    pub static ref JOB_SUBMISSIONS: CounterVec = register_counter_vec!(
        "gflow_jobs_submitted_total",
        "Total jobs submitted",
        &["user"]
    )
    .unwrap();
    pub static ref JOB_FINISHED: CounterVec = register_counter_vec!(
        "gflow_jobs_finished_total",
        "Total jobs finished",
        &["user"]
    )
    .unwrap();
    pub static ref JOB_FAILED: CounterVec = register_counter_vec!(
        "gflow_jobs_failed_total",
        "Total jobs failed",
        &["user"]
    )
    .unwrap();
    pub static ref JOB_CANCELLED: CounterVec = register_counter_vec!(
        "gflow_jobs_cancelled_total",
        "Total jobs cancelled",
        &["user"]
    )
    .unwrap();
    // Current state gauges
    pub static ref JOBS_QUEUED: GaugeVec = register_gauge_vec!(
        "gflow_jobs_queued",
        "Jobs currently queued",
        &[]
    )
    .unwrap();
    pub static ref JOBS_RUNNING: GaugeVec = register_gauge_vec!(
        "gflow_jobs_running",
        "Jobs currently running",
        &[]
    )
    .unwrap();
    // GPU metrics
    pub static ref GPU_AVAILABLE: GaugeVec = register_gauge_vec!(
        "gflow_gpus_available",
        "Available GPUs",
        &[]
    )
    .unwrap();
    pub static ref GPU_TOTAL: GaugeVec = register_gauge_vec!("gflow_gpus_total", "Total GPUs", &[])
        .unwrap();
    // Memory metrics
    pub static ref MEMORY_AVAILABLE_MB: GaugeVec = register_gauge_vec!(
        "gflow_memory_available_mb",
        "Available memory in MB",
        &[]
    )
    .unwrap();
    pub static ref MEMORY_TOTAL_MB: GaugeVec = register_gauge_vec!(
        "gflow_memory_total_mb",
        "Total memory in MB",
        &[]
    )
    .unwrap();
    // Scheduler performance
    pub static ref SCHEDULER_LATENCY: HistogramVec = register_histogram_vec!(
        "gflow_scheduler_latency_seconds",
        "Scheduler operation latency",
        &["operation"],
        vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 2.0, 5.0]
    )
    .unwrap();
}

#[cfg(feature = "metrics")]
pub fn export_metrics() -> Result<String, Box<dyn std::error::Error>> {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer)?;
    Ok(String::from_utf8(buffer)?)
}

#[cfg(not(feature = "metrics"))]
pub fn export_metrics() -> Result<String, Box<dyn std::error::Error>> {
    Ok(String::from("# Metrics feature not enabled\n"))
}

// Helper functions
#[cfg(feature = "metrics")]
pub fn update_job_state_metrics(jobs: &std::collections::HashMap<u32, crate::core::job::Job>) {
    use crate::core::job::JobState;
    let queued = jobs
        .values()
        .filter(|j| j.state == JobState::Queued)
        .count();
    let running = jobs
        .values()
        .filter(|j| j.state == JobState::Running)
        .count();
    JOBS_QUEUED
        .with_label_values(&[] as &[&str])
        .set(queued as f64);
    JOBS_RUNNING
        .with_label_values(&[] as &[&str])
        .set(running as f64);
}

#[cfg(not(feature = "metrics"))]
pub fn update_job_state_metrics(_jobs: &std::collections::HashMap<u32, crate::core::job::Job>) {
    // No-op when metrics feature is disabled
}
