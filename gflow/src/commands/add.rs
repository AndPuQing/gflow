use crate::{cli, client::Client};
use shared::Job;

pub(crate) fn handle_add(add_args: cli::AddArgs) {
    log::debug!("{:?}", add_args);
    // check is absolute path
    let mut script = add_args.script;
    if !script.is_absolute() {
        let pwd = std::env::current_dir().unwrap();
        script = pwd.join(&script);
    }
    let job = Job::new(script, add_args.gpus.unwrap_or(1));

    let client = Client::build();
    if let Err(e) = client {
        log::error!("Failed to build client: {}", e);
        std::process::exit(1);
    } else {
        client.unwrap().connect();
    }

    log::debug!("Client added job: {:?}", job);
}
