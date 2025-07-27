use anyhow::Result;
use gflow::core::{executor::Executor, job::Job};
use gflow::tmux::TmuxSession;

pub struct TmuxExecutor;

impl TmuxExecutor {
    fn generate_wrapped_command(&self, job: &Job) -> Result<String> {
        let mut user_command = String::new();
        if let Some(conda_env) = &job.conda_env {
            user_command.push_str(&format!("conda activate {conda_env}; "));
        }
        if let Some(script) = &job.script {
            if let Some(script_str) = script.to_str() {
                user_command.push_str(&format!("sh {script_str}"));
            }
        } else if let Some(cmd) = &job.command {
            user_command.push_str(cmd);
        }

        let log_path = gflow::core::get_log_file_path(job.id)?;
        let wrapped_command = format!(
            "{{ {user_command}; gflow finish {job_id}; }} || gflow fail {job_id} &> {log_path}",
            user_command = user_command,
            job_id = job.id,
            log_path = log_path.to_str().unwrap_or("/dev/null")
        );
        Ok(wrapped_command)
    }
}

impl Executor for TmuxExecutor {
    fn execute(&self, job: &Job) -> Result<()> {
        if let Some(session_name) = job.run_name.as_ref() {
            let session = TmuxSession::new(session_name.clone());
            let wrapped_command = self.generate_wrapped_command(job)?;
            session.send_command(&wrapped_command);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gflow::core::job::JobBuilder;
    use std::path::PathBuf;

    #[test]
    fn test_command_generation() {
        let mut job = JobBuilder::new()
            .script(PathBuf::from("test.sh"))
            .conda_env(&Some("myenv".to_string()))
            .build();
        job.id = 123;

        let executor = TmuxExecutor;
        let wrapped_command = executor.generate_wrapped_command(&job).unwrap();

        let log_path = gflow::core::get_log_file_path(123).unwrap();
        let expected_command = format!(
            "{{ conda activate myenv; sh test.sh; gflow finish 123; }} || gflow fail 123 &> {}",
            log_path.to_str().unwrap()
        );

        assert_eq!(wrapped_command, expected_command);
    }
}
