use anyhow::Result;
use gflow::client::Client;

pub async fn handle_reserve_cancel(client: &Client, id: u32) -> Result<()> {
    client.cancel_reservation(id).await?;

    println!("Reservation {} cancelled successfully.", id);

    Ok(())
}
