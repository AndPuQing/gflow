//! HTTP server for the gflow daemon
//!
//! # Security Note
//! The `/debug/*` endpoints expose full job details and per-user statistics without
//! authentication. In production environments, ensure the daemon is bound to localhost
//! only and protected by firewall rules. Consider gating these endpoints behind a
//! feature flag or configuration option for production deployments.

mod handlers;
mod state;

pub(crate) use handlers::UpdateJobRequest;

use crate::events::EventBus;
use crate::executor::TmuxExecutor;
use crate::scheduler_runtime;
use crate::state_saver::StateSaverHandle;
use axum::{
    routing::{get, post},
    Router,
};
use socket2::{Domain, Protocol, Socket, Type};
use std::sync::Arc;
use std::time::Duration;

pub async fn run(config: gflow::config::Config) -> anyhow::Result<()> {
    let state_dir = gflow::core::get_data_dir()?;
    let allowed_gpus = config.daemon.gpus.clone();

    // Inject TmuxExecutor
    let executor = Box::new(TmuxExecutor);

    // Create state saver channel before initializing SchedulerRuntime
    let (state_tx, state_rx) = tokio::sync::mpsc::unbounded_channel();
    let state_saver_handle = StateSaverHandle::new(state_tx);

    // Create SchedulerRuntime and set state saver
    let mut scheduler_runtime =
        scheduler_runtime::SchedulerRuntime::with_state_path(executor, state_dir, allowed_gpus)?;
    scheduler_runtime.set_state_saver(state_saver_handle.clone());

    let scheduler = Arc::new(tokio::sync::RwLock::new(scheduler_runtime));
    let scheduler_clone = Arc::clone(&scheduler);

    // Create event bus for event-driven scheduling
    let event_bus = Arc::new(EventBus::new(1000));
    let event_bus_clone = Arc::clone(&event_bus);

    // Spawn state saver task (30 second interval)
    let scheduler_for_saver = Arc::clone(&scheduler);
    let state_saver_task = tokio::spawn(async move {
        tracing::info!("Starting state saver task with 30s interval...");
        crate::state_saver::run(scheduler_for_saver, state_rx, Duration::from_secs(30)).await;
    });
    state_saver_handle.set_task_handle(state_saver_task);

    // Spawn event-driven scheduler task only when we can persist (state.json or journal).
    // Otherwise the daemon is read-only and should not mutate jobs.
    let can_schedule = scheduler.read().await.can_mutate();
    if can_schedule {
        tokio::spawn(async move {
            tracing::info!("Starting event-driven scheduler...");
            scheduler_runtime::run_event_driven(scheduler_clone, event_bus_clone).await;
        });
    } else {
        tracing::error!(
            "No persistence available; gflowd started in read-only mode (no scheduling, no mutations)"
        );
    }

    // Create server state with scheduler, event bus, and state saver
    let server_state = state::ServerState::new(scheduler, event_bus, state_saver_handle.clone());

    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .route("/jobs", get(handlers::list_jobs).post(handlers::create_job))
        .route("/jobs/batch", post(handlers::create_jobs_batch))
        .route(
            "/jobs/resolve-dependency",
            get(handlers::resolve_dependency),
        )
        .route(
            "/jobs/{id}",
            get(handlers::get_job).patch(handlers::update_job),
        )
        .route("/jobs/{id}/finish", post(handlers::finish_job))
        .route("/jobs/{id}/fail", post(handlers::fail_job))
        .route("/jobs/{id}/cancel", post(handlers::cancel_job))
        .route("/jobs/{id}/hold", post(handlers::hold_job))
        .route("/jobs/{id}/release", post(handlers::release_job))
        .route("/jobs/{id}/log", get(handlers::get_job_log))
        .route("/info", get(handlers::info))
        .route("/health", get(handlers::get_health))
        .route("/gpus", post(handlers::set_allowed_gpus))
        .route(
            "/groups/{group_id}/max-concurrency",
            post(handlers::set_group_max_concurrency),
        )
        .route(
            "/reservations",
            get(handlers::list_reservations).post(handlers::create_reservation),
        )
        .route(
            "/reservations/{id}",
            get(handlers::get_reservation).delete(handlers::cancel_reservation),
        )
        .route("/metrics", get(handlers::get_metrics))
        .route("/debug/state", get(handlers::debug_state))
        .route("/debug/jobs/{id}", get(handlers::debug_job))
        .route("/debug/metrics", get(handlers::debug_metrics))
        .with_state(server_state);

    // Create socket with SO_REUSEPORT for hot reload support
    let host = &config.daemon.host;
    let port = config.daemon.port;

    // Handle IPv6 literal addresses (e.g., "::1" -> "[::1]")
    let bind_addr = if host.contains(':') && !host.starts_with('[') {
        // IPv6 literal without brackets
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };

    // Resolve hostname to socket address (supports "localhost", IPv4, and IPv6)
    let addr = tokio::net::lookup_host(&bind_addr)
        .await?
        .next()
        .ok_or_else(|| anyhow::anyhow!("Failed to resolve address: {}", bind_addr))?;

    // Determine domain from resolved address
    let domain = if addr.is_ipv4() {
        Domain::IPV4
    } else {
        Domain::IPV6
    };

    let socket = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))?;
    socket.set_reuse_address(true)?;
    socket.set_reuse_port(true)?; // Enable SO_REUSEPORT for hot reload
    socket.set_nonblocking(true)?;
    socket.bind(&addr.into())?;
    socket.listen(1024)?;

    // Convert to tokio TcpListener
    let std_listener: std::net::TcpListener = socket.into();
    std_listener.set_nonblocking(true)?;
    let listener = tokio::net::TcpListener::from_std(std_listener)?;

    tracing::info!("Listening on: {addr} (SO_REUSEPORT enabled)");

    // Create shutdown signal handler with state saver for graceful shutdown
    let shutdown_signal = create_shutdown_signal(state_saver_handle);

    // Start Axum server with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal)
        .await?;

    tracing::info!("Server shutdown complete");
    Ok(())
}

async fn create_shutdown_signal(state_saver: StateSaverHandle) {
    use tokio::signal::unix::{signal, SignalKind};

    let mut sigterm = signal(SignalKind::terminate()).expect("Failed to register SIGTERM handler");
    let mut sigint = signal(SignalKind::interrupt()).expect("Failed to register SIGINT handler");
    let mut sigusr2 =
        signal(SignalKind::user_defined2()).expect("Failed to register SIGUSR2 handler");

    tokio::select! {
        _ = sigterm.recv() => {
            tracing::info!("Received SIGTERM, initiating graceful shutdown");
        }
        _ = sigint.recv() => {
            tracing::info!("Received SIGINT, initiating graceful shutdown");
        }
        _ = sigusr2.recv() => {
            tracing::info!("Received SIGUSR2 (reload signal), initiating graceful shutdown");
        }
    }

    // Save state before exiting
    tracing::info!("Saving state before shutdown...");
    if let Err(e) = state_saver.shutdown_and_wait().await {
        tracing::error!("Failed to save state during shutdown: {}", e);
    } else {
        tracing::info!("State saved successfully");
    }
}
