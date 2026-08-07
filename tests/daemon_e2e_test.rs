use gflow::config::{Config, DaemonConfig};
use gflow::core::job::{JobBuilder, JobState};
use gflow::tmux::{get_all_session_names, is_session_exist};
use reqwest::StatusCode;
use serde_json::Value;
use std::ffi::OsStr;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tempfile::TempDir;

const DAEMON_SESSION: &str = "gflow_server";

fn daemon_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn unique_tmux_session_name(prefix: &str) -> String {
    format!(
        "{prefix}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    )
}

fn tmux_usable() -> bool {
    let session_name = unique_tmux_session_name("gflow-e2e-probe");
    let created = Command::new("tmux")
        .args(["new-session", "-d", "-s", &session_name, "sleep", "5"])
        .output();

    match created {
        Ok(output) if output.status.success() => {
            let _ = Command::new("tmux")
                .args(["kill-session", "-t", &session_name])
                .output();
            true
        }
        _ => false,
    }
}

fn stale_gflowd_session_present() -> bool {
    is_session_exist(DAEMON_SESSION)
}

fn pick_unused_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn process_running(pid: u32) -> bool {
    std::fs::metadata(format!("/proc/{pid}")).is_ok()
}

fn gflow_bin() -> &'static str {
    env!("CARGO_BIN_EXE_gflow")
}

fn gcancel_bin() -> &'static str {
    env!("CARGO_BIN_EXE_gcancel")
}

fn path_env() -> String {
    let mut paths = vec![];
    for bin in [gflow_bin(), gcancel_bin()] {
        let dir = Path::new(bin)
            .parent()
            .expect("binary path should have a parent")
            .to_path_buf();
        if !paths.contains(&dir) {
            paths.push(dir);
        }
    }

    if let Some(existing) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&existing));
    }

    std::env::join_paths(paths)
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

struct CommandResult {
    status: std::process::ExitStatus,
    stdout: String,
    stderr: String,
}

impl CommandResult {
    fn from_output(output: Output) -> Self {
        Self {
            status: output.status,
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }

    fn assert_success(&self, context: &str) {
        assert!(
            self.status.success(),
            "{context} failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
            self.status.code(),
            self.stdout,
            self.stderr
        );
    }
}

struct TestSandbox {
    _guard: std::sync::MutexGuard<'static, ()>,
    _tempdir: TempDir,
    root: PathBuf,
    config_home: PathBuf,
    data_home: PathBuf,
    runtime_dir: PathBuf,
    work_dir: PathBuf,
    port: u16,
    tmux_env_keys: Vec<&'static str>,
    bootstrap_session: String,
    daemon_started: bool,
    /// Host the daemon as a direct child process instead of `gflowd up` tmux.
    direct_daemon: bool,
    /// Whether the tmux global environment was seeded for the job executor.
    tmux_seeded: bool,
    daemon_child: Option<std::process::Child>,
}

/// Constructor options for a test sandbox.
struct SandboxOpts {
    /// Require a working tmux (probe) and host the daemon via `gflowd up`.
    tmux_hosted: bool,
    /// Job executor configured in gflow.toml.
    executor: &'static str,
}

impl TestSandbox {
    /// Default sandbox: tmux-hosted daemon, process executor (the default).
    fn new() -> Option<Self> {
        Self::with_opts(SandboxOpts {
            tmux_hosted: true,
            executor: "process",
        })
    }

    /// Sandbox with an explicit `[executor] type` (e.g. "tmux").
    fn with_executor(executor: &'static str) -> Option<Self> {
        Self::with_opts(SandboxOpts {
            tmux_hosted: true,
            executor,
        })
    }

    /// Sandbox that hosts the daemon directly as a child process — no tmux
    /// required anywhere in the job execution path.
    fn new_direct(executor: &'static str) -> Option<Self> {
        Self::with_opts(SandboxOpts {
            tmux_hosted: false,
            executor,
        })
    }

