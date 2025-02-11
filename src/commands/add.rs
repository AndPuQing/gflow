use crate::{cli, client::Client};
use anyhow::{Context, Result};
use gflow::Job;
use std::path::PathBuf;

pub(crate) async fn handle_submit(submit_args: cli::SubmitArgs) -> Result<()> {
    log::debug!("{:?}", submit_args);

    let job = build_job(submit_args)?;
    let client = Client::build().context("Failed to build client")?;

    client.add_job(job).await.context("Failed to add job")?;

    log::info!("Job added successfully");
    Ok(())
}

fn build_job(args: cli::SubmitArgs) -> Result<Job> {
    let mut builder = Job::builder()
        .conda_env(&args.conda_env)
        .gpus(args.gpus.unwrap_or(0));

    if let Some(script) = args.script {
        let script_path = make_absolute_path(script)?;
        builder = builder.script(script_path);
    } else if let Some(command) = args.command {
        builder = builder.command(command);
    }

    Ok(builder.build())
}

fn make_absolute_path(path: PathBuf) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path)
    } else {
        std::env::current_dir()
            .map(|pwd| pwd.join(path))
            .context("Failed to get current directory")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_make_absolute_path() {
        let path = Path::new("test.txt").to_path_buf();
        let result = make_absolute_path(path.clone()).unwrap();
        assert_eq!(result, std::env::current_dir().unwrap().join(path));
    }
}
