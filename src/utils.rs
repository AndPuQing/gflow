pub mod parsers;
pub mod timezone;

use anyhow::{anyhow, Result};
use clap::builder::{
    styling::{AnsiColor, Effects},
    Styles,
};
use regex::Regex;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

// Re-export parser functions for backward compatibility
pub use parsers::{
    parse_gpu_indices, parse_job_ids, parse_memory_limit, parse_since_time, parse_time_limit,
};

/// Substitute {param_name} patterns in command with actual values.
///
/// # Examples
///
/// ```
/// use std::collections::HashMap;
/// use gflow::utils::substitute_parameters;
///
/// let mut params = HashMap::new();
/// params.insert("id".to_string(), "123".to_string());
/// params.insert("model".to_string(), "gpt-4".to_string());
///
/// let template = "python eval.py --task_id '{id}' --model '{model}'";
/// let result = substitute_parameters(template, &params).unwrap();
/// assert_eq!(result, "python eval.py --task_id '123' --model 'gpt-4'");
/// ```
pub fn substitute_parameters(
    command: &str,
    parameters: &HashMap<String, String>,
) -> Result<String> {
    let re = Regex::new(r"\{([a-zA-Z_][a-zA-Z0-9_]*)\}").unwrap();
    let mut result = command.to_string();
    let mut missing_params = Vec::new();

    for cap in re.captures_iter(command) {
        let param_name = &cap[1];
        if let Some(value) = parameters.get(param_name) {
            let pattern = format!("{{{}}}", param_name);
            result = result.replace(&pattern, value);
        } else {
            missing_params.push(param_name.to_string());
        }
    }

    if !missing_params.is_empty() {
        return Err(anyhow!(
            "Missing parameter values: {}",
            missing_params.join(", ")
        ));
    }

    Ok(result)
}

/// Format duration for display in HH:MM:SS format.
///
/// Displays time with hours as the maximum unit (no days).
/// Format: `HH:MM:SS` where hours can exceed 24.
///
/// # Examples
///
/// ```
/// use std::time::Duration;
/// use gflow::utils::format_duration;
///
/// assert_eq!(format_duration(Duration::from_secs(45)), "00:00:45");
/// assert_eq!(format_duration(Duration::from_secs(1845)), "00:30:45");
/// assert_eq!(format_duration(Duration::from_secs(9045)), "02:30:45");
/// assert_eq!(format_duration(Duration::from_secs(90000)), "25:00:00");
/// ```
pub fn format_duration(duration: Duration) -> String {
    let total_secs = duration.as_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
}

/// Format elapsed time between two system times in HH:MM:SS format.
///
/// For finished jobs, calculates the duration between `started_at` and `finished_at`.
/// For running jobs, calculates the duration from `started_at` to now.
/// Returns "-" if `started_at` is `None`.
///
/// # Examples
///
/// ```
/// use std::time::{SystemTime, Duration};
/// use gflow::utils::format_elapsed_time;
///
/// let start = SystemTime::now();
/// let end = start + Duration::from_secs(3665);
/// assert_eq!(format_elapsed_time(Some(start), Some(end)), "01:01:05");
/// assert_eq!(format_elapsed_time(None, None), "-");
/// ```
pub fn format_elapsed_time(
    started_at: Option<SystemTime>,
    finished_at: Option<SystemTime>,
) -> String {
    match started_at {
        Some(start_time) => {
            let end_time = finished_at.unwrap_or_else(SystemTime::now);

            if let Ok(elapsed) = end_time.duration_since(start_time) {
                format_duration(elapsed)
            } else {
                "-".to_string()
            }
        }
        None => "-".to_string(),
    }
}

/// Format memory in MB for display (e.g., `"2.5G"`, `"1024M"`, `"512M"`).
///
/// # Examples
///
/// ```
/// use gflow::utils::format_memory;
///
/// assert_eq!(format_memory(100), "100M");
/// assert_eq!(format_memory(1024), "1G");
/// assert_eq!(format_memory(2560), "2.5G");
/// ```
pub fn format_memory(memory_mb: u64) -> String {
    if memory_mb >= 1024 {
        let gb = memory_mb as f64 / 1024.0;
        if gb.fract() < 0.01 {
            format!("{:.0}G", gb)
        } else {
            format!("{:.1}G", gb)
        }
    } else {
        format!("{}M", memory_mb)
    }
}

pub const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Green.on_default().effects(Effects::BOLD))
    .usage(AnsiColor::Green.on_default().effects(Effects::BOLD))
    .literal(AnsiColor::Cyan.on_default().effects(Effects::BOLD))
    .placeholder(AnsiColor::Cyan.on_default());

/// Format SystemTime as a human-readable string in UTC.
///
/// Formats the time as `YYYY-MM-DD HH:MM:SS UTC`.
///
/// # Examples
///
/// ```
/// use std::time::{SystemTime, Duration};
/// use gflow::utils::format_system_time;
///
/// let time = SystemTime::UNIX_EPOCH + Duration::from_secs(1704067200);
/// assert_eq!(format_system_time(time), "2024-01-01 00:00:00 UTC");
/// ```
pub fn format_system_time(time: SystemTime) -> String {
    use chrono::{DateTime, Utc};

    let duration = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let datetime =
        DateTime::<Utc>::from_timestamp(duration.as_secs() as i64, 0).unwrap_or_default();

    datetime.format("%Y-%m-%d %H:%M:%S UTC").to_string()
}

/// Format duration in a compact, human-readable format.
///
/// Displays the most significant units only (hours and minutes, or minutes and seconds).
/// Unlike `format_duration()` which uses HH:MM:SS format, this provides a more
/// natural reading format like "2h 30m" or "45s".
///
/// # Examples
///
/// ```
/// use std::time::Duration;
/// use gflow::utils::format_duration_compact;
///
/// assert_eq!(format_duration_compact(Duration::from_secs(45)), "45s");
/// assert_eq!(format_duration_compact(Duration::from_secs(1845)), "30m 45s");
/// assert_eq!(format_duration_compact(Duration::from_secs(9045)), "2h 30m");
/// assert_eq!(format_duration_compact(Duration::from_secs(7200)), "2h");
/// ```
pub fn format_duration_compact(duration: Duration) -> String {
    let total_secs = duration.as_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    if hours > 0 {
        if minutes > 0 {
            format!("{}h {}m", hours, minutes)
        } else {
            format!("{}h", hours)
        }
    } else if minutes > 0 {
        if seconds > 0 {
            format!("{}m {}s", minutes, seconds)
        } else {
            format!("{}m", minutes)
        }
    } else {
        format!("{}s", seconds)
    }
}

/// Validates that a job is in the expected state.
/// Returns an error with a user-friendly message if the state doesn't match.
///
/// This is a convenience function to reduce boilerplate in CLI tools.
///
/// # Examples
///
/// ```no_run
/// use gflow::utils::validate_job_state;
/// use gflow::core::job::{Job, JobState, JobBuilder};
///
/// let job = JobBuilder::new()
///     .submitted_by("test".to_string())
///     .run_dir("/tmp")
///     .build();
///
/// // This will succeed since the job is in Queued state
/// validate_job_state(&job, JobState::Queued, "held").unwrap();
/// ```
pub fn validate_job_state(
    job: &crate::core::job::Job,
    expected_state: crate::core::job::JobState,
    operation: &str,
) -> Result<()> {
    if job.state != expected_state {
        Err(anyhow!(
            "Job {} is in state '{}' and cannot be {}. Only {} jobs can be {}.",
            job.id,
            job.state,
            operation,
            expected_state,
            operation
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {}
