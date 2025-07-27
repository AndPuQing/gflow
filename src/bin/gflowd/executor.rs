use anyhow::Result;
use gflow::core::{executor::Executor, job::Job};
use gflow::tmux::TmuxSession;

pub struct TmuxExecutor;

impl Executor for TmuxExecutor {
    fn execute(&self, job: &Job) -> Result<()> {
        if let Some(session_name) = job.run_name.as_ref() {
            let session = TmuxSession::new(session_name.clone());

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

            let log_path = gflow::core::get_config_log_file(job.id)?;
            let wrapped_command = format!(
                "{{ {user_command}; gflow finish {job_id}; }} || gflow fail {job_id} &> {log_path}",
                user_command = user_command,
                job_id = job.id,
                log_path = log_path.to_str().unwrap_or("/dev/null")
            );

            session.send_command(&wrapped_command);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use gflow::core::job::JobBuilder;
    use std::path::PathBuf;

    #[test]
    fn test_command_generation() {
        let job = JobBuilder::new()
            .script(PathBuf::from("test.sh"))
            .conda_env(&Some("myenv".to_string()))
            .build();

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

        let job_id = 123;
        let wrapped_command =
            format!("{{ {user_command}; gflow finish {job_id}; }} || gflow fail {job_id}");

        assert_eq!(
            wrapped_command,
            "{ conda activate myenv; sh test.sh; gflow finish 123; } || gflow fail 123"
        );
    }
}
