//! Structured progress published by a running job.
//!
//! gflow cannot infer how far a long-running job has come, so the job may
//! publish its own progress. The contract is deliberately tiny: write a small
//! JSON document to the file named by the `GFLOW_PROGRESS_FILE` environment
//! variable that the executor exports into every job:
//!
//! ```json
//! { "value": 24925, "total": 40000, "message": "epoch 25/40" }
//! ```
//!
//! The daemon reads that file on demand, derives display-only values (percent,
//! rate, ETA, staleness) and never persists them. `gjob progress` is a
//! convenience wrapper that writes the same document.
//!
//! The file lives in the daemon's data directory (`progress/<job_id>.json`),
//! not in the job's working directory, so it works for every user on the host
//! without needing a path inside the job's run directory.

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Largest progress document accepted from a job. Anything bigger is ignored
/// rather than parsed, so a runaway writer cannot make the daemon allocate.
pub const MAX_PROGRESS_FILE_BYTES: u64 = 8 * 1024;

/// Longest `message` kept from a progress document (chars, not bytes).
pub const MAX_PROGRESS_MESSAGE_CHARS: usize = 256;

/// A running job that has not refreshed its progress for this long is flagged
/// stale. The flag is advisory: a job that publishes progress rarely should
/// use a longer interval or accept the flag.
pub const PROGRESS_STALE_AFTER_SECS: u64 = 15 * 60;

/// Progress document written by a job (or by `gjob progress`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JobProgress {
    /// Schema version; currently always 1.
    pub version: u32,
    /// Job the document was published for. Advisory: the file path is already
    /// per-job, so a mismatch is only logged.
    pub job_id: Option<u32>,
    /// Completed work units (steps, epochs, samples, ...).
    pub value: u64,
    /// Total work units, when known.
    pub total: Option<u64>,
    /// Free-form short status line.
    pub message: Option<String>,
    /// Unix seconds when the measured work began, when it differs from the
    /// job's start (e.g. after a long data-loading phase).
    pub started_at_unix_secs: Option<u64>,
}

impl Default for JobProgress {
    fn default() -> Self {
        Self {
            version: PROGRESS_VERSION,
            job_id: None,
            value: 0,
            total: None,
            message: None,
            started_at_unix_secs: None,
        }
    }
}

/// Current progress document schema version.
pub const PROGRESS_VERSION: u32 = 1;

#[derive(Deserialize)]
#[serde(default)]
struct RawProgress {
    version: u32,
    job_id: Option<u32>,
    /// Required: a document without `value` carries no progress at all.
    #[serde(default)]
    value: Option<u64>,
    total: Option<u64>,
    message: Option<String>,
    started_at_unix_secs: Option<u64>,
}

impl Default for RawProgress {
    fn default() -> Self {
        Self {
            version: PROGRESS_VERSION,
            job_id: None,
            value: None,
            total: None,
            message: None,
            started_at_unix_secs: None,
        }
    }
}

/// Deserialization shares the lenient parse rules used for on-disk documents,
/// so the API and the file contract cannot drift.
impl<'de> Deserialize<'de> for JobProgress {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RawProgress::deserialize(deserializer)?;
        let value = raw
            .value
            .ok_or_else(|| serde::de::Error::missing_field("value"))?;
        Ok(Self {
            version: raw.version,
            job_id: raw.job_id,
            value,
            total: raw.total.filter(|total| *total > 0),
            message: raw.message.as_deref().and_then(normalize_message),
            started_at_unix_secs: raw.started_at_unix_secs,
        })
    }
}

impl JobProgress {
    /// Parse a progress document, rejecting ones that carry no usable value.
    pub fn from_json(bytes: &[u8]) -> Option<Self> {
        let raw: RawProgress = serde_json::from_slice(bytes).ok()?;
        let value = raw.value?;
        Some(Self {
            version: raw.version,
            job_id: raw.job_id,
            value,
            total: raw.total.filter(|total| *total > 0),
            message: raw.message.as_deref().and_then(normalize_message),
            started_at_unix_secs: raw.started_at_unix_secs,
        })
    }

    pub fn to_json(&self) -> anyhow::Result<Vec<u8>> {
        Ok(serde_json::to_vec(self)?)
    }

    /// Wall-clock at which the measured work began, when the document says so.
    pub fn started_at(&self) -> Option<SystemTime> {
        self.started_at_unix_secs
            .and_then(|secs| UNIX_EPOCH.checked_add(Duration::from_secs(secs)))
    }
}

/// Trim and bound a progress message. Blank messages are dropped.
fn normalize_message(message: &str) -> Option<String> {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut out: String = trimmed.chars().take(MAX_PROGRESS_MESSAGE_CHARS).collect();
    if trimmed.chars().count() > MAX_PROGRESS_MESSAGE_CHARS {
        out.push('…');
    }
    Some(out)
}

