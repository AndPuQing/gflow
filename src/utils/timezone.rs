use anyhow::{Context, Result};
use chrono::{DateTime, Local, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use std::time::{Duration, SystemTime};

/// Get the configured timezone or fall back to local timezone
pub fn get_timezone(config_tz: Option<&str>) -> Result<Tz> {
    if let Some(tz_str) = config_tz {
        tz_str
            .parse::<Tz>()
            .with_context(|| format!("Invalid timezone: {}", tz_str))
    } else {
        // Use local timezone
        Ok(chrono_tz::Tz::UTC) // Will be replaced with actual local detection
    }
}

/// Parse reservation time with optional timezone override
pub fn parse_reservation_time_with_tz(
    time_str: &str,
    config_tz: Option<&str>,
    override_tz: Option<&str>,
) -> Result<SystemTime> {
    use chrono::Timelike;

    // Determine which timezone to use (priority: override > config > local)
    let tz = if let Some(tz_str) = override_tz.or(config_tz) {
        tz_str
            .parse::<Tz>()
            .with_context(|| format!("Invalid timezone: {}", tz_str))?
    } else {
        // Use local timezone - convert Local to Tz
        get_local_timezone()
    };

    // Try ISO8601 format first
    let dt = if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(time_str) {
        // ISO8601 has explicit timezone, convert to target timezone
        dt.with_timezone(&tz)
    } else if let Ok(naive_dt) = NaiveDateTime::parse_from_str(time_str, "%Y-%m-%d %H:%M") {
        // Interpret naive datetime in the target timezone
        tz.from_local_datetime(&naive_dt).single().ok_or_else(|| {
            anyhow::anyhow!("Ambiguous or invalid time in timezone {}: {}", tz, time_str)
        })?
    } else {
        anyhow::bail!(
            "Invalid time format: {}. Use ISO8601 (e.g., '2026-01-28T14:00:00Z') or 'YYYY-MM-DD HH:MM'",
            time_str
        )
    };

    // Validate that minutes are either 00 or 30
    let minute = dt.minute();
    if minute != 0 && minute != 30 {
        anyhow::bail!(
            "Reservation time must be on the hour (:00) or half-hour (:30). Got: {}",
            time_str
        );
    }

    // Validate that seconds are 00
    if dt.second() != 0 {
        anyhow::bail!(
            "Reservation time must not include seconds. Got: {}",
            time_str
        );
    }

    let timestamp = dt.timestamp();
    Ok(SystemTime::UNIX_EPOCH + Duration::from_secs(timestamp as u64))
}

/// Format SystemTime for display using configured timezone
pub fn format_system_time(
    time: SystemTime,
    config_tz: Option<&str>,
    format: &str,
) -> Result<String> {
    let tz = if let Some(tz_str) = config_tz {
        tz_str
            .parse::<Tz>()
            .with_context(|| format!("Invalid timezone: {}", tz_str))?
    } else {
        get_local_timezone()
    };

    let duration = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let datetime = DateTime::<Utc>::from_timestamp(duration.as_secs() as i64, 0)
        .unwrap_or_default()
        .with_timezone(&tz);

    Ok(datetime.format(format).to_string())
}

/// Format SystemTime for short display (YYYY-MM-DD HH:MM:SS)
pub fn format_system_time_short(time: SystemTime, config_tz: Option<&str>) -> Result<String> {
    format_system_time(time, config_tz, "%Y-%m-%d %H:%M:%S")
}

/// Format SystemTime for job display (MM/DD-HH:MM:SS)
pub fn format_system_time_job(time: SystemTime, config_tz: Option<&str>) -> Result<String> {
    format_system_time(time, config_tz, "%m/%d-%H:%M:%S")
}

/// Convert SystemTime to DateTime in configured timezone
pub fn system_time_to_datetime(time: SystemTime, config_tz: Option<&str>) -> Result<DateTime<Tz>> {
    let tz = if let Some(tz_str) = config_tz {
        tz_str
            .parse::<Tz>()
            .with_context(|| format!("Invalid timezone: {}", tz_str))?
    } else {
        get_local_timezone()
    };

    let duration = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let datetime = DateTime::<Utc>::from_timestamp(duration.as_secs() as i64, 0)
        .unwrap_or_default()
        .with_timezone(&tz);

    Ok(datetime)
}

/// Convert DateTime to SystemTime
pub fn datetime_to_system_time<T: TimeZone>(dt: DateTime<T>) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(dt.timestamp() as u64)
}

/// Get local timezone as Tz
/// This is a best-effort approach since chrono-tz doesn't have direct local timezone detection
pub fn get_local_timezone() -> Tz {
    // Try to detect local timezone from system
    // For now, we'll use UTC as fallback, but in practice we should detect from Local
    let local_now = Local::now();
    let offset = local_now.offset().local_minus_utc();

    // Try to find a matching timezone by offset
    // This is approximate and may not always be accurate
    match offset {
        28800 => chrono_tz::Asia::Shanghai,        // UTC+8
        32400 => chrono_tz::Asia::Tokyo,           // UTC+9
        -28800 => chrono_tz::America::Los_Angeles, // UTC-8 (PST)
        -18000 => chrono_tz::America::New_York,    // UTC-5 (EST)
        0 => chrono_tz::UTC,
        _ => chrono_tz::UTC, // Fallback to UTC
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_reservation_time_with_tz() {
        // Test ISO8601 format
        let result =
            parse_reservation_time_with_tz("2026-01-28T14:00:00Z", None, Some("Asia/Shanghai"));
        assert!(result.is_ok());

        // Test simple format with timezone
        let result =
            parse_reservation_time_with_tz("2026-01-28 14:00", Some("Asia/Shanghai"), None);
        assert!(result.is_ok());

        // Test invalid minute (not :00 or :30)
        let result =
            parse_reservation_time_with_tz("2026-01-28 14:15", Some("Asia/Shanghai"), None);
        assert!(result.is_err());
    }

    #[test]
    fn test_format_system_time() {
        let time = SystemTime::UNIX_EPOCH + Duration::from_secs(1706443200); // 2024-01-28 14:00:00 UTC
        let result = format_system_time_short(time, Some("UTC"));
        assert!(result.is_ok());
    }
}
