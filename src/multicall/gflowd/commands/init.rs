use std::collections::HashMap;
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};

use anyhow::Context;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};
use nvml_wrapper::Nvml;

#[derive(Debug, Clone)]
pub struct InitArgs {
    pub yes: bool,
    pub force: bool,
    pub advanced: bool,
    pub gpus: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub timezone: Option<String>,
}

#[derive(Debug, Clone)]
struct DetectedGpu {
    index: u32,
    name: String,
    memory_total_mb: Option<u64>,
}

pub async fn handle_init(config_path: &Option<PathBuf>, args: InitArgs) -> anyhow::Result<()> {
    let target_path = get_target_config_path(config_path)?;

    if target_path.exists() && !args.force {
        anyhow::bail!(
            "Config file already exists: {} (use --force to overwrite)",
            target_path.display()
        );
    }

    let is_interactive = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    let detected_timezone = gflow::utils::timezone::get_local_timezone().to_string();

    let detected_gpus = detect_gpus().unwrap_or_else(|e| {
        eprintln!("Warning: GPU detection failed: {e}");
        Vec::new()
    });

    let mut cfg = gflow::config::Config::default();
    if let Some(host) = args.host.clone() {
        cfg.daemon.host = host;
    }
    if let Some(port) = args.port {
        cfg.daemon.port = port;
    }

    // Non-interactive mode: use flags + defaults.
    if args.yes || !is_interactive {
        if let Some(gpu_spec) = args.gpus.as_deref() {
            cfg.daemon.gpus = Some(parse_and_validate_gpu_indices(gpu_spec, &detected_gpus)?);
        }

        cfg.timezone = normalize_timezone_arg(args.timezone.as_deref(), &detected_timezone)?;

        let rendered = render_config_toml(&cfg)?;
        write_config_file(&target_path, &rendered, args.force)?;
        print_success(&target_path, &cfg, &detected_gpus);
        return Ok(());
    }

    // Interactive wizard.
    let theme = ColorfulTheme::default();

    println!("gflow Configuration Wizard\n");
    println!("Welcome to gflow! Let's set up your scheduler.\n");

    println!("[1/5] Detecting GPUs...");
    if detected_gpus.is_empty() {
        println!("No NVIDIA GPUs detected (or NVML is unavailable).");
    } else {
        println!("Found {} NVIDIA GPU(s):", detected_gpus.len());
        for g in &detected_gpus {
            if let Some(mem) = g.memory_total_mb {
                println!("  GPU {}: {} ({}MB)", g.index, g.name, mem);
            } else {
                println!("  GPU {}: {}", g.index, g.name);
            }
        }
    }
    println!();

    println!("[2/5] GPU Selection");
    if detected_gpus.is_empty() {
        println!("Skipping GPU selection.");
    } else {
        let all_range = format!(
            "{}-{}",
            detected_gpus.first().map(|g| g.index).unwrap_or(0),
            detected_gpus.last().map(|g| g.index).unwrap_or(0)
        );

        if let Some(gpu_spec) = args.gpus.as_deref() {
            cfg.daemon.gpus = Some(parse_and_validate_gpu_indices(gpu_spec, &detected_gpus)?);
        } else {
            let options = vec![
                format!("All GPUs ({all_range}) [recommended]"),
                "Specific GPUs".to_string(),
                "Configure later (use defaults)".to_string(),
            ];
            let choice = Select::with_theme(&theme)
                .with_prompt("Which GPUs should gflow manage?")
                .default(0)
                .items(&options)
                .interact()
                .map_err(map_dialoguer_err)?;

            match choice {
                0 | 2 => cfg.daemon.gpus = None,
                1 => {
                    let input: String = Input::with_theme(&theme)
                        .with_prompt("Enter GPU indices (e.g., 0,2 or 0-3)")
                        .default(all_range)
                        .interact_text()
                        .map_err(map_dialoguer_err)?;
                    cfg.daemon.gpus = Some(parse_and_validate_gpu_indices(&input, &detected_gpus)?);
                }
                _ => unreachable!(),
            }
        }
    }
    println!();

    println!("[3/5] Network Configuration");
    cfg.daemon.host = Input::with_theme(&theme)
        .with_prompt("Daemon host")
        .default(cfg.daemon.host.clone())
        .interact_text()
        .map_err(map_dialoguer_err)?;
    cfg.daemon.port = Input::with_theme(&theme)
        .with_prompt("Daemon port")
        .default(cfg.daemon.port)
        .interact_text()
        .map_err(map_dialoguer_err)?;
    if !port_looks_available(&cfg.daemon.host, cfg.daemon.port) {
        let ok = Confirm::with_theme(&theme)
            .with_prompt(format!(
                "Port {} may be in use on host '{}'. Continue anyway?",
                cfg.daemon.port, cfg.daemon.host
            ))
            .default(false)
            .interact()
            .map_err(map_dialoguer_err)?;
        if !ok {
            anyhow::bail!("Aborted by user");
        }
    }
    println!();

    println!("[4/5] Timezone");
    println!("Detected timezone: {}", detected_timezone);
    if let Some(ref tz) = args.timezone {
        cfg.timezone = normalize_timezone_arg(Some(tz), &detected_timezone)?;
    } else {
        let use_detected = Confirm::with_theme(&theme)
            .with_prompt("Store this timezone in config?")
            .default(true)
            .interact()
            .map_err(map_dialoguer_err)?;
        cfg.timezone = if use_detected {
            Some(detected_timezone.clone())
        } else {
            None
        };
    }
    println!();

    println!("[5/5] Advanced Options");
    let configure_advanced = if args.advanced {
        true
    } else {
        Confirm::with_theme(&theme)
            .with_prompt("Configure advanced options?")
            .default(false)
            .interact()
            .map_err(map_dialoguer_err)?
    };

    if configure_advanced {
        let enable = Confirm::with_theme(&theme)
            .with_prompt("Enable webhook notifications?")
            .default(false)
            .interact()
            .map_err(map_dialoguer_err)?;
        if enable {
            cfg.notifications.enabled = true;
            let urls: String = Input::with_theme(&theme)
                .with_prompt("Webhook URL(s) (comma-separated, leave empty to skip)")
                .allow_empty(true)
                .interact_text()
                .map_err(map_dialoguer_err)?;
            let urls: Vec<String> = urls
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect();
            cfg.notifications.webhooks = urls
                .into_iter()
                .map(|url| gflow::config::WebhookConfig {
                    url,
                    events: vec!["*".to_string()],
                    filter_users: None,
                    headers: HashMap::new(),
                    timeout_secs: 10,
                    max_retries: 3,
                })
                .collect();

            let max_conc: usize = Input::with_theme(&theme)
                .with_prompt("Max concurrent webhook deliveries")
                .default(cfg.notifications.max_concurrent_deliveries)
                .interact_text()
                .map_err(map_dialoguer_err)?;
            cfg.notifications.max_concurrent_deliveries = max_conc;
        }
    }
    println!();

    println!("Configuration preview:\n");
    let rendered = render_config_toml(&cfg)?;
    println!("{rendered}");

    let confirm = Confirm::with_theme(&theme)
        .with_prompt(format!("Write configuration to {}?", target_path.display()))
        .default(true)
        .interact()
        .map_err(map_dialoguer_err)?;
    if !confirm {
        anyhow::bail!("Aborted by user");
    }

    write_config_file(&target_path, &rendered, args.force)?;
    print_success(&target_path, &cfg, &detected_gpus);
    Ok(())
}

