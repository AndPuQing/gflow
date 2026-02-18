use anyhow::Result;
use gflow::client::{Client, UsageStats};
use gflow::utils::parse_since_time;
use std::time::Duration;

pub async fn handle_stats(
    config_path: &Option<std::path::PathBuf>,
    user: Option<&str>,
    all_users: bool,
    since: Option<&str>,
    output: &str,
) -> Result<()> {
    let config = gflow::config::load_config(config_path.as_ref())?;
    let client = Client::build(&config)?;

    let since_ts: Option<i64> = since.map(parse_since_time).transpose()?;

    // Determine user filter
    let current_user;
    let user_filter: Option<&str> = if all_users {
        None
    } else if let Some(u) = user {
        Some(u)
    } else {
        current_user = gflow::core::get_current_username();
        Some(current_user.as_str())
    };

    let stats = client.get_stats(user_filter, since_ts).await?;

    match output {
        "json" => print_json(&stats)?,
        "csv" => print_csv(&stats),
        _ => print_table(&stats),
    }

    Ok(())
}

fn print_json(stats: &UsageStats) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(stats)?);
    Ok(())
}

fn print_csv(stats: &UsageStats) {
    println!("metric,value");
    println!("total_jobs,{}", stats.total_jobs);
    println!("completed_jobs,{}", stats.completed_jobs);
    println!("failed_jobs,{}", stats.failed_jobs);
    println!("cancelled_jobs,{}", stats.cancelled_jobs);
    println!("timeout_jobs,{}", stats.timeout_jobs);
    println!("running_jobs,{}", stats.running_jobs);
    println!("queued_jobs,{}", stats.queued_jobs);
    println!(
        "avg_wait_secs,{}",
        stats
            .avg_wait_secs
            .map_or("".to_string(), |v| format!("{:.1}", v))
    );
    println!(
        "avg_runtime_secs,{}",
        stats
            .avg_runtime_secs
            .map_or("".to_string(), |v| format!("{:.1}", v))
    );
    println!("total_gpu_hours,{:.2}", stats.total_gpu_hours);
    println!("jobs_with_gpus,{}", stats.jobs_with_gpus);
    println!("avg_gpus_per_job,{:.2}", stats.avg_gpus_per_job);
    println!("peak_gpu_usage,{}", stats.peak_gpu_usage);
    println!("success_rate,{:.1}", stats.success_rate);
}

fn print_table(stats: &UsageStats) {
    let terminal_jobs =
        stats.completed_jobs + stats.failed_jobs + stats.cancelled_jobs + stats.timeout_jobs;

    // Header
    let user_label = stats.user.as_deref().unwrap_or("all users");
    println!("Usage Statistics - {}", user_label);
    println!("{}", "-".repeat(50));

    // Job summary
    println!("Job Summary:");
    println!("  Total Jobs:        {}", stats.total_jobs);
    if terminal_jobs > 0 {
        println!(
            "  Completed:         {} ({:.0}%)",
            stats.completed_jobs,
            stats.completed_jobs as f64 / terminal_jobs as f64 * 100.0
        );
        println!(
            "  Failed:            {} ({:.0}%)",
            stats.failed_jobs,
            stats.failed_jobs as f64 / terminal_jobs as f64 * 100.0
        );
        println!(
            "  Cancelled:         {} ({:.0}%)",
            stats.cancelled_jobs,
            stats.cancelled_jobs as f64 / terminal_jobs as f64 * 100.0
        );
        if stats.timeout_jobs > 0 {
            println!(
                "  Timeout:           {} ({:.0}%)",
                stats.timeout_jobs,
                stats.timeout_jobs as f64 / terminal_jobs as f64 * 100.0
            );
        }
    } else {
        println!("  Completed:         {}", stats.completed_jobs);
        println!("  Failed:            {}", stats.failed_jobs);
        println!("  Cancelled:         {}", stats.cancelled_jobs);
    }
    if stats.running_jobs > 0 || stats.queued_jobs > 0 {
        println!("  Running:           {}", stats.running_jobs);
        println!("  Queued:            {}", stats.queued_jobs);
    }

    println!();
    println!("Timing:");
    println!(
        "  Avg Wait Time:     {}",
        stats.avg_wait_secs.map_or("-".to_string(), format_secs)
    );
    println!(
        "  Avg Runtime:       {}",
        stats.avg_runtime_secs.map_or("-".to_string(), format_secs)
    );
    println!("  Total GPU-Hours:   {:.1}h", stats.total_gpu_hours);

    println!();
    println!("GPU Usage:");
    println!(
        "  Jobs with GPUs:    {} ({:.0}%)",
        stats.jobs_with_gpus,
        if stats.total_jobs > 0 {
            stats.jobs_with_gpus as f64 / stats.total_jobs as f64 * 100.0
        } else {
            0.0
        }
    );
    println!("  Avg GPUs/Job:      {:.1}", stats.avg_gpus_per_job);
    println!("  Peak GPU Usage:    {}", stats.peak_gpu_usage);

    println!();
    println!("Success Rate:        {:.1}%", stats.success_rate);

    if !stats.top_jobs.is_empty() {
        println!();
        println!("Top Jobs by Runtime:");
        for (i, job) in stats.top_jobs.iter().enumerate() {
            let name = job.name.as_deref().unwrap_or("<unnamed>");
            println!(
                "  {}. Job {:>5} ({:<20}) {}   {} GPU(s)",
                i + 1,
                job.id,
                name,
                format_secs(job.runtime_secs),
                job.gpus
            );
        }
    }
}

fn format_secs(secs: f64) -> String {
    let d = Duration::from_secs_f64(secs);
    gflow::utils::format_duration(d)
}
