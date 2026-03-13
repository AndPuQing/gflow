use anyhow::Result;
use chrono::TimeZone;
use gflow::client::{Client, UsageStats};
use gflow::utils::parse_since_time;
use owo_colors::OwoColorize;
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
        current_user = gflow::platform::get_current_username();
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
    let active_jobs = stats.running_jobs + stats.queued_jobs;

    print_header(stats, active_jobs);
    println!();

    print_section("Job Status");
    print_kv("Total Jobs", stats.total_jobs.to_string().bold());
    if terminal_jobs > 0 {
        print_job_stat(
            "Completed",
            stats.completed_jobs,
            terminal_jobs,
            BarTone::Good,
        );
        print_job_stat("Failed", stats.failed_jobs, terminal_jobs, BarTone::Bad);
        print_job_stat(
            "Cancelled",
            stats.cancelled_jobs,
            terminal_jobs,
            BarTone::Warn,
        );
        if stats.timeout_jobs > 0 {
            print_job_stat("Timeout", stats.timeout_jobs, terminal_jobs, BarTone::Bad);
        }
    } else {
        print_kv(
            "Completed",
            style_value(stats.completed_jobs, BarTone::Good),
        );
        print_kv("Failed", style_value(stats.failed_jobs, BarTone::Bad));
        print_kv(
            "Cancelled",
            style_value(stats.cancelled_jobs, BarTone::Warn),
        );
    }
    if active_jobs > 0 {
        print_job_stat(
            "Running",
            stats.running_jobs,
            stats.total_jobs.max(1),
            BarTone::Info,
        );
        print_job_stat(
            "Queued",
            stats.queued_jobs,
            stats.total_jobs.max(1),
            BarTone::Warn,
        );
    }

    println!();
    print_section("Efficiency");
    print_kv(
        "Avg Wait Time",
        stats.avg_wait_secs.map_or("-".to_string(), format_secs),
    );
    print_kv(
        "Avg Runtime",
        stats.avg_runtime_secs.map_or("-".to_string(), format_secs),
    );
    print_kv("Total GPU-Hours", format!("{:.1}h", stats.total_gpu_hours));
    let success_tone = tone_for_success_rate(stats.success_rate);
    print_kv(
        "Success Rate",
        style_value(format!("{:.1}%", stats.success_rate), success_tone),
    );

    println!();
    print_section("GPU Usage");
    let gpu_pct = if stats.total_jobs > 0 {
        stats.jobs_with_gpus as f64 / stats.total_jobs as f64 * 100.0
    } else {
        0.0
    };
    print_kv(
        "Jobs with GPUs",
        format!(
            "{} ({:.0}%)",
            style_value(stats.jobs_with_gpus, BarTone::Info),
            gpu_pct
        ),
    );
    print_kv("Avg GPUs/Job", format!("{:.1}", stats.avg_gpus_per_job));
    print_kv("Peak GPU Usage", stats.peak_gpu_usage);

    if !stats.top_jobs.is_empty() {
        println!();
        print_section("Top Jobs by Runtime");
        print_top_jobs(stats);
    }
}

fn print_section(title: &str) {
    println!("{}", title.bold());
}

fn print_kv(label: &str, value: impl std::fmt::Display) {
    println!("  {:<16} {}", format!("{label}:").dimmed(), value);
}

fn print_job_stat(label: &str, count: usize, total: usize, tone: BarTone) {
    let pct = if total == 0 {
        0.0
    } else {
        count as f64 / total as f64 * 100.0
    };
    print_kv(label, format!("{} ({:.0}%)", style_value(count, tone), pct));
}

#[derive(Debug, Clone, Copy)]
enum BarTone {
    Good,
    Warn,
    Bad,
    Info,
}

fn tone_for_success_rate(rate: f64) -> BarTone {
    if rate >= 90.0 {
        BarTone::Good
    } else if rate >= 70.0 {
        BarTone::Warn
    } else {
        BarTone::Bad
    }
}

fn print_header(stats: &UsageStats, active_jobs: usize) {
    let user_label = stats.user.as_deref().unwrap_or("all users");

    let window = stats
        .since
        .map(format_since)
        .unwrap_or_else(|| "all time".to_string());

    println!(
        "{}  {}  {}  {}",
        "Usage Statistics".bold(),
        format!("user: {user_label}").dimmed(),
        format!("since: {window}").dimmed(),
        format!("active: {active_jobs}").dimmed(),
    );
    println!("{}", "─".repeat(72).dimmed());
}

fn style_value(value: impl std::fmt::Display, tone: BarTone) -> String {
    match tone {
        BarTone::Good | BarTone::Warn | BarTone::Bad | BarTone::Info => {
            value.to_string().bold().to_string()
        }
    }
}

fn print_top_jobs(stats: &UsageStats) {
    let runtime_width = stats
        .top_jobs
        .iter()
        .map(|job| format_secs(job.runtime_secs).chars().count())
        .max()
        .unwrap_or(7)
        .max(7);
    let gpu_width = stats
        .top_jobs
        .iter()
        .map(|job| job.gpus.to_string().chars().count())
        .max()
        .unwrap_or(4)
        .max(4);
    let table_width = 50 + runtime_width + gpu_width;

    println!(
        "  {} {}  {}  {}  {}",
        format!("{:<3}", "#").bold(),
        format!("{:>6}", "JOBID").bold(),
        format!("{:<34}", "NAME").bold(),
        format!("{:>runtime_width$}", "RUNTIME").bold(),
        format!("{:>gpu_width$}", "GPUS").bold(),
    );
    println!("  {}", "─".repeat(table_width).dimmed());

    for (index, job) in stats.top_jobs.iter().enumerate() {
        let rank = format!("{:<3}", format!("{}.", index + 1))
            .bold()
            .to_string();
        let job_id = format!("{:>6}", job.id).bold().to_string();
        let name = format!(
            "{:<34}",
            truncate_for_cell(job.name.as_deref().unwrap_or("<unnamed>"), 34)
        );
        let runtime = format!("{:>runtime_width$}", format_secs(job.runtime_secs))
            .bold()
            .to_string();
        let gpus = format!("{:>gpu_width$}", job.gpus).bold().to_string();

        println!("  {} {}  {}  {}  {}", rank, job_id, name, runtime, gpus);
    }
}

fn truncate_for_cell(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        return input.to_string();
    }

    if max_chars <= 3 {
        return ".".repeat(max_chars);
    }

    let mut out = input.chars().take(max_chars - 3).collect::<String>();
    out.push_str("...");
    out
}

fn format_secs(secs: f64) -> String {
    let d = Duration::from_secs_f64(secs);
    gflow::utils::format_duration(d)
}

fn format_since(ts: u64) -> String {
    chrono::Utc
        .timestamp_opt(ts as i64, 0)
        .single()
        .map(|dt| {
            dt.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_else(|| ts.to_string())
}
