use anyhow::{bail, Context, Result};
use chrono::{DateTime, Local, NaiveDate, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use gflow::config::OffHoursConfig;
use std::{
    ops::Range,
    time::{Duration, SystemTime},
};

// Allow for the timeout monitor's polling interval and executor shutdown.
const CLEANUP_MARGIN: Duration = Duration::from_secs(30);

#[derive(Clone, Copy)]
pub(super) struct OffHoursWindow {
    start: NaiveTime,
    end: NaiveTime,
    timezone: Option<Tz>,
}

impl OffHoursWindow {
    pub(super) fn new(config: &OffHoursConfig, timezone: Option<&str>) -> Result<Self> {
        let start = NaiveTime::parse_from_str(&config.start, "%H:%M")
            .context("daemon.off_hours.start must be a time in HH:MM format")?;
        let end = NaiveTime::parse_from_str(&config.end, "%H:%M")
            .context("daemon.off_hours.end must be a time in HH:MM format")?;
        if start == end {
            bail!("daemon.off_hours.start and daemon.off_hours.end must differ");
        }
        let timezone = timezone
            .map(str::parse::<Tz>)
            .transpose()
            .context("Invalid timezone for daemon.off_hours")?;
        Ok(Self {
            start,
            end,
            timezone,
        })
    }

    pub(super) fn remaining(&self, now: SystemTime) -> Option<Duration> {
        let window = self.window_at_or_after(now)?;
        if window.contains(&now) {
            window
                .end
                .duration_since(now)
                .ok()?
                .checked_sub(CLEANUP_MARGIN)
        } else {
            None
        }
    }

    pub(super) fn next_start(&self, now: SystemTime) -> Option<SystemTime> {
        let window = self.window_at_or_after(now)?;
        if window.start > now {
            Some(window.start)
        } else {
            Some(self.window_at_or_after(window.end)?.start)
        }
    }

    fn window_at_or_after(&self, now: SystemTime) -> Option<Range<SystemTime>> {
        let utc = DateTime::<Utc>::from(now);
        match self.timezone {
            Some(tz) => self.window_in_timezone(utc.with_timezone(&tz)),
            None => self.window_in_timezone(utc.with_timezone(&Local)),
        }
    }

    fn window_in_timezone<T: TimeZone>(&self, now: DateTime<T>) -> Option<Range<SystemTime>> {
        let timezone = now.timezone();
        // Yesterday's window may still be open if it crosses midnight.
        let mut date = now.date_naive().pred_opt()?;
        let now = SystemTime::from(now);
        loop {
            if let Some(window) = self.window_on(date, &timezone) {
                if window.end > now {
                    return Some(window);
                }
            }
            date = date.succ_opt()?;
        }
    }

    fn window_on<T: TimeZone>(&self, date: NaiveDate, timezone: &T) -> Option<Range<SystemTime>> {
        let end_date = if self.end < self.start {
            date.succ_opt()?
        } else {
            date
        };
        // Use the first occurrence of a repeated wall time and skip windows
        // with a missing boundary when the clock moves forward.
        let start = timezone
            .from_local_datetime(&date.and_time(self.start))
            .earliest()?;
        let end = timezone
            .from_local_datetime(&end_date.and_time(self.end))
            .earliest()?;
        Some(start.into()..end.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timestamp(value: &str) -> SystemTime {
        DateTime::parse_from_rfc3339(value).unwrap().into()
    }

    #[test]
    fn off_hours_window_spans_midnight_and_wakes_at_next_start() {
        let window = OffHoursWindow::new(
            &OffHoursConfig {
                start: "23:00".into(),
                end: "07:00".into(),
            },
            Some("Asia/Shanghai"),
        )
        .unwrap();
        let before = timestamp("2026-11-18T22:59:59+08:00");
        let start = timestamp("2026-11-18T23:00:00+08:00");
        let end = timestamp("2026-11-19T07:00:00+08:00");

        assert_eq!(window.remaining(before), None);
        assert_eq!(window.next_start(before), Some(start));
        assert_eq!(
            window.remaining(start),
            Some(Duration::from_secs(8 * 3600) - CLEANUP_MARGIN)
        );
        assert_eq!(
            window.remaining(timestamp("2026-11-19T06:30:00+08:00")),
            Some(Duration::from_secs(30 * 60) - CLEANUP_MARGIN),
        );
        assert_eq!(window.remaining(end), None);
        assert_eq!(
            window.next_start(start),
            Some(start + Duration::from_secs(24 * 3600))
        );
    }

    #[test]
    fn off_hours_remaining_uses_elapsed_time_across_daylight_saving() {
        let window = OffHoursWindow::new(
            &OffHoursConfig {
                start: "23:00".into(),
                end: "07:00".into(),
            },
            Some("America/New_York"),
        )
        .unwrap();

        assert_eq!(
            window.remaining(timestamp("2026-03-07T23:00:00-05:00")),
            Some(Duration::from_secs(7 * 3600) - CLEANUP_MARGIN),
        );
    }
}
