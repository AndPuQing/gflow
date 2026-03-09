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
}

impl TestSandbox {
    fn new() -> Option<Self> {
        let guard = daemon_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        if !tmux_usable() {
            eprintln!("Skipping daemon E2E test: tmux not usable");
            return None;
        }

        if stale_gflowd_session_present() {
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
        let config = format!("[daemon]\nhost = \"127.0.0.1\"\nport = {port}\n");
        std::fs::write(config_home.join("gflow/gflow.toml"), config).unwrap();

        let sandbox = Self {
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
        };

        sandbox.seed_tmux_environment();
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
                gpus: None,
                gpu_allocation_strategy: Default::default(),
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
        let result = self.run_gflow(["gflowd", "up"]);
        result.assert_success("gflowd up");
        self.daemon_started = true;
    }

    fn stop_daemon(&mut self) {
        if !self.daemon_started {
            return;
        }

        let _ = self.run_gflow(["gflowd", "down"]);
        self.daemon_started = false;
    }
}

impl Drop for TestSandbox {
    fn drop(&mut self) {
        self.stop_daemon();

        let sessions = get_all_session_names();
        for session in sessions {
            if session == DAEMON_SESSION
                || session == self.bootstrap_session
                || session.starts_with("gflow_server_new_")
            {
                let _ = Command::new("tmux")
                    .args(["kill-session", "-t", &session])
                    .output();
            }
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
    while start.elapsed() < Duration::from_secs(10) {
        if !process_running(original_pid) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    assert!(
        !process_running(original_pid),
        "old daemon pid {original_pid} is still running"
    );

    sandbox.stop_daemon();
    wait_for_health_unreachable(&sandbox.base_url(), Duration::from_secs(10)).await;

    let status = sandbox.run_gflow(["gflowd", "status"]);
    status.assert_success("gflowd status after down");
    assert!(status.stdout.contains("Status: Not running"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tmux_job_execution_writes_logs_and_auto_closes_session() {
    let Some(mut sandbox) = TestSandbox::new() else {
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
