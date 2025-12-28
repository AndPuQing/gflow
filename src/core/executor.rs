use crate::core::job::Job;
use anyhow::Result;

pub trait Executor: Send + Sync {
    fn execute(&self, job: &Job) -> Result<()>;
}
