use anyhow::{anyhow, Context, Result};
use clap::builder::{
    styling::{AnsiColor, Effects},
    Styles,
};
use range_parser::parse;
use std::time::{Duration, SystemTime};

/// Parse time limit string into Duration.
///
/// Supported formats:
/// - `"HH:MM:SS"` — hours:minutes:seconds
/// - `"MM:SS"` — minutes:seconds
/// - `"MM"` — minutes
///
/// # Examples
///
/// ```
/// use std::time::Duration;
/// use gflow::utils::parse_time_limit;
///
/// assert_eq!(parse_time_limit("30").unwrap(), Duration::from_secs(1800));
/// assert_eq!(parse_time_limit("30:45").unwrap(), Duration::from_secs(1845));
/// assert_eq!(parse_time_limit("2:30:45").unwrap(), Duration::from_secs(9045));
/// ```
pub fn parse_time_limit(time_str: &str) -> Result<Duration> {
    let parts: Vec<&str> = time_str.split(':').collect();

    match parts.len() {
        1 => {
            // Minutes as a single number
            let val = time_str
                .parse::<u64>()
                .context("Invalid time format. Expected number of minutes")?;
            Ok(Duration::from_secs(val * 60))
        }
        2 => {
            // MM:SS
            let minutes = parts[0]
                .parse::<u64>()
                .context("Invalid minutes in MM:SS format")?;
            let seconds = parts[1]
                .parse::<u64>()
                .context("Invalid seconds in MM:SS format")?;
            Ok(Duration::from_secs(minutes * 60 + seconds))
        }
        3 => {
            // HH:MM:SS
            let hours = parts[0]
                .parse::<u64>()
                .context("Invalid hours in HH:MM:SS format")?;
            let minutes = parts[1]
                .parse::<u64>()
                .context("Invalid minutes in HH:MM:SS format")?;
            let seconds = parts[2]
                .parse::<u64>()
                .context("Invalid seconds in HH:MM:SS format")?;
            Ok(Duration::from_secs(hours * 3600 + minutes * 60 + seconds))
        }
        _ => Err(anyhow!(
            "Invalid time format. Expected formats: HH:MM:SS, MM:SS, or MM"
        )),
    }
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

/// Parse memory limit string into megabytes.
///
/// Supported formats:
/// - `"100G"` or `"100g"` — gigabytes (converted to MB)
/// - `"1024M"` or `"1024m"` — megabytes
/// - `"100"` — megabytes (default unit)
///
/// # Examples
///
/// ```
/// use gflow::utils::parse_memory_limit;
///
/// assert_eq!(parse_memory_limit("100").unwrap(), 100);
/// assert_eq!(parse_memory_limit("1024M").unwrap(), 1024);
/// assert_eq!(parse_memory_limit("2G").unwrap(), 2048);
/// ```
pub fn parse_memory_limit(memory_str: &str) -> Result<u64> {
    let memory_str = memory_str.trim();

    if memory_str.is_empty() {
        return Err(anyhow!("Memory limit cannot be empty"));
    }

    // Check if ends with 'G' or 'g' (gigabytes)
    if memory_str.ends_with('G') || memory_str.ends_with('g') {
        let value = memory_str[..memory_str.len() - 1]
            .trim()
            .parse::<u64>()
            .context("Invalid memory value in GB format")?;
        Ok(value * 1024) // Convert GB to MB
    }
    // Check if ends with 'M' or 'm' (megabytes)
    else if memory_str.ends_with('M') || memory_str.ends_with('m') {
        let value = memory_str[..memory_str.len() - 1]
            .trim()
            .parse::<u64>()
            .context("Invalid memory value in MB format")?;
        Ok(value)
    }
    // Otherwise, treat as megabytes
    else {
        memory_str
            .parse::<u64>()
            .context("Invalid memory format. Expected formats: 100G, 1024M, or 100 (MB)")
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

/// Parse job IDs from string inputs, supporting ranges like "1-3" or comma-separated "1,2,3".
///
/// # Examples
///
/// ```
/// use gflow::utils::parse_job_ids;
///
/// assert_eq!(parse_job_ids("1").unwrap(), vec![1]);
/// assert_eq!(parse_job_ids("1,2,3").unwrap(), vec![1, 2, 3]);
/// assert_eq!(parse_job_ids("1-3").unwrap(), vec![1, 2, 3]);
/// assert_eq!(parse_job_ids("1-3,5").unwrap(), vec![1, 2, 3, 5]);
/// ```
pub fn parse_job_ids(id_strings: &str) -> Result<Vec<u32>> {
    parse_indices(id_strings, "job ID")
}

/// Parse GPU indices from string inputs, supporting ranges like "0-2" or comma-separated "0,1,2".
///
/// # Examples
///
/// ```
/// use gflow::utils::parse_gpu_indices;
///
/// assert_eq!(parse_gpu_indices("0").unwrap(), vec![0]);
/// assert_eq!(parse_gpu_indices("0,2,4").unwrap(), vec![0, 2, 4]);
/// assert_eq!(parse_gpu_indices("0-2").unwrap(), vec![0, 1, 2]);
/// assert_eq!(parse_gpu_indices("0-1,3").unwrap(), vec![0, 1, 3]);
/// ```
pub fn parse_gpu_indices(gpu_string: &str) -> Result<Vec<u32>> {
    parse_indices(gpu_string, "GPU index")
}

/// Helper function to parse indices from string inputs.
/// Supports ranges like "1-3" or comma-separated "1,2,3".
fn parse_indices(input: &str, item_type: &str) -> Result<Vec<u32>> {
    let mut parsed: Vec<u32> =
        parse::<u32>(input.trim()).context(format!("Invalid {} or range: {}", item_type, input))?;

    parsed.sort_unstable();
    parsed.dedup();

    Ok(parsed)
}

pub const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Green.on_default().effects(Effects::BOLD))
    .usage(AnsiColor::Green.on_default().effects(Effects::BOLD))
    .literal(AnsiColor::Cyan.on_default().effects(Effects::BOLD))
    .placeholder(AnsiColor::Cyan.on_default());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_gpu_indices_single() {
        assert_eq!(parse_gpu_indices("0").unwrap(), vec![0]);
        assert_eq!(parse_gpu_indices("5").unwrap(), vec![5]);
        assert_eq!(parse_gpu_indices("10").unwrap(), vec![10]);
    }

    #[test]
    fn test_parse_gpu_indices_comma_separated() {
        assert_eq!(parse_gpu_indices("0,2,4").unwrap(), vec![0, 2, 4]);
        assert_eq!(parse_gpu_indices("1,3,5,7").unwrap(), vec![1, 3, 5, 7]);
        // Test unsorted input gets sorted
        assert_eq!(parse_gpu_indices("3,1,2").unwrap(), vec![1, 2, 3]);
    }

    #[test]
    fn test_parse_gpu_indices_range() {
        assert_eq!(parse_gpu_indices("0-2").unwrap(), vec![0, 1, 2]);
        assert_eq!(parse_gpu_indices("5-7").unwrap(), vec![5, 6, 7]);
        assert_eq!(parse_gpu_indices("0-0").unwrap(), vec![0]);
    }

    #[test]
    fn test_parse_gpu_indices_mixed() {
        assert_eq!(parse_gpu_indices("0-1,3").unwrap(), vec![0, 1, 3]);
        assert_eq!(parse_gpu_indices("0-1,3,5-6").unwrap(), vec![0, 1, 3, 5, 6]);
        assert_eq!(parse_gpu_indices("0,2-4,7").unwrap(), vec![0, 2, 3, 4, 7]);
    }

    #[test]
    fn test_parse_gpu_indices_duplicates() {
        // Duplicates should be removed
        assert_eq!(parse_gpu_indices("0,0,1,1").unwrap(), vec![0, 1]);
        assert_eq!(parse_gpu_indices("0-2,1-3").unwrap(), vec![0, 1, 2, 3]);
        assert_eq!(parse_gpu_indices("5,5,5").unwrap(), vec![5]);
    }

    #[test]
    fn test_parse_gpu_indices_whitespace() {
        assert_eq!(parse_gpu_indices("  0  ").unwrap(), vec![0]);
        assert_eq!(parse_gpu_indices(" 0,2,4 ").unwrap(), vec![0, 2, 4]);
        assert_eq!(parse_gpu_indices("  0-2  ").unwrap(), vec![0, 1, 2]);
    }

    #[test]
    fn test_parse_gpu_indices_empty() {
        assert!(parse_gpu_indices("").is_err());
        assert!(parse_gpu_indices("  ").is_err());
        assert!(parse_gpu_indices("\t").is_err());
    }

    #[test]
    fn test_parse_gpu_indices_invalid() {
        assert!(parse_gpu_indices("abc").is_err());
        assert!(parse_gpu_indices("gpu0").is_err());
        assert!(parse_gpu_indices("0..2").is_err());
        assert!(parse_gpu_indices("-1").is_err());
    }
}
