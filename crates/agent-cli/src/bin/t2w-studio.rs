use agent_cli::init_logging;
use agent_cli::studio::{StudioArgs, start_studio_server};
use clap::Parser;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_logging();
    let args = StudioArgs::parse();
    let server = start_studio_server(args).await?;
    println!("Studio server: {}", server.base_url());
    std::future::pending::<()>().await;
    Ok(())
}
