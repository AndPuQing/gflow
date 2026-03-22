use super::*;

/// Custom deserializer for jobs field that handles both old HashMap and new Vec formats
fn deserialize_jobs<'de, D>(deserializer: D) -> Result<Vec<Job>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::{IgnoredAny, MapAccess, SeqAccess, Visitor};
    use std::fmt;

    struct JobsVisitor;

    impl<'de> Visitor<'de> for JobsVisitor {
        type Value = Vec<Job>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("jobs as an array, a map of id->job, or null")
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(Vec::new())
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(Vec::new())
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut jobs = Vec::new();
            while let Some(job) = seq.next_element::<Job>()? {
                jobs.push(job);
            }
            Ok(jobs)
        }

        fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where
            A: MapAccess<'de>,
        {
            let mut jobs = Vec::new();
            while let Some((_key, job)) = map.next_entry::<IgnoredAny, Job>()? {
                jobs.push(job);
            }
            jobs.sort_by_key(|j| j.id);
            Ok(jobs)
        }
    }

    deserializer.deserialize_any(JobsVisitor)
}

#[derive(Deserialize)]
#[serde(default)]
struct SchedulerSerde {
    pub version: u32,
    pub job_specs: Vec<JobSpec>,
    pub job_runtimes: Vec<JobRuntime>,
    #[serde(deserialize_with = "deserialize_jobs", default)]
    pub jobs: Vec<Job>,
    pub(crate) state_path: PathBuf,
    pub(crate) next_job_id: u32,
    pub(crate) allowed_gpu_indices: Option<Vec<u32>>,
    pub reservations: Vec<GpuReservation>,
    pub next_reservation_id: u32,
}

#[derive(Deserialize)]
struct SchedulerSeqV2(u32, Vec<Job>, PathBuf, u32, Option<Vec<u32>>);

#[derive(Deserialize)]
#[serde(untagged)]
enum SchedulerPersisted {
    Current(SchedulerSerde),
    SeqV2(SchedulerSeqV2),
}

impl Default for SchedulerSerde {
    fn default() -> Self {
        Self {
            version: crate::core::migrations::CURRENT_VERSION,
            job_specs: Vec::new(),
            job_runtimes: Vec::new(),
            jobs: Vec::new(),
            state_path: PathBuf::from("state.json"),
            next_job_id: 1,
            allowed_gpu_indices: None,
            reservations: Vec::new(),
            next_reservation_id: 1,
        }
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self {
            version: crate::core::migrations::CURRENT_VERSION,
            job_specs: Vec::new(),
            job_runtimes: Vec::new(),
            executor: None,
            gpu_slots: HashMap::new(),
            total_memory_mb: 16 * 1024,
            available_memory_mb: 16 * 1024,
            state_path: PathBuf::from("state.json"),
            next_job_id: 1,
            allowed_gpu_indices: None,
            gpu_allocation_strategy: GpuAllocationStrategy::default(),
            user_jobs_index: HashMap::new(),
            state_jobs_index: HashMap::new(),
            project_jobs_index: HashMap::new(),
            dependency_graph: HashMap::new(),
            dependents_graph: HashMap::new(),
            group_running_count: HashMap::new(),
            reservations: Vec::new(),
            next_reservation_id: 1,
        }
    }
}

impl<'de> Deserialize<'de> for Scheduler {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;

        let persisted = match SchedulerPersisted::deserialize(deserializer)? {
            SchedulerPersisted::Current(persisted) => persisted,
            SchedulerPersisted::SeqV2(SchedulerSeqV2(
                version,
                jobs,
                state_path,
                next_job_id,
                allowed_gpu_indices,
            )) => SchedulerSerde {
                version,
                jobs,
                state_path,
                next_job_id,
                allowed_gpu_indices,
                ..SchedulerSerde::default()
            },
        };
        tracing::debug!(
            "Deserialized persisted scheduler: version={}, job_specs={}, job_runtimes={}, legacy_jobs={}",
            persisted.version,
            persisted.job_specs.len(),
            persisted.job_runtimes.len(),
            persisted.jobs.len()
        );

        let mut job_specs = persisted.job_specs;
        let mut job_runtimes = persisted.job_runtimes;
        let has_split = !job_specs.is_empty() || !job_runtimes.is_empty();

        if has_split {
            if job_specs.len() != job_runtimes.len() {
                return Err(D::Error::custom(format!(
                    "Invalid state: job_specs({}) and job_runtimes({}) length mismatch",
                    job_specs.len(),
                    job_runtimes.len()
                )));
            }
        } else if !persisted.jobs.is_empty() {
            let (specs, runtimes): (Vec<_>, Vec<_>) = persisted
                .jobs
                .into_iter()
                .map(|job| job.into_parts())
                .unzip();
            job_specs = specs;
            job_runtimes = runtimes;
        }

        let scheduler = Scheduler {
            version: persisted.version,
            job_specs,
            job_runtimes,
            executor: None,
            gpu_slots: HashMap::new(),
            total_memory_mb: 16 * 1024,
            available_memory_mb: 16 * 1024,
            state_path: persisted.state_path,
            next_job_id: persisted.next_job_id,
            allowed_gpu_indices: persisted.allowed_gpu_indices,
            gpu_allocation_strategy: GpuAllocationStrategy::default(),
            user_jobs_index: HashMap::new(),
            state_jobs_index: HashMap::new(),
            project_jobs_index: HashMap::new(),
            dependency_graph: HashMap::new(),
            dependents_graph: HashMap::new(),
            group_running_count: HashMap::new(),
            reservations: persisted.reservations,
            next_reservation_id: persisted.next_reservation_id,
        };

        Ok(scheduler)
    }
}

impl Scheduler {
    /// Apply persisted state from another Scheduler instance.
    ///
    /// This intentionally does NOT overwrite runtime-only fields like executor,
    /// gpu slots, memory tracking, or the configured state path.
    pub fn apply_persisted_state(&mut self, mut loaded: Scheduler) {
        let state_path = self.state_path.clone();

        self.version = loaded.version;
        self.job_specs = std::mem::take(&mut loaded.job_specs);
        self.job_runtimes = std::mem::take(&mut loaded.job_runtimes);
        self.next_job_id = loaded.next_job_id;
        self.allowed_gpu_indices = loaded.allowed_gpu_indices;
        self.reservations = std::mem::take(&mut loaded.reservations);
        self.next_reservation_id = loaded.next_reservation_id;

        self.state_path = state_path;
    }
}
