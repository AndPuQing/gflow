use anyhow::{bail, Result};
use gflow::client::Client;
use gflow::config::QuotaLimits;

/// Resolve the `(scope, name)` pair from the mutually exclusive subject flags.
fn resolve_subject(
    user: Option<&str>,
    project: Option<&str>,
    default_user: bool,
    default_project: bool,
) -> Result<(&'static str, Option<String>)> {
    match (user, project, default_user, default_project) {
        (Some(user), None, false, false) => Ok(("user", Some(user.to_string()))),
        (None, Some(project), false, false) => Ok(("project", Some(project.to_string()))),
        (None, None, true, false) => Ok(("default_user", None)),
        (None, None, false, true) => Ok(("default_project", None)),
        _ => bail!(
            "select exactly one subject: --user, --project, --default-user or --default-project"
        ),
    }
}

pub async fn handle_quota_list(client: &Client) -> Result<()> {
    let body = client.list_quotas().await?;

    let Some(quotas) = body.get("quotas").and_then(|v| v.as_array()) else {
        bail!("unexpected response format from daemon");
    };

    if quotas.is_empty() {
        println!("No quota subjects (no limits configured, no active jobs).");
        return Ok(());
    }

    println!(
        "{:<15} {:<16} {:>9} {:>9} {:>9}",
        "SCOPE", "NAME", "JOBS", "GPUS", "QUEUED"
    );
    for entry in quotas {
        let scope = entry.get("scope").and_then(|v| v.as_str()).unwrap_or("?");
        let name = entry.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let running_jobs = entry
            .get("running_jobs")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let running_gpus = entry
            .get("running_gpus")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let queued_jobs = entry
            .get("queued_jobs")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let limits = entry.get("limits");
        let max_jobs = limit_value(limits, "max_running_jobs");
        let max_gpus = limit_value(limits, "max_running_gpus");
        let max_queued = limit_value(limits, "max_queued_jobs");

        // Fallback rows (default_user / default_project) describe limits that
        // apply to everyone, so there is no single usage number to show —
        // render the bare limit. Named subjects render `used/limit`.
        let is_default = matches!(scope, "default_user" | "default_project");
        let (jobs_col, gpus_col, queued_col) = if is_default {
            (
                fmt_limit(max_jobs),
                fmt_limit(max_gpus),
                fmt_limit(max_queued),
            )
        } else {
            (
                fmt_usage(running_jobs, max_jobs),
                fmt_usage(running_gpus, max_gpus),
                fmt_usage(queued_jobs, max_queued),
            )
        };

        let display_name = if name.is_empty() { "*" } else { name };
        println!(
            "{:<15} {:<16} {:>9} {:>9} {:>9}",
            scope, display_name, jobs_col, gpus_col, queued_col
        );
    }

    println!();
    println!("Cells are used/limit; `-` means unlimited. Fallback rows show the limit only.");

    Ok(())
}

/// Extract a numeric limit field, or `None` when unset (unlimited).
fn limit_value(limits: Option<&serde_json::Value>, key: &str) -> Option<u64> {
    limits.and_then(|l| l.get(key)).and_then(|v| v.as_u64())
}

/// Render a limit value: the number, or `-` when unlimited.
fn fmt_limit(limit: Option<u64>) -> String {
    match limit {
        Some(v) => v.to_string(),
        None => "-".to_string(),
    }
}

/// Render a `used/limit` cell, e.g. `3/4` or `0/-` (unlimited).
fn fmt_usage(used: u64, limit: Option<u64>) -> String {
    format!("{}/{}", used, fmt_limit(limit))
}

#[allow(clippy::too_many_arguments)]
pub async fn handle_quota_set(
    client: &Client,
    user: Option<String>,
    project: Option<String>,
    default_user: bool,
    default_project: bool,
    max_running_jobs: Option<usize>,
    max_running_gpus: Option<u32>,
    max_queued_jobs: Option<usize>,
) -> Result<()> {
    let (scope, name) = resolve_subject(
        user.as_deref(),
        project.as_deref(),
        default_user,
        default_project,
    )?;

    let limits = QuotaLimits {
        max_running_jobs,
        max_running_gpus,
        max_queued_jobs,
    };
    if limits.is_empty() {
        bail!("provide at least one of --max-running-jobs, --max-running-gpus, --max-queued-jobs");
    }

    client.set_quota(scope, name.as_deref(), limits).await?;

    println!(
        "Updated {} quota{}",
        scope,
        name.as_deref()
            .map(|n| format!(" '{n}'"))
            .unwrap_or_default()
    );
    if let Some(v) = max_running_jobs {
        println!("  max_running_jobs = {v}");
    }
    if let Some(v) = max_running_gpus {
        println!("  max_running_gpus = {v}");
    }
    if let Some(v) = max_queued_jobs {
        println!("  max_queued_jobs  = {v}");
    }

    Ok(())
}

pub async fn handle_quota_remove(
    client: &Client,
    user: Option<String>,
    project: Option<String>,
    default_user: bool,
    default_project: bool,
) -> Result<()> {
    let (scope, name) = resolve_subject(
        user.as_deref(),
        project.as_deref(),
        default_user,
        default_project,
    )?;

    client.remove_quota(scope, name.as_deref()).await?;

    println!(
        "Removed {} quota override{}",
        scope,
        name.as_deref()
            .map(|n| format!(" '{n}'"))
            .unwrap_or_default()
    );

    Ok(())
}
