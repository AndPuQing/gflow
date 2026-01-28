use anyhow::{anyhow, Context, Result};
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
/// use gflow::utils::parsers::parse_time_limit;
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
/// use gflow::utils::parsers::parse_memory_limit;
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

/// Parse job IDs from string inputs, supporting ranges like "1-3" or comma-separated "1,2,3".
///
/// # Examples
///
/// ```
/// use gflow::utils::parsers::parse_job_ids;
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
/// use gflow::utils::parsers::parse_gpu_indices;
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

/// Parse time duration string into seconds since epoch (for filtering).
///
/// Supported formats:
/// - `"1h"`, `"2d"`, `"3w"` — relative time (hours, days, weeks)
/// - `"today"` — start of today (00:00:00)
/// - `"yesterday"` — start of yesterday (00:00:00)
/// - ISO 8601 timestamp (e.g., `"2024-01-10T00:00:00Z"`)
///
/// Returns Unix timestamp (seconds since epoch).
///
/// # Examples
///
/// ```
/// use gflow::utils::parsers::parse_since_time;
///
/// // These would return timestamps relative to current time
/// assert!(parse_since_time("1h").is_ok());
/// assert!(parse_since_time("2d").is_ok());
/// assert!(parse_since_time("today").is_ok());
/// ```
pub fn parse_since_time(time_str: &str) -> Result<i64> {
    let time_str = time_str.trim().to_lowercase();
    let now = SystemTime::now();

    // Handle relative time formats (1h, 2d, 3w)
    if let Some(stripped) = time_str.strip_suffix('h') {
        let hours = stripped.parse::<u64>().context("Invalid hours format")?;
        let duration = Duration::from_secs(hours * 3600);
        let since = now
            .checked_sub(duration)
            .ok_or_else(|| anyhow!("Time calculation overflow"))?;
        return Ok(since
            .duration_since(SystemTime::UNIX_EPOCH)
            .context("Failed to convert to Unix timestamp")?
            .as_secs() as i64);
    }

    if let Some(stripped) = time_str.strip_suffix('d') {
        let days = stripped.parse::<u64>().context("Invalid days format")?;
        let duration = Duration::from_secs(days * 86400);
        let since = now
            .checked_sub(duration)
            .ok_or_else(|| anyhow!("Time calculation overflow"))?;
        return Ok(since
            .duration_since(SystemTime::UNIX_EPOCH)
            .context("Failed to convert to Unix timestamp")?
            .as_secs() as i64);
    }

    if let Some(stripped) = time_str.strip_suffix('w') {
        let weeks = stripped.parse::<u64>().context("Invalid weeks format")?;
        let duration = Duration::from_secs(weeks * 604800);
        let since = now
            .checked_sub(duration)
            .ok_or_else(|| anyhow!("Time calculation overflow"))?;
        return Ok(since
            .duration_since(SystemTime::UNIX_EPOCH)
            .context("Failed to convert to Unix timestamp")?
            .as_secs() as i64);
    }

    // Handle "today" - start of current day (00:00:00)
    if time_str == "today" {
        let now_secs = now
            .duration_since(SystemTime::UNIX_EPOCH)
            .context("Failed to get current time")?
            .as_secs();
        let today_start = (now_secs / 86400) * 86400; // Round down to start of day
        return Ok(today_start as i64);
    }

    // Handle "yesterday" - start of previous day (00:00:00)
    if time_str == "yesterday" {
        let now_secs = now
            .duration_since(SystemTime::UNIX_EPOCH)
            .context("Failed to get current time")?
            .as_secs();
        let yesterday_start = ((now_secs / 86400) - 1) * 86400;
        return Ok(yesterday_start as i64);
    }

    // Try parsing as ISO 8601 timestamp or Unix timestamp
    if let Ok(timestamp) = time_str.parse::<i64>() {
        return Ok(timestamp);
    }

    Err(anyhow!(
        "Invalid time format. Expected formats: '1h', '2d', '3w', 'today', 'yesterday', or Unix timestamp"
    ))
}

