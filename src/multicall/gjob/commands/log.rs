use anyhow::{Context, Result};
use gflow::utils::terminal::TerminalCleaner;
use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How much of the captured log to print.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LogSlice {
    Full,
    First(usize),
    Last(usize),
}

/// How to render the captured bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    /// Strip escape sequences and collapse progress-bar repaints.
    Clean,
    /// Strip escape sequences but keep `\r` repaints as separate lines.
    NoAnsi,
    /// Print the capture verbatim.
    Raw,
}

impl LogFormat {
    fn cleaner(self) -> TerminalCleaner {
        match self {
            LogFormat::NoAnsi => TerminalCleaner::with_line_breaks_on_cr(),
            LogFormat::Clean | LogFormat::Raw => TerminalCleaner::new(),
        }
    }
}

/// Options for `gjob log`.
#[derive(Debug, Clone, Copy, Default)]
pub struct LogOptions {
    pub first_lines: Option<NonZeroUsize>,
    pub last_lines: Option<NonZeroUsize>,
    pub raw: bool,
    pub no_ansi: bool,
    pub follow: bool,
    pub path_only: bool,
}

impl LogOptions {
    fn slice(&self) -> Result<LogSlice> {
        resolve_log_slice(self.first_lines, self.last_lines)
    }

    fn format(&self) -> LogFormat {
        if self.raw {
            LogFormat::Raw
        } else if self.no_ansi {
            LogFormat::NoAnsi
        } else {
            LogFormat::Clean
        }
    }
}

fn resolve_log_slice(
    first_lines: Option<NonZeroUsize>,
    last_lines: Option<NonZeroUsize>,
) -> Result<LogSlice> {
    match (first_lines, last_lines) {
        (Some(_), Some(_)) => {
            anyhow::bail!("gjob log accepts only one of --first or --last");
        }
        (Some(lines), None) => Ok(LogSlice::First(lines.get())),
        (None, Some(lines)) => Ok(LogSlice::Last(lines.get())),
        (None, None) => Ok(LogSlice::Full),
    }
}

pub async fn handle_log(
    config_path: &Option<PathBuf>,
    job_id_str: &str,
    options: LogOptions,
) -> Result<()> {
    let client = gflow::create_client(config_path)?;

    // Resolve job ID (handle @ shorthand)
    let job_id = crate::multicall::gjob::utils::resolve_job_id(&client, job_id_str).await?;

    let log_path = client.get_job_log_path(job_id).await?.map(PathBuf::from);

    let stdout = io::stdout();
    let mut stdout = stdout.lock();

    // Following a just-submitted job is the common case, and its log file may
    // not exist yet, so `--follow` waits for it instead of failing up front.
    if options.follow {
        let format = options.format();
        return follow_log(&client, job_id, log_path.as_deref(), &mut stdout, format).await;
    }

    let log_path = match log_path {
        Some(path) => path,
        None => {
            eprintln!(
                "Log for job {} is not available (no log file yet, or the job is unknown to \
                 the daemon). Durable logs live at $XDG_DATA_HOME/gflow/logs/<jobid>.log \
                 (default ~/.local/share/gflow/logs/<jobid>.log).",
                job_id
            );
            return Ok(());
        }
    };

    if options.path_only {
        println!("{}", log_path.display());
        return Ok(());
    }

    let slice = options.slice()?;
    let format = options.format();

    let mut file = std::fs::File::open(&log_path).with_context(|| {
        format!(
            "Failed to open log file '{}' for job {}",
            log_path.display(),
            job_id
        )
    })?;

    match format {
        LogFormat::Raw => write_raw_log(&mut file, &mut stdout, slice)
            .context("Failed to write log contents to stdout")?,
        LogFormat::Clean | LogFormat::NoAnsi => {
            write_cleaned_log(&mut file, &mut stdout, slice, format)
                .context("Failed to write log contents to stdout")?
        }
    }

    stdout.flush().context("Failed to flush stdout")?;

    Ok(())
}

