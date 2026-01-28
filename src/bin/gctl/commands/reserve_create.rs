use anyhow::Result;
use gflow::client::Client;
use gflow::print_field;
use gflow::utils::parsers::{parse_reservation_duration, parse_reservation_time};

pub async fn handle_reserve_create(
    client: &Client,
    user: &str,
    gpus: u32,
    start: &str,
    duration: &str,
) -> Result<()> {
    // Parse start time
    let start_time = parse_reservation_time(start)?;

    // Parse duration
    let duration_secs = parse_reservation_duration(duration)?;

    // Create reservation
    let reservation_id = client
        .create_reservation(user.to_string(), gpus, start_time, duration_secs)
        .await?;

    println!("Reservation created successfully.");
    print_field!("ReservationID", "{}", reservation_id);

    Ok(())
}
