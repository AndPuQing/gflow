use anyhow::Result;
use vergen_gix::{BuildBuilder, Emitter, GixBuilder};

fn main() -> Result<()> {
    let mut gix = GixBuilder::default();
    gix.sha(true).branch(true);

    let build = BuildBuilder::all_build()?;

    Emitter::default()
        .add_instructions(&build)?
        .add_instructions(&gix.build()?)?
        .emit()?;
    Ok(())
}