/// Print the capture verbatim, slicing by physical lines.
fn write_raw_log<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    slice: LogSlice,
) -> io::Result<()> {
    match slice {
        LogSlice::Full => {
            io::copy(reader, writer)?;
        }
        LogSlice::First(lines) => {
            let mut reader = io::BufReader::new(reader);
            let mut buffer = Vec::new();

            for _ in 0..lines {
                buffer.clear();
                if io::BufRead::read_until(&mut reader, b'\n', &mut buffer)? == 0 {
                    break;
                }
                writer.write_all(&buffer)?;
            }
        }
        LogSlice::Last(lines) => {
            let mut reader = io::BufReader::new(reader);
            let mut buffer = Vec::new();
            let mut tail = VecDeque::with_capacity(lines);

            loop {
                buffer.clear();
                if io::BufRead::read_until(&mut reader, b'\n', &mut buffer)? == 0 {
                    break;
                }

                if tail.len() == lines {
                    tail.pop_front();
                }
                tail.push_back(std::mem::take(&mut buffer));
            }

            for line in tail {
                writer.write_all(&line)?;
            }
        }
    }

    Ok(())
}

/// Collects the lines the cleaner produces, honouring the requested slice.
struct LineSink<'a, W: Write> {
    writer: &'a mut W,
    slice: LogSlice,
    remaining: usize,
    tail: VecDeque<String>,
    done: bool,
}

impl<W: Write> LineSink<'_, W> {
    fn emit(&mut self, line: &str) -> io::Result<()> {
        match self.slice {
            LogSlice::Full => writeln!(self.writer, "{line}"),
            LogSlice::First(_) => {
                if self.remaining > 0 {
                    writeln!(self.writer, "{line}")?;
                    self.remaining -= 1;
                }
                self.done = self.remaining == 0;
                Ok(())
            }
            LogSlice::Last(capacity) => {
                if self.tail.len() == capacity {
                    self.tail.pop_front();
                }
                self.tail.push_back(line.to_string());
                Ok(())
            }
        }
    }

    /// Whether every requested leading line has already been written.
    fn is_done(&self) -> bool {
        self.done
    }
}

/// Print the capture with terminal control sequences resolved, slicing by the
/// lines that actually end up on screen.
fn write_cleaned_log<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    slice: LogSlice,
    format: LogFormat,
) -> io::Result<()> {
    let mut cleaner = format.cleaner();
    let mut sink = LineSink {
        writer,
        slice,
        remaining: match slice {
            LogSlice::First(lines) => lines,
            _ => 0,
        },
        tail: VecDeque::new(),
        done: false,
    };

    let mut buffer = Vec::new();
    let mut chunk = [0u8; 64 * 1024];
    let mut stopped_early = false;

    loop {
        let read = reader.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        let text = drain_utf8(&mut buffer);
        if text.is_empty() {
            continue;
        }
        cleaner.feed(&text, &mut |line| sink.emit(line))?;
        if sink.is_done() {
            stopped_early = true;
            break;
        }
    }

    if !stopped_early {
        if !buffer.is_empty() {
            let text = String::from_utf8_lossy(&buffer).into_owned();
            buffer.clear();
            if !text.is_empty() {
                cleaner.feed(&text, &mut |line| sink.emit(line))?;
            }
        }
        cleaner.finish(&mut |line| sink.emit(line))?;
    }

    for line in &sink.tail {
        writeln!(sink.writer, "{line}")?;
    }

    Ok(())
}

