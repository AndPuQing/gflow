use anyhow::Result;
use gflow::client::Client;
use gflow::core::reservation::ReservationStatus;

pub async fn handle_reserve_get(client: &Client, id: u32) -> Result<()> {
    let reservation = client.get_reservation(id).await?;

    match reservation {
        Some(r) => {
            println!("Reservation ID: {}", r.id);
            println!("User: {}", r.user);
            println!("GPU Count: {}", r.gpu_count);
            println!(
                "Start Time: {}",
                gflow::utils::format_system_time(r.start_time)
            );
            println!(
                "End Time: {}",
                gflow::utils::format_system_time(r.end_time())
            );
            println!(
                "Duration: {}",
                gflow::utils::format_duration_compact(r.duration)
            );
            println!("Status: {}", format_status(r.status));
            println!(
                "Created At: {}",
                gflow::utils::format_system_time(r.created_at)
            );
            if let Some(cancelled_at) = r.cancelled_at {
                println!(
                    "Cancelled At: {}",
                    gflow::utils::format_system_time(cancelled_at)
                );
            }
        }
        None => {
            println!("Reservation {} not found", id);
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
