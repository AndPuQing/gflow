/// Scheduler：负责任务调度
struct Scheduler {
    task_queue: Arc<Mutex<VecDeque<Task>>>, // 任务队列
    workers: Vec<Worker>,                   // Worker 列表
}

impl Scheduler {
    /// 创建 Scheduler
    fn new(worker_count: usize) -> Self {
        let workers = (0..worker_count)
            .map(|i| Worker {
                id: format!("worker_{}", i),
            })
            .collect();

        Self {
            task_queue: Arc::new(Mutex::new(VecDeque::new())),
            workers,
        }
    }

    /// 添加任务到队列
    fn add_task(&self, task: Task) {
        let mut queue = self.task_queue.lock().unwrap();
        queue.push_back(task);
        println!("Task added to queue");
    }

    /// 分发任务
    fn schedule_tasks(&self) {
        let task_queue = Arc::clone(&self.task_queue);

        for worker in &self.workers {
            let worker_id = worker.id.clone();
            let task_queue = Arc::clone(&task_queue);

            thread::spawn(move || loop {
                let task_opt = {
                    let mut queue = task_queue.lock().unwrap();
                    queue.pop_front()
                };

                if let Some(task) = task_opt {
                    println!("Worker {} picked up task {}", worker_id, task.id);
                    Worker {
                        id: worker_id.clone(),
                    }
                    .execute_task(task);
                } else {
                    thread::sleep(Duration::from_secs(1));
                }
            });
        }
    }

    /// 监控任务状态
    fn monitor_tasks(&self) {
        let task_queue = Arc::clone(&self.task_queue);

        thread::spawn(move || loop {
            {
                let queue = task_queue.lock().unwrap();
                for task in queue.iter() {
                    if !Worker::is_task_running(&task.id) {
                        println!("Task {} completed", task.id);
                    }
                }
            }
            thread::sleep(Duration::from_secs(5));
        });
    }
}
