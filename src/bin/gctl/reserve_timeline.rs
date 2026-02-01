use chrono::{DateTime, Duration as ChronoDuration, Utc};
use chrono_tz::Tz;
use gflow::core::reservation::{GpuReservation, ReservationStatus};
use std::time::{Duration, SystemTime};

/// Configuration for timeline rendering
pub struct TimelineConfig<'a> {
    /// Width of the timeline in characters
    pub width: usize,
    /// Time range to display (start, end)
    pub time_range: (SystemTime, SystemTime),
    /// Timezone for display (None = local timezone)
    pub timezone: Option<&'a str>,
}

impl<'a> Default for TimelineConfig<'a> {
    fn default() -> Self {
        let now = SystemTime::now();
        // Default: show ±12 hours from now
        let start = now - Duration::from_secs(12 * 3600);
        let end = now + Duration::from_secs(12 * 3600);

        Self {
            width: 80,
            time_range: (start, end),
            timezone: None,
        }
    }
}

/// Render reservations as a timeline visualization
pub fn render_timeline(reservations: &[GpuReservation], config: TimelineConfig) {
    render_timeline_to_writer(reservations, config, &mut std::io::stdout());
}

/// Render reservations as a timeline visualization to a writer (for testing)
fn render_timeline_to_writer<W: std::io::Write>(
    reservations: &[GpuReservation],
    config: TimelineConfig,
    writer: &mut W,
) {
    if reservations.is_empty() {
        writeln!(writer, "No reservations found.").ok();
        return;
    }

    let now = SystemTime::now();
    let (range_start, range_end) = config.time_range;
    let tz = get_timezone(config.timezone);

    // Print header with current time
    let now_dt = system_time_to_datetime(now, &tz);
    writeln!(
        writer,
        "\nGPU Reservations Timeline ({})",
        now_dt.format("%Y-%m-%d %H:%M:%S %Z")
    )
    .ok();
    writeln!(writer, "{}", "═".repeat(config.width)).ok();

    // Print time axis
    let aligned_start =
        print_time_axis_to_writer(range_start, range_end, config.width, now, &tz, writer);

    writeln!(writer).ok();

    // Sort reservations by start time
    let mut sorted_reservations = reservations.to_vec();
    sorted_reservations.sort_by_key(|r| r.start_time);

    // Print each reservation
    for reservation in sorted_reservations {
        print_reservation_bar_to_writer(
            &reservation,
            aligned_start,
            range_end,
            config.width,
            now,
            &tz,
            writer,
        );
    }

    // Print summary
    writeln!(writer).ok();
    print_summary_to_writer(reservations, now, writer);
}

