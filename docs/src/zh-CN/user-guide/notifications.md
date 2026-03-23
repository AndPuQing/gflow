# 通知

gflowd 支持在任务/系统事件发生时发送 HTTP POST webhook 和 SMTP 邮件通知。发送采用尽力而为模式。

## 概览

| 类型 | 交付方式 | 配置位置 | 收件目标 | 典型用途 |
| --- | --- | --- | --- | --- |
| Webhook | HTTP POST + JSON payload | `notifications.webhooks` | 外部服务 URL | 对接告警平台、自动化工作流、审计系统 |
| 全局 Email | SMTP 邮件 | `notifications.emails` | 固定收件人 `to = [...]` | 发给值班邮箱、团队列表、个人邮箱 |
| 单任务 Email | SMTP 邮件 | `notifications.emails` + `gbatch --notify-email` | 提交时指定的任务级收件人 | 只给某个任务追加通知 |

## 共享开关

```toml
[notifications]
enabled = true
max_concurrent_deliveries = 16
```

- `enabled = true`：启用通知系统。
- `max_concurrent_deliveries`：限制所有 webhook / email 目标共享的并发投递数。

## Webhook 通知

Webhook 适合对接外部系统。每条通知会以 HTTP POST 发送 JSON payload。

```toml
[[notifications.webhooks]]
url = "https://api.example.com/gflow/events"
events = ["job_completed", "job_failed", "job_timeout"] # 或 ["*"]
filter_users = ["alice", "bob"] # 可选
headers = { Authorization = "Bearer token123" } # 可选
timeout_secs = 10
max_retries = 3
```

- `url`：接收通知的 HTTP 端点。
- `events`：订阅的事件集合，或 `["*"]`。
- `filter_users`：按任务提交者或预约创建者过滤。
- `headers`：附加 HTTP 头，常用于认证。
- `timeout_secs` / `max_retries`：单次请求超时和重试次数。

## Email 通知

Email 适合直接通知人。gflow 会基于事件数据生成邮件主题和纯文本正文。

```toml
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

- `smtp_url`：SMTP 服务器地址，使用 lettre 的 SMTP URL 语法。
- `from`：发件人。
- `to`：全局固定收件人；如果只打算用单任务收件人，可以省略。
- `events`：订阅的事件集合，或 `["*"]`。
- `filter_users`：按任务提交者或预约创建者过滤。
- `subject_prefix`：给邮件主题加统一前缀。
- `timeout_secs` / `max_retries`：单次发送超时和重试次数。

## 事件范围

| 事件 | Webhook | 全局 Email | 单任务 Email（`gbatch --notify-on`） | 说明 |
| --- | --- | --- | --- | --- |
| `job_submitted` | 支持 | 支持 | 支持 | 任务被提交 |
| `job_updated` | 支持 | 支持 | 支持 | 任务元数据被更新 |
| `job_started` | 支持 | 支持 | 支持 | 任务开始运行 |
| `job_completed` | 支持 | 支持 | 支持 | 任务成功结束 |
| `job_failed` | 支持 | 支持 | 支持 | 任务失败结束 |
| `job_cancelled` | 支持 | 支持 | 支持 | 任务被取消 |
| `job_timeout` | 支持 | 支持 | 支持 | 任务超时结束 |
| `job_held` | 支持 | 支持 | 支持 | 任务被置为 hold |
| `job_released` | 支持 | 支持 | 支持 | 任务从 hold 恢复到队列 |
| `gpu_available` | 支持 | 支持 | 不支持 | 仅在 GPU 从不可用变为可用时发送 |
| `reservation_created` | 支持 | 支持 | 不支持 | 预约被创建 |
| `reservation_cancelled` | 支持 | 支持 | 不支持 | 预约被取消 |
| `scheduler_online` | 支持 | 支持 | 不支持 | `gflowd` 启动完成 |

::: tip 怎么选
如果你要把事件交给程序处理，优先用 webhook。
如果你要直接通知人，优先用 email。
如果你想订阅 GPU、预约、守护进程启动这类系统级事件，必须使用全局 webhook 或全局 email；单任务 email 只支持任务级事件。
:::

## Webhook Payload

不同事件可能省略部分字段。

```json
{
  "event": "job_completed",
  "timestamp": "2026-02-04T12:30:45Z",
  "job": { "id": 42, "user": "alice", "state": "Finished" },
  "scheduler": { "host": "gpu-server-01", "version": "0.4.11" }
}
```

## 单任务 Email

单任务 email 会复用 `notifications.emails` 中配置的 SMTP 通道。

- 用 `gbatch --notify-email <address>` 为单个任务追加收件人。
- 用 `gbatch --notify-on <event1,event2,...>` 指定上表中的任务级触发事件。
- 如果设置了 `--notify-email` 但没有设置 `--notify-on`，gflow 默认使用 `job_completed`、`job_failed`、`job_timeout`、`job_cancelled`。
- 单任务通知只发 email，不会额外发送 webhook。

## 备注

- `events = ["*"]` 表示订阅所有支持的事件。
- `smtp_url` 可以写成 `smtps://user:pass@smtp.example.com:465`、`smtp://user:pass@smtp.example.com:587?tls=required`，或本地明文 relay 的 `smtp://localhost:25`。
- `max_retries` 使用指数退避；系统过载时可能丢弃通知。
- webhook payload 和 email 正文都可能包含任务元数据、用户名和时间信息。

## 另见

- [配置](./configuration)
- [任务提交](./job-submission)
- [gbatch 参考](../reference/gbatch-reference)