/// Parse time string in various formats for GPU reservations.
///
/// Supported formats:
/// - ISO 8601 format (e.g., `"2026-01-28T14:00:00Z"`)
/// - `"YYYY-MM-DD HH:MM"` format
///
/// # Examples
///
/// ```
/// use gflow::utils::parsers::parse_reservation_time;
///
/// assert!(parse_reservation_time("2026-01-28T14:00:00Z").is_ok());
/// assert!(parse_reservation_time("2026-01-28 14:00").is_ok());
/// ```
pub fn parse_reservation_time(time_str: &str) -> Result<SystemTime> {
    use chrono::{DateTime, NaiveDateTime};

    // Try ISO8601 format first
    if let Ok(dt) = DateTime::parse_from_rfc3339(time_str) {
        return Ok(SystemTime::UNIX_EPOCH + Duration::from_secs(dt.timestamp() as u64));
    }

    // Try "YYYY-MM-DD HH:MM" format
    if let Ok(dt) = NaiveDateTime::parse_from_str(time_str, "%Y-%m-%d %H:%M") {
        let timestamp = dt.and_utc().timestamp();
        return Ok(SystemTime::UNIX_EPOCH + Duration::from_secs(timestamp as u64));
    }

    anyhow::bail!(
        "Invalid time format: {}. Use ISO8601 (e.g., '2026-01-28T14:00:00Z') or 'YYYY-MM-DD HH:MM'",
        time_str
    )
}

/// Parse duration string for GPU reservations (e.g., "1h", "30m", "2h30m").
///
/// Supported formats:
/// - `"1h"` — hours
/// - `"30m"` — minutes
/// - `"45s"` — seconds
/// - `"2h30m"` — combined (hours and minutes)
/// - `"1h30m45s"` — combined (hours, minutes, and seconds)
///
/// # Examples
///
/// ```
/// use gflow::utils::parsers::parse_reservation_duration;
///
/// assert_eq!(parse_reservation_duration("1h").unwrap(), 3600);
/// assert_eq!(parse_reservation_duration("30m").unwrap(), 1800);
/// assert_eq!(parse_reservation_duration("2h30m").unwrap(), 9000);
/// ```
pub fn parse_reservation_duration(duration_str: &str) -> Result<u64> {
    let mut total_secs = 0u64;
    let mut current_num = String::new();

    for ch in duration_str.chars() {
        if ch.is_ascii_digit() {
            current_num.push(ch);
        } else if ch == 'h' || ch == 'H' {
            let hours: u64 = current_num.parse().context("Invalid number before 'h'")?;
            total_secs += hours * 3600;
            current_num.clear();
        } else if ch == 'm' || ch == 'M' {
            let minutes: u64 = current_num.parse().context("Invalid number before 'm'")?;
            total_secs += minutes * 60;
            current_num.clear();
        } else if ch == 's' || ch == 'S' {
            let seconds: u64 = current_num.parse().context("Invalid number before 's'")?;
            total_secs += seconds;
            current_num.clear();
        } else {
            anyhow::bail!("Invalid character in duration: {}", ch);
        }
    }

    if !current_num.is_empty() {
        anyhow::bail!("Duration must end with a unit (h, m, or s)");
    }

    if total_secs == 0 {
        anyhow::bail!("Duration must be greater than 0");
    }

    Ok(total_secs)
}

/// Parse range specification (start:stop or start:stop:step).
/// Returns a vector of stringified values.
///
/// # Examples
///
/// ```
/// use gflow::utils::parsers::parse_range_spec;
///
/// assert_eq!(parse_range_spec("1:3").unwrap(), vec!["1", "2", "3"]);
/// assert_eq!(parse_range_spec("0:10:2").unwrap(), vec!["0", "2", "4", "6", "8", "10"]);
/// ```
pub fn parse_range_spec(spec: &str) -> Result<Vec<String>> {
    let parts: Vec<&str> = spec.split(':').collect();

    match parts.len() {
        2 => {
            // start:stop (default step = 1)
            let start = parts[0].trim().parse::<f64>()?;
            let stop = parts[1].trim().parse::<f64>()?;
            let step = if start <= stop { 1.0 } else { -1.0 };
            generate_range(start, stop, step)
        }
        3 => {
            // start:stop:step
            let start = parts[0].trim().parse::<f64>()?;
            let stop = parts[1].trim().parse::<f64>()?;
            let step = parts[2].trim().parse::<f64>()?;

            if step == 0.0 {
                return Err(anyhow!("Step cannot be zero"));
            }

            generate_range(start, stop, step)
        }
        _ => Err(anyhow!(
            "Invalid range format. Expected 'start:stop' or 'start:stop:step'"
        )),
    }
}

