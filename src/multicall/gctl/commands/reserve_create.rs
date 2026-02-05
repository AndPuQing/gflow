use anyhow::Result;
use gflow::client::Client;
use gflow::config::Config;
use gflow::core::reservation::GpuSpec;
use gflow::print_field;
use gflow::utils::parsers::{parse_gpu_indices, parse_reservation_duration};
use gflow::utils::timezone::parse_reservation_time_with_tz;

pub struct ReserveCreateParams<'a> {
    pub user: &'a str,
    pub gpus: Option<u32>,
    pub gpu_spec: Option<&'a str>,
    pub start: &'a str,
    pub duration: &'a str,
    pub timezone: Option<&'a str>,
}

pub async fn handle_reserve_create(
    client: &Client,
    config: &Config,
    params: ReserveCreateParams<'_>,
) -> Result<()> {
    // Parse start time with timezone support
    let config_tz = config.timezone.as_deref();
    let start_time = parse_reservation_time_with_tz(params.start, config_tz, params.timezone)?;

    // Parse duration
    let duration_secs = parse_reservation_duration(params.duration)?;

    // Determine GPU specification
    let gpu_spec = match (params.gpus, params.gpu_spec) {
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
        .create_reservation(params.user.to_string(), gpu_spec, start_time, duration_secs)
        .await?;

    println!("Reservation created successfully.");
    print_field!("ReservationID", "{}", reservation_id);

    Ok(())
}
