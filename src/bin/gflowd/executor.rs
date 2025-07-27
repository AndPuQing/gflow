use anyhow::Result;
use gflow::core::{executor::Executor, job::Job};
use gflow::tmux::TmuxSession;

pub struct TmuxExecutor;

impl TmuxExecutor {
    fn generate_wrapped_command(&self, job: &Job) -> Result<String> {
        let mut user_command = String::new();

        if let Some(script) = &job.script {
            if let Some(script_str) = script.to_str() {
                user_command.push_str(&format!("bash {script_str}"));
            }
        } else if let Some(cmd) = &job.command {
            user_command.push_str(cmd);
        }

        let _log_path = gflow::core::get_log_file_path(job.id)?;
        let wrapped_command = format!(
            "{user_command} && gsignal finish {job_id} || gsignal fail {job_id}",
            job_id = job.id,
        );
        Ok(wrapped_command)
    }
}

impl Executor for TmuxExecutor {
    fn execute(&self, job: &Job) -> Result<()> {
        if let Some(session_name) = job.run_name.as_ref() {
            let session = TmuxSession::new(session_name.clone());

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
