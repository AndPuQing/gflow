use anyhow::{Context, Result};
use gflow::client::Client;

pub async fn handle_reserve_create(
    client: &Client,
    user: &str,
    gpus: u32,
    start: &str,
    duration: &str,
) -> Result<()> {
    // Parse start time
    let start_time = parse_time(start)?;

    // Parse duration
    let duration_secs = parse_duration(duration)?;

    // Create reservation
    let reservation_id = client
        .create_reservation(user.to_string(), gpus, start_time, duration_secs)
        .await?;

    println!("Reservation created successfully");
    println!("Reservation ID: {}", reservation_id);

    Ok(())
}

/// Parse time string in various formats
fn parse_time(time_str: &str) -> Result<std::time::SystemTime> {
    use chrono::{DateTime, NaiveDateTime};

    // Try ISO8601 format first
    if let Ok(dt) = DateTime::parse_from_rfc3339(time_str) {
        return Ok(std::time::SystemTime::UNIX_EPOCH
            + std::time::Duration::from_secs(dt.timestamp() as u64));
    }

    // Try "YYYY-MM-DD HH:MM" format
    if let Ok(dt) = NaiveDateTime::parse_from_str(time_str, "%Y-%m-%d %H:%M") {
        let timestamp = dt.and_utc().timestamp();
        return Ok(
            std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(timestamp as u64)
        );
    }

    anyhow::bail!(
        "Invalid time format: {}. Use ISO8601 (e.g., '2026-01-28T14:00:00Z') or 'YYYY-MM-DD HH:MM'",
        time_str
    )
}

/// Parse duration string (e.g., "1h", "30m", "2h30m")
fn parse_duration(duration_str: &str) -> Result<u64> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("1h").unwrap(), 3600);
        assert_eq!(parse_duration("30m").unwrap(), 1800);
        assert_eq!(parse_duration("2h30m").unwrap(), 9000);
        assert_eq!(parse_duration("1h30m45s").unwrap(), 5445);
        assert_eq!(parse_duration("90m").unwrap(), 5400);

        assert!(parse_duration("").is_err());
        assert!(parse_duration("1").is_err());
        assert!(parse_duration("abc").is_err());
    }
}
