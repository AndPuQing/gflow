# Notifications

gflowd can send HTTP POST webhooks and SMTP email notifications for job and system events. Delivery is best-effort.

## Global Configuration

```toml
[notifications]
enabled = true
max_concurrent_deliveries = 16

[[notifications.webhooks]]
url = "https://api.example.com/gflow/events"
events = ["job_completed", "job_failed", "job_timeout"] # or ["*"]
filter_users = ["alice", "bob"] # optional
headers = { Authorization = "Bearer token123" } # optional
timeout_secs = 10
max_retries = 3

[[notifications.emails]]
smtp_url = "smtps://user:pass@smtp.example.com:465"
from = "gflow <noreply@example.com>"
to = ["alice@example.com", "ml-oncall@example.com"] # optional if only using per-job recipients
events = ["job_failed", "job_timeout"] # or ["*"]
filter_users = ["alice"] # optional
subject_prefix = "[gflow-prod]" # optional
timeout_secs = 10
max_retries = 3
```

## Events

- `job_submitted`
- `job_updated`
- `job_started`
- `job_completed`
- `job_failed`
- `job_cancelled`
- `job_timeout`
- `job_held`
- `job_released`
- `gpu_available` (only when a GPU becomes available)
- `reservation_created`
- `reservation_cancelled`
- `scheduler_online`

## Payload

Fields may be omitted depending on the event.

```json
{
  "event": "job_completed",
  "timestamp": "2026-02-04T12:30:45Z",
  "job": { "id": 42, "user": "alice", "state": "Finished" },
  "scheduler": { "host": "gpu-server-01", "version": "0.4.11" }
}
```

## Per-Job Email Recipients

Per-job notifications reuse the SMTP transports configured in `notifications.emails`.

- Use `gbatch --notify-email <address>` to add recipients for one job.
- Use `gbatch --notify-on <event1,event2,...>` to choose triggering events.
- If `--notify-email` is set without `--notify-on`, gflow defaults to `job_completed`, `job_failed`, `job_timeout`, and `job_cancelled`.

## Notes

- `events = ["*"]` subscribes to all supported events.
- Use `filter_users` to restrict notifications by job submitter or reservation owner.
- `smtp_url` follows lettre's SMTP URL syntax, for example `smtps://user:pass@smtp.example.com:465`, `smtp://user:pass@smtp.example.com:587?tls=required`, or `smtp://localhost:25`.
- `to` may be omitted if recipients are always provided per job.
- `subject_prefix` is only used for email notifications.
- `max_retries` uses exponential backoff; deliveries may be skipped if the daemon is overloaded.
- Notification payloads can include job metadata, usernames, and timing information.

## See Also

- [Configuration](./configuration)
- [Job Submission](./job-submission)
- [gbatch Reference](../reference/gbatch-reference)
