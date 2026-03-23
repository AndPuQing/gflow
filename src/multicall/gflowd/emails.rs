use super::events::{EventBus, EventEnvelope};
use super::scheduler_runtime::SchedulerRuntime;
use super::webhooks::WebhookPayload;
use compact_str::CompactString;
use gflow::config::{EmailConfig, NotificationsConfig};
use gflow::core::job::JobNotifications;
use lettre::message::{header::ContentType, Mailbox};
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{RwLock, Semaphore};
use tracing::Instrument;

pub(crate) fn spawn_email_notifier(
    notifications: NotificationsConfig,
    semaphore: Arc<Semaphore>,
    scheduler: Arc<RwLock<SchedulerRuntime>>,
    event_bus: Arc<EventBus>,
    scheduler_host: String,
) -> Option<tokio::task::JoinHandle<()>> {
    if !notifications.enabled || notifications.emails.is_empty() {
        return None;
    }

    let targets = match EmailTargets::try_from_config(&notifications.emails) {
        Ok(targets) => targets,
        Err(e) => {
            tracing::error!("Email notifier disabled due to invalid config: {e}");
            return None;
        }
    };

    let concurrency = notifications.max_concurrent_deliveries.max(1);

    tracing::info!(
        "Email notifier enabled: {} target(s), max_concurrent_deliveries={}",
        targets.len(),
        concurrency
    );

    let rx = event_bus.subscribe();

    Some(tokio::spawn(async move {
        run_email_notifier(targets, semaphore, scheduler, rx, scheduler_host).await;
    }))
}

#[derive(Clone)]
struct EmailTarget {
    mailer: Arc<AsyncSmtpTransport<Tokio1Executor>>,
    from: Mailbox,
    default_to: Vec<Mailbox>,
    matcher: EventMatcher,
    filter_users: Option<HashSet<String>>,
    subject_prefix: Option<String>,
    max_retries: u32,
}

#[derive(Clone)]
struct EmailTargets(Vec<EmailTarget>);

impl EmailTargets {
    fn len(&self) -> usize {
        self.0.len()
    }

    fn iter(&self) -> impl Iterator<Item = &EmailTarget> {
        self.0.iter()
    }

    fn try_from_config(emails: &[EmailConfig]) -> anyhow::Result<Self> {
        let mut targets = Vec::with_capacity(emails.len());
        for email in emails {
            let smtp_url = email.smtp_url.trim();
            if smtp_url.is_empty() {
                anyhow::bail!("email smtp_url cannot be empty");
            }

            let from =
                email.from.trim().parse::<Mailbox>().map_err(|e| {
                    anyhow::anyhow!("invalid email from address '{}': {e}", email.from)
                })?;

            let mut to = Vec::with_capacity(email.to.len());
            for recipient in &email.to {
                let trimmed = recipient.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let mailbox = trimmed
                    .parse::<Mailbox>()
                    .map_err(|e| anyhow::anyhow!("invalid email recipient '{}': {e}", recipient))?;
                to.push(mailbox);
            }
            let matcher = EventMatcher::from_events(&email.events);
            let filter_users = email.filter_users.as_ref().and_then(|users| {
                let set: HashSet<String> = users
                    .iter()
                    .map(|u| u.trim())
                    .filter(|u| !u.is_empty())
                    .map(|u| u.to_lowercase())
                    .collect();
                if set.is_empty() {
                    None
                } else {
                    Some(set)
                }
            });

            let subject_prefix = email
                .subject_prefix
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);

            let mailer = AsyncSmtpTransport::<Tokio1Executor>::from_url(smtp_url)
                .map_err(|e| anyhow::anyhow!("invalid smtp_url '{}': {e}", email.smtp_url))?
                .timeout(Some(Duration::from_secs(email.timeout_secs.max(1))))
                .build();

            targets.push(EmailTarget {
                mailer: Arc::new(mailer),
                from,
                default_to: to,
                matcher,
                filter_users,
                subject_prefix,
                max_retries: email.max_retries,
            });
        }

        Ok(Self(targets))
    }
}

#[derive(Clone, Debug)]
enum EventMatcher {
    All,
    Set(HashSet<String>),
}

impl EventMatcher {
    fn from_events(events: &[String]) -> Self {
        if events.iter().any(|e| e.trim() == "*") {
            return Self::All;
        }

        let set: HashSet<String> = events
            .iter()
            .map(|e| e.trim())
            .filter(|e| !e.is_empty())
            .map(|e| e.to_lowercase())
            .collect();

        if set.is_empty() {
            Self::All
        } else {
            Self::Set(set)
        }
    }

