use anyhow::Result;
use gflow::client::Client;
use gflow::core::reservation::{GpuSpec, ReservationStatus};
use gflow::print_field;

pub async fn handle_reserve_get(client: &Client, id: u32) -> Result<()> {
    let reservation = client.get_reservation(id).await?;

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
                gflow::utils::format_system_time(r.start_time)
            );
            print_field!(
                "EndTime",
                "{}",
                gflow::utils::format_system_time(r.end_time())
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
                gflow::utils::format_system_time(r.created_at)
            );
            if let Some(cancelled_at) = r.cancelled_at {
                print_field!(
                    "CancelledAt",
                    "{}",
                    gflow::utils::format_system_time(cancelled_at)
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
