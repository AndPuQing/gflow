use shared::{Job, GPU};

enum JobState {
    Queued,
    Running,
    Finished,
}

pub struct Scheduler {
    jobs: Vec<Job>,
    gpu_count: u32,
    gpu_slots: Vec<u32>,
}

impl Scheduler {
    pub fn new() -> Self {
        let gpu_count = Self::get_gpu_count();
        Self {
            jobs: Vec::new(),
            gpu_count,
            gpu_slots: vec![0; gpu_count as usize],
        }
    }
}

impl GPU for Scheduler {
    fn get_gpu_count() -> u32 {
        match std::process::Command::new("nvidia-smi")
            .args(["--query-gpu=gpu_name", "--format=csv,noheader"])
            .output()
        {
            Ok(output) => String::from_utf8_lossy(&output.stdout).lines().count() as u32,
            Err(_) => 0, // Return 0 if nvidia-smi fails or is not available
        }
    }
}
