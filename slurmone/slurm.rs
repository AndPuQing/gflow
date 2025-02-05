use nvml_wrapper::Nvml;
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tracing::{debug, info};

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

    // fn refresh_gpus(&mut self) {
    //     let nvml = Nvml::builder()
    //         .lib_path("libnvidia-ml.so.1".as_ref())
    //         .init()
    //         .unwrap();
    //     for gpu in &self.available_gpus {
    //         let mut gpu = gpu.lock().unwrap();
    //         let device = nvml.device_by_index(gpu.id as u32).unwrap();
    //     }
    // }

    fn can_allocate(&self, resources: &Job) -> bool {
        let required_gpus = resources.environment.gpus.unwrap_or(0);
        let available_count = self
            .available_gpus
            .iter()
            .filter(|gpu| !gpu.lock().unwrap().is_busy)
            .count();
        available_count >= required_gpus
    }

    fn allocate(&mut self, resources: &Job) -> Vec<usize> {
        let required_gpus = resources.environment.gpus.unwrap_or(0);
        let mut allocated_gpus = Vec::with_capacity(required_gpus);

        for gpu in &self.available_gpus {
            let mut gpu = gpu.lock().unwrap();
            if !gpu.is_busy && allocated_gpus.len() < required_gpus {
                gpu.is_busy = true;
                allocated_gpus.push(gpu.id);
            }
            if allocated_gpus.len() == required_gpus {
                break;
            }
        }

        allocated_gpus
    }

    fn deallocate(&mut self, gpu_ids: Vec<usize>) {
        for gpu_id in gpu_ids {
            if let Some(gpu) = self.available_gpus.get(gpu_id) {
                let mut gpu = gpu.lock().unwrap();
                gpu.is_busy = false;
            }
        }
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

        tokio::spawn(async move {
            loop {
                let (lock, cvar) = &*job_queue;
                let mut queue = lock.lock().unwrap();
                while queue.is_empty() {
                    queue = cvar.wait(queue).unwrap();
                }

                let mut job = queue.pop_front().unwrap();
                let resource_manager = Arc::clone(&resource_manager);
                let mut manager = resource_manager.lock().unwrap();
                if manager.can_allocate(&job) {
                    let gpu_ids = manager.allocate(&job).clone();
                    info!("Allocated GPUs: {:?}", gpu_ids);
                    let (tx, rx): (oneshot::Sender<Vec<usize>>, oneshot::Receiver<Vec<usize>>) =
                        oneshot::channel();
                    tokio::spawn({
                        async move {
                            job.execute(gpu_ids.clone());
                            tx.send(gpu_ids).unwrap();
                        }
                    });
                    tokio::spawn({
                        let resource_manager = Arc::clone(&resource_manager);
                        async move {
                            let gpu_ids = rx.await.unwrap(); // Block until the GPU release is notified
                            let mut manager = resource_manager.lock().unwrap();
                            manager.deallocate(gpu_ids.clone());
                            info!("Deallocated GPUs: {:?}", gpu_ids);
                        }
                    });
                } else {
                    queue.push_back(job);
                }
            }
        });
    }

    pub async fn listen_tcp(&self, _host: &str) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(_host).await?;

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
                        let content = std::fs::read_to_string(path)?;
                        let mut env = JobEnvironment::parse_slurm_script(content)?;
                        env.work_dir = args.work_dir.clone();
                        queue.push_back(Job {
                            id: self.job_id.lock().unwrap().clone(),
                            command: None,
                            status: JobStatus::Pending,
                            user: args.user.unwrap_or_default(),
                            environment: env,
                            priority: Priority::from(args.priority),
                            shell_script: args.file,
                        });
                        cvar.notify_one();
                        *self.job_id.lock().unwrap() += 1;
                        socket.write_all(b"Job submitted!").await?;
                    } else {
                        queue.push_back(Job {
                            id: self.job_id.lock().unwrap().clone(),
                            command: args.command,
                            status: JobStatus::Pending,
                            user: args.user.unwrap_or_default(),
                            environment: JobEnvironment::default(),
                            priority: Priority::from(args.priority),
                            shell_script: None,
                        });
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
