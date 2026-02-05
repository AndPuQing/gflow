use anyhow::Result;
use gflow::{client::Client, utils::parse_gpu_indices};

pub async fn handle_set_gpus(client: &Client, gpu_spec: &str) -> Result<()> {
    let allowed_indices = if gpu_spec.eq_ignore_ascii_case("all") {
        None
    } else {
        Some(parse_gpu_indices(gpu_spec)?)
    };

    client.set_allowed_gpus(allowed_indices.clone()).await?;

    match allowed_indices {
        None => println!("GPU restriction removed: all GPUs are now available"),
        Some(ref indices) => {
            println!(
                "GPU restriction updated: only GPUs {:?} will be used",
                indices
            )
        }
    }

    Ok(())
}
