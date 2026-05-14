use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "xtask")]
struct XtaskArgs {
    #[command(subcommand)]
    command: XtaskCommand,
}

#[derive(Debug, Subcommand)]
enum XtaskCommand {
    Fmt,
    Clippy,
    Test,
    Smoke,
    Release,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let args = XtaskArgs::parse();
    match args.command {
        XtaskCommand::Fmt => cargo(&["fmt", "--all", "--check"]),
        XtaskCommand::Clippy => cargo(&["clippy", "--all-targets", "--", "-D", "warnings"]),
        XtaskCommand::Test => cargo(&["test", "--workspace"]),
        XtaskCommand::Smoke => cargo(&["test", "-p", "agent-cli", "--test", "live_preview"]),
        XtaskCommand::Release => cargo(&["build", "--workspace", "--release"]),
    }
}

fn cargo(args: &[&str]) -> Result<()> {
    let status = Command::new("cargo")
        .args(args)
        .status()
        .with_context(|| format!("failed to spawn cargo {}", args.join(" ")))?;
    if !status.success() {
        bail!("cargo {} failed with status {status}", args.join(" "));
    }
    Ok(())
}
