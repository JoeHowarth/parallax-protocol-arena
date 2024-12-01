use anyhow::Result;
use parallax_protocol_arena::cmd_server::*;

#[tokio::main]
async fn main() -> Result<()> {
    cmd_server().await?;
    Ok(())
}
