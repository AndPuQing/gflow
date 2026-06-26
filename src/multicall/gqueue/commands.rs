use anyhow::Result;
use std::path::PathBuf;

pub mod list;
use list::ListOptions;

pub async fn handle_commands(
    config_path: &Option<PathBuf>,
    args: &super::cli::ListArgs,
) -> Result<()> {
    let client = gflow::create_client(config_path)?;

    let options = ListOptions {
        user: args.user.clone(),
        states: args.states.clone(),
        jobs: args.jobs.clone(),
        names: args.names.clone(),
        project: args.project.clone(),
        sort: args.sort.clone(),
        limit: args.limit,
        all: args.all,
        completed: args.completed,
        since: args.since.clone(),
        group: args.group,
        tree: args.tree,
        format: args.format.clone(),
        tmux: args.tmux,
        output: args.output.clone(),
        watch: args.watch,
        interval: args.interval,
    };

    list::handle_list(&client, options).await?;

    Ok(())
}