/// Display-only view of a job's progress, computed by the daemon from the
/// progress document plus the job's start time and file mtime.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct JobProgressView {
    pub value: u64,
    pub total: Option<u64>,
    /// Completion percentage in `0.0..=100.0`, when a total is known.
    pub percent: Option<f64>,
    pub message: Option<String>,
    /// Wall-clock of the last progress update (the file's mtime).
    pub updated_at: SystemTime,
    /// Seconds since `updated_at`.
    pub idle_secs: u64,
    /// Whether the job has gone without a progress update for
    /// [`PROGRESS_STALE_AFTER_SECS`].
    pub stale: bool,
    /// Average work units per second since the measured work began.
    pub rate_per_sec: Option<f64>,
    /// Estimated seconds left from now at `rate_per_sec`; `None` when unknown.
    pub eta_secs: Option<u64>,
}

impl JobProgressView {
    /// Derive the view from a parsed document.
    ///
    /// `started_at` is the job's own start time, used for the rate/ETA clock
    /// unless the document overrides it via `started_at_unix_secs`.
    pub fn compute(
        progress: &JobProgress,
        updated_at: SystemTime,
        started_at: Option<SystemTime>,
        now: SystemTime,
    ) -> Self {
        let idle_secs = now.duration_since(updated_at).unwrap_or_default().as_secs();
        let stale = idle_secs >= PROGRESS_STALE_AFTER_SECS;

        // A started_at in the future cannot be trusted as a measurement clock;
        // fall back to the job's start instead of producing negative elapsed.
        let work_started = progress
            .started_at()
            .filter(|started| *started <= now)
            .or(started_at);

        let rate_per_sec = work_started.and_then(|started| {
            let elapsed = updated_at.duration_since(started).ok()?;
            if progress.value == 0 || elapsed.is_zero() {
                return None;
            }
            Some(progress.value as f64 / elapsed.as_secs_f64())
        });

        let percent = progress
            .total
            .map(|total| (100.0 * progress.value as f64 / total as f64).min(100.0))
            .filter(|pct| pct.is_finite());

        let eta_secs = match progress.total {
            // Reaching the total is done work, whether or not a rate is known.
            Some(total) if total <= progress.value => Some(0),
            Some(total) => rate_per_sec
                .filter(|rate| *rate > 0.0)
                // Work left at the observed rate. Idle time since the last
                // update is *not* progress, so it is not discounted: a job that
                // paused is expected to need the same time left once it
                // resumes. `stale` flags the case where the rate itself is no
                // longer trustworthy.
                .map(|rate| {
                    // Round up: a sub-second remainder is still work left, and
                    // reporting 0 would render as `done` for a job that is not.
                    ((total - progress.value) as f64 / rate).ceil() as u64
                }),
            None => None,
        };

        Self {
            value: progress.value,
            total: progress.total,
            percent,
            message: progress.message.clone(),
            updated_at,
            idle_secs,
            stale,
            rate_per_sec,
            eta_secs,
        }
    }

    /// Compact one-line form used by table output, e.g. `24925/40000 (62.3%)`.
    pub fn summary(&self) -> String {
        let mut out = match (self.total, self.percent) {
            (Some(total), Some(percent)) => {
                format!("{}/{} ({})", self.value, total, format_percent(percent))
            }
            _ => self.value.to_string(),
        };
        if self.stale {
            out.push_str(" stale");
        }
        out
    }

    /// Percentage rendered for humans (`62%`, `62.3%`).
    pub fn percent_display(&self) -> Option<String> {
        self.percent.map(format_percent)
    }
}

/// Render a percentage without a pointless decimal on whole numbers.
pub fn format_percent(percent: f64) -> String {
    let rounded = percent.round();
    if (percent - rounded).abs() < 0.05 {
        format!("{}%", rounded as i64)
    } else {
        format!("{percent:.1}%")
    }
}

