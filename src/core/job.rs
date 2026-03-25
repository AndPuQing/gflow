mod model;
mod parameters;
mod state;

pub use model::{Job, JobBuilder, JobNotifications, JobRuntime, JobSpec, JobView};
pub use parameters::{DependencyIds, GpuIds, Parameters};
pub use state::{DependencyMode, GpuSharingMode, JobError, JobState, JobStateReason};

use serde::{Deserialize, Deserializer, Serializer};
use uuid::Uuid;

/// Custom serializer for group_id that outputs string format for compatibility
fn serialize_group_id<S>(group_id: &Option<Uuid>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match group_id {
        Some(uuid) => serializer.serialize_some(&uuid.to_string()),
        None => serializer.serialize_none(),
    }
}

/// Custom deserializer for group_id that accepts both string and binary UUID formats
fn deserialize_group_id<'de, D>(deserializer: D) -> Result<Option<Uuid>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;

    let opt: Option<String> = Option::deserialize(deserializer)?;
    match opt {
        Some(s) => Uuid::parse_str(&s)
            .map(Some)
            .map_err(|e| D::Error::custom(format!("Invalid UUID string: {}", e))),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use uuid::Uuid;

    #[test]
    fn test_backward_compatibility_missing_auto_close_tmux() {
        // Simulate an old state.json that doesn't have auto_close_tmux field
        let old_json = r#"{
            "id": 1,
            "script": "/tmp/test.sh",
            "command": null,
            "gpus": 0,
            "conda_env": null,
            "run_dir": "/tmp",
            "priority": 10,
            "depends_on": null,
            "task_id": null,
            "time_limit": null,
            "memory_limit_mb": null,
            "submitted_by": "test",
            "run_name": "test-job-1",
            "state": "Finished",
            "gpu_ids": [],
            "started_at": null,
            "finished_at": null
        }"#;

        let result: Result<Job, _> = serde_json::from_str(old_json);
        assert!(
            result.is_ok(),
            "Failed to deserialize old JSON format: {:?}",
            result.err()
        );

        let job = result.unwrap();
        assert_eq!(job.id, 1);
        assert!(!job.auto_close_tmux); // Should use default value
        assert_eq!(job.redone_from, None); // Should be None by default
        assert_eq!(job.max_retry, None);
        assert_eq!(job.retry_attempt, 0);
    }

    #[test]
    fn test_backward_compatibility_missing_redone_from() {
        // Simulate an old state.json that doesn't have redone_from field
        let old_json = r#"{
            "id": 2,
            "script": null,
            "command": "echo test",
            "gpus": 1,
            "conda_env": "myenv",
            "run_dir": "/home/user",
            "priority": 5,
            "depends_on": null,
            "task_id": null,
            "time_limit": null,
            "memory_limit_mb": null,
            "submitted_by": "alice",
            "auto_close_tmux": true,
            "run_name": "test-job-2",
            "state": "Running",
            "gpu_ids": [0],
            "started_at": null,
            "finished_at": null
        }"#;

        let result: Result<Job, _> = serde_json::from_str(old_json);
        assert!(
            result.is_ok(),
            "Failed to deserialize JSON without redone_from: {:?}",
            result.err()
        );

        let job = result.unwrap();
        assert_eq!(job.id, 2);
        assert!(job.auto_close_tmux);
        assert_eq!(job.redone_from, None); // Should use default value
        assert_eq!(job.max_retry, None);
        assert_eq!(job.retry_attempt, 0);
    }

    #[test]
    fn test_backward_compatibility_missing_memory_limit() {
        // Simulate an old state.json that doesn't have memory_limit_mb field
        let old_json = r#"{
            "id": 3,
            "script": "/tmp/script.sh",
            "command": null,
            "gpus": 2,
            "conda_env": null,
            "run_dir": "/workspace",
            "priority": 8,
            "depends_on": 1,
            "task_id": null,
            "time_limit": null,
            "submitted_by": "bob",
            "redone_from": null,
            "auto_close_tmux": false,
            "run_name": "test-job-3",
            "state": "Queued",
            "gpu_ids": null,
            "started_at": null,
            "finished_at": null
        }"#;

        let result: Result<Job, _> = serde_json::from_str(old_json);
        assert!(
            result.is_ok(),
            "Failed to deserialize JSON without memory_limit_mb: {:?}",
            result.err()
        );

        let job = result.unwrap();
        assert_eq!(job.id, 3);
        assert_eq!(job.memory_limit_mb, None); // Should use default value
        assert_eq!(job.max_retry, None);
        assert_eq!(job.retry_attempt, 0);
    }

    #[test]
    fn test_backward_compatibility_minimal_json() {
        // Test with absolute minimal JSON - only required fields from old version
        let minimal_json = r#"{
            "id": 4,
            "gpus": 0,
            "run_dir": "/tmp",
            "priority": 10,
            "submitted_by": "minimal",
            "state": "Queued"
        }"#;

        let result: Result<Job, _> = serde_json::from_str(minimal_json);
        assert!(
            result.is_ok(),
            "Failed to deserialize minimal JSON: {:?}",
            result.err()
        );

        let job = result.unwrap();
        assert_eq!(job.id, 4);
        assert!(!job.auto_close_tmux);
        assert_eq!(job.redone_from, None);
        assert_eq!(job.memory_limit_mb, None);
        assert_eq!(job.max_retry, None);
        assert_eq!(job.retry_attempt, 0);
        assert_eq!(job.script, None);
        assert_eq!(job.command, None);
    }

    #[test]
    fn test_backward_compatibility_string_to_compactstring() {
        // Test that old JSON with String fields can be deserialized to CompactString
        let old_json = r#"{
            "id": 5,
            "command": "python train.py --lr 0.001 --epochs 100",
            "gpus": 2,
            "conda_env": "pytorch",
            "run_dir": "/home/user/work",
            "priority": 10,
            "submitted_by": "alice",
            "run_name": "training-job-5",
            "state": "Queued",
            "parameters": {
                "lr": "0.001",
                "epochs": "100",
                "batch_size": "32"
            },
            "group_id": "550e8400-e29b-41d4-a716-446655440000"
        }"#;

        let result: Result<Job, _> = serde_json::from_str(old_json);
        assert!(
            result.is_ok(),
            "Failed to deserialize JSON with string fields: {:?}",
            result.err()
        );

        let job = result.unwrap();
        assert_eq!(job.id, 5);
        assert_eq!(
            job.command.as_ref().map(|s| s.as_str()),
            Some("python train.py --lr 0.001 --epochs 100")
        );
        assert_eq!(job.conda_env.as_ref().map(|s| s.as_str()), Some("pytorch"));
        assert_eq!(job.submitted_by.as_str(), "alice");
        assert_eq!(
            job.run_name.as_ref().map(|s| s.as_str()),
            Some("training-job-5")
        );

        // Verify parameters
        assert_eq!(job.parameters.len(), 3);
        assert_eq!(job.parameters.get("lr").map(|s| s.as_str()), Some("0.001"));
        assert_eq!(
            job.parameters.get("epochs").map(|s| s.as_str()),
            Some("100")
        );
        assert_eq!(
            job.parameters.get("batch_size").map(|s| s.as_str()),
            Some("32")
        );

        // Verify group_id (now deserialized as UUID)
        assert_eq!(
            job.group_id.as_ref().map(|u| u.to_string()),
            Some("550e8400-e29b-41d4-a716-446655440000".to_string())
        );
    }

    #[test]
    fn test_compactstring_serialization_roundtrip() {
        // Test that CompactString fields serialize and deserialize correctly
        let job = JobBuilder::new()
            .command("python script.py --arg value")
            .submitted_by("testuser")
            .run_dir("/tmp/test")
            .conda_env(Some("myenv".to_string()))
            .parameters(HashMap::from([
                ("key1".to_string(), "value1".to_string()),
                ("key2".to_string(), "value2".to_string()),
            ]))
            .group_id(Some("test-group-id".to_string()))
            .build();

        // Serialize to JSON
        let json = serde_json::to_string(&job).expect("Failed to serialize");

        // Deserialize back
        let deserialized: Job = serde_json::from_str(&json).expect("Failed to deserialize");

        assert_eq!(job.command, deserialized.command);
        assert_eq!(job.submitted_by, deserialized.submitted_by);
        assert_eq!(job.conda_env, deserialized.conda_env);
        assert_eq!(job.parameters, deserialized.parameters);
        assert_eq!(job.group_id, deserialized.group_id);
    }

    #[test]
    fn test_group_id_uuid_serialization() {
        // Test UUID serialization and deserialization
        let uuid_str = "550e8400-e29b-41d4-a716-446655440000";
        let uuid = Uuid::parse_str(uuid_str).unwrap();

        let job = JobBuilder::new()
            .command("test command")
            .submitted_by("testuser")
            .run_dir("/tmp/test")
            .group_id_uuid(Some(uuid))
            .build();

        // Serialize to JSON
        let json = serde_json::to_string(&job).expect("Failed to serialize");

        // Verify it serializes as a string
        assert!(json.contains(uuid_str));

        // Deserialize back
        let deserialized: Job = serde_json::from_str(&json).expect("Failed to deserialize");

        assert_eq!(job.group_id, deserialized.group_id);
        assert_eq!(deserialized.group_id, Some(uuid));
    }

    #[test]
    fn test_group_id_backward_compatibility() {
        // Test that old JSON with string group_id can be deserialized to UUID
        let old_json = r#"{
            "id": 6,
            "gpus": 1,
            "run_dir": "/tmp",
            "priority": 10,
            "submitted_by": "test",
            "state": "Queued",
            "group_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890"
        }"#;

        let result: Result<Job, _> = serde_json::from_str(old_json);
        assert!(
            result.is_ok(),
            "Failed to deserialize old JSON with string group_id: {:?}",
            result.err()
        );

        let job = result.unwrap();
        assert_eq!(
            job.group_id.map(|u| u.to_string()),
            Some("a1b2c3d4-e5f6-7890-abcd-ef1234567890".to_string())
        );
    }
}
