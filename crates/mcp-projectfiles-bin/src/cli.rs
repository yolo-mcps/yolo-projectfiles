use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "projectfiles")]
#[command(author, version, about = "MCP Protocol Server", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run the MCP server using stdio transport
    Stdio {
        /// Server name
        #[arg(long, default_value = "projectfiles")]
        name: String,

        /// Server version
        #[arg(long, default_value = env!("CARGO_PKG_VERSION"))]
        version: String,
    },
    /// Test the tool handler implementation
    Test,
    /// Show various MCP server components
    Show {
        #[command(subcommand)]
        command: ShowCommands,
    },
    /// Claude integration commands
    Claude {
        #[command(subcommand)]
        command: ClaudeCommands,
    },
}

#[derive(Subcommand)]
pub enum ShowCommands {
    /// Show all available tools with their MCP naming convention
    Tools {
        /// Custom name to replace 'projectfiles' in tool names
        #[arg(long, default_value = "projectfiles")]
        name: String,
    },
}

#[derive(Subcommand)]
pub enum ClaudeCommands {
    /// Register/unregister this MCP server with Claude
    Register,
    /// Show Claude integration status
    Status,
    /// Manage tool permissions for Claude
    Permissions,
    /// Configure Claude integration (TUI)
    Configure,
}