use anyhow::{Context, Result};
use std::collections::VecDeque;
use std::io::{self, BufRead, BufReader, Write};
use std::num::NonZeroUsize;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LogSlice {
    Full,
    First(usize),
    Last(usize),
}

pub async fn handle_log(
    config_path: &Option<PathBuf>,
    job_id_str: &str,
    first_lines: Option<NonZeroUsize>,
    last_lines: Option<NonZeroUsize>,
) -> Result<()> {
    let client = gflow::create_client(config_path)?;

    // Resolve job ID (handle @ shorthand)
    let job_id = crate::multicall::gjob::utils::resolve_job_id(&client, job_id_str).await?;

    let log_path = match client.get_job_log_path(job_id).await? {
        Some(path) => PathBuf::from(path),
        None => {
            eprintln!("Log for job {} is not available.", job_id);
            return Ok(());
        }
    };

    let mut file = std::fs::File::open(&log_path).with_context(|| {
        format!(
            "Failed to open log file '{}' for job {}",
            log_path.display(),
            job_id
        )
    })?;

    let slice = match (first_lines, last_lines) {
        (Some(lines), None) => LogSlice::First(lines.get()),
        (None, Some(lines)) => LogSlice::Last(lines.get()),
        (None, None) => LogSlice::Full,
        (Some(_), Some(_)) => unreachable!("clap enforces --first/--last mutual exclusion"),
    };

    let mut stdout = io::stdout();
    write_selected_log(&mut file, &mut stdout, slice)
        .context("Failed to write log contents to stdout")?;
    stdout.flush().context("Failed to flush stdout")?;

    Ok(())
}

fn write_selected_log<R: io::Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    slice: LogSlice,
) -> io::Result<()> {
    match slice {
        LogSlice::Full => {
            io::copy(reader, writer)?;
        }
        LogSlice::First(lines) => {
            let mut reader = BufReader::new(reader);
            let mut buffer = Vec::new();

            for _ in 0..lines {
                buffer.clear();
                if reader.read_until(b'\n', &mut buffer)? == 0 {
                    break;
                }
                writer.write_all(&buffer)?;
            }
        }
        LogSlice::Last(lines) => {
            let mut reader = BufReader::new(reader);
            let mut buffer = Vec::new();
            let mut tail = VecDeque::with_capacity(lines);

            loop {
                buffer.clear();
                if reader.read_until(b'\n', &mut buffer)? == 0 {
                    break;
                }

                if tail.len() == lines {
                    tail.pop_front();
                }
                tail.push_back(buffer.clone());
            }

            for line in tail {
                writer.write_all(&line)?;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{write_selected_log, LogSlice};
    use std::io::Cursor;

    #[test]
    fn writes_first_n_lines() {
        let input = b"line1\nline2\nline3\n".to_vec();
        let mut reader = Cursor::new(input);
        let mut output = Vec::new();

        write_selected_log(&mut reader, &mut output, LogSlice::First(2)).unwrap();

        assert_eq!(output, b"line1\nline2\n");
    }

    #[test]
    fn writes_last_n_lines() {
        let input = b"line1\nline2\nline3\nline4\n".to_vec();
        let mut reader = Cursor::new(input);
        let mut output = Vec::new();

        write_selected_log(&mut reader, &mut output, LogSlice::Last(2)).unwrap();

        assert_eq!(output, b"line3\nline4\n");
    }

    #[test]
    fn preserves_partial_last_line_when_tailing() {
        let input = b"line1\nline2\nline3".to_vec();
        let mut reader = Cursor::new(input);
        let mut output = Vec::new();

        write_selected_log(&mut reader, &mut output, LogSlice::Last(2)).unwrap();

        assert_eq!(output, b"line2\nline3");
    }
}
