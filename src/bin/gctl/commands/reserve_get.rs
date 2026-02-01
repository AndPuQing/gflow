use anyhow::Result;
use gflow::client::Client;
use gflow::config::Config;
use gflow::core::reservation::{GpuSpec, ReservationStatus};
use gflow::print_field;
use gflow::utils::timezone::format_system_time;

pub async fn handle_reserve_get(client: &Client, config: &Config, id: u32) -> Result<()> {
    let reservation = client.get_reservation(id).await?;

    let config_tz = config.timezone.as_deref();

    match reservation {
        Some(r) => {
            println!("Reservation Details:");
            print_field!("ID", "{}", r.id);
            print_field!("User", "{}", r.user);

            // Display GPU specification
            match &r.gpu_spec {
                GpuSpec::Count(count) => {
                    print_field!("GPUCount", "{}", count);
                }
                GpuSpec::Indices(indices) => {
                    let indices_str = indices
                        .iter()
                        .map(|i| i.to_string())
                        .collect::<Vec<_>>()
                        .join(",");
                    print_field!("GPUIndices", "{}", indices_str);
                }
            }

            print_field!(
                "StartTime",
                "{}",
                format_system_time(r.start_time, config_tz, "%Y-%m-%d %H:%M:%S %Z")?
            );
            print_field!(
                "EndTime",
                "{}",
                format_system_time(r.end_time(), config_tz, "%Y-%m-%d %H:%M:%S %Z")?
            );
            print_field!(
                "Duration",
                "{}",
                gflow::utils::format_duration_compact(r.duration)
            );
            print_field!("Status", "{}", format_status(r.status));
            print_field!(
                "CreatedAt",
                "{}",
                format_system_time(r.created_at, config_tz, "%Y-%m-%d %H:%M:%S %Z")?
            );
            if let Some(cancelled_at) = r.cancelled_at {
                print_field!(
                    "CancelledAt",
                    "{}",
                    format_system_time(cancelled_at, config_tz, "%Y-%m-%d %H:%M:%S %Z")?
                );
            }
        }
        None => {
            println!("Reservation {} not found.", id);
        }
    }

    Ok(())
}

fn format_status(status: ReservationStatus) -> &'static str {
    match status {
        ReservationStatus::Pending => "Pending",
        ReservationStatus::Active => "Active",
        ReservationStatus::Completed => "Completed",
        ReservationStatus::Cancelled => "Cancelled",
    }
}