/// Generate range values from start to stop with given step.
fn generate_range(start: f64, stop: f64, step: f64) -> Result<Vec<String>> {
    if (step > 0.0 && start > stop) || (step < 0.0 && start < stop) {
        return Err(anyhow!("Step direction doesn't match start/stop"));
    }

    // Determine decimal places to use for formatting based on step
    let step_str = format!("{}", step.abs());
    let decimal_places = if let Some(dot_pos) = step_str.find('.') {
        let after_dot = &step_str[dot_pos + 1..];
        // Remove trailing zeros and count
        after_dot.trim_end_matches('0').len()
    } else {
        0
    };

    let mut values = Vec::new();
    let mut index = 0;

    // Use epsilon for float comparison
    const EPSILON: f64 = 1e-10;

    loop {
        // Calculate current value from index to avoid accumulation of floating point errors
        let current = start + step * (index as f64);

        // Check if we've exceeded the stop value
        if step > 0.0 && current > stop + EPSILON {
            break;
        }
        if step < 0.0 && current < stop - EPSILON {
            break;
        }

        // Round to avoid floating point precision issues
        let power = 10_f64.powi(decimal_places.max(10) as i32);
        let rounded = (current * power).round() / power;

        // Format as integer if it's a whole number, otherwise as float with appropriate precision
        let formatted = if (rounded - rounded.round()).abs() < EPSILON {
            format!("{}", rounded.round() as i64)
        } else if decimal_places > 0 {
            format!("{:.prec$}", rounded, prec = decimal_places)
        } else {
            format!("{}", rounded)
        };

        values.push(formatted);
        index += 1;

        // Safety limit
        if values.len() > 10000 {
            return Err(anyhow!("Range too large (max 10000 values)"));
        }
    }

    Ok(values)
}

