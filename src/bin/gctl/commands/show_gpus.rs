use anyhow::Result;
use gflow::client::Client;

pub async fn handle_show_gpus(client: &Client) -> Result<()> {
    let info = client.get_info().await?;

    println!("=== GPU Configuration ===\n");

    match &info.allowed_gpu_indices {
        None => {
            println!("GPU Restriction: None (all GPUs available)");
        }
        Some(allowed) => {
            println!("GPU Restriction: Only GPUs {:?} are allowed", allowed);
        }
    }

    println!("\n=== Detected GPUs ===\n");

    for gpu in &info.gpus {
        let status = if gpu.available { "Available" } else { "In Use" };

        let restricted = match &info.allowed_gpu_indices {
            None => false,
            Some(a) => !a.contains(&gpu.index),
        };

        let suffix = if restricted { " (RESTRICTED)" } else { "" };

        println!("GPU {}: {}{}", gpu.index, status, suffix);
    }

    Ok(())
}
