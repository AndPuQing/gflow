use crate::events::{EventBus, SchedulerEvent};
use crate::scheduler_runtime::SchedulerRuntime;
use gflow::config::{NotificationsConfig, WebhookConfig};
use gflow::core::job::{Job, JobState};
use gflow::core::reservation::GpuReservation;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{RwLock, Semaphore};

pub(crate) fn spawn_webhook_notifier(
    notifications: NotificationsConfig,
    scheduler: Arc<RwLock<SchedulerRuntime>>,
    event_bus: Arc<EventBus>,
    scheduler_host: String,
) -> Option<tokio::task::JoinHandle<()>> {
    if !notifications.enabled || notifications.webhooks.is_empty() {
        return None;
    }

    gflow::tls::ensure_rustls_provider_installed();

    let targets = match WebhookTargets::try_from_config(&notifications.webhooks) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("Webhook notifier disabled due to invalid config: {e}");
            return None;
        }
    };

    let concurrency = notifications.max_concurrent_deliveries.max(1);
    let semaphore = Arc::new(Semaphore::new(concurrency));

    let client = match reqwest::Client::builder()
        .user_agent(format!("gflow/{}/webhooks", env!("CARGO_PKG_VERSION")))
        .build()
    {
        Ok(c) => Arc::new(c),
        Err(e) => {
            tracing::error!("Webhook notifier disabled: failed to build HTTP client: {e}");
            return None;
        }
    };

    tracing::info!(
        "Webhook notifier enabled: {} target(s), max_concurrent_deliveries={}",
        targets.len(),
        concurrency
    );

    Some(tokio::spawn(async move {
        run_webhook_notifier(
            targets,
            client,
            semaphore,
            scheduler,
            event_bus,
            scheduler_host,
        )
        .await;
    }))
}

#[derive(Clone)]
struct WebhookTarget {
    url: String,
    matcher: EventMatcher,
    filter_users: Option<HashSet<String>>,
    headers: HashMap<String, String>,
    timeout: Duration,
    max_retries: u32,
}

#[derive(Clone)]
struct WebhookTargets(Vec<WebhookTarget>);

impl WebhookTargets {
    fn len(&self) -> usize {
        self.0.len()
    }

    fn iter(&self) -> impl Iterator<Item = &WebhookTarget> {
        self.0.iter()
    }