fn map_dialoguer_err(err: dialoguer::Error) -> io::Error {
    match err {
        dialoguer::Error::IO(e) => e,
    }
}

fn get_target_config_path(config_path: &Option<PathBuf>) -> anyhow::Result<PathBuf> {
    if let Some(p) = config_path {
        return Ok(p.clone());
    }
    Ok(gflow::core::get_config_dir()?.join("gflow.toml"))
}

fn normalize_timezone_arg(
    tz: Option<&str>,
    detected_timezone: &str,
) -> anyhow::Result<Option<String>> {
    match tz {
        None => Ok(Some(detected_timezone.to_string())),
        Some(tz) => {
            let tz = tz.trim();
            if tz.is_empty() || tz.eq_ignore_ascii_case("local") {
                return Ok(None);
            }
            tz.parse::<chrono_tz::Tz>()
                .with_context(|| format!("Invalid timezone: {tz}"))?;
            Ok(Some(tz.to_string()))
        }
    }
}

fn detect_gpus() -> anyhow::Result<Vec<DetectedGpu>> {
    let nvml = Nvml::init().context("NVML init failed")?;
    let count = nvml.device_count().context("NVML device_count failed")?;

    let mut gpus = Vec::with_capacity(count as usize);
    for idx in 0..count {
        let device = nvml
            .device_by_index(idx)
            .context("NVML device_by_index failed")?;
        let name = device.name().unwrap_or_else(|_| "Unknown".to_string());
        let memory_total_mb = device.memory_info().ok().map(|m| m.total / 1024 / 1024);
        gpus.push(DetectedGpu {
            index: idx,
            name,
            memory_total_mb,
        });
    }
    Ok(gpus)
}

