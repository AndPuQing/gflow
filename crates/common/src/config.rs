use serde::{Deserialize, Serialize};
use std::{error::Error, path::PathBuf};

static CONFIG: &str = r#"[slurmlet]
# The log directory of the slurmlet server
log_dir = "$HOME/.slurmlet/logs"

# The log level of the slurmlet server
# Valid values are "debug", "info", "warn", "error". Default is "info"
log_level = "info"

# The standard output of the slurmlet server
# Default is None
stdout = "$HOME/.slurmlet/slurmlet.stdout.log"

# The standard error of the slurmlet server
# Default is None
stderr = "$HOME/.slurmlet/slurmlet.stderr.log"

# The pid file of the slurmlet server
# Default is `$HOME/.slurmlet/slurmlet.pid`
pid = "$HOME/.slurmlet/slurmlet.pid"

# Tasks cache file, json format
cache = "$HOME/.slurmlet/cache.json"

[sock]
# The unix socket path of the slurmlet server
path = "/tmp/slurmlet.sock"
"#;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    pub slurmlet: Slurmlet,
    pub sock: Sock,
}

impl Config {
    pub fn init(path: Option<String>) -> Result<Config, Box<dyn Error>> {
        let path = match path {
            Some(p) => PathBuf::from(p),
            None => {
                let home = dirs::home_dir().ok_or("No home dir found")?;
                let config_path = home.join(".slurmlet/config.toml");
                if config_path.exists() {
                    config_path
                } else {
                    let config_dir = home.join(".slurmlet");
                    std::fs::create_dir_all(&config_dir)?;
                    let config_path = config_dir.join("config.toml");
                    std::fs::write(&config_path, CONFIG)?;
                    config_path
                }
            }
        };
        Config::from(path)
    }
}

pub fn get_with_home_path(path: &str) -> PathBuf {
    let expanded_path = shellexpand::env(path).unwrap();
    PathBuf::from(expanded_path.into_owned())
}

impl Config {
    fn from(s: PathBuf) -> Result<Self, Box<dyn Error>> {
        let config_str = std::fs::read_to_string(s)?;
        let mut config: Config = toml::from_str(&config_str)?;

        fn create_dir_if_not_exists(path: &PathBuf) -> Result<(), Box<dyn Error>> {
            if let Some(parent) = path.parent() {
                if !parent.exists() {
                    std::fs::create_dir_all(parent)?;
                }
            }
            Ok(())
        }

        if let Some(log_dir) = &config.slurmlet.log_dir {
            let log_dir = get_with_home_path(log_dir);
            create_dir_if_not_exists(&log_dir)?;
            config.slurmlet.log_dir = Some(log_dir.to_str().unwrap().to_string());
        }

        if let Some(log_level) = &config.slurmlet.log_level {
            let allowed = ["debug", "info", "warn", "error"];
            if !allowed.contains(&log_level.as_str()) {
                config.slurmlet.log_level = Some("info".to_string());
            }
        }

        if let Some(stdout) = &config.slurmlet.stdout {
            let stdout = get_with_home_path(stdout);
            create_dir_if_not_exists(&stdout)?;
            config.slurmlet.stdout = Some(stdout.to_str().unwrap().to_string());
        }

        if let Some(stderr) = &config.slurmlet.stderr {
            let stderr = get_with_home_path(stderr);
            create_dir_if_not_exists(&stderr)?;
            config.slurmlet.stderr = Some(stderr.to_str().unwrap().to_string());
        }

        if let Some(pid) = &config.slurmlet.pid {
            let pid = get_with_home_path(pid);
            create_dir_if_not_exists(&pid)?;
            config.slurmlet.pid = Some(pid.to_str().unwrap().to_string());
        }

        let path = get_with_home_path(&config.sock.path);
        create_dir_if_not_exists(&path)?;
        config.sock.path = path.to_str().unwrap().to_string();

        Ok(config)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Slurmlet {
    pub log_dir: Option<String>,
    pub log_level: Option<String>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub pid: Option<String>,
    pub cache: Option<String>,
    pub interval: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Sock {
    pub path: String,
}
