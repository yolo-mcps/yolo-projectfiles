use anyhow::Result;
use clap::Parser;
use std::io::IsTerminal;
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt};

mod cli;
use cli::{Cli, Commands, ShowCommands};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging with TTY detection for conditional ANSI colors
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("mcp_projectfiles_core=info,mcp_projectfiles_bin=info,info"));

    fmt()
        .with_ansi(std::io::stderr().is_terminal()) // Only use ANSI colors if stderr is a TTY
        .with_env_filter(filter)
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Stdio { name: _, version: _ } => {
            info!("Starting MCP server with stdio transport");
            mcp_projectfiles_core::run_stdio_server().await
        }
        Commands::Test => mcp_projectfiles_core::test_handler().await,
        Commands::Show { command } => match command {
            ShowCommands::Tools { name } => {
                list_tools(&name);
                Ok(())
            }
        },
    }
}

fn list_tools(name: &str) {
    let tools = mcp_projectfiles_core::tools::ProtocolTools::tools();

    for tool in tools {
        println!("mcp__{}__{}", name, tool.name);
    }
}