    fn try_from_config(webhooks: &[WebhookConfig]) -> anyhow::Result<Self> {
        let mut targets = Vec::with_capacity(webhooks.len());
        for w in webhooks {
            let url = w.url.trim();
            if url.is_empty() {
                anyhow::bail!("webhook url cannot be empty");
            }

            let matcher = EventMatcher::from_events(&w.events);
            let filter_users = w.filter_users.as_ref().and_then(|users| {
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

            targets.push(WebhookTarget {
                url: url.to_string(),
                matcher,
                filter_users,
                headers: w.headers.clone(),
                timeout: Duration::from_secs(w.timeout_secs.max(1)),
                max_retries: w.max_retries,
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
}

async fn run_webhook_notifier(
    targets: WebhookTargets,
    client: Arc<reqwest::Client>,
    semaphore: Arc<Semaphore>,
    scheduler: Arc<RwLock<SchedulerRuntime>>,
    event_bus: Arc<EventBus>,
    scheduler_host: String,
) {
    let mut rx = event_bus.subscribe();

    loop {
        let event = match rx.recv().await {
            Ok(e) => e,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                tracing::warn!("Webhook notifier lagged behind; skipped {skipped} event(s)");
                continue;
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                tracing::info!("Webhook notifier stopping: event bus closed");
                break;
            }
        };

        let payloads = build_payloads(&scheduler, &scheduler_host, &event).await;
        if payloads.is_empty() {
            continue;
        }

        for payload in &payloads {
            for target in targets.iter() {
                if !target.matcher.matches(&payload.event) {
                    continue;
                }

                if let Some(ref allowed_users) = target.filter_users {
                    let Some(user) = payload.user_for_filtering() else {
                        continue;
                    };
                    if !allowed_users.contains(&user.to_lowercase()) {
                        continue;
                    }
                }

                let permit = match semaphore.clone().acquire_owned().await {
                    Ok(p) => p,
                    Err(_) => return,
                };

                let client = Arc::clone(&client);
                let target = target.clone();
                let payload = payload.clone();
                tokio::spawn(async move {
                    let _permit = permit;
                    if let Err(e) = deliver_with_retries(client, &target, &payload).await {
                        tracing::warn!(
                            "Webhook delivery failed (event={}, url={}): {e}",
                            payload.event,
                            target.url
                        );
                    }
                });
            }
        }
    }
}

async fn deliver_with_retries(
    client: Arc<reqwest::Client>,
    target: &WebhookTarget,
    payload: &WebhookPayload,
) -> anyhow::Result<()> {
    let mut attempt: u32 = 0;
    let max_attempts = 1u32.saturating_add(target.max_retries);

    loop {
        attempt += 1;
        match deliver_once(&client, target, payload).await {
            Ok(()) => return Ok(()),
            Err(e) if attempt < max_attempts => {
                let delay = backoff_delay(attempt);
                tracing::debug!(
                    "Webhook delivery attempt {}/{} failed: {} (retrying in {:?})",
                    attempt,
                    max_attempts,
                    e,
                    delay
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
    client: &reqwest::Client,
    target: &WebhookTarget,
    payload: &WebhookPayload,
) -> anyhow::Result<()> {
    let mut req = client
        .post(&target.url)
        .json(payload)
        .timeout(target.timeout);

    for (k, v) in &target.headers {
        let name = reqwest::header::HeaderName::from_bytes(k.as_bytes())
            .map_err(|_| anyhow::anyhow!("invalid header name: {k}"))?;
        let value = reqwest::header::HeaderValue::from_str(v)
            .map_err(|_| anyhow::anyhow!("invalid header value for {k}"))?;
        req = req.header(name, value);
    }

    let resp = req.send().await?;
    let status = resp.status();
    if status.is_success() {
        return Ok(());
    }

    // Avoid retrying for most 4xx errors (auth/config problems).
    if status.is_client_error() && status != reqwest::StatusCode::TOO_MANY_REQUESTS {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {status} (non-retriable): {body}");
    }

    let body = resp.text().await.unwrap_or_default();
    anyhow::bail!("HTTP {status}: {body}");
}

#[derive(Debug, Clone, Serialize)]
struct WebhookPayload {
    event: String,
    timestamp: String,
    scheduler: SchedulerInfoPayload,
    #[serde(skip_serializing_if = "Option::is_none")]
    job: Option<JobPayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reservation: Option<ReservationPayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    gpu: Option<GpuPayload>,
}

impl WebhookPayload {
    fn user_for_filtering(&self) -> Option<&str> {
        if let Some(ref job) = self.job {
            return job.user.as_deref();
        }
        if let Some(ref reservation) = self.reservation {
            return Some(&reservation.user);
        }
        None
    }
}

#[derive(Debug, Clone, Serialize)]
struct SchedulerInfoPayload {
    host: String,
    version: String,
}

#[derive(Debug, Clone, Serialize)]
struct JobPayload {
    id: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    runtime: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    gpus: Option<Vec<u32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    submitted_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    finished_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ReservationPayload {
    id: u32,
    user: String,
    gpu_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    gpu_indices: Option<Vec<u32>>,
    start_time: String,
    end_time: String,
    status: String,
    created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    cancelled_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct GpuPayload {
    index: u32,
    available: bool,
}

async fn build_payloads(
    scheduler: &Arc<RwLock<SchedulerRuntime>>,
    scheduler_host: &str,
    event: &SchedulerEvent,
) -> Vec<WebhookPayload> {
    let scheduler_info = SchedulerInfoPayload {
        host: resolve_host(scheduler_host),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };

    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

    match event {
        SchedulerEvent::JobSubmitted { job_id } => {
            let job = scheduler.read().await.get_job(*job_id);
            vec![WebhookPayload {
                event: "job_submitted".to_string(),
                timestamp: now,
                scheduler: scheduler_info,
                job: Some(job_payload(*job_id, job)),
                reservation: None,
                gpu: None,
            }]
        }
        SchedulerEvent::JobStateChanged {
            job_id,
            old_state,
            new_state,
            ..
        } => {
            let Some(event_name) = state_change_event_name(*old_state, *new_state) else {
                return vec![];
            };
            let job = scheduler.read().await.get_job(*job_id);
            vec![WebhookPayload {
                event: event_name.to_string(),
                timestamp: now,
                scheduler: scheduler_info,
                job: Some(job_payload(*job_id, job)),
                reservation: None,
                gpu: None,
            }]
        }
        SchedulerEvent::JobCompleted {
            job_id,
            final_state,
            ..
        } => {
            let Some(event_name) = completed_event_name(*final_state) else {
                return vec![];
            };
            let job = scheduler.read().await.get_job(*job_id);
            vec![WebhookPayload {
                event: event_name.to_string(),
                timestamp: now,
                scheduler: scheduler_info,
                job: Some(job_payload(*job_id, job)),
                reservation: None,
                gpu: None,
            }]
        }
        SchedulerEvent::JobTimedOut { job_id, .. } => {
            let job = scheduler.read().await.get_job(*job_id);
            vec![WebhookPayload {
                event: "job_timeout".to_string(),
                timestamp: now,
                scheduler: scheduler_info,
                job: Some(job_payload(*job_id, job)),
                reservation: None,
                gpu: None,
            }]
        }
        SchedulerEvent::ReservationCreated { reservation_id } => {
            let reservation = scheduler
                .read()
                .await
                .get_reservation(*reservation_id)
                .cloned();
            let Some(reservation) = reservation else {
                return vec![];
            };
            vec![WebhookPayload {
                event: "reservation_created".to_string(),
                timestamp: now,
                scheduler: scheduler_info,
                job: None,
                reservation: Some(reservation_payload(&reservation)),
                gpu: None,
            }]
        }
        SchedulerEvent::ReservationCancelled { reservation_id } => {
            let reservation = scheduler
                .read()
                .await
                .get_reservation(*reservation_id)
                .cloned();
            let Some(reservation) = reservation else {
                return vec![];
            };
            vec![WebhookPayload {
                event: "reservation_cancelled".to_string(),
                timestamp: now,
                scheduler: scheduler_info,
                job: None,
                reservation: Some(reservation_payload(&reservation)),
                gpu: None,
            }]
        }
        SchedulerEvent::GpuAvailabilityChanged {
            gpu_index,
            available,
        } => {
            if !*available {
                return vec![];
            }
            vec![WebhookPayload {
                event: "gpu_available".to_string(),
                timestamp: now,
                scheduler: scheduler_info,
                job: None,
                reservation: None,
                gpu: Some(GpuPayload {
                    index: *gpu_index,
                    available: *available,
                }),
            }]
        }
        SchedulerEvent::MemoryAvailabilityChanged { .. }
        | SchedulerEvent::ZombieJobDetected { .. }
        | SchedulerEvent::PeriodicHealthCheck => vec![],
    }
}

fn resolve_host(default_host: &str) -> String {
    std::env::var("HOSTNAME")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| default_host.to_string())
}

fn state_change_event_name(old: JobState, new: JobState) -> Option<&'static str> {
    match (old, new) {
        (_, JobState::Running) => Some("job_started"),
        (_, JobState::Hold) => Some("job_held"),
        (JobState::Hold, JobState::Queued) => Some("job_released"),
        _ => None,
    }
}

fn completed_event_name(state: JobState) -> Option<&'static str> {
    match state {
        JobState::Finished => Some("job_completed"),
        JobState::Failed => Some("job_failed"),
        JobState::Cancelled => Some("job_cancelled"),
        JobState::Timeout => Some("job_timeout"),
        _ => None,
    }
}

fn job_payload(job_id: u32, job: Option<Job>) -> JobPayload {
    let Some(job) = job else {
        return JobPayload {
            id: job_id,
            name: None,
            user: None,
            state: None,
            runtime: None,
            gpus: None,
            submitted_at: None,
            started_at: None,
            finished_at: None,
            reason: None,
        };
    };

    let name = job
        .script
        .as_deref()
        .and_then(|p| p.file_name())
        .map(|s| s.to_string_lossy().to_string())
        .or_else(|| job.command.as_ref().map(|c| c.to_string()));

    let runtime = match (job.started_at, job.finished_at) {
        (Some(start), Some(end)) => end
            .duration_since(start)
            .ok()
            .map(gflow::utils::format_duration),
        (Some(start), None) => SystemTime::now()
            .duration_since(start)
            .ok()
            .map(gflow::utils::format_duration),
        _ => None,
    };

    JobPayload {
        id: job.id,
        name,
        user: Some(job.submitted_by.to_string()),
        state: Some(job.state.to_string()),
        runtime,
        gpus: job.gpu_ids.map(|ids| ids.into_iter().collect()),
        submitted_at: job.submitted_at.map(system_time_to_rfc3339),
        started_at: job.started_at.map(system_time_to_rfc3339),
        finished_at: job.finished_at.map(system_time_to_rfc3339),
        reason: job.reason.map(|r| r.to_string()),
    }
}

fn reservation_payload(r: &GpuReservation) -> ReservationPayload {
    ReservationPayload {
        id: r.id,
        user: r.user.to_string(),
        gpu_count: r.gpu_spec.count(),
        gpu_indices: r.gpu_spec.indices().map(|idx| idx.to_vec()),
        start_time: system_time_to_rfc3339(r.start_time),
        end_time: system_time_to_rfc3339(r.end_time()),
        status: format!("{:?}", r.status),
        created_at: system_time_to_rfc3339(r.created_at),
        cancelled_at: r.cancelled_at.map(system_time_to_rfc3339),
    }
}

fn system_time_to_rfc3339(time: SystemTime) -> String {
    let duration = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    chrono::DateTime::<chrono::Utc>::from_timestamp(duration.as_secs() as i64, 0)
        .unwrap_or_default()
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{extract::State, http::HeaderMap, routing::post, Json, Router};
    use gflow::core::executor::Executor;
    use gflow::core::job::Job;
    use serde_json::Value;
    use std::net::SocketAddr;
    use std::sync::Mutex;
    use tempfile::tempdir;
    use tokio::time::timeout;

    struct NoopExecutor;
    impl Executor for NoopExecutor {
        fn execute(&self, _job: &Job) -> anyhow::Result<()> {
            Ok(())
        }
    }

    type Received = Arc<Mutex<Vec<(Value, Option<String>)>>>;

    #[derive(Clone)]
    struct ReceiverState {
        received: Received,
    }

    async fn webhook_receiver(
        State(state): State<ReceiverState>,
        headers: HeaderMap,
        Json(body): Json<Value>,
    ) {
        let auth = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        state.received.lock().unwrap().push((body, auth));
    }

    async fn start_receiver() -> (String, Received) {
        let received: Received = Arc::new(Mutex::new(vec![]));
        let state = ReceiverState {
            received: Arc::clone(&received),
        };

        let app = Router::new()
            .route("/hook", post(webhook_receiver))
            .with_state(state);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        let url = format!("http://{}/hook", addr);

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        (url, received)
    }

    #[tokio::test]
    async fn test_event_name_mapping() {
        assert_eq!(
            state_change_event_name(JobState::Queued, JobState::Running),
            Some("job_started")
        );
        assert_eq!(
            state_change_event_name(JobState::Queued, JobState::Hold),
            Some("job_held")
        );
        assert_eq!(
            state_change_event_name(JobState::Hold, JobState::Queued),
            Some("job_released")
        );
        assert_eq!(
            completed_event_name(JobState::Finished),
            Some("job_completed")
        );
        assert_eq!(completed_event_name(JobState::Failed), Some("job_failed"));
        assert_eq!(
            completed_event_name(JobState::Cancelled),
            Some("job_cancelled")
        );
        assert_eq!(completed_event_name(JobState::Timeout), Some("job_timeout"));
    }

    #[tokio::test]
    async fn test_webhook_delivery_job_submitted_with_user_filter_and_headers() {
        let (url, received) = start_receiver().await;

        let dir = tempdir().unwrap();
        let mut runtime = SchedulerRuntime::with_state_path(
            Box::new(NoopExecutor),
            dir.path().to_path_buf(),
            None,
        )
        .unwrap();

        let job = Job::builder()
            .command("echo test")
            .submitted_by("alice")
            .build();
        let (job_id, _run_name, _job_clone) = runtime.submit_job(job).await;
        let scheduler = Arc::new(RwLock::new(runtime));

        let event_bus = Arc::new(EventBus::new(16));

        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer token123".to_string());

        let notifications = NotificationsConfig {
            enabled: true,
            webhooks: vec![WebhookConfig {
                url,
                events: vec!["job_submitted".to_string()],
                filter_users: Some(vec!["alice".to_string()]),
                headers,
                timeout_secs: 5,
                max_retries: 0,
            }],
            max_concurrent_deliveries: 4,
        };

        let _handle = spawn_webhook_notifier(
            notifications,
            Arc::clone(&scheduler),
            Arc::clone(&event_bus),
            "localhost".to_string(),
        )
        .unwrap();

        // Publish after notifier has subscribed.
        tokio::time::sleep(Duration::from_millis(50)).await;
        event_bus.publish(SchedulerEvent::JobSubmitted { job_id });

        timeout(Duration::from_secs(2), async {
            loop {
                if !received.lock().unwrap().is_empty() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();

        let guard = received.lock().unwrap();
        let (payload, auth) = guard.first().unwrap();

        assert_eq!(payload["event"], "job_submitted");
        assert_eq!(payload["job"]["id"], job_id);
        assert_eq!(payload["job"]["user"], "alice");
        assert_eq!(auth.as_deref(), Some("Bearer token123"));
    }
}
