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

## Event Scope

| Event | Global notifications (webhook / email) | Per-job email (`gbatch --notify-on`) | Notes |
| --- | --- | --- | --- |
| `job_submitted` | Supported | Supported | Job was submitted |
| `job_updated` | Supported | Supported | Job metadata changed |
| `job_started` | Supported | Supported | Job entered running state |
| `job_completed` | Supported | Supported | Job finished successfully |
| `job_failed` | Supported | Supported | Job finished with failure |
| `job_cancelled` | Supported | Supported | Job was cancelled |
| `job_timeout` | Supported | Supported | Job hit its time limit |
| `job_held` | Supported | Supported | Job was moved to hold |
| `job_released` | Supported | Supported | Job was released from hold back to queue |
| `gpu_available` | Supported | Not supported | Only emitted when a GPU becomes available again |
| `reservation_created` | Supported | Not supported | Reservation was created |
| `reservation_cancelled` | Supported | Not supported | Reservation was cancelled |
| `scheduler_online` | Supported | Not supported | `gflowd` finished starting up |

::: tip Choosing between global and per-job notifications
Use global notifications for system-level events such as GPU availability, reservations, or daemon startup.
Use `--notify-email` and `--notify-on` when you only want extra email delivery for one job; per-job notifications only match the job-scoped events in the table above.
:::

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

## Per-Job Email Notifications

Per-job notifications reuse the SMTP transports configured in `notifications.emails`.

- Use `gbatch --notify-email <address>` to add recipients for one job.
- Use `gbatch --notify-on <event1,event2,...>` to choose job-scoped triggering events from the table above.
- If `--notify-email` is set without `--notify-on`, gflow defaults to `job_completed`, `job_failed`, `job_timeout`, and `job_cancelled`.
- Per-job notifications send email only; they do not produce webhook deliveries.

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
