use chrono::{DateTime, Duration as ChronoDuration, Local};
use gflow::core::reservation::{GpuReservation, ReservationStatus};
use std::time::{Duration, SystemTime};

/// Configuration for timeline rendering
pub struct TimelineConfig {
    /// Width of the timeline in characters
    pub width: usize,
    /// Time range to display (start, end)
    pub time_range: (SystemTime, SystemTime),
}

impl Default for TimelineConfig {
    fn default() -> Self {
        let now = SystemTime::now();
        // Default: show ±12 hours from now
        let start = now - Duration::from_secs(12 * 3600);
        let end = now + Duration::from_secs(12 * 3600);

        Self {
            width: 80,
            time_range: (start, end),
        }
    }
}

/// Render reservations as a timeline visualization
pub fn render_timeline(reservations: &[GpuReservation], config: TimelineConfig) {
    if reservations.is_empty() {
        println!("No reservations found.");
        return;
    }

    let now = SystemTime::now();
    let (range_start, range_end) = config.time_range;

    // Print header
    println!("\nGPU Reservations Timeline");
    println!("{}", "═".repeat(config.width));

    // Print time axis
    print_time_axis(range_start, range_end, config.width, now);

    println!();

    // Sort reservations by start time
    let mut sorted_reservations = reservations.to_vec();
    sorted_reservations.sort_by_key(|r| r.start_time);

    // Print each reservation
    for reservation in sorted_reservations {
        print_reservation_bar(&reservation, range_start, range_end, config.width, now);
    }

    // Print summary
    println!();
    print_summary(reservations, now);
}

/// Print the time axis with markers
fn print_time_axis(start: SystemTime, end: SystemTime, width: usize, now: SystemTime) {
    let start_dt = system_time_to_datetime(start);
    let end_dt = system_time_to_datetime(end);

    // Calculate time span
    let duration = end.duration_since(start).unwrap_or_default();
    let hours = duration.as_secs() / 3600;

    // Determine time marker interval (every 2, 4, 6, or 12 hours)
    let interval_hours = if hours <= 12 {
        2
    } else if hours <= 24 {
        4
    } else if hours <= 48 {
        6
    } else {
        12
    };

    // Print time markers
    let mut time_markers = Vec::new();
    let mut current = start_dt;
    let mut last_date = None;
    while current <= end_dt {
        let pos = time_to_position(datetime_to_system_time(current), start, end, width);
        // Show date only when it changes
        let current_date = current.date_naive();
        let time_str = if last_date.is_none() || last_date != Some(current_date) {
            last_date = Some(current_date);
            current.format("%m/%d %H:%M").to_string()
        } else {
            current.format("%H:%M").to_string()
        };
        time_markers.push((pos, time_str));
        current += ChronoDuration::hours(interval_hours as i64);
    }

    // Print the axis line
    let mut axis = vec!['─'; width];

    // Mark positions
    for (pos, _) in &time_markers {
        if *pos < width {
            axis[*pos] = '┬';
        }
    }

    // Mark "now" position
    let now_pos = time_to_position(now, start, end, width);
    if now_pos < width {
        axis[now_pos] = '┃';
    }

    println!("{}", axis.iter().collect::<String>());

    // Print time labels
    let mut label_line = vec![' '; width];
    let now_pos = time_to_position(now, start, end, width);

    for (pos, time_str) in &time_markers {
        // Skip if too close to "now" position
        if (*pos as i32 - now_pos as i32).abs() < 6 {
            continue;
        }
        // Ensure we don't overflow the line
        let available_space = width.saturating_sub(*pos);
        if *pos < width && time_str.len() <= available_space {
            for (i, ch) in time_str.chars().enumerate() {
                if pos + i < width {
                    label_line[pos + i] = ch;
                }
            }
        }
    }

    // Print "Now" label - centered on the position
    if now_pos >= 2 && now_pos + 2 < width {
        let now_label = "Now";
        let start_pos = now_pos.saturating_sub(1);
        for (i, ch) in now_label.chars().enumerate() {
            if start_pos + i < width {
                label_line[start_pos + i] = ch;
            }
        }
    }

    println!("{}", label_line.iter().collect::<String>());
}

