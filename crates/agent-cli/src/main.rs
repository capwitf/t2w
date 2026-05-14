use agent_cli::{CliArgs, SystemBrowserLauncher, init_logging, run_with_callback};
use clap::Parser;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_logging();
    let args = CliArgs::parse();
    let launcher = SystemBrowserLauncher;
    run_with_callback(args, &launcher, |message| println!("{message}")).await?;
    Ok(())
}
