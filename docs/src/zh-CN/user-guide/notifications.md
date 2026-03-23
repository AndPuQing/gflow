# 通知

gflowd 支持在任务/系统事件发生时发送 HTTP POST webhook 和 SMTP 邮件通知。发送采用尽力而为模式。

## 全局配置

```toml
[notifications]
enabled = true
max_concurrent_deliveries = 16

[[notifications.webhooks]]
url = "https://api.example.com/gflow/events"
events = ["job_completed", "job_failed", "job_timeout"] # 或 ["*"]
filter_users = ["alice", "bob"] # 可选
headers = { Authorization = "Bearer token123" } # 可选
timeout_secs = 10
max_retries = 3

[[notifications.emails]]
smtp_url = "smtps://user:pass@smtp.example.com:465"
from = "gflow <noreply@example.com>"
to = ["alice@example.com", "ml-oncall@example.com"] # 仅使用 per-job 收件人时可省略
events = ["job_failed", "job_timeout"] # 或 ["*"]
filter_users = ["alice"] # 可选
subject_prefix = "[gflow-prod]" # 可选
timeout_secs = 10
max_retries = 3
```

## 事件范围

| 事件 | 全局通知（webhook / email） | 单任务邮件（`gbatch --notify-on`） | 说明 |
| --- | --- | --- | --- |
| `job_submitted` | 支持 | 支持 | 任务被提交 |
| `job_updated` | 支持 | 支持 | 任务元数据被更新 |
| `job_started` | 支持 | 支持 | 任务开始运行 |
| `job_completed` | 支持 | 支持 | 任务成功结束 |
| `job_failed` | 支持 | 支持 | 任务失败结束 |
| `job_cancelled` | 支持 | 支持 | 任务被取消 |
| `job_timeout` | 支持 | 支持 | 任务超时结束 |
| `job_held` | 支持 | 支持 | 任务被置为 hold |
| `job_released` | 支持 | 支持 | 任务从 hold 恢复到队列 |
| `gpu_available` | 支持 | 不支持 | 仅在 GPU 从不可用变为可用时发送 |
| `reservation_created` | 支持 | 不支持 | 预约被创建 |
| `reservation_cancelled` | 支持 | 不支持 | 预约被取消 |
| `scheduler_online` | 支持 | 不支持 | `gflowd` 启动完成 |

::: tip 怎么选
如果你想订阅 GPU、预约、守护进程启动这类系统级事件，只能用全局通知。
如果你只想给某一个任务额外发邮件，用 `--notify-email` / `--notify-on` 即可，但它只会匹配上表里的任务级事件。
:::

## Payload

不同事件可能省略部分字段。

```json
{
  "event": "job_completed",
  "timestamp": "2026-02-04T12:30:45Z",
  "job": { "id": 42, "user": "alice", "state": "Finished" },
  "scheduler": { "host": "gpu-server-01", "version": "0.4.11" }
}
```

## 单任务邮件通知

单任务通知会复用 `notifications.emails` 中配置的 SMTP 通道。

- 用 `gbatch --notify-email <address>` 为单个任务追加收件人。
- 用 `gbatch --notify-on <event1,event2,...>` 指定上表中的任务级触发事件。
- 如果设置了 `--notify-email` 但没有设置 `--notify-on`，gflow 默认使用 `job_completed`、`job_failed`、`job_timeout`、`job_cancelled`。
- 单任务通知只发邮件，不会额外发送 webhook。

## 备注

- `events = ["*"]` 表示订阅所有支持的事件。
- `filter_users` 用于按任务提交者或预约创建者过滤通知。
- `smtp_url` 使用 lettre 的 SMTP URL 语法，例如 `smtps://user:pass@smtp.example.com:465`、`smtp://user:pass@smtp.example.com:587?tls=required`，或本地明文 relay 的 `smtp://localhost:25`。
- 如果收件人总是按任务提供，则 `to` 可以省略。
- `subject_prefix` 仅用于邮件通知主题前缀。
- `max_retries` 使用指数退避；系统过载时可能丢弃通知。
- 通知内容可能包含任务元数据、用户名和时间信息。

## 另见

- [配置](./configuration)
- [任务提交](./job-submission)
- [gbatch 参考](../reference/gbatch-reference)