/// Print a single reservation as a bar
fn print_reservation_bar(
    reservation: &GpuReservation,
    range_start: SystemTime,
    range_end: SystemTime,
    width: usize,
    _now: SystemTime,
) {
    let res_start = reservation.start_time;
    let res_end = reservation.end_time();

    // Skip if completely outside range
    if res_end < range_start || res_start > range_end {
        return;
    }

    // Calculate bar position and length
    let bar_start = time_to_position(res_start.max(range_start), range_start, range_end, width);
    let bar_end = time_to_position(res_end.min(range_end), range_start, range_end, width);
    let bar_length = bar_end.saturating_sub(bar_start).max(1);

    // Create the bar
    let mut bar = vec![' '; width];

    // Fill the bar
    let bar_char = match reservation.status {
        ReservationStatus::Active => '█',
        ReservationStatus::Pending => '░',
        ReservationStatus::Completed => '▓',
        ReservationStatus::Cancelled => '▒',
    };

    #[allow(clippy::needless_range_loop)]
    for pos in bar_start..bar_start + bar_length {
        if pos < width {
            bar[pos] = bar_char;
        }
    }

    // Create label
    let label = format!(
        "{} ({} GPU{})",
        reservation.user,
        reservation.gpu_count,
        if reservation.gpu_count > 1 { "s" } else { "" }
    );

    // Print user label and bar
    let bar_str: String = bar.iter().collect();

    println!("{:<15} {}", label, bar_str);

    // Print status info below
    let status_info = format!(
        "  └─ {} ({}→{})",
        format_status(reservation.status),
        format_time_short(res_start),
        format_time_short(res_end)
    );
    println!("{}", status_info);
}

/// Convert time to position on the timeline
fn time_to_position(
    time: SystemTime,
    range_start: SystemTime,
    range_end: SystemTime,
    width: usize,
) -> usize {
    let total_duration = range_end
        .duration_since(range_start)
        .unwrap_or_default()
        .as_secs_f64();

    let time_offset = time
        .duration_since(range_start)
        .unwrap_or_default()
        .as_secs_f64();

    let ratio = time_offset / total_duration;
    (ratio * width as f64).round() as usize
}

/// Format status
fn format_status(status: ReservationStatus) -> String {
    match status {
        ReservationStatus::Active => "Active".to_string(),
        ReservationStatus::Pending => "Pending".to_string(),
        ReservationStatus::Completed => "Completed".to_string(),
        ReservationStatus::Cancelled => "Cancelled".to_string(),
    }
}

/// Format time in short format (HH:MM)
fn format_time_short(time: SystemTime) -> String {
    let dt = system_time_to_datetime(time);
    dt.format("%H:%M").to_string()
}

/// Print summary statistics
fn print_summary(reservations: &[GpuReservation], now: SystemTime) {
    let active_count = reservations.iter().filter(|r| r.is_active(now)).count();

    let pending_count = reservations
        .iter()
        .filter(|r| r.status == ReservationStatus::Pending)
        .count();

    let total_active_gpus: u32 = reservations
        .iter()
        .filter(|r| r.is_active(now))
        .map(|r| r.gpu_count)
        .sum();

    println!("{}", "─".repeat(80));
    println!(
        "Summary: {} active, {} pending | {} GPUs currently reserved",
        active_count, pending_count, total_active_gpus
    );
    println!();
    println!("Legend: █ Active  ░ Pending  ▓ Completed  ▒ Cancelled");
}

/// Convert SystemTime to DateTime<Local>
fn system_time_to_datetime(time: SystemTime) -> DateTime<Local> {
    time.into()
}

/// Convert DateTime<Local> to SystemTime
fn datetime_to_system_time(dt: DateTime<Local>) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(dt.timestamp() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use compact_str::CompactString;

    #[test]
    fn test_time_to_position() {
        let start = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
        let end = start + Duration::from_secs(3600); // 1 hour later
        let width = 100;

        // At start
        assert_eq!(time_to_position(start, start, end, width), 0);

        // At end
        assert_eq!(time_to_position(end, start, end, width), 100);

        // At middle
        let middle = start + Duration::from_secs(1800);
        let pos = time_to_position(middle, start, end, width);
        assert!(pos >= 49 && pos <= 51); // Allow for rounding
    }

    #[test]
    fn test_render_empty_reservations() {
        let reservations: Vec<GpuReservation> = vec![];
        let config = TimelineConfig::default();
        // Should not panic
        render_timeline(&reservations, config);
    }

    #[test]
    fn test_render_single_reservation() {
        let now = SystemTime::now();
        let reservation = GpuReservation {
            id: 1,
            user: CompactString::from("alice"),
            gpu_count: 2,
            start_time: now,
            duration: Duration::from_secs(3600),
            status: ReservationStatus::Active,
            created_at: now,
            cancelled_at: None,
        };

        let config = TimelineConfig::default();
        // Should not panic
        render_timeline(&[reservation], config);
    }
}
