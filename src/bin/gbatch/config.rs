use crate::cli::GBatch;
use gflow::core::get_config_dir;

pub fn load_config(args: &GBatch) -> Result<config::Config, config::ConfigError> {
    let mut config_vec = vec![];

    // User-provided config file
    if let Some(config_path) = &args.config {
        if config_path.exists() {
            config_vec.push(config_path.clone());
        } else {
            return Err(config::ConfigError::NotFound(format!(
                "Config file {config_path:?} does not exist",
            )));
        }
    }

    // Default config file
    if let Ok(default_config_path) = get_config_dir().map(|d| d.join("gflow.toml")) {
        if default_config_path.exists() {
            config_vec.push(default_config_path);
        }
    }

    let settings = config::Config::builder();
    let settings = config_vec.iter().fold(settings, |s, path| {
        s.add_source(config::File::from(path.as_path()))
    });

    settings
        .add_source(config::Environment::with_prefix("GFLOW"))
        .set_default("port", 59000)
        .unwrap()
        .set_default("host", "localhost")
        .unwrap()
        .build()
}
