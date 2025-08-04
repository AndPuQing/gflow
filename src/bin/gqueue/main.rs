mod cli;
use anyhow::Result;
use clap::Parser;
use gflow::{client::Client, config::load_config, core::job::JobState};

#[tokio::main]
async fn main() -> Result<()> {
    let args = cli::GQueue::parse();
    let config = load_config(args.config.as_ref())?;

    let client = Client::build(&config)?;
    let mut jobs = client.list_jobs().await?;

    if let Some(states) = args.states {
        let states: Vec<JobState> = states
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        if !states.is_empty() {
            jobs.retain(|job| states.contains(&job.state));
        }
    }

    if let Some(job_ids) = args.jobs {
        let job_ids: Vec<u32> = job_ids
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        if !job_ids.is_empty() {
            jobs.retain(|job| job_ids.contains(&job.id));
        }
    }

    if let Some(names) = args.names {
        let names: Vec<String> = names.split(',').map(|s| s.trim().to_string()).collect();
        if !names.is_empty() {
            jobs.retain(|job| {
                job.run_name
                    .as_ref()
                    .is_some_and(|run_name| names.contains(run_name))
            });
        }
    }

    if jobs.is_empty() {
        println!("No jobs found.");
        return Ok(());
    }

    let format = args
        .format
        .unwrap_or_else(|| "JOBID,NAME,ST,NODES,NODELIST(REASON)".to_string());
    let headers: Vec<&str> = format.split(',').collect();
    println!(
        "{}",
        headers
            .iter()
            .map(|h| format!("{:<width$}", h, width = get_width(h)))
            .collect::<Vec<_>>()
            .join(" ")
    );

    for job in jobs {
        let mut row = Vec::new();
        for header in &headers {
            let value = match *header {
                "JOBID" => job.id.to_string(),
                "NAME" => job.run_name.as_deref().unwrap_or("-").to_string(),
                "ST" => match job.state {
                    JobState::Queued => "PD",
                    JobState::Running => "R",
                    JobState::Finished => "CD",
                    JobState::Failed => "F",
                }
                .to_string(),
                "NODES" => job.gpus.to_string(),
                "NODELIST(REASON)" => job.gpu_ids.as_ref().map_or_else(
                    || "-".to_string(),
                    |ids| {
                        ids.iter()
                            .map(|id| id.to_string())
                            .collect::<Vec<_>>()
                            .join(",")
                    },
                ),
                _ => "".to_string(),
            };
            row.push(format!("{:<width$}", value, width = get_width(header)));
        }
        println!("{}", row.join(" "));
    }

    Ok(())
}

fn get_width(header: &str) -> usize {
    match header {
        "JOBID" => 8,
        "NAME" => 20,
        "ST" => 5,
        "NODES" => 8,
        "NODELIST(REASON)" => 15,
        _ => 10,
    }
}
