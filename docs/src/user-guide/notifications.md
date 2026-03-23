# Notifications

gflowd can send HTTP POST webhooks and SMTP email notifications for job and system events. Delivery is best-effort.

## Overview

| Type | Delivery | Configuration | Target | Typical use |
| --- | --- | --- | --- | --- |
| Webhook | HTTP POST + JSON payload | `notifications.webhooks` | External service URL | Alerting systems, automation, audit hooks |
| Global Email | SMTP email | `notifications.emails` | Fixed recipients in `to = [...]` | Team inboxes, on-call aliases, personal mailboxes |
| Per-Job Email | SMTP email | `notifications.emails` + `gbatch --notify-email` | Recipients chosen at submission time | Extra notifications for one job |

## Shared Switches

```toml
[notifications]
enabled = true
max_concurrent_deliveries = 16
```

- `enabled = true` turns on the notification system.
- `max_concurrent_deliveries` limits delivery concurrency shared across all webhook and email targets.

## Webhook Notifications

Webhooks are for machine-to-machine delivery. Each notification is sent as an HTTP POST with a JSON payload.

```toml
[[notifications.webhooks]]
url = "https://api.example.com/gflow/events"
events = ["job_completed", "job_failed", "job_timeout"] # or ["*"]
filter_users = ["alice", "bob"] # optional
headers = { Authorization = "Bearer token123" } # optional
timeout_secs = 10
max_retries = 3
```

- `url` is the HTTP endpoint that receives notifications.
- `events` selects subscribed events, or `["*"]`.
- `filter_users` filters by job submitter or reservation owner.
- `headers` adds HTTP headers, usually for authentication.
- `timeout_secs` and `max_retries` control request timeout and retry count.

## Email Notifications

Email is for people. gflow builds the subject line and plain-text body from the event data.

```toml
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

- `smtp_url` is the SMTP server URL using lettre's SMTP URL syntax.
- `from` sets the sender address.
- `to` sets global fixed recipients; omit it if you only plan to use per-job recipients.
- `events` selects subscribed events, or `["*"]`.
- `filter_users` filters by job submitter or reservation owner.
- `subject_prefix` prepends a shared prefix to mail subjects.
- `timeout_secs` and `max_retries` control send timeout and retry count.

## Event Scope

| Event | Webhook | Global Email | Per-Job Email (`gbatch --notify-on`) | Notes |
| --- | --- | --- | --- | --- |
| `job_submitted` | Supported | Supported | Supported | Job was submitted |
| `job_updated` | Supported | Supported | Supported | Job metadata changed |
| `job_started` | Supported | Supported | Supported | Job entered running state |
| `job_completed` | Supported | Supported | Supported | Job finished successfully |
| `job_failed` | Supported | Supported | Supported | Job finished with failure |
| `job_cancelled` | Supported | Supported | Supported | Job was cancelled |
| `job_timeout` | Supported | Supported | Supported | Job hit its time limit |
| `job_held` | Supported | Supported | Supported | Job was moved to hold |
| `job_released` | Supported | Supported | Supported | Job was released from hold back to queue |
| `gpu_available` | Supported | Supported | Not supported | Only emitted when a GPU becomes available again |
| `reservation_created` | Supported | Supported | Not supported | Reservation was created |
| `reservation_cancelled` | Supported | Supported | Not supported | Reservation was cancelled |
| `scheduler_online` | Supported | Supported | Not supported | `gflowd` finished starting up |

::: tip Choosing between webhook and email
Choose webhook when another system should process the event.
Choose email when a person should read it directly.
For system-level events such as GPU availability, reservations, or daemon startup, use global webhook or global email; per-job email only supports job-scoped events.
:::

## Webhook Payload

Fields may be omitted depending on the event.

```json
{
  "event": "job_completed",
  "timestamp": "2026-02-04T12:30:45Z",
  "job": { "id": 42, "user": "alice", "state": "Finished" },
  "scheduler": { "host": "gpu-server-01", "version": "0.4.11" }
}
```

## Per-Job Email

Per-job email reuses the SMTP transports configured in `notifications.emails`.

- Use `gbatch --notify-email <address>` to add recipients for one job.
- Use `gbatch --notify-on <event1,event2,...>` to choose job-scoped triggering events from the table above.
- If `--notify-email` is set without `--notify-on`, gflow defaults to `job_completed`, `job_failed`, `job_timeout`, and `job_cancelled`.
- Per-job notifications send email only; they do not produce webhook deliveries.

## Notes

- `events = ["*"]` subscribes to all supported events.
- `smtp_url` can look like `smtps://user:pass@smtp.example.com:465`, `smtp://user:pass@smtp.example.com:587?tls=required`, or `smtp://localhost:25`.
- `max_retries` uses exponential backoff; deliveries may be skipped if the daemon is overloaded.
- Webhook payloads and email bodies can include job metadata, usernames, and timing information.

## See Also

- [Configuration](./configuration)
- [Job Submission](./job-submission)
- [gbatch Reference](../reference/gbatch-reference)
