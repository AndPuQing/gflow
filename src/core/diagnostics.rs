//! Failure diagnostics derived from a job's log file.
//!
//! A job that dies from out-of-memory often exits with a generic non-zero code,
//! so `gqueue` would only show `Failed (F)` with no hint. These helpers inspect
//! the tail of the job log and classify the failure so the scheduler can record
//! a self-describing reason (`OutOfMemory`) that surfaces in `gqueue` /
//! `gjob show`.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// How many bytes to read from the end of a job log when diagnosing a failure.
/// Large enough to hold a Python traceback while staying cheap on every failure.
pub const LOG_TAIL_BYTES: u64 = 64 * 1024;

/// Signatures that identify an out-of-memory failure. Matched case-insensitively
/// against the log tail.
///
/// Covers CUDA / ROCm / Apple MPS device OOM as well as host allocation
/// failures. Kept deliberately specific to avoid flagging a payload that merely
/// mentions "out of memory" in passing, e.g. while handling an error itself.
const OOM_SIGNATURES: &[&str] = &[
    "cuda out of memory",
    "cuda_error_out_of_memory",
    "cuda error: out of memory",
    "torch.outofmemoryerror",
    "outofmemoryerror",
    "out_of_memory",
    "hip out of memory",
    "mps backend out of memory",
    "cublas_status_alloc_failed",
    "cudnn_status_alloc_failed",
    "std::bad_alloc",
    "memoryerror: unable to allocate",
];

/// Returns true when `text` contains a known out-of-memory signature.
pub fn contains_oom_signature(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    OOM_SIGNATURES.iter().any(|sig| lower.contains(sig))
}

/// Read at most the last [`LOG_TAIL_BYTES`] of `path` as lossy UTF-8.
///
/// Returns `None` when the file cannot be opened or read, so callers can treat
/// "no log yet" as "no diagnosis available" rather than an error.
pub fn read_log_tail(path: &Path) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let size = file.metadata().ok()?.len();
    let start = size.saturating_sub(LOG_TAIL_BYTES);
    if start > 0 {
        file.seek(SeekFrom::Start(start)).ok()?;
    }

    let mut buffer = Vec::new();
    file.take(LOG_TAIL_BYTES).read_to_end(&mut buffer).ok()?;
    Some(String::from_utf8_lossy(&buffer).into_owned())
}

/// Returns true when the last [`LOG_TAIL_BYTES`] of the log at `path` show an
/// out-of-memory failure. Missing or unreadable logs yield `false`.
pub fn log_tail_indicates_oom(path: &Path) -> bool {
    read_log_tail(path)
        .as_deref()
        .is_some_and(contains_oom_signature)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_log(dir: &Path, content: &str) -> std::path::PathBuf {
        let path = dir.join("1.log");
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        path
    }

    #[test]
    fn detects_pytorch_cuda_oom_traceback() {
        let text = "\
step=1000 stage2: thawed 104 block tensors (1.745B parameters)
torch.OutOfMemoryError: CUDA out of memory.
  GPU 0 has a total capacity of 94.97 GiB of which 10.81 MiB is free.
";
        assert!(contains_oom_signature(text));
    }

    #[test]
    fn detects_apple_and_host_oom_variants() {
        assert!(contains_oom_signature(
            "RuntimeError: MPS backend out of memory (MPS allocated: 18.00 GB)"
        ));
        assert!(contains_oom_signature(
            "terminate called after throwing: std::bad_alloc"
        ));
    }

    #[test]
    fn ignores_unrelated_failures() {
        assert!(!contains_oom_signature(
            "ValueError: invalid batch size\nexit status 1"
        ));
    }

    #[test]
    fn missing_log_is_not_an_oom() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!log_tail_indicates_oom(&dir.path().join("absent.log")));
    }

    #[test]
    fn reads_only_the_tail_of_a_large_log() {
        let dir = tempfile::tempdir().unwrap();
        // An OOM near the start must not be seen once it falls outside the tail.
        let mut content = String::from("CUDA out of memory\n");
        content.push_str(&"filler line\n".repeat(20_000));
        let path = write_log(dir.path(), &content);

        let tail = read_log_tail(&path).unwrap();
        assert!(tail.len() as u64 <= LOG_TAIL_BYTES);
        assert!(tail.ends_with("filler line\n"));
        assert!(!contains_oom_signature(&tail));
    }

    #[test]
    fn finds_oom_at_the_end_of_a_large_log() {
        let dir = tempfile::tempdir().unwrap();
        let mut content = "filler line\n".repeat(20_000);
        content.push_str("torch.OutOfMemoryError: CUDA out of memory.\n");
        let path = write_log(dir.path(), &content);

        assert!(log_tail_indicates_oom(&path));
    }
}