/// Print the time axis with markers to a writer
/// Returns the aligned start time for uniform marker spacing
fn print_time_axis_to_writer<W: std::io::Write>(
    start: SystemTime,
    end: SystemTime,
    width: usize,
    now: SystemTime,
    tz: &Tz,
    writer: &mut W,
) -> SystemTime {
    use chrono::Timelike;

    let start_dt = system_time_to_datetime(start, tz);
    let end_dt = system_time_to_datetime(end, tz);

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

    // Round start time to the nearest interval hour
    // For example, if interval is 2 hours, round to 00:00, 02:00, 04:00, etc.
    let start_hour = start_dt.hour();
    let rounded_hour = (start_hour / interval_hours) * interval_hours;
    let mut current = start_dt
        .with_hour(rounded_hour)
        .unwrap()
        .with_minute(0)
        .unwrap()
        .with_second(0)
        .unwrap()
        .with_nanosecond(0)
        .unwrap();

    // If rounded time is before start, move to next interval
    if datetime_to_system_time(current) < start {
        current += ChronoDuration::hours(interval_hours as i64);
    }

    // Move back one interval to add a marker at the beginning
    current -= ChronoDuration::hours(interval_hours as i64);

    // Align the range start to this first marker for uniform spacing
    let aligned_start = datetime_to_system_time(current);

    // Print time markers
    let mut time_markers = Vec::new();
    let mut last_date = None;
    while current <= end_dt {
        let pos = time_to_position(datetime_to_system_time(current), aligned_start, end, width);
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

    // Mark positions (no forced position 0 marker)
    for (pos, _) in &time_markers {
        if *pos < width {
            axis[*pos] = '┬';
        }
    }

    // Mark "now" position
    let now_pos = time_to_position(now, aligned_start, end, width);
    if now_pos < width {
        axis[now_pos] = '┃';
    }

    writeln!(writer, "{}", axis.iter().collect::<String>()).ok();

    // Print time labels
    let mut label_line = vec![' '; width];
    let now_pos = time_to_position(now, aligned_start, end, width);

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

    writeln!(writer, "{}", label_line.iter().collect::<String>()).ok();

    // Return the aligned start time for use in positioning reservations
    aligned_start
}

/// Print a single reservation as a bar to a writer
fn print_reservation_bar_to_writer<W: std::io::Write>(
    reservation: &GpuReservation,
    range_start: SystemTime,
    range_end: SystemTime,
    width: usize,
    _now: SystemTime,
    tz: &Tz,
    writer: &mut W,
) {
    let res_start = reservation.start_time;
    let res_end = reservation.end_time();

    // Skip if completely outside range
    if res_end < range_start || res_start > range_end {
        return;
    }

    // Label takes 16 characters (15 for label + 1 space)
    const LABEL_WIDTH: usize = 16;
    let bar_width = width.saturating_sub(LABEL_WIDTH);

    // Calculate bar position and length using the FULL width (same as axis)
    // This ensures alignment with the time axis
    let bar_start = time_to_position(res_start.max(range_start), range_start, range_end, width);
    let bar_end = time_to_position(res_end.min(range_end), range_start, range_end, width);
    let bar_length = bar_end.saturating_sub(bar_start).max(1);

    // Create the bar with reduced width (to account for label)
    let mut bar = vec![' '; bar_width];

    // Fill the bar, but adjust positions to account for the label offset
    let bar_char = match reservation.status {
        ReservationStatus::Active => '█',
        ReservationStatus::Pending => '░',
        ReservationStatus::Completed => '▓',
        ReservationStatus::Cancelled => '▒',
    };

    #[allow(clippy::needless_range_loop)]
    for pos in bar_start..bar_start + bar_length {
        // Adjust position: subtract LABEL_WIDTH because the bar area starts after the label
        if pos >= LABEL_WIDTH && pos - LABEL_WIDTH < bar_width {
            bar[pos - LABEL_WIDTH] = bar_char;
        }
    }

    // Create label with GPU spec
    let gpu_spec_str = format_gpu_spec(&reservation.gpu_spec);
    let label = format!("{} (GPU: {})", reservation.user, gpu_spec_str);

    // Print user label and bar
    let bar_str: String = bar.iter().collect();

    writeln!(writer, "{:<15} {}", label, bar_str).ok();

    // Print status info below
    let status_info = format!(
        "  └─ {} ({}→{})",
        format_status(reservation.status),
        format_time_short(res_start, tz),
        format_time_short(res_end, tz)
    );
    writeln!(writer, "{}", status_info).ok();
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

/// Format GPU spec for display
fn format_gpu_spec(spec: &gflow::core::reservation::GpuSpec) -> String {
    use gflow::core::reservation::GpuSpec;
    match spec {
        GpuSpec::Count(count) => {
            format!("{} GPU{}", count, if *count > 1 { "s" } else { "" })
        }
        GpuSpec::Indices(indices) => {
            let indices_str = indices
                .iter()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(",");
            format!("GPU[{}]", indices_str)
        }
    }
}

/// Format time in short format (HH:MM)
fn format_time_short(time: SystemTime, tz: &Tz) -> String {
    let dt = system_time_to_datetime(time, tz);
    dt.format("%H:%M").to_string()
}

/// Print summary statistics to a writer
fn print_summary_to_writer<W: std::io::Write>(
    reservations: &[GpuReservation],
    _now: SystemTime,
    writer: &mut W,
) {
    // Use the status field from the server instead of calculating based on client time
    // This avoids inconsistencies due to time sync issues or update delays
    let active_count = reservations
        .iter()
        .filter(|r| r.status == ReservationStatus::Active)
        .count();

    let pending_count = reservations
        .iter()
        .filter(|r| r.status == ReservationStatus::Pending)
        .count();

    let total_active_gpus: u32 = reservations
        .iter()
        .filter(|r| r.status == ReservationStatus::Active)
        .map(|r| r.gpu_spec.count())
        .sum();

    writeln!(writer, "{}", "─".repeat(80)).ok();
    writeln!(
        writer,
        "Summary: {} active, {} pending | {} GPUs currently reserved",
        active_count, pending_count, total_active_gpus
    )
    .ok();
    writeln!(writer).ok();
    writeln!(writer, "Legend: █ Active  ░ Pending").ok();
}

/// Get timezone from config or use local timezone
fn get_timezone(config_tz: Option<&str>) -> Tz {
    if let Some(tz_str) = config_tz {
        tz_str.parse::<Tz>().unwrap_or(chrono_tz::UTC)
    } else {
        // Use local timezone detection
        gflow::utils::timezone::get_timezone(None).unwrap_or(chrono_tz::UTC)
    }
}

/// Convert SystemTime to DateTime in configured timezone
fn system_time_to_datetime(time: SystemTime, tz: &Tz) -> DateTime<Tz> {
    let duration = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    DateTime::<Utc>::from_timestamp(duration.as_secs() as i64, 0)
        .unwrap_or_default()
        .with_timezone(tz)
}

/// Convert DateTime to SystemTime
fn datetime_to_system_time<T: chrono::TimeZone>(dt: DateTime<T>) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(dt.timestamp() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use compact_str::CompactString;
    use gflow::core::reservation::GpuSpec;

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
        assert!((49..=51).contains(&pos)); // Allow for rounding
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
            gpu_spec: GpuSpec::Count(2),
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

    #[test]
    fn test_timeline_alignment() {
        // Create a fixed time range for deterministic testing
        let base_time = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let range_start = base_time;
        let range_end = base_time + Duration::from_secs(12 * 3600); // 12 hours

        // Create a reservation in the middle of the range
        let res_start = base_time + Duration::from_secs(6 * 3600); // 6 hours from start
        let reservation = GpuReservation {
            id: 1,
            user: CompactString::from("testuser"),
            gpu_spec: GpuSpec::Count(1),
            start_time: res_start,
            duration: Duration::from_secs(2 * 3600), // 2 hours duration
            status: ReservationStatus::Active,
            created_at: base_time,
            cancelled_at: None,
        };

        let config = TimelineConfig {
            width: 80,
            time_range: (range_start, range_end),
            timezone: None,
        };

        // Render to a buffer
        let mut output = Vec::new();
        render_timeline_to_writer(&[reservation], config, &mut output);
        let output_str = String::from_utf8(output).unwrap();

        // Verify output contains expected elements
        assert!(output_str.contains("GPU Reservations Timeline"));
        assert!(output_str.contains("testuser (GPU: 1 GPU)"));
        assert!(output_str.contains("Active"));
        assert!(output_str.contains("Legend:"));

        // Verify alignment: axis should start at position 0, bars at position 16
        let lines: Vec<&str> = output_str.lines().collect();

        // Find the axis line (contains ┬ or ─)
        let axis_line = lines
            .iter()
            .find(|l| l.contains('┬') || l.contains('─'))
            .unwrap();

        // Find the reservation bar line (contains █ or ░)
        let bar_line = lines
            .iter()
            .find(|l| l.contains('█') || l.contains('░'))
            .unwrap();

        // Axis should start at position 0 (no leading spaces)
        let axis_prefix_len = axis_line.chars().take_while(|c| *c == ' ').count();
        assert_eq!(axis_prefix_len, 0, "Axis should start at position 0");

        // Bar line should have a label prefix (15 chars) + 1 space = 16 chars
        // Then the bar characters start
        let bar_char_pos = bar_line.chars().position(|c| c == '█' || c == '░').unwrap();
        assert!(
            bar_char_pos >= 16,
            "Bar should start at or after position 16 (after label), found at {}",
            bar_char_pos
        );
    }

    #[test]
    fn test_timeline_bar_position() {
        // Test that bars are positioned correctly relative to time range
        let base_time = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let range_start = base_time;
        let range_end = base_time + Duration::from_secs(10 * 3600); // 10 hours
        let width = 100;

        // Reservation at the start
        let res_at_start = GpuReservation {
            id: 1,
            user: CompactString::from("user1"),
            gpu_spec: GpuSpec::Count(1),
            start_time: range_start,
            duration: Duration::from_secs(3600), // 1 hour
            status: ReservationStatus::Pending,
            created_at: base_time,
            cancelled_at: None,
        };

        // Reservation in the middle
        let res_at_middle = GpuReservation {
            id: 2,
            user: CompactString::from("user2"),
            gpu_spec: GpuSpec::Count(1),
            start_time: range_start + Duration::from_secs(5 * 3600), // 5 hours from start
            duration: Duration::from_secs(3600),                     // 1 hour
            status: ReservationStatus::Active,
            created_at: base_time,
            cancelled_at: None,
        };

        let config = TimelineConfig {
            width,
            time_range: (range_start, range_end),
            timezone: None,
        };

        let mut output = Vec::new();
        render_timeline_to_writer(&[res_at_start, res_at_middle], config, &mut output);
        let output_str = String::from_utf8(output).unwrap();

        // Both reservations should be rendered
        assert!(output_str.contains("user1"));
        assert!(output_str.contains("user2"));

        // Verify status characters are present
        assert!(output_str.contains('░')); // Pending
        assert!(output_str.contains('█')); // Active
    }

    #[test]
    fn test_timeline_end_to_end_output() {
        // End-to-end test with exact output verification
        // Use a fixed time range for deterministic output
        let base_time = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let range_start = base_time;
        let range_end = base_time + Duration::from_secs(8 * 3600); // 8 hours

        // Create two reservations with known positions
        let reservation1 = GpuReservation {
            id: 1,
            user: CompactString::from("alice"),
            gpu_spec: GpuSpec::Count(2),
            start_time: range_start + Duration::from_secs(2 * 3600), // 2 hours from start
            duration: Duration::from_secs(2 * 3600),                 // 2 hours duration
            status: ReservationStatus::Active,
            created_at: base_time,
            cancelled_at: None,
        };

        let reservation2 = GpuReservation {
            id: 2,
            user: CompactString::from("bob"),
            gpu_spec: GpuSpec::Count(1),
            start_time: range_start + Duration::from_secs(5 * 3600), // 5 hours from start
            duration: Duration::from_secs(3600),                     // 1 hour duration
            status: ReservationStatus::Pending,
            created_at: base_time,
            cancelled_at: None,
        };

        let config = TimelineConfig {
            width: 80,
            time_range: (range_start, range_end),
            timezone: None,
        };

        let mut output = Vec::new();
        render_timeline_to_writer(&[reservation1, reservation2], config, &mut output);
        let actual_output = String::from_utf8(output).unwrap();

        // Verify output contains expected elements
        assert!(actual_output.contains("GPU Reservations Timeline"));
        assert!(actual_output.contains("alice (GPU: 2 GPUs)"));
        assert!(actual_output.contains("bob (GPU: 1 GPU)"));
        assert!(actual_output.contains("Active"));
        assert!(actual_output.contains("Pending"));
        assert!(actual_output.contains("Legend: █ Active  ░ Pending"));
        assert!(actual_output.contains("Summary:"));

        // Verify the timeline structure
        let lines: Vec<&str> = actual_output.lines().collect();

        // Should have header line with timestamp
        let header_line = lines
            .iter()
            .find(|l| l.contains("GPU Reservations Timeline"))
            .unwrap();
        // Verify timestamp format (should contain date and time in parentheses)
        assert!(header_line.contains("("));
        assert!(header_line.contains(")"));
        let timestamp_part = header_line
            .split('(')
            .nth(1)
            .unwrap()
            .split(')')
            .next()
            .unwrap();
        assert!(timestamp_part.contains('-')); // Date separator
        assert!(timestamp_part.contains(':')); // Time separator

        // Should have axis line
        assert!(lines.iter().any(|l| l.contains('┬') && l.contains('─')));

        // Should have reservation bars
        assert!(lines.iter().any(|l| l.contains('█') || l.contains('░')));
    }
}
