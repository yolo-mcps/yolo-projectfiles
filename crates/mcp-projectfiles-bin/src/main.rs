use anyhow::Result;
use clap::Parser;
use std::io::IsTerminal;
use tracing::{error, info, instrument};
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
        Commands::Stdio { name, version } => {
            info!("Starting MCP server with stdio transport");
            run_server(&name, &version, false, 0).await
        }
        Commands::Sse {
            name,
            version,
            port,
        } => {
            info!("Starting MCP server with SSE transport on port {}", port);
            run_server(&name, &version, true, port).await
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

#[instrument(level = "info", fields(name, version, use_sse, port))]
async fn run_server(name: &str, version: &str, use_sse: bool, port: u16) -> Result<()> {
    info!(
        name,
        version,
        transport = if use_sse { "sse" } else { "stdio" },
        "Initializing MCP server"
    );

    let result = if use_sse {
        mcp_projectfiles_core::run_sse_server(port).await
    } else {
        mcp_projectfiles_core::run_stdio_server().await
    };

    match result {
        Ok(_) => {
            info!(name, version, "MCP server completed successfully");
            Ok(())
        }
        Err(e) => {
            error!(name, version, error = %e, "MCP server encountered error");
            Err(e)
        }
    }
}