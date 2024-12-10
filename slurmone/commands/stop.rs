use std::error::Error;

use crate::common::{arg::StopArgs, config::Config};

pub async fn handle_stop(_args: StopArgs, _config: &Config) -> Result<(), Box<dyn Error>> {
    Ok(())
}