    fn matches(&self, event: &str) -> bool {
        let event = event.to_lowercase();
        match self {
            Self::All => true,
            Self::Set(set) => set.contains(&event),
        }
    }

    fn from_compact_strings(events: &[CompactString]) -> Self {
        if events.is_empty() {
            return Self::Set(
                [
                    "job_completed",
                    "job_failed",
                    "job_timeout",
                    "job_cancelled",
                ]
                .into_iter()
                .map(str::to_string)
                .collect(),
            );
        }
        if events.iter().any(|event| event.as_str() == "*") {
            return Self::All;
        }
        let set: HashSet<String> = events
            .iter()
            .map(|event| event.as_str().to_lowercase())
            .collect();
        Self::Set(set)
    }
}

impl EmailTarget {
    fn user_allowed(&self, payload: &WebhookPayload) -> bool {
        if let Some(ref allowed_users) = self.filter_users {
            let Some(user) = payload.user_for_filtering() else {
                return false;
            };
            allowed_users.contains(&user.to_lowercase())
        } else {
            true
        }
    }
}

async fn run_email_notifier(
    targets: EmailTargets,
    semaphore: Arc<Semaphore>,
    scheduler: Arc<RwLock<SchedulerRuntime>>,
    mut rx: tokio::sync::broadcast::Receiver<EventEnvelope>,
    scheduler_host: String,
) {
    loop {
        let event = match rx.recv().await {
            Ok(event) => event,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                tracing::warn!(skipped, "Email notifier lagged behind");
                continue;
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                tracing::info!("Email notifier stopping: event bus closed");
                break;
            }
        };

        let handling_span = event.handling_span("email_notifier");
        let _entered = handling_span.enter();
        let payloads =
            super::webhooks::build_payloads(&scheduler, &scheduler_host, &event.event).await;
        if payloads.is_empty() {
            continue;
        }

        for payload in &payloads {
            let job_notifications = per_job_notifications(&scheduler, payload).await;
            for target in targets.iter() {
                let recipients = resolve_recipients(target, payload, job_notifications.as_ref());
                if recipients.is_empty() {
                    continue;
                }

                let permit = match semaphore.clone().acquire_owned().await {
                    Ok(permit) => permit,
                    Err(_) => return,
                };

                let target = target.clone();
                let payload = payload.clone();
                let recipient_summary = summarize_recipients(&recipients);
                let delivery_span = tracing::info_span!(
                    "email_delivery",
                    event = %payload.event,
                    recipients = %recipient_summary
                );

                tokio::spawn(
                    async move {
                        let _permit = permit;
                        if let Err(e) = deliver_with_retries(&target, &payload, &recipients).await {
                            tracing::warn!(
                                event = %payload.event,
                                recipients = %recipient_summary,
                                error = %e,
                                "Email delivery failed"
                            );
                        }
                    }
                    .instrument(delivery_span),
                );
            }
        }
    }
}

async fn deliver_with_retries(
    target: &EmailTarget,
    payload: &WebhookPayload,
    recipients: &[Mailbox],
) -> anyhow::Result<()> {
    let mut attempt: u32 = 0;
    let max_attempts = 1u32.saturating_add(target.max_retries);

    loop {
        attempt += 1;
        match deliver_once(target, payload, recipients).await {
            Ok(()) => return Ok(()),
            Err(e) if attempt < max_attempts => {
                let delay = backoff_delay(attempt);
                tracing::debug!(
                    attempt,
                    max_attempts,
                    error = %e,
                    retry_delay_secs = delay.as_secs(),
                    "Email delivery attempt failed; retrying"
                );
                tokio::time::sleep(delay).await;
            }
            Err(e) => return Err(e),
        }
    }
}

fn backoff_delay(attempt: u32) -> Duration {
    let secs = 2u64.saturating_pow(attempt.saturating_sub(1).min(5));
    Duration::from_secs(secs.clamp(1, 30))
}

async fn deliver_once(
    target: &EmailTarget,
    payload: &WebhookPayload,
    recipients: &[Mailbox],
) -> anyhow::Result<()> {
    let message = build_message(target, payload, recipients)?;
    target.mailer.send(message).await?;
    Ok(())
}

