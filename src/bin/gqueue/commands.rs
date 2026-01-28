use anyhow::Result;
use gflow::client::Client;

pub mod list;
use list::ListOptions;

pub async fn handle_commands(config: &gflow::Config, args: &crate::cli::ListArgs) -> Result<()> {
    let client = Client::build(config)?;

    let options = ListOptions {
        states: args.states.clone(),
        jobs: args.jobs.clone(),
        names: args.names.clone(),
        sort: args.sort.clone(),
        limit: args.limit,
        all: args.all,
        completed: args.completed,
        since: args.since.clone(),
        group: args.group,
        tree: args.tree,
        format: args.format.clone(),
        tmux: args.tmux,
    };

    list::handle_list(&client, options).await?;

    Ok(())
}