fn parse_and_validate_gpu_indices(
    spec: &str,
    detected: &[DetectedGpu],
) -> anyhow::Result<Vec<u32>> {
    let indices = gflow::utils::parse_gpu_indices(spec)?;
    if detected.is_empty() {
        return Ok(indices);
    }
    let max = detected.len() as u32;
    let (valid, invalid): (Vec<_>, Vec<_>) = indices.into_iter().partition(|&i| i < max);
    if !invalid.is_empty() {
        eprintln!(
            "Warning: Ignoring invalid GPU indices {:?} (only {} GPU(s) detected).",
            invalid,
            detected.len()
        );
    }
    if valid.is_empty() {
        anyhow::bail!(
            "No valid GPU indices specified (detected {} GPU(s)).",
            detected.len()
        );
    }
    Ok(valid)
}

fn port_looks_available(host: &str, port: u16) -> bool {
    use std::net::TcpListener;

    let host = match host {
        "localhost" => "127.0.0.1",
        other => other,
    };
    TcpListener::bind((host, port)).is_ok()
}

fn render_config_toml(cfg: &gflow::config::Config) -> anyhow::Result<String> {
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    let mut out = String::new();
    out.push_str(&format!("# Generated by gflowd init on {now}\n\n"));
    out.push_str(&toml::to_string_pretty(cfg).context("Failed to serialize config to TOML")?);
    Ok(out)
}

fn write_config_file(path: &Path, content: &str, force: bool) -> anyhow::Result<()> {
    if path.exists() && !force {
        anyhow::bail!(
            "Refusing to overwrite existing file: {} (use --force)",
            path.display()
        );
    }

    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Invalid path: {}", path.display()))?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("Failed to create config directory {}", parent.display()))?;

    let tmp = parent.join(format!(".gflow.toml.tmp.{}", std::process::id()));
    std::fs::write(&tmp, content)
        .with_context(|| format!("Failed to write temp file {}", tmp.display()))?;

    // On Windows, rename won't replace an existing file. Remove it first when forcing.
    if force && path.exists() {
        std::fs::remove_file(path)
            .with_context(|| format!("Failed to remove existing file {}", path.display()))?;
    }
    std::fs::rename(&tmp, path).with_context(|| {
        format!(
            "Failed to move temp file {} to {}",
            tmp.display(),
            path.display()
        )
    })?;
    Ok(())
}

fn print_success(path: &Path, cfg: &gflow::config::Config, detected_gpus: &[DetectedGpu]) {
    println!("Configuration saved to: {}", path.display());
    println!();
    println!("Configuration Summary:");

    match cfg.daemon.gpus {
        None => {
            if detected_gpus.is_empty() {
                println!("  GPUs: all (no GPUs detected at init time)");
            } else {
                let first = detected_gpus.first().map(|g| g.index).unwrap_or(0);
                let last = detected_gpus.last().map(|g| g.index).unwrap_or(0);
                println!("  GPUs: {}-{} (all detected)", first, last);
            }
        }
        Some(ref gpus) => {
            println!("  GPUs: {}", format_indices(gpus));
        }
    }

    println!("  Host: {}", cfg.daemon.host);
    println!("  Port: {}", cfg.daemon.port);
    println!("  Timezone: {}", cfg.timezone.as_deref().unwrap_or("local"));
    if cfg.notifications.enabled && !cfg.notifications.webhooks.is_empty() {
        println!(
            "  Notifications: enabled ({} webhook(s))",
            cfg.notifications.webhooks.len()
        );
    }
    println!();
    println!("Next steps:");
    println!("  1. Start the daemon: gflowd up");
    println!("  2. Check status: gflowd status");
    println!("  3. Submit a job: gbatch --gpus 1 script.sh");
}

fn format_indices(indices: &[u32]) -> String {
    indices
        .iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_config_is_loadable() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("gflow.toml");

        let mut cfg = gflow::config::Config::default();
        cfg.daemon.host = "localhost".to_string();
        cfg.daemon.port = 60001;
        cfg.daemon.gpus = Some(vec![0, 2, 3]);
        cfg.timezone = Some("UTC".to_string());
        cfg.notifications.enabled = true;
        cfg.notifications.webhooks = vec![gflow::config::WebhookConfig {
            url: "https://example.com/hook".to_string(),
            events: vec!["*".to_string()],
            filter_users: None,
            headers: HashMap::new(),
            timeout_secs: 10,
            max_retries: 3,
        }];

        let rendered = render_config_toml(&cfg).unwrap();
        write_config_file(&path, &rendered, true).unwrap();

        let loaded = gflow::config::load_config(Some(&path)).unwrap();
        assert_eq!(loaded.daemon.host, "localhost");
        assert_eq!(loaded.daemon.port, 60001);
        assert_eq!(loaded.daemon.gpus, Some(vec![0, 2, 3]));
        assert_eq!(loaded.timezone.as_deref(), Some("UTC"));
        assert!(loaded.notifications.enabled);
        assert_eq!(loaded.notifications.webhooks.len(), 1);
        assert_eq!(
            loaded.notifications.webhooks[0].url,
            "https://example.com/hook"
        );
    }
}
