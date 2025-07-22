use crate::job::Job;
use anyhow::Result;

pub trait Executor {
    fn execute(&self, job: &Job) -> Result<()>;
}
