use anyhow::Result;
use clap::{Parser, Subcommand};
use std::io::IsTerminal;
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt};

#[derive(Parser)]
#[command(name = "yolo-executioner")]
#[command(author, version, about = "YOLO Executioner MCP Server - Process Management", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the MCP server using stdio transport
    Stdio {
        /// Server name
        #[arg(long, default_value = "yolo-executioner")]
        name: String,

        /// Server version
        #[arg(long, default_value = env!("CARGO_PKG_VERSION"))]
        version: String,

        /// Project root directory (defaults to current working directory)
        #[arg(long, env = "MCP_PROJECT_ROOT")]
        project_root: Option<std::path::PathBuf>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging with TTY detection for conditional ANSI colors
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("mcp_projectfiles_core=info,yolo_executioner=info,info"));

    fmt()
        .with_ansi(std::io::stderr().is_terminal()) // Only use ANSI colors if stderr is a TTY
        .with_env_filter(filter)
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Stdio { name: _, version: _, project_root } => {
            // Initialize project root if provided
            if let Some(root) = project_root {
                info!("Setting project root to: {:?}", root);
                mcp_projectfiles_core::config::init_project_root(root);
            }
            info!("Starting YOLO Executioner MCP server with stdio transport");
            mcp_projectfiles_core::run_stdio_server().await
        }
    }
}