/// Stream appended log content until the job reaches a final state.
///
/// `log_path` is `None` while the daemon has not decided on a log file yet
/// (a just-submitted job); the loop waits for it to appear, and exits early
/// when the job is already known to be finished without ever producing one.
async fn follow_log<W: Write>(
    client: &gflow::Client,
    job_id: u32,
    log_path: Option<&Path>,
    writer: &mut W,
    format: LogFormat,
) -> Result<()> {
    eprintln!("Following job {} — press Ctrl-C to stop", job_id);

    // The daemon only reports a log path once the file exists, so a job that is
    // still queued has none. Re-resolve while waiting rather than giving up on
    // the first miss.
    let mut log_path = log_path.map(Path::to_path_buf);

    // Wait for the log file to exist before opening it, giving up once the job
    // is known to be finished without ever producing one.
    let file = loop {
        if log_path.is_none() {
            log_path = client
                .get_job_log_path(job_id)
                .await
                .ok()
                .flatten()
                .map(PathBuf::from);
        }

        match open_log_file(log_path.as_deref()) {
            Ok(file) => break Some(file),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("Failed to open log file for job {job_id}"));
            }
        }

        if job_is_finished(client, job_id).await {
            // The job is done; the daemon's last write may still be landing, so
            // re-resolve and retry once before concluding there is no log.
            log_path = client
                .get_job_log_path(job_id)
                .await
                .ok()
                .flatten()
                .map(PathBuf::from);
            break open_log_file(log_path.as_deref()).ok();
        }

        tokio::time::sleep(Duration::from_millis(200)).await;
    };

    let Some(mut file) = file else {
        eprintln!(
            "Job {} finished without producing a log file (durable logs live at \
             $XDG_DATA_HOME/gflow/logs/<jobid>.log).",
            job_id
        );
        return Ok(());
    };

    let mut cleaner = format.cleaner();
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 64 * 1024];
    let mut final_seen = false;
    let mut last_check = Instant::now();

    // Raw mode writes bytes verbatim; cleaned modes buffer and resolve the
    // terminal state machine, emitting whole lines as they complete.
    let raw = format == LogFormat::Raw;
    // Lines produced while draining the cleaner, written after each chunk so the
    // writer is never borrowed by the emit callback.
    let mut pending: Vec<String> = Vec::new();

    loop {
        let read = file.read(&mut chunk)?;

        if read > 0 {
            // New output resets the completion probe: trailing lines often land
            // just after the state transition.
            final_seen = false;
            if raw {
                writer.write_all(&chunk[..read])?;
                writer.flush()?;
            } else {
                buffer.extend_from_slice(&chunk[..read]);
                let text = drain_utf8(&mut buffer);
                if !text.is_empty() {
                    cleaner.feed(&text, &mut |line| {
                        pending.push(line.to_string());
                        Ok(())
                    })?;
                    write_pending(&mut pending, writer)?;
                }
            }
            continue;
        }

        if final_seen {
            break;
        }

        if last_check.elapsed() >= Duration::from_secs(1) {
            last_check = Instant::now();
            if job_is_finished(client, job_id).await {
                final_seen = true;
                // Give the writer a moment to flush the last lines.
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
        }

        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    if !raw {
        if !buffer.is_empty() {
            let text = String::from_utf8_lossy(&buffer).into_owned();
            buffer.clear();
            if !text.is_empty() {
                cleaner.feed(&text, &mut |line| {
                    pending.push(line.to_string());
                    Ok(())
                })?;
                write_pending(&mut pending, writer)?;
            }
        }
        cleaner.finish(&mut |line| {
            pending.push(line.to_string());
            Ok(())
        })?;
        write_pending(&mut pending, writer)?;
    }

    writer.flush()?;
    Ok(())
}

/// Whether the job exists and has reached a final state.
async fn job_is_finished(client: &gflow::Client, job_id: u32) -> bool {
    matches!(
        client.get_job(job_id).await,
        Ok(Some(job)) if job.state.is_final()
    )
}

/// Open the job's log file when the daemon has reported a path for it.
fn open_log_file(log_path: Option<&Path>) -> io::Result<std::fs::File> {
    match log_path {
        Some(path) => std::fs::File::open(path),
        // The daemon has not chosen a log path yet; treat it as "not there yet"
        // so the follow loop keeps waiting.
        None => Err(io::Error::from(io::ErrorKind::NotFound)),
    }
}

/// Write drained lines to `writer` and clear the queue.
fn write_pending<W: Write>(pending: &mut Vec<String>, writer: &mut W) -> io::Result<()> {
    for line in pending.drain(..) {
        writeln!(writer, "{line}")?;
    }
    writer.flush()
}

/// Decode the longest valid UTF-8 prefix of `buffer`, leaving any incomplete
/// trailing sequence in place for the next read. Invalid bytes are replaced.
fn drain_utf8(buffer: &mut Vec<u8>) -> String {
    let mut output = String::new();

    loop {
        match std::str::from_utf8(buffer) {
            Ok(text) => {
                output.push_str(text);
                buffer.clear();
                return output;
            }
            Err(error) => {
                let valid = error.valid_up_to();
                // The prefix is guaranteed valid UTF-8.
                output.push_str(&String::from_utf8_lossy(&buffer[..valid]));
                match error.error_len() {
                    None => {
                        // Incomplete sequence at the end: keep it for later.
                        buffer.drain(..valid);
                        return output;
                    }
                    Some(len) => {
                        output.push('\u{FFFD}');
                        buffer.drain(..valid + len);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        drain_utf8, resolve_log_slice, write_cleaned_log, write_raw_log, LogFormat, LogSlice,
    };
    use std::io::Cursor;
    use std::num::NonZeroUsize;

    #[test]
    fn rejects_conflicting_log_slice_options() {
        let err = resolve_log_slice(NonZeroUsize::new(10), NonZeroUsize::new(20))
            .expect_err("conflicting options should fail");

        assert!(err.to_string().contains("only one of --first or --last"));
    }

    #[test]
    fn writes_first_n_lines() {
        let input = b"line1\nline2\nline3\n".to_vec();
        let mut reader = Cursor::new(input);
        let mut output = Vec::new();

        write_raw_log(&mut reader, &mut output, LogSlice::First(2)).unwrap();

        assert_eq!(output, b"line1\nline2\n");
    }

    #[test]
    fn writes_last_n_lines() {
        let input = b"line1\nline2\nline3\nline4\n".to_vec();
        let mut reader = Cursor::new(input);
        let mut output = Vec::new();

        write_raw_log(&mut reader, &mut output, LogSlice::Last(2)).unwrap();

        assert_eq!(output, b"line3\nline4\n");
    }

    #[test]
    fn preserves_partial_last_line_when_tailing() {
        let input = b"line1\nline2\nline3".to_vec();
        let mut reader = Cursor::new(input);
        let mut output = Vec::new();

        write_raw_log(&mut reader, &mut output, LogSlice::Last(2)).unwrap();

        assert_eq!(output, b"line2\nline3");
    }

    #[test]
    fn raw_mode_preserves_escape_sequences() {
        let input = b"\x1b[31mred\x1b[0m\r\n".to_vec();
        let mut reader = Cursor::new(input.clone());
        let mut output = Vec::new();

        write_raw_log(&mut reader, &mut output, LogSlice::Full).unwrap();

        assert_eq!(output, input);
    }

    #[test]
    fn clean_mode_strips_ansi_and_collapses_repaints() {
        let input = b"\x1b[32mtrain 0%\rtrain 100%\x1b[0m\n".to_vec();
        let mut reader = Cursor::new(input);
        let mut output = Vec::new();

        write_cleaned_log(&mut reader, &mut output, LogSlice::Full, LogFormat::Clean).unwrap();

        assert_eq!(String::from_utf8(output).unwrap(), "train 100%\n");
    }

    #[test]
    fn no_ansi_mode_keeps_every_progress_frame() {
        let input = b"train 0%\rtrain 100%\n".to_vec();
        let mut reader = Cursor::new(input);
        let mut output = Vec::new();

        write_cleaned_log(&mut reader, &mut output, LogSlice::Full, LogFormat::NoAnsi).unwrap();

        assert_eq!(String::from_utf8(output).unwrap(), "train 0%\ntrain 100%\n");
    }

    #[test]
    fn clean_slicing_counts_rendered_lines_not_repaints() {
        // One physical line holding three repaints renders as a single line.
        let input = b"a\ra\ra\nb\nc\n".to_vec();
        let mut reader = Cursor::new(input);
        let mut output = Vec::new();

        write_cleaned_log(
            &mut reader,
            &mut output,
            LogSlice::Last(1),
            LogFormat::Clean,
        )
        .unwrap();

        assert_eq!(String::from_utf8(output).unwrap(), "c\n");
    }

    #[test]
    fn clean_first_stops_reading_after_the_requested_lines() {
        let input = b"one\ntwo\nthree\n".to_vec();
        let mut reader = Cursor::new(input);
        let mut output = Vec::new();

        write_cleaned_log(
            &mut reader,
            &mut output,
            LogSlice::First(2),
            LogFormat::Clean,
        )
        .unwrap();

        assert_eq!(String::from_utf8(output).unwrap(), "one\ntwo\n");
    }

    #[test]
    fn drain_utf8_holds_back_incomplete_sequences() {
        // "é" is 0xC3 0xA9; split it across two reads.
        let mut buffer = vec![b'o', b'k', 0xC3];
        let first = drain_utf8(&mut buffer);
        assert_eq!(first, "ok");
        assert_eq!(buffer, vec![0xC3]);

        buffer.push(0xA9);
        let second = drain_utf8(&mut buffer);
        assert_eq!(second, "é");
        assert!(buffer.is_empty());
    }
}
