use anyhow::Result;

pub(crate) async fn handle_list() -> Result<()> {
    let _ = crate::tui::show_tui();
    Ok(())
}