/// Read and derive the progress of a job, if it published any.
///
/// Returns `None` when no progress document exists, when it is unreadable, or
/// when it cannot be parsed as a usable document.
pub fn load(job_id: u32, started_at: Option<SystemTime>) -> Option<JobProgressView> {
    let path = crate::paths::get_progress_file_path(job_id).ok()?;
    let metadata = std::fs::metadata(&path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_PROGRESS_FILE_BYTES {
        return None;
    }
    let bytes = std::fs::read(&path).ok()?;
    let progress = JobProgress::from_json(&bytes)?;
    if progress.job_id.is_some_and(|id| id != job_id) {
        tracing::warn!(
            job_id,
            document_job_id = progress.job_id,
            "Ignoring progress document published for a different job"
        );
        return None;
    }
    let updated_at = metadata.modified().ok()?;
    Some(JobProgressView::compute(
        &progress,
        updated_at,
        started_at,
        SystemTime::now(),
    ))
}

/// Write a progress document atomically, replacing any previous one.
pub fn write(job_id: u32, progress: &JobProgress) -> anyhow::Result<PathBuf> {
    let path = crate::paths::get_progress_file_path(job_id)?;
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("progress path has no parent: {}", path.display()))?;
    std::fs::create_dir_all(parent)?;

    let tmp_path = parent.join(format!(
        ".{}.tmp.{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("progress"),
        std::process::id()
    ));
    let mut document = progress.clone();
    document.version = PROGRESS_VERSION;
    document.job_id = Some(job_id);

    let mut file = std::fs::File::create(&tmp_path)?;
    file.write_all(&document.to_json()?)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    std::fs::rename(&tmp_path, &path)?;
    Ok(path)
}