/// Parse array specification like "1-10".
/// Returns a vector of task IDs.
///
/// # Examples
///
/// ```
/// use gflow::utils::parsers::parse_array_spec;
///
/// assert_eq!(parse_array_spec("1-5").unwrap(), vec![1, 2, 3, 4, 5]);
/// assert_eq!(parse_array_spec("10-12").unwrap(), vec![10, 11, 12]);
/// ```
pub fn parse_array_spec(spec: &str) -> Result<Vec<u32>> {
    if let Some(parts) = spec.split_once('-') {
        let start = parts
            .0
            .parse::<u32>()
            .context("Invalid array start index")?;
        let end = parts.1.parse::<u32>().context("Invalid array end index")?;
        if start > end {
            return Err(anyhow!(
                "Array start index cannot be greater than end index"
            ));
        }
        Ok((start..=end).collect())
    } else {
        Err(anyhow!(
            "Invalid array format. Expected format like '1-10'."
        ))
    }
}

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

    #[test]
    fn test_parse_since_time_hours() {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let result = parse_since_time("1h").unwrap();
        // Should be approximately 1 hour ago (within 2 seconds tolerance)
        assert!((now - result - 3600).abs() < 2);

        let result = parse_since_time("24h").unwrap();
        assert!((now - result - 86400).abs() < 2);

        let result = parse_since_time("0h").unwrap();
        assert!((now - result).abs() < 2);
    }

    #[test]
    fn test_parse_since_time_days() {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let result = parse_since_time("1d").unwrap();
        // Should be approximately 1 day ago (within 2 seconds tolerance)
        assert!((now - result - 86400).abs() < 2);

        let result = parse_since_time("7d").unwrap();
        assert!((now - result - 604800).abs() < 2);

        let result = parse_since_time("0d").unwrap();
        assert!((now - result).abs() < 2);
    }

    #[test]
    fn test_parse_since_time_weeks() {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let result = parse_since_time("1w").unwrap();
        // Should be approximately 1 week ago (within 2 seconds tolerance)
        assert!((now - result - 604800).abs() < 2);

        let result = parse_since_time("2w").unwrap();
        assert!((now - result - 1209600).abs() < 2);

        let result = parse_since_time("0w").unwrap();
        assert!((now - result).abs() < 2);
    }

    #[test]
    fn test_parse_since_time_today() {
        let result = parse_since_time("today").unwrap();
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let expected_start = (now / 86400) * 86400;
        assert_eq!(result, expected_start as i64);

        // Test case insensitivity
        let result = parse_since_time("TODAY").unwrap();
        assert_eq!(result, expected_start as i64);

        let result = parse_since_time("  today  ").unwrap();
        assert_eq!(result, expected_start as i64);
    }

    #[test]
    fn test_parse_since_time_yesterday() {
        let result = parse_since_time("yesterday").unwrap();
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let expected_start = ((now / 86400) - 1) * 86400;
        assert_eq!(result, expected_start as i64);

        // Test case insensitivity
        let result = parse_since_time("YESTERDAY").unwrap();
        assert_eq!(result, expected_start as i64);

        let result = parse_since_time("  yesterday  ").unwrap();
        assert_eq!(result, expected_start as i64);
    }

    #[test]
    fn test_parse_since_time_unix_timestamp() {
        let timestamp = 1704067200i64; // 2024-01-01 00:00:00 UTC
        let result = parse_since_time("1704067200").unwrap();
        assert_eq!(result, timestamp);

        let result = parse_since_time("0").unwrap();
        assert_eq!(result, 0);

        let result = parse_since_time("1000000000").unwrap();
        assert_eq!(result, 1000000000);
    }

    #[test]
    fn test_parse_since_time_whitespace() {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let result = parse_since_time("  1h  ").unwrap();
        assert!((now - result - 3600).abs() < 2);

        let result = parse_since_time("\t2d\t").unwrap();
        assert!((now - result - 172800).abs() < 2);
    }

    #[test]
    fn test_parse_since_time_invalid() {
        assert!(parse_since_time("").is_err());
        assert!(parse_since_time("abc").is_err());
        assert!(parse_since_time("1x").is_err());
        assert!(parse_since_time("h1").is_err());
        assert!(parse_since_time("1.5h").is_err());
        assert!(parse_since_time("-1h").is_err());
        assert!(parse_since_time("tomorrow").is_err());
        assert!(parse_since_time("1 hour").is_err());
    }

    #[test]
    fn test_parse_since_time_edge_cases() {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Very large values should work
        let result = parse_since_time("1000h").unwrap();
        assert!((now - result - 3600000).abs() < 2);

        let result = parse_since_time("365d").unwrap();
        assert!((now - result - 31536000).abs() < 2);

        let result = parse_since_time("52w").unwrap();
        assert!((now - result - 31449600).abs() < 2);
    }

    #[test]
    fn test_parse_reservation_duration() {
        assert_eq!(parse_reservation_duration("1h").unwrap(), 3600);
        assert_eq!(parse_reservation_duration("30m").unwrap(), 1800);
        assert_eq!(parse_reservation_duration("2h30m").unwrap(), 9000);
        assert_eq!(parse_reservation_duration("1h30m45s").unwrap(), 5445);
        assert_eq!(parse_reservation_duration("90m").unwrap(), 5400);

        assert!(parse_reservation_duration("").is_err());
        assert!(parse_reservation_duration("1").is_err());
        assert!(parse_reservation_duration("abc").is_err());
    }

    #[test]
    fn test_parse_array_spec() {
        assert_eq!(parse_array_spec("1-5").unwrap(), vec![1, 2, 3, 4, 5]);
        assert_eq!(parse_array_spec("10-12").unwrap(), vec![10, 11, 12]);
        assert!(parse_array_spec("5-1").is_err());
        assert!(parse_array_spec("abc").is_err());
    }
}
