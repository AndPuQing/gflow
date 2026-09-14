use gflow::core::job::Job;
use gflow::utils::terminal::clean_terminal_text;

use super::schemas::GetJobLogRequest;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TextSlice {
    Full,
    First(usize),
    Last(usize),
}

pub(super) fn resolve_log_slice(params: &GetJobLogRequest) -> anyhow::Result<TextSlice> {
    match (params.first_lines, params.last_lines) {
        (Some(_), Some(_)) => {
            anyhow::bail!("get_job_log accepts only one of first_lines or last_lines")
        }
        (Some(lines), None) => Ok(TextSlice::First(lines)),
        (None, Some(lines)) => Ok(TextSlice::Last(lines)),
        (None, None) => Ok(TextSlice::Full),
    }
}

pub(super) fn slice_text(text: String, slice: TextSlice, max_bytes: Option<usize>) -> String {
    let mut output = text;

    match slice {
        TextSlice::Full => {}
        TextSlice::First(first_lines) => {
            output = output
                .lines()
                .take(first_lines)
                .collect::<Vec<_>>()
                .join("\n");
        }
        TextSlice::Last(last_lines) => {
            let lines: Vec<_> = output.lines().collect();
            output = lines
                .into_iter()
                .rev()
                .take(last_lines)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n");
        }
    }

    if let Some(max_bytes) = max_bytes {
        let bytes = output.as_bytes();
        if bytes.len() > max_bytes {
            output = String::from_utf8_lossy(&bytes[bytes.len() - max_bytes..]).to_string();
        }
    }

    output
}

pub(super) fn clean_terminal_output(text: &str) -> String {
    // Shares the crate-level terminal state machine with `gjob log` so both
    // surfaces agree on what the capture means. `extract_likely_program_output`
    // below still applies its own job-specific noise filtering on top.
    clean_terminal_text(text).trim().to_string()
}

pub(super) fn extract_likely_program_output(text: &str, job: &Job) -> String {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !is_shell_noise_line(line))
        .filter(|line| !is_internal_gflow_line(line, job.id))
        .filter(|line| !is_wrapped_user_command_line(line, job))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn is_shell_noise_line(line: &str) -> bool {
    line.starts_with("cd ")
        || line.starts_with("export GFLOW_ARRAY_TASK_ID=")
        || line.starts_with("export CUDA_VISIBLE_DEVICES=")
        || line.starts_with("conda activate ")
        || line.starts_with("➜ ")
        || line == "✗"
        || line.starts_with('¶')
        || line.contains("[$?] is")
        || line.contains(" via ")
        || line.contains('…')
}

fn is_internal_gflow_line(line: &str, job_id: u32) -> bool {
    line.contains("target/debug/gflow __multicall gcancel")
        || line.contains("Running `target/debug/gflow __multicall gcancel")
        || line.contains("Finished `dev` profile")
        || line.contains(&format!("gcancel --finish {job_id}"))
        || line.contains(&format!("gcancel --fail {job_id}"))
}

fn is_wrapped_user_command_line(line: &str, job: &Job) -> bool {
    if line.starts_with("bash -c ") {
        return true;
    }

    if let Some(command) = &job.command {
        let normalized_command = command.split_whitespace().collect::<Vec<_>>().join(" ");
        let normalized_line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if normalized_line.contains(&normalized_command)
            || normalized_line.contains(&normalized_command.replace('"', "\\\""))
        {
            return true;
        }
    }

    if let Some(script) = &job.script {
        if line.contains(script.to_string_lossy().as_ref()) {
            return true;
        }
    }

    false
}
