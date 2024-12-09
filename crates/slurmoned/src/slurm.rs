use common::jobs::{Job, GPU};
use num_cpus;
use nvml_wrapper::Nvml;
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use tracing::info;

#[derive(Clone)]
pub struct ResourceManager {
    available_cpus: usize,
    available_gpus: Vec<Arc<Mutex<GPU>>>,
}

impl ResourceManager {
    fn new() -> Self {
        let nvml = Nvml::init().unwrap();
        let cpus = num_cpus::get();
        let gpus = nvml.device_count().unwrap();

        let gpus = (0..gpus)
            .map(|i| {
                let device = nvml.device_by_index(i).unwrap();
                let total_memory = device.memory_info().unwrap().total;
                let available_memory = device.memory_info().unwrap().free;
                let is_busy = false;

                Arc::new(Mutex::new(GPU {
                    id: i as usize,
                    total_memory: total_memory.try_into().unwrap(),
                    available_memory: available_memory.try_into().unwrap(),
                    is_busy,
                }))
            })
            .collect::<Vec<_>>();

        Self {
            available_cpus: cpus,
            available_gpus: gpus,
        }
    }

    fn refresh_gpus(&mut self) {
        let nvml = Nvml::init().unwrap();
        for gpu in &self.available_gpus {
            let mut gpu = gpu.lock().unwrap();
            let device = nvml.device_by_index(gpu.id as u32).unwrap();
            let available_memory = device.memory_info().unwrap().free;
            gpu.available_memory = available_memory.try_into().unwrap();

            gpu.is_busy = device.utilization_rates().unwrap().gpu > 90;
        }
    }

    fn can_allocate(&mut self, resources: &Job) -> bool {
        if resources.resources_required.cpus > self.available_cpus {
            return false;
        }

        let mut available_gpus = Vec::new();
        self.refresh_gpus();
        for gpu in &self.available_gpus {
            let gpu = gpu.lock().unwrap();
            if !gpu.is_busy && gpu.available_memory >= resources.resources_required.gpu_memory {
                available_gpus.push(gpu);
            }
        }

        if resources.resources_required.gpus > available_gpus.len() {
            return false;
        }

        true
    }

    fn allocate(&mut self, resources: &Job) -> Vec<usize> {
        self.available_cpus -= resources.resources_required.cpus;

        let mut allocated_gpus = Vec::new();
        for gpu in &self.available_gpus {
            let mut gpu = gpu.lock().unwrap();
            if !gpu.is_busy && gpu.available_memory >= resources.resources_required.gpu_memory {
                gpu.available_memory -= resources.resources_required.gpu_memory;
                allocated_gpus.push(gpu.id);
            }

            if allocated_gpus.len() == resources.resources_required.gpus {
                break;
            }
        }
        allocated_gpus
    }

    fn release(&mut self, resources: &Job) {
        self.available_cpus += resources.resources_required.cpus;

        for gpu in &self.available_gpus {
            let mut gpu = gpu.lock().unwrap();
            gpu.available_memory += resources.resources_required.gpu_memory;
        }
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

    pub fn start(&mut self) {
        let job_queue = Arc::clone(&self.job_queue);
        let resource_manager = Arc::clone(&self.resource_manager);

        thread::spawn(move || loop {
            let (lock, cvar) = &*job_queue;
            let mut queue = lock.lock().unwrap();

            while queue.is_empty() {
                queue = cvar.wait(queue).unwrap();
            }

            let mut job = queue.pop_front().unwrap();

            let mut rm = resource_manager.lock().unwrap();
            if rm.can_allocate(&job) && job.status == common::jobs::JobStatus::Pending {
                let gpu_ids = rm.allocate(&job);
                info!("Job started: {:?}", job);
                let job_copy = job.clone();
                thread::spawn(move || {
                    job.execute(gpu_ids);
                    info!("Job finished: {:?}", job);
                });
                rm.release(&job_copy);
            } else {
                queue.push_back(job);
                info!("Insufficient resources, job returned to queue");
            }
        });
    }

    pub fn submit(&mut self, job: Job) {
        let (lock, cvar) = &*self.job_queue;
        let mut queue = lock.lock().unwrap();
        queue.push_back(job);
        cvar.notify_one();
    }

    pub fn cancel(&mut self, job_id: usize) {
        let (lock, _) = &*self.job_queue;
        let mut queue = lock.lock().unwrap();
        queue.retain(|job| job.id != job_id);
    }

    pub fn list(&self) -> Vec<Job> {
        let (lock, _) = &*self.job_queue;
        let queue = lock.lock().unwrap();
        queue.iter().cloned().collect()
    }
}
