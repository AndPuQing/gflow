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
use std::time::Duration;

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
    pub static ref GPU_UTILIZATION_RATIO: GaugeVec = register_gauge_vec!(
        "gflow_gpu_utilization_ratio",
        "Allocated GPU ratio (0.0-1.0)",
        &[]
    )
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
    pub static ref MEMORY_UTILIZATION_RATIO: GaugeVec = register_gauge_vec!(
        "gflow_memory_utilization_ratio",
        "Allocated memory ratio (0.0-1.0)",
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
pub fn update_job_state_metrics(jobs: &[crate::core::job::Job]) {
    use crate::core::job::JobState;
    let queued = jobs.iter().filter(|j| j.state == JobState::Queued).count();
    let running = jobs.iter().filter(|j| j.state == JobState::Running).count();
    JOBS_QUEUED
        .with_label_values(&[] as &[&str])
        .set(queued as f64);
    JOBS_RUNNING
        .with_label_values(&[] as &[&str])
        .set(running as f64);
}

#[cfg(not(feature = "metrics"))]
pub fn update_job_state_metrics(_jobs: &[crate::core::job::Job]) {
    // No-op when metrics feature is disabled
}

#[cfg(feature = "metrics")]
pub fn update_job_state_metrics_runtimes(runtimes: &[crate::core::job::JobRuntime]) {
    use crate::core::job::JobState;
    let queued = runtimes
        .iter()
        .filter(|rt| rt.state == JobState::Queued)
        .count();
    let running = runtimes
        .iter()
        .filter(|rt| rt.state == JobState::Running)
        .count();

    JOBS_QUEUED
        .with_label_values(&[] as &[&str])
        .set(queued as f64);
    JOBS_RUNNING
        .with_label_values(&[] as &[&str])
        .set(running as f64);
}

#[cfg(not(feature = "metrics"))]
pub fn update_job_state_metrics_runtimes(_runtimes: &[crate::core::job::JobRuntime]) {
    // No-op when metrics feature is disabled
}

#[cfg(feature = "metrics")]
pub fn update_resource_metrics(
    available_gpus: usize,
    total_gpus: usize,
    available_memory_mb: u64,
    total_memory_mb: u64,
) {
    let gpu_utilization = if total_gpus == 0 {
        0.0
    } else {
        1.0 - (available_gpus as f64 / total_gpus as f64)
    };
    let memory_utilization = if total_memory_mb == 0 {
        0.0
    } else {
        1.0 - (available_memory_mb as f64 / total_memory_mb as f64)
    };

    GPU_AVAILABLE
        .with_label_values(&[] as &[&str])
        .set(available_gpus as f64);
    GPU_TOTAL
        .with_label_values(&[] as &[&str])
        .set(total_gpus as f64);
    GPU_UTILIZATION_RATIO
        .with_label_values(&[] as &[&str])
        .set(gpu_utilization);
    MEMORY_AVAILABLE_MB
        .with_label_values(&[] as &[&str])
        .set(available_memory_mb as f64);
    MEMORY_TOTAL_MB
        .with_label_values(&[] as &[&str])
        .set(total_memory_mb as f64);
    MEMORY_UTILIZATION_RATIO
        .with_label_values(&[] as &[&str])
        .set(memory_utilization);
}

#[cfg(not(feature = "metrics"))]
pub fn update_resource_metrics(
    _available_gpus: usize,
    _total_gpus: usize,
    _available_memory_mb: u64,
    _total_memory_mb: u64,
) {
    // No-op when metrics feature is disabled
}

#[cfg(feature = "metrics")]
pub fn observe_scheduler_latency(operation: &str, duration: Duration) {
    SCHEDULER_LATENCY
        .with_label_values(&[operation])
        .observe(duration.as_secs_f64());
}

#[cfg(not(feature = "metrics"))]
pub fn observe_scheduler_latency(_operation: &str, _duration: std::time::Duration) {
    // No-op when metrics feature is disabled
}
