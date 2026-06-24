use gflow::core::job::Job;
use rmcp::model::CallToolResult;
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn structured_response<T: serde::Serialize>(
    value: T,
) -> Result<CallToolResult, rmcp::ErrorData> {
    let value = serde_json::to_value(value).map_err(|err| {
        rmcp::ErrorData::internal_error(format!("Failed to serialize MCP response: {}", err), None)
    })?;

    Ok(CallToolResult::structured(value))
}

pub(super) fn stringify_error(err: anyhow::Error) -> rmcp::ErrorData {
    rmcp::ErrorData::internal_error(err.to_string(), None)
}

pub(super) fn serialize_job_value(job: &Job) -> Value {
    serde_json::to_value(job).unwrap_or_else(|err| {
        json!({
            "error": format!("Failed to serialize job: {}", err),
        })
    })
}

pub(super) fn system_time_to_unix_secs(ts: SystemTime) -> Option<u64> {
    ts.duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs())
}
