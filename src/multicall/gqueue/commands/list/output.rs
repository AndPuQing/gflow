use crate::multicall::gqueue::commands::list::display::get_job_reason_display;
use anyhow::Result;
use gflow::core::job::JobState;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumString)]
#[strum(ascii_case_insensitive)]
pub(super) enum OutputFormat {
    Table,
    Json,
    Csv,
    Yaml,
}

#[derive(Debug, Serialize)]
pub(super) struct JobOutput {
    pub(super) id: u32,
    pub(super) name: Option<String>,
    pub(super) state: String,
    pub(super) time: String,
    pub(super) gpus: Vec<u32>,
    pub(super) user: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) project: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) submitted_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) memory_mb: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) time_limit: Option<String>,
}

#[derive(Debug, Serialize)]
struct JobListOutput {
    jobs: Vec<JobOutput>,
    total: usize,
    timestamp: String,
}

impl JobOutput {
    pub(super) fn from_job(job: &gflow::core::job::Job) -> Self {
        Self {
            id: job.id,
            name: job.run_name.as_ref().map(|s| s.to_string()),
            state: job.state.to_string(),
            time: gflow::utils::format_elapsed_time(job.started_at, job.finished_at),
            gpus: job
                .gpu_ids
                .as_ref()
                .map_or_else(Vec::new, |ids| ids.to_vec()),
            user: job.submitted_by.to_string(),
            project: job.project.as_ref().map(|s| s.to_string()),
            submitted_at: job.submitted_at.and_then(|t| {
                chrono::DateTime::<chrono::Utc>::from(t)
                    .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
                    .into()
            }),
            reason: match job.state {
                JobState::Queued | JobState::Hold | JobState::Cancelled => Some(
                    get_job_reason_display(job)
                        .trim_matches(|c| c == '(' || c == ')')
                        .to_string(),
                ),
                _ => None,
            },
            memory_mb: job.memory_limit_mb,
            time_limit: job.time_limit.map(gflow::utils::format_duration),
        }
    }
}

pub(super) fn output_json(jobs: &[gflow::core::job::Job]) -> Result<()> {
    let job_outputs: Vec<JobOutput> = jobs.iter().map(JobOutput::from_job).collect();
    let output = JobListOutput {
        jobs: job_outputs,
        total: jobs.len(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    };
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

pub(super) fn output_csv(jobs: &[gflow::core::job::Job]) -> Result<()> {
    let mut wtr = csv::Writer::from_writer(std::io::stdout());
    wtr.write_record([
        "id",
        "name",
        "state",
        "time",
        "gpus",
        "user",
        "submitted_at",
        "reason",
    ])?;

    for job in jobs {
        let job_output = JobOutput::from_job(job);
        wtr.write_record(&[
            job_output.id.to_string(),
            job_output.name.unwrap_or_else(|| "-".to_string()),
            job_output.state,
            job_output.time,
            format!(
                "[{}]",
                job_output
                    .gpus
                    .iter()
                    .map(|g| g.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            job_output.user,
            job_output.submitted_at.unwrap_or_else(|| "-".to_string()),
            job_output.reason.unwrap_or_else(|| "-".to_string()),
        ])?;
    }

    wtr.flush()?;
    Ok(())
}

pub(super) fn output_yaml(jobs: &[gflow::core::job::Job]) -> Result<()> {
    let job_outputs: Vec<JobOutput> = jobs.iter().map(JobOutput::from_job).collect();
    let output = JobListOutput {
        jobs: job_outputs,
        total: jobs.len(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    };
    println!("{}", serde_yaml::to_string(&output)?);
    Ok(())
}
