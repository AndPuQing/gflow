use anyhow::{anyhow, Context, Result};
use gflow::client::Client;

/// Resolve job ID from string (handles @ shorthand notation)
///
/// Accepts:
/// - Numeric job ID: "123"
/// - Most recent job: "@"
/// - Nth most recent job: "@~1", "@~2", etc.
pub async fn resolve_job_id(client: &Client, job_id_str: &str) -> Result<u32> {
    let trimmed = job_id_str.trim();

    if trimmed.starts_with('@') {
        // Use dependency resolution to handle @ shorthand
        let username = gflow::core::get_current_username();
        client
            .resolve_dependency(&username, trimmed)
            .await
            .with_context(|| format!("Failed to resolve job ID '{}'", trimmed))
    } else {
        // Parse as numeric job ID
        trimmed
            .parse::<u32>()
            .map_err(|_| anyhow!("Invalid job ID: {}", trimmed))
    }
}

/// Resolve dependency expression to job ID
///
/// Accepts:
/// - Numeric job ID: "123"
/// - Most recent job: "@"
/// - Nth most recent job: "@~1", "@~2", etc.
pub async fn resolve_dependency(client: &Client, depends_on: &str) -> Result<u32> {
    let trimmed = depends_on.trim();

    if trimmed.starts_with('@') {
        let username = gflow::core::get_current_username();
        client
            .resolve_dependency(&username, trimmed)
            .await
            .with_context(|| format!("Failed to resolve dependency '{}'", trimmed))
    } else {
        trimmed
            .parse::<u32>()
            .map_err(|_| anyhow!("Invalid dependency value: {}", trimmed))
    }
}