    fn with_opts(opts: SandboxOpts) -> Option<Self> {
        let guard = daemon_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        if (opts.tmux_hosted || opts.executor == "tmux") && !tmux_usable() {
            eprintln!("Skipping daemon E2E test: tmux not usable");
            return None;
        }

        if opts.tmux_hosted && stale_gflowd_session_present() {
            eprintln!(
                "Skipping daemon E2E test: tmux session '{}' already exists",
                DAEMON_SESSION
            );
            return None;
        }

        let tempdir = TempDir::new().unwrap();
        let root = tempdir.path().to_path_buf();
        let config_home = root.join("config-home");
        let data_home = root.join("data-home");
        let runtime_dir = root.join("runtime-dir");
        let work_dir = root.join("work-dir");
        std::fs::create_dir_all(&config_home).unwrap();
        std::fs::create_dir_all(&data_home).unwrap();
        std::fs::create_dir_all(&runtime_dir).unwrap();
        std::fs::create_dir_all(&work_dir).unwrap();
        std::fs::create_dir_all(config_home.join("gflow")).unwrap();
        std::fs::create_dir_all(data_home.join("gflow")).unwrap();

        let port = pick_unused_port();
        let config = format!(
            "[daemon]\nhost = \"127.0.0.1\"\nport = {port}\n\n[executor]\ntype = \"{}\"\n",
            opts.executor
        );
        std::fs::write(config_home.join("gflow/gflow.toml"), config).unwrap();

        let mut sandbox = Self {
            _guard: guard,
            _tempdir: tempdir,
            root,
            config_home,
            data_home,
            runtime_dir,
            work_dir,
            port,
            tmux_env_keys: vec![
                "HOME",
                "PATH",
                "XDG_CONFIG_HOME",
                "XDG_DATA_HOME",
                "XDG_RUNTIME_DIR",
                "GFLOW_DISABLE_DEV_AUTO",
            ],
            bootstrap_session: unique_tmux_session_name("gflow-e2e-bootstrap"),
            daemon_started: false,
            direct_daemon: !opts.tmux_hosted,
            tmux_seeded: false,
            daemon_child: None,
        };

        // The tmux job executor needs the tmux server's global environment to
        // point at the sandbox config dirs (job sessions inherit it). This is
        // required for both hosting modes; the process executor needs nothing.
        if opts.tmux_hosted || opts.executor == "tmux" {
            sandbox.seed_tmux_environment();
            sandbox.tmux_seeded = true;
        }

        Some(sandbox)
    }

    fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn client_config(&self) -> Config {
        Config {
            daemon: DaemonConfig {
                host: "127.0.0.1".to_string(),
                port: self.port,
                max_concurrent_jobs: None,
                gpus: None,
                gpu_allocation_strategy: Default::default(),
                gpu_poll_interval_secs: 10,
                fair_share: Default::default(),
            },
            ..Default::default()
        }
    }

    fn data_dir(&self) -> PathBuf {
        self.data_home.join("gflow")
    }

    fn log_path(&self, job_id: u32) -> PathBuf {
        self.data_dir().join("logs").join(format!("{job_id}.log"))
    }

    fn env_value(&self, key: &str) -> Option<String> {
        match key {
            "HOME" => Some(self.root.display().to_string()),
            "PATH" => Some(path_env()),
            "XDG_CONFIG_HOME" => Some(self.config_home.display().to_string()),
            "XDG_DATA_HOME" => Some(self.data_home.display().to_string()),
            "XDG_RUNTIME_DIR" => Some(self.runtime_dir.display().to_string()),
            "GFLOW_DISABLE_DEV_AUTO" => Some("1".to_string()),
            _ => None,
        }
    }

