use anyhow::Result;
use gflow::client::Client;
use gflow::core::reservation::ReservationStatus;
use tabled::{builder::Builder, settings::style::Style};

pub async fn handle_reserve_list(
    client: &Client,
    user: Option<String>,
    status: Option<String>,
    active_only: bool,
) -> Result<()> {
    let reservations = client.list_reservations(user, status, active_only).await?;

    if reservations.is_empty() {
        println!("No reservations found");
        return Ok(());
    }

    let mut builder = Builder::default();
    builder.push_record(["ID", "User", "GPUs", "Start", "End", "Status"]);

    for reservation in reservations {
        let start_time = format_system_time_short(reservation.start_time);
        let end_time = format_system_time_short(reservation.end_time());
        let status_str = format_status(reservation.status);

        builder.push_record([
            reservation.id.to_string(),
            reservation.user.to_string(),
            reservation.gpu_count.to_string(),
            start_time,
            end_time,
            status_str,
        ]);
    }

    let table = builder.build().with(Style::rounded()).to_string();
    println!("{}", table);

    Ok(())
}

/// Format SystemTime for table display (shorter format without "UTC" suffix)
fn format_system_time_short(time: std::time::SystemTime) -> String {
    use chrono::{DateTime, Utc};

    let duration = time
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let datetime =
        DateTime::<Utc>::from_timestamp(duration.as_secs() as i64, 0).unwrap_or_default();

    datetime.format("%Y-%m-%d %H:%M:%S").to_string()
}

fn format_status(status: ReservationStatus) -> String {
    match status {
        ReservationStatus::Pending => "Pending".to_string(),
        ReservationStatus::Active => "Active".to_string(),
        ReservationStatus::Completed => "Completed".to_string(),
        ReservationStatus::Cancelled => "Cancelled".to_string(),
    }
}
