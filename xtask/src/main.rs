use std::process;

use anyhow::Result;
use clap::{ArgMatches, Command};

fn main() -> Result<()> {
    let args = clap::command!()
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(Command::new("install").about("Cargo Install"))
        .get_matches();

    match args.subcommand() {
        Some(("install", args)) => handle_install_command(args),
        Some((command, _)) => anyhow::bail!("Unexpected command: {command}"),
        None => anyhow::bail!("Expected subcommand"),
    }
}

fn handle_install_command(_args: &ArgMatches) -> Result<()> {
    let mut command = process::Command::new("cargo");
    command
        .args(["install", "--path", "crates/mcp-projectfiles-bin"])
        .status()?;

    Ok(())
}