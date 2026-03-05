use anyhow::Result;
use gflow::client::{Client, UsageStats};
use gflow::utils::parse_since_time;
use owo_colors::OwoColorize;
use std::time::Duration;
use tabled::{builder::Builder, settings::style::Style};

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

    let user_label = stats.user.as_deref().unwrap_or("all users");
    println!("{}", format!("Usage Statistics · {}", user_label).bold());
    println!("{}", "─".repeat(68).dimmed());
    println!();

    print_section("Job Summary");
    print_kv("Total Jobs", stats.total_jobs);
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
        print_kv("Completed", stats.completed_jobs);
        print_kv("Failed", stats.failed_jobs);
        print_kv("Cancelled", stats.cancelled_jobs);
    }
    if stats.running_jobs > 0 || stats.queued_jobs > 0 {
        print_kv("Running", stats.running_jobs);
        print_kv("Queued", stats.queued_jobs);
    }

    println!();
    print_section("Timing");
    print_kv(
        "Avg Wait Time",
        stats.avg_wait_secs.map_or("-".to_string(), format_secs),
    );
    print_kv(
        "Avg Runtime",
        stats.avg_runtime_secs.map_or("-".to_string(), format_secs),
    );
    print_kv("Total GPU-Hours", format!("{:.1}h", stats.total_gpu_hours));

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
            "{} ({:.0}%) {}",
            stats.jobs_with_gpus,
            gpu_pct,
            progress_bar(gpu_pct, 20, BarTone::Info)
        ),
    );
    print_kv("Avg GPUs/Job", format!("{:.1}", stats.avg_gpus_per_job));
    print_kv("Peak GPU Usage", stats.peak_gpu_usage);

    println!();
    let success_tone = tone_for_success_rate(stats.success_rate);
    print_kv(
        "Success Rate",
        format!(
            "{:.1}% {}",
            stats.success_rate,
            progress_bar(stats.success_rate, 30, success_tone)
        ),
    );

    if !stats.top_jobs.is_empty() {
        println!();
        print_section("Top Jobs by Runtime");
        let mut builder = Builder::default();
        builder.push_record(["#", "JOBID", "NAME", "RUNTIME", "GPUS"]);
        for (i, job) in stats.top_jobs.iter().enumerate() {
            let name = job.name.as_deref().unwrap_or("<unnamed>");
            let short_name = truncate_for_cell(name, 32);
            builder.push_record([
                (i + 1).to_string(),
                job.id.to_string(),
                short_name,
                format_secs(job.runtime_secs),
                job.gpus.to_string(),
            ]);
        }
        let mut table = builder.build();
        table.with(Style::blank());
        println!("{table}");
    }
}

fn print_section(title: &str) {
    println!("{}", title.bold().cyan());
}

fn print_kv(label: &str, value: impl std::fmt::Display) {
    println!("  {:<16} {}", format!("{label}:"), value);
}

fn print_job_stat(label: &str, count: usize, total: usize, tone: BarTone) {
    let pct = if total == 0 {
        0.0
    } else {
        count as f64 / total as f64 * 100.0
    };
    print_kv(
        label,
        format!("{} ({:.0}%) {}", count, pct, progress_bar(pct, 20, tone)),
    );
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

fn progress_bar(percentage: f64, width: usize, tone: BarTone) -> String {
    let pct = percentage.clamp(0.0, 100.0);
    let filled = ((pct / 100.0) * width as f64).round() as usize;
    let filled = filled.min(width);
    let empty = width - filled;

    let filled_block = "█".repeat(filled);
    let empty_block = "░".repeat(empty).dimmed().to_string();

    let styled_filled = match tone {
        BarTone::Good => filled_block.green().bold().to_string(),
        BarTone::Warn => filled_block.yellow().bold().to_string(),
        BarTone::Bad => filled_block.red().bold().to_string(),
        BarTone::Info => filled_block.cyan().bold().to_string(),
    };

    format!("[{}{}]", styled_filled, empty_block)
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