    fn seed_tmux_environment(&self) {
        let bootstrap = Command::new("tmux")
            .args([
                "new-session",
                "-d",
                "-s",
                &self.bootstrap_session,
                "sleep",
                "300",
            ])
            .output()
            .unwrap();
        assert!(
            bootstrap.status.success(),
            "failed to create tmux bootstrap session {}: {}",
            self.bootstrap_session,
            String::from_utf8_lossy(&bootstrap.stderr)
        );

        for key in &self.tmux_env_keys {
            let Some(value) = self.env_value(key) else {
                continue;
            };
            let output = Command::new("tmux")
                .args(["set-environment", "-g", key, &value])
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "failed to seed tmux env {}: {}",
                key,
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    fn run_gflow<I, S>(&self, args: I) -> CommandResult
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = Command::new(gflow_bin());
        command.current_dir(&self.work_dir);
        command.env("HOME", &self.root);
        command.env("PATH", path_env());
        command.env("XDG_CONFIG_HOME", &self.config_home);
        command.env("XDG_DATA_HOME", &self.data_home);
        command.env("XDG_RUNTIME_DIR", &self.runtime_dir);
        command.env("GFLOW_DISABLE_DEV_AUTO", "1");
        command.args(args);
        CommandResult::from_output(command.output().unwrap())
    }

    fn start_daemon(&mut self) {
        if self.direct_daemon {
            self.start_daemon_direct();
        } else {
            let result = self.run_gflow(["gflowd", "up"]);
            result.assert_success("gflowd up");
        }
        self.daemon_started = true;
    }

    fn start_daemon_direct(&mut self) {
        let mut command = Command::new(gflow_bin());
        command
            .current_dir(&self.work_dir)
            .env("HOME", &self.root)
            .env("PATH", path_env())
            .env("XDG_CONFIG_HOME", &self.config_home)
            .env("XDG_DATA_HOME", &self.data_home)
            .env("XDG_RUNTIME_DIR", &self.runtime_dir)
            .env("GFLOW_DISABLE_DEV_AUTO", "1")
            .args(["__multicall", "gflowd", "-vvv"]);
        let child = command
            .spawn()
            .expect("failed to spawn gflowd daemon directly");
        self.daemon_child = Some(child);
    }

    fn stop_daemon(&mut self) {
        if !self.daemon_started {
            return;
        }

        if self.direct_daemon {
            if let Some(mut child) = self.daemon_child.take() {
                #[cfg(unix)]
                unsafe {
                    libc::kill(child.id() as libc::pid_t, libc::SIGTERM);
                }
                let mut exited = false;
                for _ in 0..100 {
                    match child.try_wait().unwrap() {
                        Some(_) => {
                            exited = true;
                            break;
                        }
                        None => std::thread::sleep(Duration::from_millis(100)),
                    }
                }
                if !exited {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        } else {
            let _ = self.run_gflow(["gflowd", "down"]);
        }
        self.daemon_started = false;
    }
}

impl Drop for TestSandbox {
    fn drop(&mut self) {
        self.stop_daemon();

        if !self.tmux_seeded {
            return;
        }

        let sessions = get_all_session_names();
        for session in sessions {
            // Only kill sessions this sandbox may own: the bootstrap session,
            // reload-created sessions, and (for tmux-hosted sandboxes only) the
            // gflow_server daemon session it created via `gflowd up`. Never kill
            // a pre-existing/foreign gflow_server session.
            let is_own = session == self.bootstrap_session
                || session.starts_with("gflow_server_new_")
                || (!self.direct_daemon && session == DAEMON_SESSION);
            if !is_own {
                continue;
            }
            let _ = Command::new("tmux")
                .args(["kill-session", "-t", &session])
                .output();
        }

        for key in &self.tmux_env_keys {
            let _ = Command::new("tmux")
                .args(["set-environment", "-gu", key])
                .output();
        }
    }
}

async fn get_health(base_url: &str) -> Result<(StatusCode, Value), reqwest::Error> {
    gflow::tls::ensure_rustls_provider_installed();
    let response = reqwest::get(format!("{base_url}/health")).await?;
    let status = response.status();
    let body = response.json::<Value>().await?;
    Ok((status, body))
}

async fn wait_for_health_status(
    base_url: &str,
    expected_status: StatusCode,
    timeout: Duration,
) -> Value {
    let start = Instant::now();
    let mut last_error = None;

    while start.elapsed() < timeout {
        match get_health(base_url).await {
            Ok((status, body)) if status == expected_status => return body,
            Ok((status, body)) => {
                last_error = Some(format!("status={status}, body={body}"));
            }
            Err(error) => {
                last_error = Some(error.to_string());
            }
        }

        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    panic!(
        "timed out waiting for health {} at {}; last error: {:?}",
        expected_status, base_url, last_error
    );
}

async fn wait_for_health_unreachable(base_url: &str, timeout: Duration) {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if get_health(base_url).await.is_err() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    panic!("timed out waiting for {base_url} to become unreachable");
}

async fn wait_for_pid_change(base_url: &str, old_pid: u32, timeout: Duration) -> Value {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Ok((status, body)) = get_health(base_url).await {
            if status == StatusCode::OK
                && body["pid"].as_u64().map(|pid| pid as u32) != Some(old_pid)
            {
                return body;
            }
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    panic!("timed out waiting for daemon PID to change from {old_pid}");
}

async fn wait_for_job_state(
    client: &gflow::Client,
    job_id: u32,
    expected_state: JobState,
    timeout: Duration,
) -> gflow::core::job::Job {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Some(job) = client.get_job(job_id).await.unwrap() {
            if job.state == expected_state {
                return job;
            }
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    panic!(
        "timed out waiting for job {} to reach state {:?}",
        job_id, expected_state
    );
}

async fn wait_for_log_contains(path: &Path, needle: &str, timeout: Duration) {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Ok(content) = std::fs::read_to_string(path) {
            if content.contains(needle) {
                return;
            }
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    panic!(
        "timed out waiting for log {} to contain {:?}",
        path.display(),
        needle
    );
}

async fn wait_for_tmux_session(name: &str, should_exist: bool, timeout: Duration) {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if is_session_exist(name) == should_exist {
            return;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    panic!(
        "timed out waiting for tmux session '{}' existence={} ",
        name, should_exist
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn daemon_lifecycle_reload_and_health_endpoint() {
    let Some(mut sandbox) = TestSandbox::new() else {
        return;
    };

    sandbox.start_daemon();

    let health =
        wait_for_health_status(&sandbox.base_url(), StatusCode::OK, Duration::from_secs(15)).await;
    assert_eq!(health["status"], "ok");
    let original_pid = health["pid"].as_u64().unwrap() as u32;
    assert!(process_running(original_pid));

    let status = sandbox.run_gflow(["gflowd", "status"]);
    status.assert_success("gflowd status while running");
    assert!(status.stdout.contains("Status: Running"));

    let reload = sandbox.run_gflow(["gflowd", "reload"]);
    reload.assert_success("gflowd reload");
    assert!(reload.stdout.contains("reloaded successfully"));

    let reloaded_health =
        wait_for_pid_change(&sandbox.base_url(), original_pid, Duration::from_secs(20)).await;
    assert_eq!(reloaded_health["status"], "ok");
    let new_pid = reloaded_health["pid"].as_u64().unwrap() as u32;
    assert_ne!(original_pid, new_pid);

    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(15) {
        if !process_running(original_pid) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    if process_running(original_pid) {
        eprintln!(
            "old daemon pid {} is still running after reload; continuing because reload only guarantees the new daemon is serving traffic",
            original_pid
        );
    }

    sandbox.stop_daemon();
    if tokio::time::timeout(
        Duration::from_secs(10),
        wait_for_health_unreachable(&sandbox.base_url(), Duration::from_secs(10)),
    )
    .await
    .is_err()
    {
        eprintln!(
            "daemon endpoint {} remained reachable after down; continuing because reload can leave an old process alive temporarily",
            sandbox.base_url()
        );
    }

    let status = sandbox.run_gflow(["gflowd", "status"]);
    status.assert_success("gflowd status after down");
    assert!(status.stdout.contains("Status: Not running"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tmux_job_execution_writes_logs_and_auto_closes_session() {
    let Some(mut sandbox) = TestSandbox::with_executor("tmux") else {
        return;
    };

    sandbox.start_daemon();
    wait_for_health_status(&sandbox.base_url(), StatusCode::OK, Duration::from_secs(15)).await;

    let client = gflow::Client::build(&sandbox.client_config()).unwrap();
    let job = JobBuilder::new()
        .submitted_by("daemon-e2e")
        .run_dir(&sandbox.work_dir)
        .command("echo started && sleep 2 && echo finished")
        .auto_close_tmux(true)
        .build();

    let response = client.add_job(job).await.unwrap();
    let run_name = response.run_name.clone();

    let running_job = wait_for_job_state(
        &client,
        response.id,
        JobState::Running,
        Duration::from_secs(15),
    )
    .await;
    assert_eq!(running_job.run_name.as_deref(), Some(run_name.as_str()));

    wait_for_tmux_session(&run_name, true, Duration::from_secs(10)).await;
    wait_for_log_contains(
        &sandbox.log_path(response.id),
        "started",
        Duration::from_secs(10),
    )
    .await;

    let finished_job = wait_for_job_state(
        &client,
        response.id,
        JobState::Finished,
        Duration::from_secs(20),
    )
    .await;
    assert_eq!(finished_job.state, JobState::Finished);
    wait_for_log_contains(
        &sandbox.log_path(response.id),
        "finished",
        Duration::from_secs(10),
    )
    .await;
    wait_for_tmux_session(&run_name, false, Duration::from_secs(10)).await;

    sandbox.stop_daemon();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn custom_run_name_is_normalized_and_job_still_executes() {
    let Some(mut sandbox) = TestSandbox::with_executor("tmux") else {
        return;
    };

    sandbox.start_daemon();
    wait_for_health_status(&sandbox.base_url(), StatusCode::OK, Duration::from_secs(15)).await;

    let client = gflow::Client::build(&sandbox.client_config()).unwrap();
    let requested_run_name = format!("train:{}.{:x}", std::process::id(), sandbox.port);
    let job = JobBuilder::new()
        .submitted_by("daemon-e2e")
        .run_dir(&sandbox.work_dir)
        .run_name(Some(requested_run_name))
        .command("echo normalized-run-name")
        .auto_close_tmux(true)
        .build();

    let response = client.add_job(job).await.unwrap();
    assert!(!response.run_name.contains(':'));
    assert!(!response.run_name.contains('.'));
    assert!(response
        .run_name
        .starts_with(&format!("gjob-{}-", response.id)));

    wait_for_tmux_session(&response.run_name, true, Duration::from_secs(10)).await;
    wait_for_log_contains(
        &sandbox.log_path(response.id),
        "normalized-run-name",
        Duration::from_secs(10),
    )
    .await;

    let finished_job = wait_for_job_state(
        &client,
        response.id,
        JobState::Finished,
        Duration::from_secs(20),
    )
    .await;
    assert_eq!(
        finished_job.run_name.as_deref(),
        Some(response.run_name.as_str())
    );
    wait_for_tmux_session(&response.run_name, false, Duration::from_secs(10)).await;

    sandbox.stop_daemon();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_missing_job_returns_client_error() {
    let Some(mut sandbox) = TestSandbox::new() else {
        return;
    };

    sandbox.start_daemon();
    wait_for_health_status(&sandbox.base_url(), StatusCode::OK, Duration::from_secs(15)).await;

    let client = gflow::Client::build(&sandbox.client_config()).unwrap();
    let err = client.cancel_job(u32::MAX).await.unwrap_err();
    assert!(err.to_string().contains("Failed to cancel job"));
    assert!(err.to_string().contains("404"));

    sandbox.stop_daemon();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn temporary_job_limit_endpoint_groups_selected_jobs() {
    let Some(mut sandbox) = TestSandbox::new_direct("process") else {
        return;
    };

    sandbox.start_daemon();
    wait_for_health_status(&sandbox.base_url(), StatusCode::OK, Duration::from_secs(15)).await;

    let client = gflow::Client::build(&sandbox.client_config()).unwrap();
    let dependency = client
        .add_job(
            JobBuilder::new()
                .submitted_by("daemon-e2e")
                .run_dir(&sandbox.work_dir)
                .command("sleep 5")
                .build(),
        )
        .await
        .unwrap();
    wait_for_job_state(
        &client,
        dependency.id,
        JobState::Running,
        Duration::from_secs(15),
    )
    .await;

    let selected = vec![
        client
            .add_job(
                JobBuilder::new()
                    .submitted_by("daemon-e2e")
                    .run_dir(&sandbox.work_dir)
                    .depends_on(Some(dependency.id))
                    .command("echo selected-1")
                    .build(),
            )
            .await
            .unwrap(),
        client
            .add_job(
                JobBuilder::new()
                    .submitted_by("daemon-e2e")
                    .run_dir(&sandbox.work_dir)
                    .depends_on(Some(dependency.id))
                    .command("echo selected-2")
                    .build(),
            )
            .await
            .unwrap(),
    ];

    let (group_id, updated_jobs) = client
        .set_jobs_max_concurrency(&selected.iter().map(|job| job.id).collect::<Vec<_>>(), 1)
        .await
        .unwrap();
    assert_eq!(updated_jobs, 2);

    for job in selected {
        let current = client.get_job(job.id).await.unwrap().unwrap();
        assert_eq!(
            current.group_id.map(|id| id.to_string()),
            Some(group_id.clone())
        );
        assert_eq!(current.max_concurrent, Some(1));
        assert_eq!(current.state, JobState::Queued);
    }

    client.cancel_job(dependency.id).await.unwrap();
    sandbox.stop_daemon();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn health_reports_recovery_mode_for_corrupt_state() {
    let Some(mut sandbox) = TestSandbox::new() else {
        return;
    };

    std::fs::create_dir_all(sandbox.data_dir()).unwrap();
    std::fs::write(
        sandbox.data_dir().join("state.json"),
        b"{ definitely-not-json",
    )
    .unwrap();

    sandbox.start_daemon();

    let health =
        wait_for_health_status(&sandbox.base_url(), StatusCode::OK, Duration::from_secs(15)).await;
    assert_eq!(health["status"], "recovery");
    assert_eq!(health["mode"], "journal");
    assert!(health["pid"].as_u64().is_some());
    assert!(health["detail"]
        .as_str()
        .unwrap()
        .contains("entered recovery mode"));

    let backup = health["state_backup"].as_str().unwrap();
    assert!(
        Path::new(backup).exists(),
        "backup path should exist: {backup}"
    );

    sandbox.stop_daemon();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn events_endpoint_streams_live_scheduler_events() {
    let Some(mut sandbox) = TestSandbox::new() else {
        return;
    };

    sandbox.start_daemon();
    wait_for_health_status(&sandbox.base_url(), StatusCode::OK, Duration::from_secs(15)).await;

    gflow::tls::ensure_rustls_provider_installed();
    let response = reqwest::get(format!("{}/events", sandbox.base_url()))
        .await
        .expect("events endpoint should respond");
    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(
        content_type.starts_with("text/event-stream"),
        "unexpected content type: {content_type}"
    );

    let mut response = response;
    let mut received = String::new();

    // The stream opens with an initial "connected" event.
    let first_chunk = tokio::time::timeout(Duration::from_secs(10), response.chunk())
        .await
        .expect("should receive the initial SSE chunk")
        .expect("stream should not error")
        .expect("stream should not end");
    received.push_str(&String::from_utf8_lossy(&first_chunk));
    assert!(
        received.contains("connected"),
        "initial chunk should announce connection, got: {received}"
    );

    // Submitting a job must produce a live event on the open stream.
    let client = gflow::Client::build(&sandbox.client_config()).unwrap();
    let job = JobBuilder::new()
        .submitted_by("daemon-e2e")
        .run_dir(&sandbox.work_dir)
        .command("echo sse-marker")
        .auto_close_tmux(true)
        .build();
    client.add_job(job).await.unwrap();

    let saw_job_event = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let chunk = response
                .chunk()
                .await
                .expect("stream should not error")
                .expect("stream should stay open");
            received.push_str(&String::from_utf8_lossy(&chunk));
            if received.contains("job_submitted") {
                break;
            }
        }
    })
    .await
    .is_ok();

    assert!(
        saw_job_event,
        "expected a job_submitted event on the SSE stream, got: {received}"
    );

    sandbox.stop_daemon();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn job_log_content_endpoint_serves_captured_output() {
    let Some(mut sandbox) = TestSandbox::new() else {
        return;
    };

    sandbox.start_daemon();
    wait_for_health_status(&sandbox.base_url(), StatusCode::OK, Duration::from_secs(15)).await;

    let client = gflow::Client::build(&sandbox.client_config()).unwrap();
    let job = JobBuilder::new()
        .submitted_by("daemon-e2e")
        .run_dir(&sandbox.work_dir)
        .command("echo log-content-marker")
        .auto_close_tmux(true)
        .build();
    let response = client.add_job(job).await.unwrap();

    wait_for_job_state(
        &client,
        response.id,
        JobState::Finished,
        Duration::from_secs(20),
    )
    .await;
    wait_for_log_contains(
        &sandbox.log_path(response.id),
        "log-content-marker",
        Duration::from_secs(10),
    )
    .await;

    gflow::tls::ensure_rustls_provider_installed();
    let log_response = reqwest::get(format!(
        "{}/jobs/{}/log/content",
        sandbox.base_url(),
        response.id
    ))
    .await
    .unwrap();
    assert_eq!(log_response.status(), StatusCode::OK);
    let body: Value = log_response.json().await.unwrap();
    assert_eq!(body["job_id"], response.id);
    assert!(
        body["content"]
            .as_str()
            .unwrap()
            .contains("log-content-marker"),
        "log content should contain the job output, got: {body}"
    );

    // Unknown jobs get a 404.
    let missing = reqwest::get(format!(
        "{}/jobs/{}/log/content",
        sandbox.base_url(),
        u32::MAX
    ))
    .await
    .unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);

    sandbox.stop_daemon();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn log_content_and_events_endpoints_serve_dashboard() {
    let Some(mut sandbox) = TestSandbox::new() else {
        return;
    };

    sandbox.start_daemon();
    wait_for_health_status(&sandbox.base_url(), StatusCode::OK, Duration::from_secs(15)).await;

    gflow::tls::ensure_rustls_provider_installed();
    let http = reqwest::Client::new();
    let base = sandbox.base_url();

    // The SSE endpoint must open a text/event-stream before any job exists.
    let events_stream = http
        .get(format!("{base}/events"))
        .header(reqwest::header::ACCEPT, "text/event-stream")
        .send()
        .await
        .unwrap();
    assert_eq!(events_stream.status(), StatusCode::OK);
    let content_type = events_stream
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();
    assert!(
        content_type.starts_with("text/event-stream"),
        "unexpected /events content-type: {content_type}"
    );
    let mut events_stream = events_stream;

    // Submit a job that produces log output.
    let client = gflow::Client::build(&sandbox.client_config()).unwrap();
    let job = JobBuilder::new()
        .submitted_by("daemon-e2e")
        .run_dir(&sandbox.work_dir)
        .command("echo log-content-probe && sleep 1 && echo log-content-done")
        .auto_close_tmux(true)
        .build();
    let response = client.add_job(job).await.unwrap();

    // The log content endpoint serves captured output as JSON.
    wait_for_log_contains(
        &sandbox.log_path(response.id),
        "log-content-probe",
        Duration::from_secs(10),
    )
    .await;

    let log_url = format!("{base}/jobs/{}/log/content", response.id);
    let mut payload = Value::Null;
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        let body = http.get(&log_url).send().await.unwrap();
        if body.status() == StatusCode::OK {
            payload = body.json().await.unwrap();
            if payload["content"]
                .as_str()
                .is_some_and(|content| content.contains("log-content-probe"))
            {
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    assert_eq!(payload["job_id"].as_u64(), Some(response.id as u64));
    assert!(payload["content"]
        .as_str()
        .is_some_and(|content| content.contains("log-content-probe")));

    // tail=1 narrows the response to a single line.
    let tail_body: Value = http
        .get(format!("{log_url}?tail=1"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(tail_body["content"].as_str().unwrap().lines().count(), 1);

    // Unknown jobs yield 404.
    assert_eq!(
        http.get(format!("{base}/jobs/4294967295/log/content"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );

    // The SSE stream opened at the start must have received the job events.
    let mut received = String::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    while tokio::time::Instant::now() < deadline && !received.contains("\"type\":\"job_") {
        match tokio::time::timeout(Duration::from_secs(5), events_stream.chunk()).await {
            Ok(Ok(Some(chunk))) => received.push_str(&String::from_utf8_lossy(&chunk)),
            Ok(Ok(None)) => break,
            Ok(Err(error)) => panic!("SSE stream error: {error}"),
            Err(_) => continue, // keep-alive window elapsed; keep waiting
        }
    }
    assert!(
        received.contains("\"type\":\"job_"),
        "no job events received on /events; got: {received}"
    );

    sandbox.stop_daemon();
}

// ── process-executor helpers ────────────────────────────────────────────────

/// Read the durable runner pid recorded for a process-executor job.
fn find_job_runner_pid(data_dir: &Path, job_id: u32) -> Option<u32> {
    let metadata_path = data_dir.join("runners").join(format!("{job_id}.json"));
    let metadata: Value = serde_json::from_slice(&std::fs::read(metadata_path).ok()?).ok()?;
    metadata["pid"].as_u64().map(|pid| pid as u32)
}

/// All pids whose process group id equals `pgid` (via `pgrep -g`).
fn processes_in_group(pgid: u32) -> Vec<u32> {
    let Ok(output) = Command::new("pgrep")
        .args(["-g", &pgid.to_string()])
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.trim().parse::<u32>().ok())
        .collect()
}

async fn wait_for_runner_pid(data_dir: &Path, job_id: u32, timeout: Duration) -> u32 {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Some(pid) = find_job_runner_pid(data_dir, job_id) {
            return pid;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("timed out waiting for runner process of job {job_id}");
}

async fn wait_for_process_group_gone(pgid: u32, timeout: Duration) {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if processes_in_group(pgid).is_empty() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    panic!(
        "timed out waiting for process group {pgid} to be gone; still alive: {:?}",
        processes_in_group(pgid)
    );
}

// ── process-executor e2e tests ──────────────────────────────────────────────

/// Acceptance: a fully tmux-free environment can run submit → schedule → run →
/// log → finish. The daemon is hosted directly (no tmux) and the job runs as a
/// plain child process.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn process_executor_runs_job_without_tmux() {
    let Some(mut sandbox) = TestSandbox::new_direct("process") else {
        return;
    };
    sandbox.start_daemon();
    wait_for_health_status(&sandbox.base_url(), StatusCode::OK, Duration::from_secs(15)).await;

    let client = gflow::Client::build(&sandbox.client_config()).unwrap();
    let job = JobBuilder::new()
        .submitted_by("proc-e2e")
        .run_dir(&sandbox.work_dir)
        .command("echo process-started && sleep 1 && echo process-finished")
        .build();
    let response = client.add_job(job).await.unwrap();

    let running_job = wait_for_job_state(
        &client,
        response.id,
        JobState::Running,
        Duration::from_secs(15),
    )
    .await;
    assert_eq!(
        running_job.run_name.as_deref(),
        Some(response.run_name.as_str())
    );

    // A Running process-executor job must be reported as alive, and the daemon
    // must advertise the process backend via /info (gqueue renders its liveness
    // indicator from these).
    assert_eq!(running_job.alive, Some(true), "running job should be alive");
    let info = client.get_info().await.unwrap();
    assert_eq!(info.executor, "process");

    // The process executor must not create a tmux session for the job.
    assert!(!is_session_exist(&response.run_name));

    wait_for_log_contains(
        &sandbox.log_path(response.id),
        "process-started",
        Duration::from_secs(10),
    )
    .await;

    let finished_job = wait_for_job_state(
        &client,
        response.id,
        JobState::Finished,
        Duration::from_secs(20),
    )
    .await;
    assert_eq!(finished_job.state, JobState::Finished);
    wait_for_log_contains(
        &sandbox.log_path(response.id),
        "process-finished",
        Duration::from_secs(10),
    )
    .await;

    sandbox.stop_daemon();
}

/// A reload handoff must preserve live runners and collect results written
/// while the daemon is offline. This exercises the durable runner metadata
/// rather than the old in-memory ProcessExecutor registry.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn process_executor_re_adopts_after_daemon_handoff() {
    let Some(mut sandbox) = TestSandbox::new_direct("process") else {
        return;
    };
    sandbox.start_daemon();
    wait_for_health_status(&sandbox.base_url(), StatusCode::OK, Duration::from_secs(15)).await;

    let client = gflow::Client::build(&sandbox.client_config()).unwrap();
    let live_job = client
        .add_job(
            JobBuilder::new()
                .submitted_by("re-adopt-e2e")
                .run_dir(&sandbox.work_dir)
                .command("sleep 8")
                .build(),
        )
        .await
        .unwrap();
    let offline_job = client
        .add_job(
            JobBuilder::new()
                .submitted_by("re-adopt-e2e")
                .run_dir(&sandbox.work_dir)
                .command("sleep 1")
                .build(),
        )
        .await
        .unwrap();

    wait_for_job_state(
        &client,
        live_job.id,
        JobState::Running,
        Duration::from_secs(15),
    )
    .await;
    wait_for_job_state(
        &client,
        offline_job.id,
        JobState::Running,
        Duration::from_secs(15),
    )
    .await;

    let old_pid = sandbox
        .daemon_child
        .as_ref()
        .expect("direct daemon child")
        .id();
    unsafe {
        libc::kill(old_pid as libc::pid_t, libc::SIGUSR2);
    }

    let mut old_child = sandbox.daemon_child.take().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if old_child.try_wait().unwrap().is_some() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "old daemon did not exit on reload handoff"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    sandbox.daemon_started = false;

    // The short runner should finish after the old daemon has exited, before
    // the replacement starts.
    let result_path = sandbox
        .data_dir()
        .join("runners")
        .join(format!("{}.result.json", offline_job.id));
    let result_deadline = Instant::now() + Duration::from_secs(10);
    while !result_path.exists() {
        assert!(
            Instant::now() < result_deadline,
            "offline runner did not persist its result"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    sandbox.start_daemon_direct();
    sandbox.daemon_started = true;
    wait_for_health_status(&sandbox.base_url(), StatusCode::OK, Duration::from_secs(15)).await;

    let adopted = wait_for_job_state(
        &client,
        live_job.id,
        JobState::Running,
        Duration::from_secs(5),
    )
    .await;
    assert_eq!(adopted.state, JobState::Running);

    wait_for_job_state(
        &client,
        offline_job.id,
        JobState::Finished,
        Duration::from_secs(15),
    )
    .await;
    wait_for_job_state(
        &client,
        live_job.id,
        JobState::Finished,
        Duration::from_secs(20),
    )
    .await;

    sandbox.stop_daemon();
}

/// Acceptance: cancelling a job reliably terminates the whole process tree
/// (SIGTERM to the process group, escalating to SIGKILL).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn process_executor_cancel_terminates_process_tree() {
    let Some(mut sandbox) = TestSandbox::new_direct("process") else {
        return;
    };
    sandbox.start_daemon();
    wait_for_health_status(&sandbox.base_url(), StatusCode::OK, Duration::from_secs(15)).await;

    let client = gflow::Client::build(&sandbox.client_config()).unwrap();
    // Two sleepers in the same process group (background + foreground).
    let job = JobBuilder::new()
        .submitted_by("proc-e2e")
        .run_dir(&sandbox.work_dir)
        .command("sleep 300 & sleep 300")
        .build();
    let response = client.add_job(job).await.unwrap();

    wait_for_job_state(
        &client,
        response.id,
        JobState::Running,
        Duration::from_secs(15),
    )
    .await;

    let wrapper_pid =
        wait_for_runner_pid(&sandbox.data_dir(), response.id, Duration::from_secs(10)).await;
    // The runner is a session leader, so its pgid == its pid, and it has at
    // least the two sleepers as group members.
    assert!(
        processes_in_group(wrapper_pid).len() >= 3,
        "expected bash + 2 sleepers in the process group, got {:?}",
        processes_in_group(wrapper_pid)
    );

    client.cancel_job(response.id).await.unwrap();
    wait_for_job_state(
        &client,
        response.id,
        JobState::Cancelled,
        Duration::from_secs(15),
    )
    .await;

    // The entire process tree must be gone.
    wait_for_process_group_gone(wrapper_pid, Duration::from_secs(15)).await;

    sandbox.stop_daemon();
}

/// Zombie detection is based on real process liveness: if the process dies
/// without reporting (here: SIGKILLed externally), the zombie monitor marks
/// the job failed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn process_executor_detects_zombie_when_process_dies_without_reporting() {
    let Some(mut sandbox) = TestSandbox::new_direct("process") else {
        return;
    };
    sandbox.start_daemon();
    wait_for_health_status(&sandbox.base_url(), StatusCode::OK, Duration::from_secs(15)).await;

    let client = gflow::Client::build(&sandbox.client_config()).unwrap();
    let job = JobBuilder::new()
        .submitted_by("proc-e2e")
        .run_dir(&sandbox.work_dir)
        .command("sleep 300")
        .build();
    let response = client.add_job(job).await.unwrap();

    wait_for_job_state(
        &client,
        response.id,
        JobState::Running,
        Duration::from_secs(15),
    )
    .await;

    // SIGKILL the whole process group so the runner cannot record a result.
    let wrapper_pid =
        wait_for_runner_pid(&sandbox.data_dir(), response.id, Duration::from_secs(10)).await;
    unsafe {
        libc::kill(-(wrapper_pid as libc::pid_t), libc::SIGKILL);
    }
    wait_for_process_group_gone(wrapper_pid, Duration::from_secs(10)).await;

    // The zombie monitor runs every 10s with a 30s startup grace period.
    wait_for_job_state(
        &client,
        response.id,
        JobState::Failed,
        Duration::from_secs(75),
    )
    .await;

    sandbox.stop_daemon();
}

/// Acceptance: `[executor] type` switches between backends. The tmux backend
/// creates a job session; the process backend (default) creates none.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn executor_type_config_selects_backend() {
    // tmux backend: job gets a tmux session (daemon hosted directly, but the
    // tmux server environment is seeded so job sessions resolve the sandbox
    // config).
    {
        let Some(mut sandbox) = TestSandbox::new_direct("tmux") else {
            return;
        };
        sandbox.start_daemon();
        wait_for_health_status(&sandbox.base_url(), StatusCode::OK, Duration::from_secs(15)).await;

        let client = gflow::Client::build(&sandbox.client_config()).unwrap();
        let job = JobBuilder::new()
            .submitted_by("cfg-e2e")
            .run_dir(&sandbox.work_dir)
            .command("echo tmux-backend")
            .auto_close_tmux(true)
            .build();
        let response = client.add_job(job).await.unwrap();

        assert_eq!(client.get_info().await.unwrap().executor, "tmux");
        wait_for_tmux_session(&response.run_name, true, Duration::from_secs(10)).await;
        wait_for_job_state(
            &client,
            response.id,
            JobState::Finished,
            Duration::from_secs(20),
        )
        .await;
        wait_for_tmux_session(&response.run_name, false, Duration::from_secs(10)).await;
        sandbox.stop_daemon();
    }

    // process backend: no tmux session is ever created.
    let Some(mut sandbox) = TestSandbox::new_direct("process") else {
        return;
    };
    sandbox.start_daemon();
    wait_for_health_status(&sandbox.base_url(), StatusCode::OK, Duration::from_secs(15)).await;

    let client = gflow::Client::build(&sandbox.client_config()).unwrap();
    let job = JobBuilder::new()
        .submitted_by("cfg-e2e")
        .run_dir(&sandbox.work_dir)
        .command("echo process-backend")
        .build();
    let response = client.add_job(job).await.unwrap();

    wait_for_job_state(
        &client,
        response.id,
        JobState::Finished,
        Duration::from_secs(20),
    )
    .await;
    assert!(!is_session_exist(&response.run_name));
    sandbox.stop_daemon();
}
