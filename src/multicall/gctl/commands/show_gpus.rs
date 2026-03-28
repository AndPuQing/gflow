use anyhow::Result;
use gflow::client::Client;

pub async fn handle_show_gpus(client: &Client) -> Result<()> {
    let info = client.get_info().await?;

    for gpu in &info.gpus {
        let status = if gpu.available { "available" } else { "in_use" };
        let mut annotations = Vec::new();

        let restricted = match &info.allowed_gpu_indices {
            None => false,
            Some(a) => !a.contains(&gpu.index),
        };

        if restricted {
            annotations.push("restricted".to_string());
        }
        if let Some(reason) = &gpu.reason {
            annotations.push(reason.clone());
        }

        if annotations.is_empty() {
            println!("{}\t{}", gpu.index, status);
        } else {
            println!("{}\t{}\t{}", gpu.index, status, annotations.join("\t"));
        }
    }

    Ok(())
}
