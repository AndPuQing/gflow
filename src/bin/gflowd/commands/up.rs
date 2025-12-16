use anyhow::Result;
use gflow::tmux::TmuxSession;

pub async fn handle_up(gpus: Option<String>) -> Result<()> {
    let session = TmuxSession::new(super::TMUX_SESSION_NAME.to_string());

    let mut command = String::from("gflowd -vvv");
    if let Some(gpu_spec) = gpus {
        command.push_str(&format!(" --gpus-internal '{}'", gpu_spec));
    }

    session.send_command(&command);
    println!("gflowd started.");
    Ok(())
}
