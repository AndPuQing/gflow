use anyhow::Result;
use gflow::tmux::TmuxSession;
use gflow_core::{executor::Executor, job::Job};

pub struct TmuxExecutor;

impl Executor for TmuxExecutor {
    fn execute(&self, job: &Job) -> Result<()> {
        if let Some(session_name) = job.run_name.as_ref() {
            let session = TmuxSession::new(session_name.clone());

            let mut command = String::new();
            if let Some(conda_env) = &job.conda_env {
                command.push_str(&format!("conda activate {}; ", conda_env));
            }
            if let Some(script) = &job.script {
                if let Some(script_str) = script.to_str() {
                    command.push_str(&format!("sh {}", script_str));
                }
            } else if let Some(cmd) = &job.command {
                command.push_str(cmd);
            }

            session.send_command(&command);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use gflow_core::job::JobBuilder;
    use std::path::PathBuf;

    #[test]
    fn test_command_generation() {
        let job = JobBuilder::new()
            .script(PathBuf::from("test.sh"))
            .conda_env(&Some("myenv".to_string()))
            .build();

        let mut command = String::new();
        if let Some(conda_env) = &job.conda_env {
            command.push_str(&format!("conda activate {conda_env}; "));
        }
        if let Some(script) = &job.script {
            if let Some(script_str) = script.to_str() {
                command.push_str(&format!("sh {script_str}"));
            }
        } else if let Some(cmd) = &job.command {
            command.push_str(cmd);
        }

        assert_eq!(command, "conda activate myenv; sh test.sh");
    }
}
