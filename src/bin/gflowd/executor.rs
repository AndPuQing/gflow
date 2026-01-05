use anyhow::{anyhow, Result};
use gflow::core::{executor::Executor, job::Job};
use gflow::tmux::TmuxSession;
use regex::Regex;
use std::collections::HashMap;
use std::fs;

/// Substitute {param_name} patterns in command with actual values
fn substitute_parameters(command: &str, parameters: &HashMap<String, String>) -> Result<String> {
    let re = Regex::new(r"\{([a-zA-Z_][a-zA-Z0-9_]*)\}").unwrap();
    let mut result = command.to_string();
    let mut missing_params = Vec::new();

    for cap in re.captures_iter(command) {
        let param_name = &cap[1];
        if let Some(value) = parameters.get(param_name) {
            let pattern = format!("{{{}}}", param_name);
            result = result.replace(&pattern, value);
        } else {
            missing_params.push(param_name.to_string());
        }
    }

    if !missing_params.is_empty() {
        return Err(anyhow!(
            "Missing parameter values: {}",
            missing_params.join(", ")
        ));
    }

    Ok(result)
}

pub struct TmuxExecutor;

impl TmuxExecutor {
    fn generate_wrapped_command(&self, job: &Job) -> Result<String> {
        let mut user_command = String::new();

        if let Some(script) = &job.script {
            if let Some(script_str) = script.to_str() {
                user_command.push_str(&format!("bash {script_str}"));
            }
        } else if let Some(cmd) = &job.command {
            // Apply parameter substitution
            let substituted = substitute_parameters(cmd, &job.parameters)?;
            user_command.push_str(&substituted);
        }

        // Wrap the command in bash -c to ensure && and || operators work
        // regardless of the user's default shell (fish, zsh, etc.)
        // Use double quotes to avoid the ugly '\'' escaping pattern
        // Need to escape: backslash, double-quote, dollar sign, backtick
        let escaped_command = user_command
            .replace('\\', r"\\")
            .replace('"', r#"\""#)
            .replace('$', r"\$")
            .replace('`', r"\`");
        let wrapped_command = format!(
            r#"bash -c "{escaped_command} && gcancel --finish {job_id} || gcancel --fail {job_id}""#,
            job_id = job.id,
        );
        Ok(wrapped_command)
    }
}

impl Executor for TmuxExecutor {
    fn execute(&self, job: &Job) -> Result<()> {
        if let Some(session_name) = job.run_name.as_ref() {
            let session = TmuxSession::new(session_name.clone());

            // Enable pipe-pane to capture output to log file
            let log_path = gflow::core::get_log_file_path(job.id)?;
            if let Some(parent) = log_path.parent() {
                fs::create_dir_all(parent)?;
            }
            session.enable_pipe_pane(&log_path)?;

            session.send_command(&format!("cd {}", job.run_dir.display()));
            session.send_command(&format!(
                "export GFLOW_ARRAY_TASK_ID={}",
                job.task_id.unwrap_or(0)
            ));
            if let Some(gpu_ids) = &job.gpu_ids {
                session.send_command(&format!(
                    "export CUDA_VISIBLE_DEVICES={}",
                    gpu_ids
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(",")
                ));
            }

            if let Some(conda_env) = &job.conda_env {
                session.send_command(&format!("conda activate {conda_env}"));
            }

            let wrapped_command = self.generate_wrapped_command(job)?;
            session.send_command(&wrapped_command);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gflow::core::job::JobState;
    use std::path::PathBuf;

    #[test]
    fn test_generate_wrapped_command_basic() {
        let executor = TmuxExecutor;
        let job = Job {
            id: 123,
            command: Some("echo hello".to_string()),
            state: JobState::Queued,
            run_dir: PathBuf::from("/tmp"),
            ..Default::default()
        };

        let wrapped = executor.generate_wrapped_command(&job).unwrap();
        assert_eq!(
            wrapped,
            r#"bash -c "echo hello && gcancel --finish 123 || gcancel --fail 123""#
        );
    }

    #[test]
    fn test_generate_wrapped_command_with_quotes() {
        let executor = TmuxExecutor;
        let job = Job {
            id: 456,
            command: Some("echo 'hello world'".to_string()),
            state: JobState::Queued,
            run_dir: PathBuf::from("/tmp"),
            ..Default::default()
        };

        let wrapped = executor.generate_wrapped_command(&job).unwrap();
        // Single quotes don't need escaping in double-quoted context
        assert_eq!(
            wrapped,
            r#"bash -c "echo 'hello world' && gcancel --finish 456 || gcancel --fail 456""#
        );
    }

    #[test]
    fn test_generate_wrapped_command_with_script() {
        let executor = TmuxExecutor;
        let job = Job {
            id: 789,
            script: Some(PathBuf::from("/tmp/script.sh")),
            state: JobState::Queued,
            run_dir: PathBuf::from("/tmp"),
            ..Default::default()
        };

        let wrapped = executor.generate_wrapped_command(&job).unwrap();
        assert_eq!(
            wrapped,
            r#"bash -c "bash /tmp/script.sh && gcancel --finish 789 || gcancel --fail 789""#
        );
    }

    #[test]
    fn test_generate_wrapped_command_with_special_chars() {
        let executor = TmuxExecutor;
        let job = Job {
            id: 527,
            command: Some("lighteval vllm 'model_name=meta-llama/Llama-3.2-1B-Instruct,dtype=bfloat16' 'lighteval|gsm8k|5'".to_string()),
            state: JobState::Queued,
            run_dir: PathBuf::from("/tmp"),
            ..Default::default()
        };

        let wrapped = executor.generate_wrapped_command(&job).unwrap();
        // Single quotes are preserved in double-quoted context
        assert_eq!(
            wrapped,
            r#"bash -c "lighteval vllm 'model_name=meta-llama/Llama-3.2-1B-Instruct,dtype=bfloat16' 'lighteval|gsm8k|5' && gcancel --finish 527 || gcancel --fail 527""#
        );
    }

    #[test]
    fn test_generate_wrapped_command_with_double_quotes() {
        let executor = TmuxExecutor;
        let job = Job {
            id: 100,
            command: Some(r#"echo "hello world""#.to_string()),
            state: JobState::Queued,
            run_dir: PathBuf::from("/tmp"),
            ..Default::default()
        };

        let wrapped = executor.generate_wrapped_command(&job).unwrap();
        // Double quotes should be escaped
        assert_eq!(
            wrapped,
            r#"bash -c "echo \"hello world\" && gcancel --finish 100 || gcancel --fail 100""#
        );
    }

    #[test]
    fn test_generate_wrapped_command_with_dollar_sign() {
        let executor = TmuxExecutor;
        let job = Job {
            id: 200,
            command: Some("echo $HOME".to_string()),
            state: JobState::Queued,
            run_dir: PathBuf::from("/tmp"),
            ..Default::default()
        };

        let wrapped = executor.generate_wrapped_command(&job).unwrap();
        // Dollar signs should be escaped to prevent variable expansion
        assert_eq!(
            wrapped,
            r#"bash -c "echo \$HOME && gcancel --finish 200 || gcancel --fail 200""#
        );
    }
}
