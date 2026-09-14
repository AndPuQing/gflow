//! Small helpers for summarising a job's log file.
//!
//! `gjob show` reports the log file's path, size, last-modified time and its
//! last line. Together with the job's progress (when it publishes any) this
//! answers the everyday "is this job still making progress?" question without
//! making the user parse the log themselves.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Longest last-log-line kept for display (chars, not bytes).
pub const MAX_LAST_LINE_CHARS: usize = 200;

/// How much of the log tail is read to find the last non-empty line. A job
/// whose last line is longer than this is reported with a truncated tail.
const TAIL_READ_BYTES: u64 = 64 * 1024;

/// A cheap summary of a job's log file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogSummary {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub modified_at: Option<SystemTime>,
    /// Last non-empty line of the log, with trailing whitespace trimmed.
    pub last_line: Option<String>,
}

impl LogSummary {
    /// Seconds since the log file was last modified, if its mtime is readable.
    pub fn idle_secs(&self) -> Option<u64> {
        let modified = self.modified_at?;
        SystemTime::now()
            .duration_since(modified)
            .ok()
            .map(|d| d.as_secs())
    }
}

/// Summarise the log file at `path`, or `None` when it does not exist.
///
/// Read errors on the tail are not fatal: the file metadata is still reported,
/// which is what tells the user whether the log is still being written.
pub fn summarize(path: &Path) -> Option<LogSummary> {
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }

    Some(LogSummary {
        path: path.to_path_buf(),
        size_bytes: metadata.len(),
        modified_at: metadata.modified().ok(),
        last_line: last_non_empty_line(path).ok().flatten(),
    })
}

/// Read the last non-empty line of a file without loading the whole log.
fn last_non_empty_line(path: &Path) -> std::io::Result<Option<String>> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    let start = len.saturating_sub(TAIL_READ_BYTES);
    file.seek(SeekFrom::Start(start))?;

    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;

    let text = String::from_utf8_lossy(&buffer);
    // When the read started mid-file the first line is likely partial; drop it
    // rather than reporting a cut-off line as the job's latest output.
    let text = if start > 0 {
        text.split_once('\n').map(|(_, rest)| rest).unwrap_or("")
    } else {
        text.as_ref()
    };

    // Terminal output uses both `\n` and `\r` as line breaks (a progress bar
    // rewrites its line with `\r`), so split on both to find the true last
    // line rather than gluing successive updates together.
    Ok(text.split(['\n', '\r']).rev().find_map(normalize))
}

/// Trim a log line and bound its length; blank lines yield `None`.
fn normalize(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Strip terminal control characters a pipe-pane capture may have left in.
    let cleaned: String = trimmed
        .chars()
        .filter(|ch| *ch != '\r' && !(ch.is_control() && *ch != '\t'))
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        return None;
    }

    let mut out: String = cleaned.chars().take(MAX_LAST_LINE_CHARS).collect();
    if cleaned.chars().count() > MAX_LAST_LINE_CHARS {
        out.push('…');
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_temp(contents: &str) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(contents.as_bytes()).unwrap();
        file.flush().unwrap();
        file
    }

    #[test]
    fn summarizes_size_and_last_non_empty_line() {
        let file = write_temp("epoch 1\nepoch 2\n\n");
        let summary = summarize(file.path()).expect("log should be summarizable");
        assert_eq!(summary.size_bytes, "epoch 1\nepoch 2\n\n".len() as u64);
        assert_eq!(summary.last_line.as_deref(), Some("epoch 2"));
        assert!(summary.idle_secs().is_some());
    }

    #[test]
    fn missing_log_has_no_summary() {
        assert!(summarize(Path::new("/nonexistent/gflow/log/file.log")).is_none());
    }

    #[test]
    fn skips_blank_and_control_only_lines() {
        let file = write_temp("done\n   \n\r\n");
        let summary = summarize(file.path()).unwrap();
        assert_eq!(summary.last_line.as_deref(), Some("done"));
    }

    #[test]
    fn bounds_a_long_last_line() {
        let long = "x".repeat(MAX_LAST_LINE_CHARS + 10);
        let file = write_temp(&format!("{long}\n"));
        let summary = summarize(file.path()).unwrap();
        let line = summary.last_line.unwrap();
        assert_eq!(line.chars().count(), MAX_LAST_LINE_CHARS + 1);
        assert!(line.ends_with('…'));
    }

    #[test]
    fn strips_terminal_carriage_returns() {
        let file = write_temp("step 10\rstep 11\r");
        let summary = summarize(file.path()).unwrap();
        assert_eq!(summary.last_line.as_deref(), Some("step 11"));
    }
}
