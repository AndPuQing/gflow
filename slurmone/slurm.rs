use nvml_wrapper::Nvml;
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tracing::debug;

use crate::common::arg::Commands;
use crate::common::jobs::{Gpu, Job, JobStatus, ResourceRequirements};

#[derive(Clone)]
pub struct ResourceManager {
    available_gpus: Vec<Arc<Mutex<Gpu>>>,
}

impl ResourceManager {
    fn new() -> Self {
        let nvml = Nvml::builder()
            .lib_path("libnvidia-ml.so.1".as_ref())
            .init()
            .unwrap();
        let gpus = nvml.device_count().unwrap();

        let gpus = (0..gpus)
            .map(|i| {
                let device = nvml.device_by_index(i).unwrap();
                let is_busy = device.utilization_rates().unwrap().gpu > 90;

                Arc::new(Mutex::new(Gpu {
                    id: i as usize,
                    is_busy,
                }))
            })
            .collect::<Vec<_>>();

        Self {
            available_gpus: gpus,
        }
    }

    fn refresh_gpus(&mut self) {
        let nvml = Nvml::init().unwrap();
        for gpu in &self.available_gpus {
            let mut gpu = gpu.lock().unwrap();
            let device = nvml.device_by_index(gpu.id as u32).unwrap();
            gpu.is_busy = device.utilization_rates().unwrap().gpu > 90;
        }
    }

    fn can_allocate(&mut self, resources: &Job) -> bool {
        let mut available_gpus = Vec::new();
        self.refresh_gpus();
        for gpu in &self.available_gpus {
            let gpu = gpu.lock().unwrap();
            if !gpu.is_busy {
                available_gpus.push(gpu);
            }
        }
        if resources.resources_required.gpus > available_gpus.len() {
            return false;
        }
        true
    }

    fn allocate(&mut self, resources: &Job) -> Vec<usize> {
        let mut allocated_gpus = Vec::new();
        for gpu in &self.available_gpus {
            let mut gpu = gpu.lock().unwrap();
            if !gpu.is_busy {
                gpu.is_busy = true;
                allocated_gpus.push(gpu.id);
            }
            if allocated_gpus.len() == resources.resources_required.gpus {
                break;
            }
        }
        allocated_gpus
    }
}

pub struct Slurm {
    job_queue: Arc<(Mutex<VecDeque<Job>>, Condvar)>,
    resource_manager: Arc<Mutex<ResourceManager>>,
}

impl Default for Slurm {
    fn default() -> Self {
        Self::new()
    }
}

impl Slurm {
    pub fn new() -> Self {
        Self {
            job_queue: Arc::new((Mutex::new(VecDeque::new()), Condvar::new())),
            resource_manager: Arc::new(Mutex::new(ResourceManager::new())),
        }
    }

    pub fn start(&self) {
        let job_queue = Arc::clone(&self.job_queue);
        let resource_manager = Arc::clone(&self.resource_manager);

        thread::spawn(move || loop {
            let (lock, cvar) = &*job_queue;
            let mut queue = lock.lock().unwrap();

            while queue.is_empty() {
                queue = cvar.wait(queue).unwrap();
            }

            if let Ok(mut resource_manager) = resource_manager.lock() {
                let job = queue.pop_front().unwrap();
                if resource_manager.can_allocate(&job) {
                    let gpu_ids = resource_manager.allocate(&job);
                    let mut job = job;
                    job.execute(gpu_ids);
                }
            }
        });
    }

    pub async fn listen_unix_socket(
        &self,
        sock_path: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let _ = std::fs::remove_file(sock_path);
        let listener = UnixListener::bind(sock_path)?;

        loop {
            let (mut socket, _) = listener.accept().await?;
            let mut buf = vec![0; 1024];
            let n = socket.read(&mut buf).await?;

            let command: Commands = rmp_serde::from_slice(&buf[..n])?;
            debug!("Received command: {:?}", command);
            match command {
                Commands::Submit(_args) => {
                    // 将作业提交到队列
                    let (lock, cvar) = &*self.job_queue;
                    let mut queue = lock.lock().unwrap();
                    let random_id = rand::random::<usize>();
                    let resources_required = ResourceRequirements { gpus: 1 };
                    queue.push_back(Job {
                        id: random_id,
                        command: todo!(),
                        resources_required: resources_required,
                        status: JobStatus::Pending,
                    });
                    cvar.notify_one();
                    socket.write_all(b"Job submitted!").await?;
                }
                Commands::Status(_status_args) => todo!(),
                Commands::Cancel(_cancel_args) => todo!(),
                Commands::List(_list_args) => {
                    let (lock, _) = &*self.job_queue;
                    let jobs: Vec<_> = {
                        let queue = lock.lock().unwrap();
                        queue.iter().map(|job| job.id).collect()
                    };
                    let jobs = rmp_serde::to_vec(&jobs)?;
                    socket.write_all(&jobs).await?;
                }
                Commands::Log(_log_args) => todo!(),
                Commands::Priority(_priority_args) => todo!(),
                Commands::Hold(_hold_args) => todo!(),
                Commands::Resume(_resume_args) => todo!(),
                Commands::Info(_info_args) => todo!(),
                _ => {}
            }
        }
    }
}
