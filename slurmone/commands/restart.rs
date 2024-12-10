use std::error::Error;

use crate::common::{arg::RestartArgs, config::Config};

pub async fn handle_restart(_args: RestartArgs, _config: &Config) -> Result<(), Box<dyn Error>> {
    Ok(())
}
