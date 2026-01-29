use anyhow::Result;
use gflow::client::Client;
use gflow::core::reservation::ReservationStatus;
use gflow::utils::parsers::parse_reservation_time;
use tabled::{builder::Builder, settings::style::Style};

use crate::reserve_timeline::{render_timeline, TimelineConfig};

#[derive(Debug, Default)]
pub struct TimelineRangeOpts {
    pub range: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
}

pub async fn handle_reserve_list(
    client: &Client,
    user: Option<String>,
    status: Option<String>,
    active_only: bool,
    timeline: bool,
    timeline_range: TimelineRangeOpts,
) -> Result<()> {
    let mut reservations = client.list_reservations(user, status, active_only).await?;

    if reservations.is_empty() {
        println!("No reservations found.");
        return Ok(());
    }

    // Sort reservations by ID (creation order)
    reservations.sort_by_key(|r| r.id);

    if timeline {
        if timeline_range.range.is_some()
            && (timeline_range.from.is_some() || timeline_range.to.is_some())
        {
            anyhow::bail!("--range cannot be combined with --from/--to");
        }
        if timeline_range.from.is_some() ^ timeline_range.to.is_some() {
            anyhow::bail!("--from and --to must be used together");
        }

        // Render timeline view
        let config = if let Some(spec) = timeline_range.range.as_deref() {
            let now = std::time::SystemTime::now();
            TimelineConfig {
                time_range: parse_relative_time_range(now, spec)?,
                ..Default::default()
            }
        } else if let (Some(from), Some(to)) =
            (timeline_range.from.as_deref(), timeline_range.to.as_deref())
        {
            let start = parse_reservation_time(from)?;
            let end = parse_reservation_time(to)?;
            ensure_valid_time_range(start, end)?;
            TimelineConfig {
                time_range: (start, end),
                ..Default::default()
            }
        } else {
            TimelineConfig::default()
        };
        render_timeline(&reservations, config);
    } else {
        if timeline_range.range.is_some()
            || timeline_range.from.is_some()
            || timeline_range.to.is_some()
        {
            anyhow::bail!("--range/--from/--to can only be used with --timeline");
        }

        // Render table view
        let mut builder = Builder::default();
        builder.push_record(["ID", "USER", "GPUS", "START", "END", "STATUS"]);

        for reservation in reservations {
            let start_time = format_system_time_short(reservation.start_time);
            let end_time = format_system_time_short(reservation.end_time());
            let status_str = format_status(reservation.status);

            builder.push_record([
                reservation.id.to_string(),
                reservation.user.to_string(),
                reservation.gpu_count.to_string(),
                start_time,
                end_time,
                status_str,
            ]);
        }

        let table = builder.build().with(Style::blank()).to_string();
        println!("{}", table);
    }

    Ok(())
}

fn ensure_valid_time_range(start: std::time::SystemTime, end: std::time::SystemTime) -> Result<()> {
    if end <= start {
        anyhow::bail!("Invalid time range: end time must be after start time");
    }
    Ok(())
}

fn parse_relative_time_range(
    now: std::time::SystemTime,
    spec: &str,
) -> Result<(std::time::SystemTime, std::time::SystemTime)> {
    // Supported formats:
    // - "48h" -> now..now+48h
    // - "-24h" -> now-24h..now
    // - "-24h:+24h" -> now-24h..now+24h
    let spec = spec.trim();

    let (start, end) = if let Some((start_str, end_str)) = spec.split_once(':') {
        let start_offset = parse_signed_duration_secs(start_str)?;
        let end_offset = parse_signed_duration_secs(end_str)?;
        (
            shift_system_time(now, start_offset)?,
            shift_system_time(now, end_offset)?,
        )
    } else {
        let offset = parse_signed_duration_secs(spec)?;
        if offset >= 0 {
            (now, shift_system_time(now, offset)?)
        } else {
            (shift_system_time(now, offset)?, now)
        }
    };

    ensure_valid_time_range(start, end)?;
    Ok((start, end))
}

fn parse_signed_duration_secs(spec: &str) -> Result<i64> {
    use gflow::utils::parsers::parse_reservation_duration;

    let spec = spec.trim();
    if spec.is_empty() {
        anyhow::bail!("Invalid duration: empty string");
    }

    let (sign, rest) = match spec.as_bytes()[0] {
        b'+' => (1i64, &spec[1..]),
        b'-' => (-1i64, &spec[1..]),
        _ => (1i64, spec),
    };

    if rest.trim().is_empty() {
        anyhow::bail!("Invalid duration: {}", spec);
    }

    let secs = parse_reservation_duration(rest.trim())? as i64;
    Ok(sign * secs)
}

fn shift_system_time(
    now: std::time::SystemTime,
    offset_secs: i64,
) -> Result<std::time::SystemTime> {
    use std::time::Duration;

    if offset_secs >= 0 {
        now.checked_add(Duration::from_secs(offset_secs as u64))
            .ok_or_else(|| anyhow::anyhow!("Time range end is out of bounds"))
    } else {
        now.checked_sub(Duration::from_secs((-offset_secs) as u64))
            .ok_or_else(|| anyhow::anyhow!("Time range start is out of bounds"))
    }
}

/// Format SystemTime for table display (shorter format without "UTC" suffix)
fn format_system_time_short(time: std::time::SystemTime) -> String {
    use chrono::{DateTime, Utc};

    let duration = time
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let datetime =
        DateTime::<Utc>::from_timestamp(duration.as_secs() as i64, 0).unwrap_or_default();

    datetime.format("%Y-%m-%d %H:%M:%S").to_string()
}

fn format_status(status: ReservationStatus) -> String {
    match status {
        ReservationStatus::Pending => "Pending".to_string(),
        ReservationStatus::Active => "Active".to_string(),
        ReservationStatus::Completed => "Completed".to_string(),
        ReservationStatus::Cancelled => "Cancelled".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    #[test]
    fn test_parse_relative_time_range_single_positive() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
        let (start, end) = parse_relative_time_range(now, "2h").unwrap();
        assert_eq!(start, now);
        assert_eq!(end, now + Duration::from_secs(7200));
    }

    #[test]
    fn test_parse_relative_time_range_single_negative() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
        let (start, end) = parse_relative_time_range(now, "-30m").unwrap();
        assert_eq!(start, now - Duration::from_secs(1800));
        assert_eq!(end, now);
    }

    #[test]
    fn test_parse_relative_time_range_two_sided() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10_000);
        let (start, end) = parse_relative_time_range(now, "-1h:+2h").unwrap();
        assert_eq!(start, now - Duration::from_secs(3600));
        assert_eq!(end, now + Duration::from_secs(7200));
    }
}
