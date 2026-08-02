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
        "{:<14} {:<16} {:>8} {:>8} {:>7}  LIMITS",
        "SCOPE", "NAME", "RUNNING", "GPUS", "QUEUED"
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

        let mut limits: Vec<String> = Vec::new();
        if let Some(limits_obj) = entry.get("limits") {
            for (key, label) in [
                ("max_running_jobs", "jobs"),
                ("max_running_gpus", "gpus"),
                ("max_queued_jobs", "queued"),
            ] {
                if let Some(value) = limits_obj.get(key).and_then(|v| v.as_u64()) {
                    limits.push(format!("{label}<={value}"));
                }
            }
        }
        let limits_str = if limits.is_empty() {
            "-".to_string()
        } else {
            limits.join(" ")
        };

        let display_name = if name.is_empty() { "-" } else { name };
        println!(
            "{:<14} {:<16} {:>8} {:>8} {:>7}  {}",
            scope, display_name, running_jobs, running_gpus, queued_jobs, limits_str
        );
    }

    Ok(())
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
