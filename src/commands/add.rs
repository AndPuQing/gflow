use crate::{cli, client::Client};
use gflow::Job;

pub(crate) async fn handle_add(add_args: cli::AddArgs) {
    log::debug!("{:?}", add_args);
    // check is absolute path
    let mut script = add_args.script;
    if !script.is_absolute() {
        let pwd = std::env::current_dir().unwrap();
        script = pwd.join(&script);
    }
    let job = Job::new(script, add_args.gpus.unwrap_or(0));

    let client = Client::build();
    if let Err(e) = client {
        log::error!("Failed to build client: {}", e);
        std::process::exit(1);
    } else {
        let client = client.unwrap();
        let response = client.add_job(job).await;
        if let Err(e) = response {
            log::error!("Failed to add job: {}", e);
            std::process::exit(1);
        } else {
            log::info!("Job added successfully");
        }
    }
}
