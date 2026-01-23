use anyhow::Result;
use gflow::client::Client;

pub async fn handle_show_gpus(client: &Client) -> Result<()> {
    let info = client.get_info().await?;

    for gpu in &info.gpus {
        let status = if gpu.available { "available" } else { "in_use" };

        let restricted = match &info.allowed_gpu_indices {
            None => false,
            Some(a) => !a.contains(&gpu.index),
        };

        if restricted {
            println!("{}\t{}\trestricted", gpu.index, status);
        } else {
            println!("{}\t{}", gpu.index, status);
        }
    }

    Ok(())
}
