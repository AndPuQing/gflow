use std::path::PathBuf;

use crate::cli::GFlowd;

static DEFAULT_CONFIG: &str = "/etc/default/gflowd";

pub fn load_config(args: GFlowd) -> Result<config::Config, config::ConfigError> {
    let mut config_vec = vec![PathBuf::from(DEFAULT_CONFIG)];
    if !config_vec[0].exists() {
        config_vec.clear();
    }
    if let Some(config) = args.config {
        if config.exists() {
            config_vec.push(config);
        } else {
            return Err(config::ConfigError::NotFound(format!(
                "Config file {:?} does not exist",
                config
            )));
        }
    }
    let settings = config::Config::builder();
    let settings = config_vec.iter().fold(settings, |settings, path| {
        settings.add_source(config::File::from(path.as_path()))
    });
    settings
        .add_source(config::Environment::with_prefix("GFLOW"))
        .set_default("PORT", 59000)
        .unwrap()
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_load_config() {
        let mut temp_file = NamedTempFile::with_suffix(".toml").unwrap();
        temp_file.write_all(b"app.name = 'gflowd'").unwrap();
        let args = GFlowd {
            config: Some(temp_file.path().to_path_buf()),
            verbose: Default::default(),
        };
        let config = load_config(args).unwrap();
        assert_eq!(config.get_string("app.name").unwrap(), "gflowd");
    }

    #[test]
    fn test_load_config_not_found() {
        let args = GFlowd {
            config: Some(PathBuf::from("/tmp/does-not-exist.toml")),
            verbose: Default::default(),
        };
        let config = load_config(args);
        assert!(config.is_err());
    }

    #[test]
    fn test_load_config_default() {
        let args = GFlowd {
            config: None,
            verbose: Default::default(),
        };
        let config = load_config(args).unwrap();
        assert_eq!(config.get_int("PORT").unwrap(), 59000);
    }
}
