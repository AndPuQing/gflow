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
            println!("Start Time: {}", format_system_time(r.start_time));
            println!("End Time: {}", format_system_time(r.end_time()));
            println!("Duration: {}", format_duration(r.duration));
            println!("Status: {}", format_status(r.status));
            println!("Created At: {}", format_system_time(r.created_at));
            if let Some(cancelled_at) = r.cancelled_at {
                println!("Cancelled At: {}", format_system_time(cancelled_at));
            }
        }
        None => {
            println!("Reservation {} not found", id);
        }
    }

    Ok(())
}

fn format_system_time(time: std::time::SystemTime) -> String {
    use chrono::{DateTime, Utc};

    let duration = time
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let datetime =
        DateTime::<Utc>::from_timestamp(duration.as_secs() as i64, 0).unwrap_or_default();

    datetime.format("%Y-%m-%d %H:%M:%S UTC").to_string()
}

fn format_duration(duration: std::time::Duration) -> String {
    let total_secs = duration.as_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    if hours > 0 {
        if minutes > 0 {
            format!("{}h {}m", hours, minutes)
        } else {
            format!("{}h", hours)
        }
    } else if minutes > 0 {
        if seconds > 0 {
            format!("{}m {}s", minutes, seconds)
        } else {
            format!("{}m", minutes)
        }
    } else {
        format!("{}s", seconds)
    }
}

fn format_status(status: ReservationStatus) -> &'static str {
    match status {
        ReservationStatus::Pending => "Pending",
        ReservationStatus::Active => "Active",
        ReservationStatus::Completed => "Completed",
        ReservationStatus::Cancelled => "Cancelled",
    }
}
