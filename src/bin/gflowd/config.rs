use crate::cli::GFlowd;

pub fn load_config(args: GFlowd) -> Result<config::Config, config::ConfigError> {
    let mut config_vec = vec![];
    if let Some(config) = args.config {
        if config.exists() {
            config_vec.push(config);
        } else {
            return Err(config::ConfigError::NotFound(format!(
                "Config file {config:?} does not exist",
            )));
        }
    }
    let settings = config::Config::builder();
    let settings = config_vec.iter().fold(settings, |settings, path| {
        settings.add_source(config::File::from(path.as_path()))
    });
    settings
        .add_source(config::Environment::with_prefix("GFLOW"))
        .set_default("port", 59000)
        .unwrap()
        .set_default("host", "localhost")
        .unwrap()
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, path::PathBuf};
    use tempfile::NamedTempFile;

    #[test]
    fn test_load_config() {
        let mut temp_file = NamedTempFile::with_suffix(".toml").unwrap();
        temp_file.write_all(b"app.name = 'gflowd'").unwrap();
        let args = GFlowd {
            config: Some(temp_file.path().to_path_buf()),
            verbose: Default::default(),
            cleanup: false,
        };
        let config = load_config(args).unwrap();
        assert_eq!(config.get_string("app.name").unwrap(), "gflowd");
    }

    #[test]
    fn test_load_config_not_found() {
        let args = GFlowd {
            config: Some(PathBuf::from("/tmp/does-not-exist.toml")),
            verbose: Default::default(),
            cleanup: false,
        };
        let config = load_config(args);
        assert!(config.is_err());
    }

    #[test]
    fn test_load_config_default() {
        let args = GFlowd {
            config: None,
            verbose: Default::default(),
            cleanup: false,
        };
        let config = load_config(args).unwrap();
        assert_eq!(config.get_int("PORT").unwrap(), 59000);
    }
}
