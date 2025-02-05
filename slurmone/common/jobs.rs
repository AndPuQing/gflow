use std::{
    fs::{read_to_string, File},
    io::Seek,
    path::Path,
    process::{Command, Stdio},
};
extern crate tmux_interface;
use serde::{Deserialize, Serialize};
use tmux_interface::{
    HasSession, KillSession, New, NewSession, NewWindow, PipePane, RunShell, Send, SendKeys,
    SplitWindow, Tmux,
};
use tracing::info;
use tracing_subscriber::fmt::format;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub enum Priority {
    Low,
    Medium,
    High,
}

impl From<String> for Priority {
    fn from(priority: String) -> Self {
        match priority.to_lowercase().as_str() {
            "low" => Priority::Low,
            "normal" => Priority::Medium,
            "high" => Priority::High,
            _ => Priority::Medium,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Job {
    pub id: usize,
    pub user: String,
    pub command: Option<String>,
    pub shell_script: Option<String>,
    pub priority: Priority,
    pub environment: JobEnvironment,
    pub status: JobStatus,
}

impl Job {
    pub fn to_absolute_path(&self, path: &str, work_dir: &str) -> String {
        let input_path = Path::new(path);

        let absolute_path = if input_path.is_relative() {
            Path::new(work_dir).join(input_path)
        } else {
            input_path.to_path_buf()
        };

        absolute_path
            .to_str()
            .unwrap_or_else(|| panic!("Failed to convert path {:?} to string", absolute_path))
            .to_string()
    }

    pub fn is_task_running(self) -> bool {
        let output = Command::new("tmux")
            .arg("list-sessions")
            .output()
            .expect("Failed to list tmux sessions");
        let sessions = String::from_utf8_lossy(&output.stdout);
        sessions.contains(
            &self
                .environment
                .job_name
                .clone()
                .unwrap_or(format!("job_{}", self.id)),
        )
    }

    pub fn execute(&mut self, gpu_ids: Vec<usize>) {
        self.status = JobStatus::Running;

        let gpu_env = gpu_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let work_dir = self.environment.work_dir.clone().unwrap_or_else(|| {
            std::env::current_dir()
                .expect("Failed to get current directory")
                .to_str()
                .expect("Failed to convert path to string")
                .to_string()
        });

        let work_dir = if work_dir == "." {
            std::env::current_dir()
                .expect("Failed to get current directory")
                .to_str()
                .expect("Failed to convert path to string")
                .to_string()
        } else {
            work_dir
        };

        let output = self
            .environment
            .output
            .clone()
            .unwrap_or("slurmone.stdout.log".to_string());
        let conda_env = self
            .environment
            .conda_env
            .clone()
            .unwrap_or("base".to_string());

        let mut command = format!("CUDA_VISIBLE_DEVICES={} ", gpu_env);
        if let Some(command_str) = &self.command {
            command.push_str(&command_str);
        } else if let Some(shell_script) = &self.shell_script {
            command.push_str(&format!("sh {}", shell_script));
        }

        Tmux::new()
            .add_command(
                NewSession::new()
                    .detached()
                    .session_name(
                        &self
                            .environment
                            .job_name
                            .clone()
                            .unwrap_or(format!("job_{}", self.id)),
                    )
                    .group_name("slurmone"),
            )
            .add_command(PipePane::new().open().shell_command(format!(
                "cat >> {}",
                self.to_absolute_path(&output, &work_dir)
            )))
            .add_command(SendKeys::new().key(format!("conda activate {}\n", conda_env)))
            .add_command(SendKeys::new().key("Enter"))
            .add_command(SendKeys::new().key(format!("cd {}\n", work_dir)))
            .add_command(SendKeys::new().key("Enter"))
            .add_command(SendKeys::new().key(command))
            .add_command(SendKeys::new().key("Enter"))
            .output()
            .unwrap();
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct JobEnvironment {
    pub job_name: Option<String>,
    pub gpus: Option<usize>,
    pub conda_env: Option<String>,
    pub work_dir: Option<String>,
    pub output: Option<String>,
    pub time_limit: Option<String>,
}

impl JobEnvironment {
    pub fn parse_slurm_script(
        content: String,
    ) -> Result<JobEnvironment, Box<dyn std::error::Error>> {
        let mut config = JobEnvironment::default();
        for line in content.lines() {
            if line.starts_with("#SBATCH") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    // #SBATCH --job-name=slurmone
                    let key_value: Vec<&str> = parts[1].split('=').collect();
                    let key = key_value[0].trim_start_matches("--");
                    let value = key_value[1].to_string();
                    match key {
                        "job-name" => config.job_name = Some(value),
                        "gres" => {
                            let gres: Vec<&str> = value.split(':').collect();
                            if gres.len() == 2 {
                                config.gpus = Some(gres[1].parse()?);
                            }
                        }
                        "time" => config.time_limit = Some(value),
                        "output" => config.output = Some(value),
                        "workdir" => config.work_dir = Some(value),
                        "conda-env" => config.conda_env = Some(value),
                        _ => {}
                    }
                }
            }
        }

        Ok(config)
    }
}

#[derive(Debug)]
pub struct Gpu {
    pub id: usize,
    pub is_busy: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Canceled,
    Hold,
}

impl Default for JobStatus {
    fn default() -> Self {
        JobStatus::Pending
    }
}

impl From<String> for JobStatus {
    fn from(status: String) -> Self {
        match status.to_lowercase().as_str() {
            "pending" => JobStatus::Pending,
            "running" => JobStatus::Running,
            "completed" => JobStatus::Completed,
            "failed" => JobStatus::Failed,
            "canceled" => JobStatus::Canceled,
            _ => JobStatus::Pending,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_priority_from_string() {
        assert_eq!(Priority::from("low".to_string()), Priority::Low);
        assert_eq!(Priority::from("normal".to_string()), Priority::Medium);
        assert_eq!(Priority::from("high".to_string()), Priority::High);
        assert_eq!(Priority::from("unknown".to_string()), Priority::Medium);
    }

    #[test]
    fn test_job_environment_parse_slurm_script() {
        let content = r#"#!/bin/bash
#SBATCH --job-name=slurmone
#SBATCH --gres=gpu:3
#SBATCH --time=1:00:00
#SBATCH --output=slurmone.stdout.log
#SBATCH --workdir=.
#SBATCH --conda-env=base
echo "Hello, SlurmOne!"
"#;
        let env = JobEnvironment::parse_slurm_script(content.to_string()).unwrap();
        assert_eq!(env.job_name, Some("slurmone".to_string()));
        assert_eq!(env.gpus, Some(3));
        assert_eq!(env.time_limit, Some("1:00:00".to_string()));
        assert_eq!(env.output, Some("slurmone.stdout.log".to_string()));
        assert_eq!(env.work_dir, Some(".".to_string()));
    }

    #[test]
    fn test_job_run() {
        let env = JobEnvironment {
            job_name: Some("slurmone_test".to_string()),
            gpus: Some(1),
            conda_env: Some("base".to_string()),
            work_dir: Some(".".to_string()),
            output: Some("./slurmone.stdout.log".to_string()),
            time_limit: Some("1:00:00".to_string()),
        };
        let mut job = Job {
            id: 1,
            command: Some("sleep 10".to_string()),
            user: "slurmone".to_string(),
            priority: Priority::Medium,
            environment: env,
            status: JobStatus::Pending,
            shell_script: None,
        };
        job.execute(vec![0]);
        assert!(job.is_task_running());
    }
}