fn build_message(
    target: &EmailTarget,
    payload: &WebhookPayload,
    recipients: &[Mailbox],
) -> anyhow::Result<Message> {
    let mut builder = Message::builder().from(target.from.clone());
    for recipient in recipients {
        builder = builder.to(recipient.clone());
    }

    builder
        .subject(build_subject(payload, target.subject_prefix.as_deref()))
        .header(ContentType::TEXT_PLAIN)
        .body(build_body(payload))
        .map_err(Into::into)
}

fn build_subject(payload: &WebhookPayload, subject_prefix: Option<&str>) -> String {
    let base = if let Some(job) = &payload.job {
        let mut subject = format!("{}: job #{}", humanize_event(&payload.event), job.id);
        if let Some(name) = job.name.as_deref() {
            subject.push_str(" (");
            subject.push_str(name);
            subject.push(')');
        }
        subject
    } else if let Some(reservation) = &payload.reservation {
        format!(
            "{}: reservation #{}",
            humanize_event(&payload.event),
            reservation.id
        )
    } else if let Some(gpu) = &payload.gpu {
        format!("{}: GPU {}", humanize_event(&payload.event), gpu.index)
    } else {
        humanize_event(&payload.event)
    };

    match subject_prefix.map(str::trim).filter(|s| !s.is_empty()) {
        Some(prefix) => format!("{prefix} {base}"),
        None => base,
    }
}

