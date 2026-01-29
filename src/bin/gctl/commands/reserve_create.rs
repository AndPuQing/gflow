use anyhow::Result;
use gflow::client::Client;
use gflow::core::reservation::GpuSpec;
use gflow::print_field;
use gflow::utils::parsers::{
    parse_gpu_indices, parse_reservation_duration, parse_reservation_time,
};

pub async fn handle_reserve_create(
    client: &Client,
    user: &str,
    gpus: Option<u32>,
    gpu_spec: Option<&str>,
    start: &str,
    duration: &str,
) -> Result<()> {
    // Parse start time
    let start_time = parse_reservation_time(start)?;

    // Parse duration
    let duration_secs = parse_reservation_duration(duration)?;

    // Determine GPU specification
    let gpu_spec = match (gpus, gpu_spec) {
        (Some(count), None) => GpuSpec::Count(count),
        (None, Some(spec_str)) => {
            let indices = parse_gpu_indices(spec_str)?;
            if indices.is_empty() {
                anyhow::bail!("GPU specification cannot be empty");
            }
            GpuSpec::Indices(indices)
        }
        (Some(_), Some(_)) => {
            anyhow::bail!("Cannot specify both --gpus and --gpu-spec");
        }
        (None, None) => {
            anyhow::bail!("Must specify either --gpus or --gpu-spec");
        }
    };

    // Create reservation
    let reservation_id = client
        .create_reservation(user.to_string(), gpu_spec, start_time, duration_secs)
        .await?;

    println!("Reservation created successfully.");
    print_field!("ReservationID", "{}", reservation_id);

    Ok(())
}
