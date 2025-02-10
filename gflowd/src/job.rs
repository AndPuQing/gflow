use shared::{random_run_name, Job, JobState};
use tmux_interface::{NewSession, SendKeys, Tmux};

pub fn execute_job(job: &mut Job, gpu_slots: &[u32]) {
    let worker_name = random_run_name();
    job.run_name = Some(worker_name.clone());

    Tmux::new()
        .add_command(NewSession::new().detached().session_name(&worker_name))
        .output()
        .unwrap();
    // sleep for 1 second to allow tmux to create the session
    std::thread::sleep(std::time::Duration::from_secs(1));

    if !gpu_slots.is_empty() {
        let cuda_visible_devices = gpu_slots
            .iter()
            .map(|gpu_id| gpu_id.to_string())
            .collect::<Vec<String>>()
            .join(",");

        Tmux::new()
            .add_command(SendKeys::new().target_client(&worker_name).key(format!(
                "export CUDA_VISIBLE_DEVICES={}",
                cuda_visible_devices
            )))
            .add_command(SendKeys::new().target_client(&worker_name).key("Enter"))
            .output()
            .unwrap();
    }
    Tmux::new()
        .add_command(
            SendKeys::new()
                .target_client(&worker_name)
                .key(format!("sh {}", job.script.display())),
        )
        .add_command(SendKeys::new().target_client(&worker_name).key("Enter"))
        .output()
        .unwrap();
    job.state = JobState::Running;
}
