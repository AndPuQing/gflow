use anyhow::{anyhow, Context, Result};
use range_parser::parse;
use std::time::Duration;

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

/// Format duration for display (e.g., `"2h 30m 45s"`, `"45m 30s"`, `"30s"`).
///
/// # Examples
///
/// ```
/// use std::time::Duration;
/// use gflow::utils::format_duration;
///
/// assert_eq!(format_duration(Duration::from_secs(45)), "45s");
/// assert_eq!(format_duration(Duration::from_secs(1845)), "30m 45s");
/// assert_eq!(format_duration(Duration::from_secs(9045)), "2h 30m 45s");
/// ```
pub fn format_duration(duration: Duration) -> String {
    let total_secs = duration.as_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
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
    let mut parsed_ids: Vec<u32> =
        parse::<u32>(id_strings.trim()).context(format!("Invalid ID or range: {}", id_strings))?;

    parsed_ids.sort_unstable();
    parsed_ids.dedup();

    Ok(parsed_ids)
}