/// Remove a job's progress document and make sure the progress directory
/// exists, so a job can create its document directly even on a fresh host.
pub fn reset(job_id: u32) -> anyhow::Result<()> {
    let path = crate::paths::get_progress_file_path(job_id)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

/// Remove a job's progress document. Missing files are not an error.
pub fn clear(job_id: u32) -> anyhow::Result<()> {
    let path = crate::paths::get_progress_file_path(job_id)?;
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn progress(value: u64, total: Option<u64>) -> JobProgress {
        JobProgress {
            value,
            total,
            ..JobProgress::default()
        }
    }

    fn now() -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_000_000)
    }

    #[test]
    fn parses_a_json_document() {
        let parsed = JobProgress::from_json(
            br#"{"version":1,"job_id":7,"value":25,"total":100,"message":"epoch 25"}"#,
        )
        .expect("document should parse");
        assert_eq!(parsed.value, 25);
        assert_eq!(parsed.total, Some(100));
        assert_eq!(parsed.message.as_deref(), Some("epoch 25"));
        assert_eq!(parsed.job_id, Some(7));
    }

    #[test]
    fn rejects_documents_without_a_value() {
        assert!(JobProgress::from_json(b"{}").is_none());
        assert!(JobProgress::from_json(br#"{"message":"working"}"#).is_none());
        assert!(JobProgress::from_json(b"not json").is_none());
    }

    #[test]
    fn ignores_a_zero_total_and_blank_message() {
        let parsed = JobProgress::from_json(br#"{"value":3,"total":0,"message":"   "}"#).unwrap();
        assert_eq!(parsed.total, None);
        assert_eq!(parsed.message, None);
    }

    #[test]
    fn bounds_long_messages() {
        let long = "x".repeat(MAX_PROGRESS_MESSAGE_CHARS + 50);
        let parsed =
            JobProgress::from_json(format!(r#"{{"value":1,"message":"{long}"}}"#).as_bytes())
                .unwrap();
        let message = parsed.message.expect("message should be kept");
        assert_eq!(message.chars().count(), MAX_PROGRESS_MESSAGE_CHARS + 1);
        assert!(message.ends_with('…'));
    }

    #[test]
    fn computes_percent_rate_and_eta() {
        // 25 of 100 units in 50s of work -> 0.5 units/s -> 150s of work left.
        // The document's clock runs up to the last update, one second ago.
        let mut document = progress(25, Some(100));
        document.started_at_unix_secs = Some(999_949);
        let updated_at = now() - Duration::from_secs(1);
        let view = JobProgressView::compute(&document, updated_at, None, now());

        assert_eq!(view.percent, Some(25.0));
        assert_eq!(view.percent_display().as_deref(), Some("25%"));
        let rate = view.rate_per_sec.expect("rate should be known");
        assert!((rate - 0.5).abs() < 0.001, "unexpected rate {rate}");
        // 75 units / 0.5 = 150s of work, regardless of how long the job has
        // been idle since the update: idle time is not progress.
        assert_eq!(view.eta_secs, Some(150));
        assert_eq!(view.idle_secs, 1);
        assert!(!view.stale);
        assert_eq!(view.summary(), "25/100 (25%)");
    }

    #[test]
    fn an_idle_job_is_not_reported_as_finished() {
        // 25 of 100 units done, then no update for longer than the stale
        // threshold. The job has not finished, so the ETA must stay positive
        // and the report must be stale.
        let idle = PROGRESS_STALE_AFTER_SECS + 60;
        let mut document = progress(25, Some(100));
        document.started_at_unix_secs = Some(1_000_000 - idle - 50);
        let updated_at = now() - Duration::from_secs(idle);
        let view = JobProgressView::compute(&document, updated_at, None, now());

        assert!(view.stale, "{idle}s without an update is stale");
        assert_eq!(view.percent, Some(25.0));
        assert_eq!(
            view.eta_secs,
            Some(150),
            "idle time must not be deducted from the remaining work"
        );
    }

    #[test]
    fn a_sub_second_remainder_is_not_reported_as_done() {
        // A job publishing every few seconds reaches a high observed rate, so
        // the remaining work can round to under a second. Reporting 0 would
        // render as `done` (and `ETA=done`) for a job that is still running.
        let mut document = progress(99, Some(100));
        document.started_at_unix_secs = Some(1_000_000 - 1);
        let view = JobProgressView::compute(&document, now(), None, now());
        assert_eq!(view.eta_secs, Some(1), "one unit left is not `done`");
    }

    #[test]
    fn eta_is_zero_once_the_total_is_reached() {
        let mut document = progress(100, Some(100));
        document.started_at_unix_secs = Some(999_900);
        let view = JobProgressView::compute(&document, now(), None, now());
        assert_eq!(view.eta_secs, Some(0));
        assert_eq!(view.percent, Some(100.0));
    }

    #[test]
    fn clamps_percent_when_value_exceeds_total() {
        let document = progress(120, Some(100));
        let view = JobProgressView::compute(&document, now(), None, now());
        assert_eq!(view.percent, Some(100.0));
        assert_eq!(view.eta_secs, Some(0));
    }

    #[test]
    fn no_total_means_no_percent_or_eta() {
        let mut document = progress(25, None);
        document.started_at_unix_secs = Some(999_000);
        let view = JobProgressView::compute(&document, now(), None, now());
        assert_eq!(view.percent, None);
        assert_eq!(view.eta_secs, None);
        assert!(view.rate_per_sec.is_some());
        assert_eq!(view.summary(), "25");
    }

    #[test]
    fn a_zero_value_cannot_produce_a_rate() {
        let mut document = progress(0, Some(100));
        document.started_at_unix_secs = Some(999_000);
        let view = JobProgressView::compute(&document, now(), None, now());
        assert_eq!(view.rate_per_sec, None);
        assert_eq!(view.eta_secs, None);
        assert_eq!(view.percent, Some(0.0));
    }

    #[test]
    fn falls_back_to_the_job_start_time() {
        let document = progress(10, Some(20));
        let job_started = now() - Duration::from_secs(100);
        let view = JobProgressView::compute(&document, now(), Some(job_started), now());
        let rate = view.rate_per_sec.expect("rate should use the job start");
        assert!((rate - 0.1).abs() < 0.001, "unexpected rate {rate}");
    }

    #[test]
    fn a_future_start_time_falls_back_to_the_job_start() {
        let mut document = progress(10, Some(20));
        document.started_at_unix_secs = Some(1_000_000 + 500);
        let job_started = now() - Duration::from_secs(50);
        let view = JobProgressView::compute(&document, now(), Some(job_started), now());
        let rate = view.rate_per_sec.expect("future start must be ignored");
        assert!((rate - 0.2).abs() < 0.001, "unexpected rate {rate}");
    }

    #[test]
    fn marks_stale_progress() {
        let document = progress(1, Some(10));
        let updated_at = now() - Duration::from_secs(PROGRESS_STALE_AFTER_SECS);
        let view = JobProgressView::compute(&document, updated_at, None, now());
        assert!(view.stale);
        assert_eq!(view.idle_secs, PROGRESS_STALE_AFTER_SECS);
        assert!(view.summary().ends_with("stale"));
    }

    #[test]
    fn a_document_round_trips_through_the_api_payload_shape() {
        // The API sends the same `JobProgress` type the job may write to disk,
        // so a round trip must preserve every field.
        let document = JobProgress {
            version: PROGRESS_VERSION,
            job_id: Some(42),
            value: 25,
            total: Some(100),
            message: Some("epoch 25/40".into()),
            started_at_unix_secs: Some(999_949),
        };
        let json = document.to_json().expect("document should serialize");
        let parsed = JobProgress::from_json(&json).expect("document should parse");
        assert_eq!(parsed, document);
    }

    #[test]
    fn rejecting_a_missing_value_matches_the_file_contract() {
        // Deserialization is shared between the API and the file, so a document
        // without `value` must fail the same way in both directions.
        let err = serde_json::from_slice::<JobProgress>(br#"{"total":10}"#)
            .expect_err("a document without value must be rejected");
        assert!(err.to_string().contains("value"), "unexpected error: {err}");
    }

    #[test]
    fn formats_percent_without_a_noisy_decimal() {
        assert_eq!(format_percent(62.0), "62%");
        assert_eq!(format_percent(62.31), "62.3%");
        assert_eq!(format_percent(99.96), "100%");
    }
}
