use nvml_wrapper::Nvml;
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tracing::debug;

use crate::common::arg::Commands;
use crate::common::jobs::{Gpu, Job, JobEnvironment, JobStatus, Priority};

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
        available_gpus.len() >= resources.environment.gpus.unwrap_or(0)
    }

    fn allocate(&mut self, resources: &Job) -> Vec<usize> {
        let mut allocated_gpus = Vec::new();
        for gpu in &self.available_gpus {
            let mut gpu = gpu.lock().unwrap();
            if !gpu.is_busy {
                gpu.is_busy = true;
                allocated_gpus.push(gpu.id);
            }
        }
        allocated_gpus
    }
}

pub struct Slurm {
    job_queue: Arc<(Mutex<VecDeque<Job>>, Condvar)>,
    resource_manager: Arc<Mutex<ResourceManager>>,
    job_id: Arc<Mutex<usize>>,
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
            job_id: Arc::new(Mutex::new(0)),
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
                Commands::Submit(args) => {
                    let (lock, cvar) = &*self.job_queue;
                    let mut queue = lock.lock().unwrap();
                    if let Some(ref path) = args.file {
                        let path = std::path::Path::new(path);
                        let env = JobEnvironment::parse_slurm_script(path)?;
                        queue.push_back(Job {
                            id: self.job_id.lock().unwrap().clone(),
                            command: std::fs::read_to_string(path)?,
                            status: JobStatus::Pending,
                            user: args.user.unwrap_or_default(),
                            environment: env,
                            priority: Priority::from(args.priority),
                        });
                        cvar.notify_one();
                        *self.job_id.lock().unwrap() += 1;
                        socket.write_all(b"Job submitted!").await?;
                        debug!("Job {} submitted", self.job_id.lock().unwrap());
                    }
                }
                Commands::Status(_status_args) => todo!(),
                Commands::Cancel(_cancel_args) => todo!(),
                Commands::List(list_args) => {
                    let (lock, _) = &*self.job_queue;
                    let jobs: Vec<_> = {
                        let queue = lock.lock().unwrap();
                        queue.iter().cloned().collect()
                    };
                    if list_args.all {
                        let jobs = rmp_serde::to_vec(&jobs)?;
                        socket.write_all(&jobs).await?;
                    } else {
                        let user = list_args.user;
                        let mut jobs = jobs
                            .iter()
                            .filter(|job| job.user == user)
                            .cloned()
                            .collect::<Vec<_>>();
                        let status = list_args.state;
                        if status != "all" {
                            jobs = jobs
                                .iter()
                                .filter(|job| JobStatus::from(status.clone()) == job.status)
                                .cloned()
                                .collect::<Vec<_>>();
                        }
                        let jobs = rmp_serde::to_vec(&jobs)?;
                        socket.write_all(&jobs).await?;
                    }
                }
                Commands::Log(_log_args) => todo!(),
                Commands::Priority(priority_args) => {
                    let (lock, _) = &*self.job_queue;
                    let mut queue = lock.lock().unwrap();
                    if let Some(job) = queue.iter_mut().find(|job| job.id == priority_args.job_id) {
                        job.priority = Priority::from(priority_args.priority);
                    }
                    socket.write_all(b"Job priority updated!").await?;
                }
                Commands::Hold(hold_args) => {
                    let (lock, _) = &*self.job_queue;
                    let mut queue = lock.lock().unwrap();
                    if let Some(job) = queue.iter_mut().find(|job| job.id == hold_args.job_id) {
                        job.status = JobStatus::Hold;
                    }
                    socket.write_all(b"Job paused!").await?;
                }
                Commands::Resume(resume_args) => {
                    let (lock, _) = &*self.job_queue;
                    let mut queue = lock.lock().unwrap();
                    if let Some(job) = queue.iter_mut().find(|job| job.id == resume_args.job_id) {
                        job.status = JobStatus::Pending;
                    }
                    socket.write_all(b"Job resumed!").await?;
                }
                Commands::Info(_info_args) => {}
                _ => {}
            }
        }
    }
}
