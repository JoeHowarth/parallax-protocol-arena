use deno_bevy_interop::cmd_server::*;
use eyre::Result;

#[tokio::main]
async fn main() -> Result<()> {
    cmd_server().await?;
    Ok(())
}