fn build_body(payload: &WebhookPayload) -> String {
    let mut lines = vec![
        format!("Event: {}", payload.event),
        format!("Time: {}", payload.timestamp),
        format!(
            "Scheduler: {} ({})",
            payload.scheduler.host, payload.scheduler.version
        ),
    ];

    if let Some(text) = payload.text.as_deref() {
        lines.push(String::new());
        lines.push(text.to_string());
    }

    if let Some(job) = &payload.job {
        lines.push(String::new());
        lines.push(format!("Job ID: {}", job.id));
        push_optional_line(&mut lines, "Job Name", job.name.as_deref());
        push_optional_line(&mut lines, "User", job.user.as_deref());
        push_optional_line(&mut lines, "State", job.state.as_deref());
        push_optional_line(&mut lines, "Runtime", job.runtime.as_deref());
        if let Some(gpus) = &job.gpus {
            lines.push(format!(
                "GPUs: {}",
                gpus.iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        push_optional_line(&mut lines, "Submitted At", job.submitted_at.as_deref());
        push_optional_line(&mut lines, "Started At", job.started_at.as_deref());
        push_optional_line(&mut lines, "Finished At", job.finished_at.as_deref());
        push_optional_line(&mut lines, "Reason", job.reason.as_deref());
    }

    if let Some(reservation) = &payload.reservation {
        lines.push(String::new());
        lines.push(format!("Reservation ID: {}", reservation.id));
        lines.push(format!("User: {}", reservation.user));
        lines.push(format!("GPU Count: {}", reservation.gpu_count));
        if let Some(indices) = &reservation.gpu_indices {
            lines.push(format!(
                "GPU Indices: {}",
                indices
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        lines.push(format!("Start Time: {}", reservation.start_time));
        lines.push(format!("End Time: {}", reservation.end_time));
        lines.push(format!("Status: {}", reservation.status));
        lines.push(format!("Created At: {}", reservation.created_at));
        push_optional_line(
            &mut lines,
            "Cancelled At",
            reservation.cancelled_at.as_deref(),
        );
    }

    if let Some(gpu) = &payload.gpu {
        lines.push(String::new());
        lines.push(format!("GPU Index: {}", gpu.index));
        lines.push(format!("Available: {}", gpu.available));
    }

    lines.join("\n")
}

fn push_optional_line(lines: &mut Vec<String>, label: &str, value: Option<&str>) {
    if let Some(value) = value {
        lines.push(format!("{label}: {value}"));
    }
}

fn humanize_event(event: &str) -> String {
    let humanized = event.replace('_', " ");
    let mut chars = humanized.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => "Notification".to_string(),
    }
}

fn resolve_recipients(
    target: &EmailTarget,
    payload: &WebhookPayload,
    job_notifications: Option<&JobNotifications>,
) -> Vec<Mailbox> {
    let mut recipients = Vec::new();

    if !target.user_allowed(payload) {
        return recipients;
    }

    if target.matcher.matches(&payload.event) {
        append_unique_mailboxes(&mut recipients, &target.default_to);
    }

    if let Some(job_notifications) = job_notifications {
        if EventMatcher::from_compact_strings(&job_notifications.events).matches(&payload.event) {
            match parse_mailboxes(job_notifications.emails.iter()) {
                Ok(mailboxes) => append_unique_mailboxes(&mut recipients, &mailboxes),
                Err(error) => {
                    tracing::warn!(
                        event = %payload.event,
                        error = %error,
                        "Skipping invalid per-job email recipients"
                    );
                }
            }
        }
    }

    recipients
}

fn append_unique_mailboxes(target: &mut Vec<Mailbox>, additional: &[Mailbox]) {
    for mailbox in additional {
        if !target.iter().any(|existing| existing == mailbox) {
            target.push(mailbox.clone());
        }
    }
}

fn summarize_recipients(recipients: &[Mailbox]) -> String {
    recipients
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

async fn per_job_notifications(
    scheduler: &Arc<RwLock<SchedulerRuntime>>,
    payload: &WebhookPayload,
) -> Option<JobNotifications> {
    let job_id = payload.job.as_ref().map(|job| job.id)?;
    let job = scheduler.read().await.get_job(job_id)?;
    if job.notifications.is_empty() {
        None
    } else {
        Some(job.notifications)
    }
}

fn parse_mailboxes<'a>(
    values: impl IntoIterator<Item = &'a CompactString>,
) -> anyhow::Result<Vec<Mailbox>> {
    let mut out = Vec::new();
    for value in values {
        let mailbox = value
            .as_str()
            .parse::<Mailbox>()
            .map_err(|e| anyhow::anyhow!("invalid email recipient '{}': {e}", value))?;
        out.push(mailbox);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::multicall::gflowd::webhooks::{
        GpuPayload, JobPayload, ReservationPayload, SchedulerInfoPayload,
    };
    use gflow::core::job::JobNotifications;

    #[test]
    fn test_email_target_validation() {
        let targets = EmailTargets::try_from_config(&[EmailConfig {
            smtp_url: "smtp://127.0.0.1:2525".to_string(),
            from: "gflow <noreply@example.com>".to_string(),
            to: vec!["alice@example.com".to_string()],
            events: vec!["job_completed".to_string()],
            filter_users: None,
            subject_prefix: Some("[gflow]".to_string()),
            timeout_secs: 10,
            max_retries: 2,
        }])
        .unwrap();

        assert_eq!(targets.len(), 1);
    }

    #[test]
    fn test_email_target_allows_transport_without_default_recipient() {
        let result = EmailTargets::try_from_config(&[EmailConfig {
            smtp_url: "smtp://127.0.0.1:2525".to_string(),
            from: "noreply@example.com".to_string(),
            to: vec![],
            events: vec!["*".to_string()],
            filter_users: None,
            subject_prefix: None,
            timeout_secs: 10,
            max_retries: 0,
        }])
        .unwrap();

        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_build_subject_and_body_for_job_payload() {
        let payload = WebhookPayload {
            event: "job_completed".to_string(),
            timestamp: "2026-03-23T12:00:00Z".to_string(),
            scheduler: SchedulerInfoPayload {
                host: "gpu-server-01".to_string(),
                version: "0.4.14".to_string(),
            },
            text: None,
            job: Some(JobPayload {
                id: 42,
                name: Some("train.py".to_string()),
                user: Some("alice".to_string()),
                state: Some("Finished".to_string()),
                runtime: Some("3h 12m".to_string()),
                gpus: Some(vec![0, 1]),
                submitted_at: Some("2026-03-23T08:00:00Z".to_string()),
                started_at: Some("2026-03-23T08:01:00Z".to_string()),
                finished_at: Some("2026-03-23T11:13:00Z".to_string()),
                reason: None,
            }),
            reservation: None,
            gpu: None,
        };

        let subject = build_subject(&payload, Some("[prod]"));
        let body = build_body(&payload);

        assert_eq!(subject, "[prod] Job completed: job #42 (train.py)");
        assert!(body.contains("Event: job_completed"));
        assert!(body.contains("Job ID: 42"));
        assert!(body.contains("User: alice"));
        assert!(body.contains("GPUs: 0, 1"));
    }

    #[test]
    fn test_build_subject_for_non_job_payloads() {
        let reservation_payload = WebhookPayload {
            event: "reservation_created".to_string(),
            timestamp: "2026-03-23T12:00:00Z".to_string(),
            scheduler: SchedulerInfoPayload {
                host: "gpu-server-01".to_string(),
                version: "0.4.14".to_string(),
            },
            text: None,
            job: None,
            reservation: Some(ReservationPayload {
                id: 7,
                user: "alice".to_string(),
                gpu_count: 2,
                gpu_indices: Some(vec![0, 1]),
                start_time: "2026-03-23T12:00:00Z".to_string(),
                end_time: "2026-03-23T14:00:00Z".to_string(),
                status: "Active".to_string(),
                created_at: "2026-03-23T11:59:00Z".to_string(),
                cancelled_at: None,
            }),
            gpu: None,
        };
        let gpu_payload = WebhookPayload {
            event: "gpu_available".to_string(),
            timestamp: "2026-03-23T12:00:00Z".to_string(),
            scheduler: SchedulerInfoPayload {
                host: "gpu-server-01".to_string(),
                version: "0.4.14".to_string(),
            },
            text: None,
            job: None,
            reservation: None,
            gpu: Some(GpuPayload {
                index: 3,
                available: true,
            }),
        };

        assert_eq!(
            build_subject(&reservation_payload, None),
            "Reservation created: reservation #7"
        );
        assert_eq!(build_subject(&gpu_payload, None), "Gpu available: GPU 3");
    }

    #[test]
    fn test_resolve_recipients_merges_default_and_per_job_notifications() {
        let target = EmailTarget {
            mailer: Arc::new(
                AsyncSmtpTransport::<Tokio1Executor>::from_url("smtp://127.0.0.1:2525")
                    .unwrap()
                    .build(),
            ),
            from: "gflow <noreply@example.com>".parse().unwrap(),
            default_to: vec!["ops@example.com".parse().unwrap()],
            matcher: EventMatcher::from_events(&["job_failed".to_string()]),
            filter_users: None,
            subject_prefix: None,
            max_retries: 0,
        };
        let payload = WebhookPayload {
            event: "job_failed".to_string(),
            timestamp: "2026-03-23T12:00:00Z".to_string(),
            scheduler: SchedulerInfoPayload {
                host: "gpu-server-01".to_string(),
                version: "0.4.14".to_string(),
            },
            text: None,
            job: Some(JobPayload {
                id: 42,
                name: Some("train.py".to_string()),
                user: Some("alice".to_string()),
                state: Some("Failed".to_string()),
                runtime: None,
                gpus: None,
                submitted_at: None,
                started_at: None,
                finished_at: None,
                reason: Some("OOM".to_string()),
            }),
            reservation: None,
            gpu: None,
        };
        let job_notifications = JobNotifications::normalized(
            vec![
                "alice@example.com".to_string(),
                "ops@example.com".to_string(),
            ],
            vec!["job_failed".to_string()],
        );

        let recipients = resolve_recipients(&target, &payload, Some(&job_notifications));

        assert_eq!(recipients.len(), 2);
        assert!(recipients
            .iter()
            .any(|mailbox| mailbox.to_string() == "ops@example.com"));
        assert!(recipients
            .iter()
            .any(|mailbox| mailbox.to_string() == "alice@example.com"));
    }

    #[test]
    fn test_resolve_recipients_respects_filter_users_for_per_job_notifications() {
        let target = EmailTarget {
            mailer: Arc::new(
                AsyncSmtpTransport::<Tokio1Executor>::from_url("smtp://127.0.0.1:2525")
                    .unwrap()
                    .build(),
            ),
            from: "gflow <noreply@example.com>".parse().unwrap(),
            default_to: vec!["ops@example.com".parse().unwrap()],
            matcher: EventMatcher::from_events(&["job_failed".to_string()]),
            filter_users: Some(["bob".to_string()].into_iter().collect()),
            subject_prefix: None,
            max_retries: 0,
        };
        let payload = WebhookPayload {
            event: "job_failed".to_string(),
            timestamp: "2026-03-23T12:00:00Z".to_string(),
            scheduler: SchedulerInfoPayload {
                host: "gpu-server-01".to_string(),
                version: "0.4.14".to_string(),
            },
            text: None,
            job: Some(JobPayload {
                id: 42,
                name: Some("train.py".to_string()),
                user: Some("alice".to_string()),
                state: Some("Failed".to_string()),
                runtime: None,
                gpus: None,
                submitted_at: None,
                started_at: None,
                finished_at: None,
                reason: Some("OOM".to_string()),
            }),
            reservation: None,
            gpu: None,
        };
        let job_notifications = JobNotifications::normalized(
            vec!["alice@example.com".to_string()],
            vec!["job_failed".to_string()],
        );

        let recipients = resolve_recipients(&target, &payload, Some(&job_notifications));

        assert!(recipients.is_empty());
    }
}
